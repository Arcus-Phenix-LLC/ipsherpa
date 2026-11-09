//! Read-only, process-lifetime snapshots for one-shot DHCP query workers.
use std::{
    net::Ipv4Addr,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone)]
pub struct ClientRecord {
    pub address: Ipv4Addr,
    pub name: String,
    pub client_id: String,
    pub description: String,
    pub client_type: String,
    pub state: Option<String>,
    pub expires_unix_seconds: Option<i64>,
    pub expiry_label: Option<String>,
    pub is_reservation: bool,
    pub options_present: Option<bool>,
}

pub fn validate_scope_id(scope: &str) -> Result<Ipv4Addr, String> {
    let id: Ipv4Addr = scope
        .parse()
        .map_err(|_| "Select a valid IPv4 scope ID.".to_owned())?;
    // The lease API interprets zero as ALL subnets; never permit that wildcard.
    if id.is_unspecified() || id.is_broadcast() || id.is_multicast() {
        return Err("Select a specific IPv4 scope; wildcard scope queries are disabled.".into());
    }
    Ok(id)
}

fn client_type(value: u8) -> String {
    if value == 100 {
        return "Manual".into();
    }
    if value & !7 != 0 {
        return format!("Unknown ({value})");
    }
    match value & 3 {
        0 => "Unspecified".into(),
        1 => "DHCP".into(),
        2 => "BOOTP".into(),
        3 => "DHCP / BOOTP".into(),
        _ => unreachable!(),
    }
}

fn lease_state(value: u8) -> String {
    if value & 0x1c != 0 {
        return format!("Unknown ({value})");
    }
    let mut result = match value & 3 {
        0 => "Offered",
        1 => "Active",
        2 => "Declined",
        3 => "Doomed",
        _ => unreachable!(),
    }
    .to_owned();
    for (bit, label) in [
        (0x80, "Deleted"),
        (0x40, "DNS unregistered"),
        (0x20, "Both DNS records"),
    ] {
        if value & bit != 0 {
            result.push_str(" · ");
            result.push_str(label);
        }
    }
    result
}

fn expiry(ticks: u64) -> (Option<i64>, Option<String>) {
    match ticks {
        0 => (None, Some("Not set".into())),
        u64::MAX | 0x7fff_ffff_ffff_ffff => (None, Some("Never".into())),
        _ => (Some((ticks / 10_000_000) as i64 - 11_644_473_600), None),
    }
}

struct Batch {
    rows: Vec<ClientRecord>,
    next: u32,
    more: bool,
}

