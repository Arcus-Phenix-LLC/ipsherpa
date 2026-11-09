//! Native reservation workflow. Only invoked inside a disposable child process.
use crate::reservation::{Outcome, Request};
use ipsherpa::{AddressRange, ClientRecord};
use std::net::Ipv4Addr;
#[derive(Clone)]
struct Snapshot {
    mask: u32,
    ranges: Vec<AddressRange>,
    exclusions: Vec<AddressRange>,
    reservations: Vec<ClientRecord>,
    leases: Vec<ClientRecord>,
}
trait Backend {
    fn snapshot(&mut self, r: &Request) -> Result<Snapshot, String>;
    fn add(&mut self, r: &Request) -> Result<(), String>;
    fn metadata(&mut self, r: &Request) -> Result<(), String>;
}
fn kind(r: &Request) -> &str {
    r.original
        .as_ref()
        .map_or("DHCP", |o| o.client_type.as_str())
}
fn bytes(text: &str) -> Option<Vec<u8>> {
    if !text
        .chars()
        .all(|c| c.is_ascii_hexdigit() || "-:.".contains(c))
    {
        return None;
    }
    let s: String = text.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}
fn same_client(value: &str, r: &Request) -> bool {
    let Some(mut uid) = bytes(value) else {
        return false;
    };
    let Some(mac) = bytes(&r.mac) else {
        return false;
    };
    // MS-DHCPM 2.2.1.2.5.2: subnet DWORD little-endian + hardware type 1 + ID.
    // Never infer a MAC by blindly taking the tail of an arbitrary client ID.
    if uid.len() == 11 {
        let Ok(scope) = r.scope_id.parse::<Ipv4Addr>() else {
            return false;
        };
        if uid[..4] != u32::from(scope).to_le_bytes() || uid[4] != 1 {
            return false;
        }
        uid = uid[5..].to_vec();
    }
    uid == mac
}
fn ip(r: &Request) -> Ipv4Addr {
    r.ip_address.parse().expect("validated request")
}
fn target<'a>(s: &'a Snapshot, r: &Request) -> Result<&'a ClientRecord, String> {
    let mut found = s.reservations.iter().filter(|v| v.address == ip(r));
    let row = found
        .next()
        .ok_or("Reservation is missing. Refresh reservations and review again.")?;
    if found.next().is_some() {
        return Err("Ambiguous reservation identity.".into());
    }
    if !same_client(&row.client_id, r) || row.client_type != kind(r) {
        return Err("Reservation identity or type changed. Refresh reservations.".into());
    }
    Ok(row)
}
fn check(s: &Snapshot, r: &Request) -> Result<(), String> {
    let scope: u32 = u32::from(
        r.scope_id
            .parse::<Ipv4Addr>()
            .map_err(|_| "Invalid scope")?,
    );
    let addr = u32::from(ip(r));
    if s.mask == 0 || (addr & s.mask) != scope || addr == scope || addr == (scope | !s.mask) {
        return Err("Address is not a usable address in the selected subnet.".into());
    }
    if !s.ranges.iter().any(|v| v.start <= ip(r) && ip(r) <= v.end) {
        return Err("Address is outside the configured allocation ranges.".into());
    }
    if s.exclusions
        .iter()
        .any(|v| v.start <= ip(r) && ip(r) <= v.end)
    {
        return Err("Address is in an exclusion range.".into());
    }
    if let Some(original) = &r.original {
        let row = target(s, r)?;
        if row.name != original.name || row.description != original.description {
            return Err(
                "Reservation metadata changed. Refresh reservations and review again.".into(),
            );
        }
    }
    for row in &s.reservations {
        if row.address == ip(r) {
            if r.original.is_some() {
                continue;
            }
            return Err("A reservation already exists at this address.".into());
        }
        if same_client(&row.client_id, r) {
            return Err("This MAC already has a reservation in this scope.".into());
        }
    }
    for lease in &s.leases {
        let same = same_client(&lease.client_id, r);
        if lease.address == ip(r)
            && (!same
                || lease.state.as_deref().and_then(|v| v.split(" · ").next()) != Some("Active"))
        {
            return Err("The address has a conflicting or non-active lease record.".into());
        }
        if same && lease.address != ip(r) {
            return Err(
                "This MAC has a lease at another address; automatic moves are not supported."
                    .into(),
            );
        }
    }
    Ok(())
}
fn outcome(status: &str, message: impl Into<String>) -> Outcome {
    Outcome {
        status: status.into(),
        message: message.into(),
    }
}
fn execute(backend: &mut impl Backend, phase: &str, r: &Request) -> Outcome {
    let before = match backend.snapshot(r).and_then(|s| {
        check(&s, r)?;
        Ok(s)
    }) {
        Ok(s) => s,
        Err(e) => return outcome("blocked", e),
    };
    if phase == "preview" {
        return outcome(
            "ready",
            "Native DHCP range, exclusion, reservation and lease checks passed. No changes made.",
        );
    }
    if phase != "create" {
        return outcome("blocked", "Unknown native reservation operation.");
    }
    let write = (|| {
        if r.original.is_none() {
            backend.add(r)?;
        }
        backend.metadata(r)?;
        let after = backend.snapshot(r)?;
        let row = target(&after, r)?;
        if row.name != r.name || row.description != r.description {
            return Err("Metadata read-back did not match the requested values.".into());
        }
        if r.original.is_some() && target(&before, r)?.options_present != row.options_present {
            return Err("Reservation option presence changed during the update.".into());
        }
        Ok::<(), String>(())
    })();
    match write{Ok(())=>outcome(if r.original.is_some(){"updated"}else{"created"},"Reservation saved and verified. Refresh reservations to load current data."),Err(e)=>outcome("unknown",format!("The reservation may have changed or been partially created. Inspect DHCP Manager before trying again. {e}"))}
}
pub fn child_main(args: &[String]) -> i32 {
    use std::io::{Read, Write};
    #[cfg(windows)]
    unsafe {
        #[link(name = "kernel32")]
        extern "system" {
            fn SetErrorMode(mode: u32) -> u32;
        }
        SetErrorMode(3);
    }
    let result = (|| -> Result<Outcome, String> {
        if args.len() != 1 || !matches!(args[0].as_str(), "preview" | "create") {
            return Err("Invalid reservation worker arguments.".into());
        }
        let mut input = String::new();
        std::io::stdin()
            .take(16385)
            .read_to_string(&mut input)
            .map_err(|e| e.to_string())?;
        if input.len() > 16384 {
            return Err("Reservation request exceeds size limit.".into());
        }
        #[derive(serde::Deserialize)]
        struct Envelope {
            phase: String,
            request: Request,
        }
        let envelope: Envelope =
            serde_json::from_str(&input).map_err(|_| "Invalid reservation request.")?;
        if envelope.phase != args[0] {
            return Err("Reservation phase mismatch.".into());
        }
        let r = crate::reservation::validate(envelope.request)?;
        Ok(execute(&mut native::Native, &envelope.phase, &r))
    })();
    let reply = result.unwrap_or_else(|e| outcome("blocked", e));
    let mut output = std::io::stdout().lock();
    if serde_json::to_writer(&mut output, &reply).is_err() || output.flush().is_err() {
        return 1;
    }
    0
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::ptr;
    #[derive(Clone, Copy)]
    #[repr(C)]
    struct Binary {
        length: u32,
        data: *mut u8,
    }
    #[derive(Clone, Copy, Debug, PartialEq)]
    #[repr(C)]
    struct DateTime {
        low: u32,
        high: u32,
    }
    #[derive(Clone, Copy)]
    #[repr(C)]
    struct Host {
        address: u32,
        netbios: *mut u16,
        name: *mut u16,
    }
    #[derive(Clone, Copy)]
    #[repr(C)]
    struct Client {
        address: u32,
        mask: u32,
        uid: Binary,
        name: *mut u16,
        comment: *mut u16,
        expires: DateTime,
        owner: Host,
        kind: u8,
    }
    #[repr(C)]
    struct Subnet {
        address: u32,
        mask: u32,
        name: *mut u16,
        comment: *mut u16,
        host: Host,
        state: u32,
    }
    #[repr(C)]
    struct Reserved {
        address: u32,
        uid: *const Binary,
        kind: u8,
    }
    #[repr(C)]
    struct Element {
        kind: u32,
        reserved: *const Reserved,
    }
    #[repr(C)]
    union SearchData {
        address: u32,
        uid: Binary,
        name: *mut u16,
    }
    #[repr(C)]
    struct Search {
        kind: u32,
        data: SearchData,
    }
    #[link(name = "dhcpsapi")]
    extern "system" {
        fn DhcpGetSubnetInfo(server: *const u16, scope: u32, result: *mut *mut Subnet) -> u32;
        fn DhcpAddSubnetElementV4(server: *const u16, scope: u32, element: *const Element) -> u32;
        fn DhcpGetClientInfoV4(
            server: *const u16,
            search: *const Search,
            result: *mut *mut Client,
        ) -> u32;
        fn DhcpSetClientInfoV4(server: *const u16, client: *const Client) -> u32;
    }
    pub(super) struct Native;
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    fn status(api: &str, code: u32) -> Result<(), String> {
        if code == 0 {
            Ok(())
        } else {
            Err(format!(
                "{api}: {} (code {code})",
                std::io::Error::from_raw_os_error(code as i32)
            ))
        }
    }
    unsafe fn text(p: *const u16) -> Result<String, String> {
        if p.is_null() {
            return Ok(String::new());
        }
        for n in 0..32768 {
            if unsafe { *p.add(n) } == 0 {
                return String::from_utf16(unsafe { std::slice::from_raw_parts(p, n) })
                    .map_err(|_| "Invalid native text".into());
            }
        }
        Err("Native text too long".into())
    }
    unsafe fn client_id(c: &Client) -> Result<String, String> {
        if c.uid.length == 0 || c.uid.length > 65536 || c.uid.data.is_null() {
            return Err("Invalid native client ID".into());
        }
        Ok(
            unsafe { std::slice::from_raw_parts(c.uid.data, c.uid.length as usize) }
                .iter()
                .map(|v| format!("{v:02X}"))
                .collect::<Vec<_>>()
                .join("-"),
        )
    }
    fn metadata_copy(
        c: &Client,
        r: &Request,
        name: *mut u16,
        comment: *mut u16,
    ) -> Result<Client, String> {
        if c.address != u32::from(ip(r))
            || (c.address & c.mask)
                != u32::from(
                    r.scope_id
                        .parse::<Ipv4Addr>()
                        .map_err(|_| "Invalid scope")?,
                )
            || !same_client(&unsafe { client_id(c) }?, r)
        {
            return Err("Native client identity does not match the reviewed reservation.".into());
        }
        if let Some(original) = &r.original {
            if unsafe { text(c.name) }? != original.name
                || unsafe { text(c.comment) }? != original.description
            {
                return Err("Native client metadata changed after the scope checks.".into());
            }
        }
        let mut result = *c;
        result.name = name;
        result.comment = comment;
        Ok(result)
    }
    fn add_element(
        r: &Request,
        call: impl FnOnce(&Element) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut mac = bytes(&r.mac).ok_or("Invalid MAC")?;
        let uid = Binary {
            length: mac.len() as u32,
            data: mac.as_mut_ptr(),
        };
        let reserved = Reserved {
            address: u32::from(ip(r)),
            uid: &uid,
            kind: 1,
        };
        let element = Element {
            kind: 2,
            reserved: &reserved,
        };
        call(&element)
    }
    impl Backend for Native {
        fn snapshot(&mut self, r: &Request) -> Result<Snapshot, String> {
            let server = wide(&r.server);
            let mut raw = ptr::null_mut();
            status("DhcpGetSubnetInfo", unsafe {
                DhcpGetSubnetInfo(
                    server.as_ptr(),
                    u32::from(
                        r.scope_id
                            .parse::<Ipv4Addr>()
                            .map_err(|_| "Invalid scope")?,
                    ),
                    &mut raw,
                )
            })?;
            let subnet = unsafe { raw.as_ref() }.ok_or("Null subnet result")?;
            if subnet.address
                != u32::from(
                    r.scope_id
                        .parse::<Ipv4Addr>()
                        .map_err(|_| "Invalid scope")?,
                )
            {
                return Err("Unexpected native scope".into());
            }
            Ok(Snapshot {
                mask: subnet.mask,
                ranges: ipsherpa::ranges_in_write_worker(&r.server, &r.scope_id, false)?,
                exclusions: ipsherpa::ranges_in_write_worker(&r.server, &r.scope_id, true)?,
                reservations: ipsherpa::records_in_write_worker(
                    &r.server,
                    &r.scope_id,
                    "reservations",
                )?,
                leases: ipsherpa::records_in_write_worker(&r.server, &r.scope_id, "leases")?,
            })
        }
        fn add(&mut self, r: &Request) -> Result<(), String> {
            let server = wide(&r.server);
            add_element(r, |element| {
                status("DhcpAddSubnetElementV4", unsafe {
                    DhcpAddSubnetElementV4(
                        server.as_ptr(),
                        u32::from(
                            r.scope_id
                                .parse::<Ipv4Addr>()
                                .map_err(|_| "Invalid scope")?,
                        ),
                        element,
                    )
                })
            })
        }
        fn metadata(&mut self, r: &Request) -> Result<(), String> {
            let server = wide(&r.server);
            let search = Search {
                kind: 0,
                data: SearchData {
                    address: u32::from(ip(r)),
                },
            };
            let mut raw = ptr::null_mut();
            status("DhcpGetClientInfoV4", unsafe {
                DhcpGetClientInfoV4(server.as_ptr(), &search, &mut raw)
            })?;
            let client = unsafe { raw.as_ref() }.ok_or("Null native client record")?;
            let mut name = wide(&r.name);
            let mut comment = wide(&r.description);
            let update = metadata_copy(client, r, name.as_mut_ptr(), comment.as_mut_ptr())?;
            // Preserve UID, address, mask, expiry, owner and client type verbatim. Native
            // output buffers have process lifetime; never free nested RPC allocations.
            status("DhcpSetClientInfoV4", unsafe {
                DhcpSetClientInfoV4(server.as_ptr(), &update)
            })
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use std::mem::{offset_of, size_of};
        #[test]
        fn sdk_layouts() {
            assert_eq!(size_of::<Client>(), 80);
            assert_eq!(offset_of!(Client, kind), 72);
            assert_eq!(size_of::<Search>(), 24);
            assert_eq!(offset_of!(Search, data), 8);
            assert_eq!(size_of::<Reserved>(), 24);
            assert_eq!(offset_of!(Reserved, kind), 16);
            assert_eq!(size_of::<Element>(), 16);
            assert_eq!(size_of::<Subnet>(), 56);
        }
        #[test]
        fn reservation_input_has_owned_mac_and_dhcp_type() {
            let r = super::super::tests::request();
            add_element(&r, |element| {
                assert_eq!(element.kind, 2);
                let reserved = unsafe { &*element.reserved };
                assert_eq!(reserved.address, u32::from(ip(&r)));
                assert_eq!(reserved.kind, 1);
                let uid = unsafe { &*reserved.uid };
                assert_eq!(uid.length, 6);
                assert_eq!(
                    unsafe { std::slice::from_raw_parts(uid.data, 6) },
                    [2, 170, 187, 204, 221, 238]
                );
                Ok(())
            })
            .unwrap();
        }
        #[test]
        fn changes_only_metadata_and_preserves_native_storage() {
            let r = super::super::tests::request();
            let mut uid = bytes(&r.mac).unwrap();
            let mut name = wide("Old");
            let mut desc = wide("Before");
            let c = Client {
                address: u32::from(ip(&r)),
                mask: 0xffffff00,
                uid: Binary {
                    length: 6,
                    data: uid.as_mut_ptr(),
                },
                name: name.as_mut_ptr(),
                comment: desc.as_mut_ptr(),
                expires: DateTime {
                    low: 123,
                    high: 456,
                },
                owner: Host {
                    address: 44,
                    netbios: ptr::null_mut(),
                    name: ptr::null_mut(),
                },
                kind: 100,
            };
            let mut after = wide("New");
            let updated = metadata_copy(&c, &r, after.as_mut_ptr(), ptr::null_mut()).unwrap();
            assert_eq!(updated.uid.data, c.uid.data);
            assert_eq!(updated.expires, c.expires);
            assert_eq!(updated.owner.address, 44);
            assert_eq!(updated.kind, 100);
            assert_eq!(unsafe { text(c.name) }.unwrap(), "Old");
            assert_eq!(unsafe { text(updated.name) }.unwrap(), "New");
            let mut bad = r;
            bad.mac = "02-00-00-00-00-01".into();
            assert!(metadata_copy(&c, &bad, ptr::null_mut(), ptr::null_mut()).is_err());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn request() -> Request {
        Request {
            server: "EXAMPLE-DHCP".into(),
            scope_id: "192.0.2.0".into(),
            ip_address: "192.0.2.25".into(),
            mac: "02-AA-BB-CC-DD-EE".into(),
            name: "After 東京".into(),
            description: "New".into(),
            original: None,
        }
    }
    fn row() -> ClientRecord {
        ClientRecord {
            address: "192.0.2.25".parse().unwrap(),
            name: "Before".into(),
            description: "Previous".into(),
            client_id: "02-AA-BB-CC-DD-EE".into(),
            client_type: "DHCP / BOOTP".into(),
            state: None,
            expires_unix_seconds: None,
            expiry_label: None,
            is_reservation: true,
            options_present: Some(true),
        }
    }
    fn snapshot() -> Snapshot {
        Snapshot {
            mask: 0xffffff00,
            ranges: vec![AddressRange {
                start: "192.0.2.10".parse().unwrap(),
                end: "192.0.2.200".parse().unwrap(),
            }],
            exclusions: vec![],
            reservations: vec![],
            leases: vec![],
        }
    }
    struct Mock {
        data: Snapshot,
        events: Vec<&'static str>,
        fail: Option<&'static str>,
        mismatch: bool,
    }
    impl Backend for Mock {
        fn snapshot(&mut self, _: &Request) -> Result<Snapshot, String> {
            self.events.push("read");
            if self.fail == Some("read")
                || (self.fail == Some("verify") && self.events.contains(&"metadata"))
            {
                return Err("Synthetic read failure".into());
            }
            Ok(self.data.clone())
        }
        fn add(&mut self, r: &Request) -> Result<(), String> {
            self.events.push("add");
            if self.fail == Some("add") {
                return Err("Synthetic add failure".into());
            }
            let mut row = row();
            row.client_type = "DHCP".into();
            row.name = String::new();
            row.description = String::new();
            row.client_id = r.mac.clone();
            self.data.reservations.push(row);
            Ok(())
        }
        fn metadata(&mut self, r: &Request) -> Result<(), String> {
            self.events.push("metadata");
            if self.fail == Some("metadata") {
                return Err("Synthetic metadata failure".into());
            }
            let row = &mut self.data.reservations[0];
            row.name = if self.mismatch {
                "Mismatch".into()
            } else {
                r.name.clone()
            };
            row.description = r.description.clone();
            Ok(())
        }
    }
    fn mock() -> Mock {
        Mock {
            data: snapshot(),
            events: vec![],
            fail: None,
            mismatch: false,
        }
    }
    #[test]
    fn preview_never_writes_and_create_verifies() {
        let r = request();
        let mut b = mock();
        assert_eq!(execute(&mut b, "preview", &r).status, "ready");
        assert_eq!(b.events, ["read"]);
        b.events.clear();
        assert_eq!(execute(&mut b, "create", &r).status, "created");
        assert_eq!(b.events, ["read", "add", "metadata", "read"]);
    }
    #[test]
    fn edits_preserve_all_supported_types() {
        for kind in ["DHCP", "BOOTP", "DHCP / BOOTP"] {
            let mut b = mock();
            let mut row = row();
            row.client_type = kind.into();
            let mut r = request();
            r.original = Some(crate::reservation::Original {
                name: row.name.clone(),
                description: row.description.clone(),
                client_type: kind.into(),
            });
            b.data.reservations.push(row);
            assert_eq!(execute(&mut b, "create", &r).status, "updated");
            assert_eq!(b.events, ["read", "metadata", "read"]);
            assert_eq!(b.data.reservations[0].client_type, kind);
        }
    }
    #[test]
    fn blocks_stale_missing_identity_and_type_changes() {
        let mut r = request();
        r.original = Some(crate::reservation::Original {
            name: "Before".into(),
            description: "Previous".into(),
            client_type: "DHCP / BOOTP".into(),
        });
        for mode in ["missing", "name", "description", "identity", "type"] {
            let mut b = mock();
            if mode != "missing" {
                let mut row = row();
                match mode {
                    "name" => row.name = "External".into(),
                    "description" => row.description = "External".into(),
                    "identity" => row.client_id = "02-00-00-00-00-01".into(),
                    "type" => row.client_type = "DHCP".into(),
                    _ => (),
                };
                b.data.reservations.push(row);
            }
            assert_eq!(execute(&mut b, "create", &r).status, "blocked");
            assert_eq!(b.events, ["read"]);
        }
    }
    #[test]
    fn blocks_allocation_exclusion_existing_and_lease_conflicts() {
        for mode in [
            "range",
            "excluded",
            "existing",
            "duplicate",
            "lease",
            "move",
            "declined",
            "read",
        ] {
            let r = request();
            let mut b = mock();
            match mode {
                "range" => b.data.ranges.clear(),
                "excluded" => b.data.exclusions = b.data.ranges.clone(),
                "existing" => b.data.reservations.push(row()),
                "duplicate" => {
                    let mut row = row();
                    row.address = "192.0.2.99".parse().unwrap();
                    b.data.reservations.push(row)
                }
                "read" => b.fail = Some("read"),
                _ => {
                    let mut row = row();
                    row.state = Some("Active".into());
                    if mode == "lease" {
                        row.client_id = "02-00-00-00-00-01".into();
                    }
                    if mode == "move" {
                        row.address = "192.0.2.99".parse().unwrap();
                    }
                    if mode == "declined" {
                        row.state = Some("Declined".into());
                    }
                    b.data.leases.push(row);
                }
            }
            assert_eq!(execute(&mut b, "create", &r).status, "blocked", "{mode}");
            assert_eq!(b.events, ["read"]);
        }
    }
    #[test]
    fn reports_partial_failures_without_retry_or_rollback() {
        for stage in ["add", "metadata", "verify"] {
            let mut b = mock();
            b.fail = Some(stage);
            let result = execute(&mut b, "create", &request());
            assert_eq!(result.status, "unknown");
            assert_eq!(b.events.iter().filter(|v| **v == "add").count(), 1);
            assert!(b.events.iter().filter(|v| **v == "metadata").count() <= 1);
        }
        let mut b = mock();
        b.mismatch = true;
        assert_eq!(execute(&mut b, "create", &request()).status, "unknown");
    }
    #[test]
    fn validates_internal_uid_prefix_and_full_ids() {
        let r = request();
        assert!(same_client("00-02-00-C0-01-02-AA-BB-CC-DD-EE", &r));
        assert!(!same_client("00-03-00-C0-01-02-AA-BB-CC-DD-EE", &r));
        assert!(!same_client("01-02-AA-BB-CC-DD-EE", &r));
        assert!(!same_client("XX", &r));
        let mut b = mock();
        let mut lease = row();
        lease.client_id = "00-02-00-C0-01-02-AA-BB-CC-DD-EE".into();
        lease.state = Some("Active · DNS unregistered".into());
        b.data.leases.push(lease);
        assert_eq!(execute(&mut b, "preview", &r).status, "ready");
    }
}