fn collect_pages(
    mut fetch: impl FnMut(u32) -> Result<Batch, String>,
) -> Result<Vec<ClientRecord>, String> {
    let mut resume = 0;
    let mut seen = std::collections::HashSet::from([0]);
    let mut rows = Vec::new();
    loop {
        let page = fetch(resume)?;
        if page.more && (page.rows.is_empty() || !seen.insert(page.next)) {
            return Err(
                "DHCP enumeration made no progress; no partial results were returned.".into(),
            );
        }
        rows.extend(page.rows);
        if !page.more {
            break;
        }
        resume = page.next;
    }
    rows.sort_by(|a, b| {
        a.address
            .cmp(&b.address)
            .then(a.client_id.cmp(&b.client_id))
    });
    Ok(rows)
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::ptr;
    #[repr(C)]
    struct Binary {
        length: u32,
        data: *mut u8,
    }
    #[repr(C)]
    struct DateTime {
        low: u32,
        high: u32,
    }
    impl DateTime {
        fn ticks(&self) -> u64 {
            u64::from(self.high) << 32 | u64::from(self.low)
        }
    }
    #[repr(C)]
    struct Host {
        address: u32,
        netbios: *mut u16,
        name: *mut u16,
    }
    #[repr(C)]
    struct Lease {
        address: u32,
        mask: u32,
        uid: Binary,
        name: *mut u16,
        comment: *mut u16,
        expires: DateTime,
        owner: Host,
        kind: u8,
        state: u8,
        quarantine: u32,
        probation: DateTime,
        capable: i32,
    }
    #[repr(C)]
    struct Reservation {
        address: u32,
        uid: Binary,
        name: *mut u16,
        description: *mut u16,
        kind: u8,
        options: u8,
    }
    // SDK definitions use arrays of POINTERS, not contiguous record structs.
    #[repr(C)]
    struct PointerPage<T> {
        count: u32,
        items: *mut *mut T,
    }
    type Enumerator<T> = unsafe extern "system" fn(
        *const u16,
        u32,
        *mut u32,
        u32,
        *mut *mut PointerPage<T>,
        *mut u32,
        *mut u32,
    ) -> u32;
    #[link(name = "dhcpsapi")]
    extern "system" {
        fn DhcpEnumSubnetClientsVQ(
            server: *const u16,
            scope: u32,
            resume: *mut u32,
            size: u32,
            page: *mut *mut PointerPage<Lease>,
            read: *mut u32,
            total: *mut u32,
        ) -> u32;
        fn DhcpV4EnumSubnetReservations(
            server: *const u16,
            scope: u32,
            resume: *mut u32,
            size: u32,
            page: *mut *mut PointerPage<Reservation>,
            read: *mut u32,
            total: *mut u32,
        ) -> u32;
    }
    unsafe fn text(value: *const u16) -> Result<String, String> {
        if value.is_null() {
            return Ok(String::new());
        }
        // SAFETY: successful RPC responses provide terminated UTF-16 strings.
        for length in 0..32768 {
            if unsafe { *value.add(length) } == 0 {
                return String::from_utf16(unsafe { std::slice::from_raw_parts(value, length) })
                    .map_err(|_| "DHCP returned invalid UTF-16 text.".into());
            }
        }
        Err("DHCP text exceeded the supported string length.".into())
    }
    unsafe fn uid(value: &Binary) -> Result<String, String> {
        if value.length == 0 {
            return Ok(String::new());
        }
        if value.length > 65536 || value.data.is_null() {
            return Err("DHCP returned an invalid client ID buffer.".into());
        }
        // SAFETY: validated RPC length/non-null pointer; copy all bytes as a UID.
        let bytes = unsafe { std::slice::from_raw_parts(value.data, value.length as usize) };
        Ok(bytes
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join("-"))
    }
    unsafe fn lease(value: &Lease) -> Result<ClientRecord, String> {
        let (expires_unix_seconds, expiry_label) = expiry(value.expires.ticks());
        Ok(ClientRecord {
            address: Ipv4Addr::from(value.address),
            name: unsafe { text(value.name) }?,
            description: unsafe { text(value.comment) }?,
            client_id: unsafe { uid(&value.uid) }?,
            client_type: client_type(value.kind),
            state: Some(lease_state(value.state)),
            expires_unix_seconds,
            expiry_label,
            is_reservation: value.kind != 100 && value.kind & 4 != 0,
            options_present: None,
        })
    }
    unsafe fn reservation(value: &Reservation) -> Result<ClientRecord, String> {
        Ok(ClientRecord {
            address: Ipv4Addr::from(value.address),
            name: unsafe { text(value.name) }?,
            description: unsafe { text(value.description) }?,
            client_id: unsafe { uid(&value.uid) }?,
            client_type: client_type(value.kind),
            state: None,
            expires_unix_seconds: None,
            expiry_label: None,
            is_reservation: true,
            options_present: Some(value.options != 0),
        })
    }
    unsafe fn copy_page<T>(
        raw: *const PointerPage<T>,
        read: u32,
        convert: unsafe fn(&T) -> Result<ClientRecord, String>,
    ) -> Result<Vec<ClientRecord>, String> {
        if raw.is_null() {
            return if read == 0 {
                Ok(vec![])
            } else {
                Err("DHCP returned a null page with records.".into())
            };
        }
        // SAFETY: caller supplies a successful, live SDK response. This function
        // copies owned values without freeing any RPC buffers or nested pointers.
        let page = unsafe { &*raw };
        if page.count != read || (read > 0 && page.items.is_null()) {
            return Err("DHCP returned inconsistent record counts.".into());
        }
        let mut rows = Vec::new();
        for index in 0..read as usize {
            let record = unsafe { (*page.items.add(index)).as_ref() }
                .ok_or("DHCP returned a null record pointer.")?;
            rows.push(unsafe { convert(record) }?);
        }
        Ok(rows)
    }
    fn enumerate<T>(
        server: &str,
        scope: Ipv4Addr,
        api: Enumerator<T>,
        label: &str,
        convert: unsafe fn(&T) -> Result<ClientRecord, String>,
    ) -> Result<Vec<ClientRecord>, String> {
        let wide: Vec<u16> = server.encode_utf16().chain(Some(0)).collect();
        collect_pages(|mut resume| {
            let mut raw = ptr::null_mut();
            let (mut read, mut total) = (0, 0);
            eprintln!("{label}: page begin");
            // SAFETY: fixed SDK ABI, terminated server input, initialized outputs.
            let code = unsafe {
                api(
                    wide.as_ptr(),
                    u32::from(scope),
                    &mut resume,
                    65536,
                    &mut raw,
                    &mut read,
                    &mut total,
                )
            };
            eprintln!("{label}: returned {code}, records={read}");
            match code {
                259 => Ok(Batch {rows:vec![],next:resume,more:false}),
                0 | 234 => Ok(Batch {rows:unsafe {copy_page(raw,read,convert)}?,next:resume,more:code==234}),
                code => Err(format!("{label} failed: {} (code {code}). Check read permissions, scope existence and server support.",std::io::Error::from_raw_os_error(code as i32))),
            }
            // RPC buffers have process lifetime and are reclaimed at worker exit.
        })
    }
    pub fn query(server: &str, scope: Ipv4Addr, kind: &str) -> Result<Vec<ClientRecord>, String> {
        match kind {
            "leases" => enumerate(
                server,
                scope,
                DhcpEnumSubnetClientsVQ,
                "DhcpEnumSubnetClientsVQ",
                lease,
            ),
            "reservations" => enumerate(
                server,
                scope,
                DhcpV4EnumSubnetReservations,
                "DhcpV4EnumSubnetReservations",
                reservation,
            ),
            _ => Err("Unknown record kind.".into()),
        }
    }
    #[cfg(all(test, target_pointer_width = "64"))]
    mod tests {
        use super::*;
        use std::mem::{offset_of, size_of};
        #[test]
        fn sdk_layouts_match_x64() {
            assert_eq!(size_of::<Lease>(), 96);
            assert_eq!(offset_of!(Lease, expires), 40);
            assert_eq!(offset_of!(Lease, owner), 48);
            assert_eq!(offset_of!(Lease, state), 73);
            assert_eq!(offset_of!(Lease, quarantine), 76);
            assert_eq!(size_of::<Reservation>(), 48);
            assert_eq!(offset_of!(Reservation, uid), 8);
            assert_eq!(offset_of!(Reservation, kind), 40);
            assert_eq!(size_of::<PointerPage<Lease>>(), 16);
        }
        #[test]
        fn copies_pointer_array_unicode_and_variable_length_ids() {
            let mut name: Vec<u16> = "Printer 東京".encode_utf16().chain(Some(0)).collect();
            let mut bytes = vec![1, 2, 3, 4, 5, 6, 7];
            let mut source = Reservation {
                address: 0xC000020A,
                uid: Binary {
                    length: 7,
                    data: bytes.as_mut_ptr(),
                },
                name: name.as_mut_ptr(),
                description: name.as_mut_ptr(),
                kind: 3,
                options: 1,
            };
            let mut pointers = vec![&mut source as *mut Reservation];
            let page = PointerPage {
                count: 1,
                items: pointers.as_mut_ptr(),
            };
            let result = unsafe { copy_page(&page, 1, reservation) }.unwrap();
            drop(name);
            drop(bytes);
            drop(pointers);
            assert_eq!(result[0].name, "Printer 東京");
            assert_eq!(result[0].client_id, "01-02-03-04-05-06-07");
            assert_eq!(result[0].address, Ipv4Addr::new(192, 0, 2, 10));
            assert_eq!(result[0].options_present, Some(true));
            assert!(unsafe { copy_page::<Reservation>(ptr::null(), 1, reservation) }.is_err());
            assert!(unsafe { copy_page(&page, 2, reservation) }.is_err());
        }
        #[test]
        fn copies_reserved_lease_without_assuming_mac_or_expiry() {
            let mut source = Lease {
                address: 0xC000020A,
                mask: 0xFFFFFF00,
                uid: Binary {
                    length: 0,
                    data: ptr::null_mut(),
                },
                name: ptr::null_mut(),
                comment: ptr::null_mut(),
                expires: DateTime {
                    low: u32::MAX,
                    high: 0x7FFFFFFF,
                },
                owner: Host {
                    address: 0,
                    netbios: ptr::null_mut(),
                    name: ptr::null_mut(),
                },
                kind: 5,
                state: 1,
                quarantine: 0,
                probation: DateTime { low: 0, high: 0 },
                capable: 0,
            };
            let mut pointer = &mut source as *mut Lease;
            let page = PointerPage {
                count: 1,
                items: &mut pointer,
            };
            let result = unsafe { copy_page(&page, 1, lease) }.unwrap();
            assert_eq!(result[0].state.as_deref(), Some("Active"));
            assert!(result[0].is_reservation);
            assert!(result[0].client_id.is_empty());
            assert_eq!(result[0].expiry_label.as_deref(), Some("Never"));
            assert!(result[0].expires_unix_seconds.is_none());
            assert!(unsafe {
                uid(&Binary {
                    length: 1,
                    data: ptr::null_mut(),
                })
            }
            .is_err());
        }
    }
}

/// Read leases or reservations once in a short-lived worker. Native buffers have
/// process lifetime; exit after serializing the owned result, as with scope details.
pub fn list_records_for_worker(
    server: &str,
    scope: &str,
    kind: &str,
) -> Result<Vec<ClientRecord>, String> {
    let server = crate::validate_server(server)?;
    let scope = validate_scope_id(scope)?;
    if !matches!(kind, "leases" | "reservations") {
        return Err("Unknown record kind.".into());
    }
    static USED: AtomicBool = AtomicBool::new(false);
    if USED.swap(true, Ordering::AcqRel) {
        return Err("Record queries require a fresh one-shot worker.".into());
    }
    #[cfg(windows)]
    {
        native::query(server, scope, kind)
    }
    #[cfg(not(windows))]
    {
        let _ = (server, scope);
        Err("Native DHCP queries require Windows.".into())
    }
}

/// Internal bridge for an isolated native write worker. Returned RPC storage
/// lives until that process exits; callers may read again for verification.
#[doc(hidden)]
pub fn records_in_write_worker(
    server: &str,
    scope: &str,
    kind: &str,
) -> Result<Vec<ClientRecord>, String> {
    let server = crate::validate_server(server)?;
    let id = validate_scope_id(scope)?;
    #[cfg(windows)]
    {
        native::query(server, id, kind)
    }
    #[cfg(not(windows))]
    {
        let _ = (server, id, kind);
        Err("Windows is required.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(address: u32) -> ClientRecord {
        ClientRecord {
            address: Ipv4Addr::from(address),
            name: String::new(),
            client_id: String::new(),
            description: String::new(),
            client_type: String::new(),
            state: None,
            expires_unix_seconds: None,
            expiry_label: None,
            is_reservation: false,
            options_present: None,
        }
    }
    #[test]
    fn validates_scope_without_allowing_all_subnets() {
        for scope in ["", "0.0.0.0", "255.255.255.255", "224.0.0.1", "not-an-ip"] {
            assert!(validate_scope_id(scope).is_err());
        }
        assert_eq!(
            validate_scope_id("192.0.2.0").unwrap(),
            Ipv4Addr::new(192, 0, 2, 0)
        );
    }
    #[test]
    fn handles_expiry_sentinels_and_filetime_epoch() {
        assert_eq!(expiry(0).1.as_deref(), Some("Not set"));
        assert_eq!(expiry(u64::MAX).1.as_deref(), Some("Never"));
        assert_eq!(expiry(0x7fff_ffff_ffff_ffff).1.as_deref(), Some("Never"));
        assert_eq!(expiry(116_444_736_000_000_000).0, Some(0));
        assert_eq!(expiry(116_444_736_010_000_000).0, Some(1));
        assert_eq!(lease_state(1), "Active");
        assert!(lease_state(0x81).contains("Deleted"));
        assert_eq!(lease_state(4), "Unknown (4)");
        assert_eq!(client_type(5), "DHCP");
        assert_eq!(client_type(100), "Manual");
    }
    #[test]
    fn collects_all_pages_and_propagates_later_failures() {
        let result = collect_pages(|resume| match resume {
            0 => Ok(Batch {
                rows: vec![row(20)],
                next: 20,
                more: true,
            }),
            20 => Ok(Batch {
                rows: vec![row(10)],
                next: 30,
                more: false,
            }),
            _ => unreachable!(),
        })
        .unwrap();
        assert_eq!(
            result
                .iter()
                .map(|r| u32::from(r.address))
                .collect::<Vec<_>>(),
            [10, 20]
        );
        assert!(collect_pages(|_| Ok(Batch {
            rows: vec![],
            next: 1,
            more: true
        }))
        .is_err());
        assert!(collect_pages(|_| Ok(Batch {
            rows: vec![row(1)],
            next: 0,
            more: true
        }))
        .is_err());
        assert!(collect_pages(|resume| if resume == 0 {
            Ok(Batch {
                rows: vec![row(1)],
                next: 1,
                more: true,
            })
        } else {
            Err("denied".into())
        })
        .is_err());
        assert!(collect_pages(|_| Ok(Batch {
            rows: vec![],
            next: 0,
            more: false
        }))
        .unwrap()
        .is_empty());
    }
}
