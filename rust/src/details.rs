use std::net::Ipv4Addr;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AddressRange {
    pub start: Ipv4Addr,
    pub end: Ipv4Addr,
}

#[derive(Debug, Clone)]
pub struct ScopeDetails {
    pub scope_id: Ipv4Addr,
    pub name: Option<String>,
    pub state: Option<String>,
    /// None means the query failed; an empty list means no ranges were configured.
    pub ranges: Option<Vec<AddressRange>>,
    pub warnings: Vec<String>,
}

fn state_label(state: u32) -> String {
    match state {
        0 => "Active".into(),
        1 => "Inactive".into(),
        2 => "Active (switched)".into(),
        3 => "Inactive (switched)".into(),
        4 => "Invalid".into(),
        value => format!("Unknown ({value})"),
    }
}

fn assemble(
    scope_id: Ipv4Addr,
    info: Result<(String, u32), String>,
    ranges: Result<Vec<AddressRange>, String>,
) -> ScopeDetails {
    let mut warnings = Vec::new();
    let (name, state) = match info {
        Ok((name, state)) => (Some(name), Some(state_label(state))),
        Err(error) => {
            warnings.push(error);
            (None, None)
        }
    };
    let ranges = match ranges {
        Ok(mut ranges) => {
            ranges.sort_unstable();
            ranges.dedup();
            Some(ranges)
        }
        Err(error) => {
            warnings.push(error);
            None
        }
    };
    ScopeDetails {
        scope_id,
        name,
        state,
        ranges,
        warnings,
    }
}

#[cfg(windows)]
mod native {
    use super::{assemble, AddressRange, ScopeDetails};
    use crate::scope_address;
    use std::{net::Ipv4Addr, ptr};

    #[repr(C)]
    struct HostInfo {
        address: u32,
        netbios_name: *mut u16,
        host_name: *mut u16,
    }
    #[repr(C)]
    struct SubnetInfo {
        address: u32,
        mask: u32,
        name: *mut u16,
        comment: *mut u16,
        host: HostInfo,
        state: u32,
    }
    #[repr(C)]
    struct IpRange {
        start: u32,
        end: u32,
    }
    // All arms of DHCP_SUBNET_ELEMENT_UNION are pointers. Only IP range
    // elements are requested or dereferenced by this module.
    #[repr(C)]
    struct Element {
        kind: u32,
        data: *mut IpRange,
    }
    #[repr(C)]
    struct ElementArray {
        count: u32,
        elements: *mut Element,
    }

    #[link(name = "dhcpsapi")]
    extern "system" {
        fn DhcpGetSubnetInfo(server: *const u16, subnet: u32, info: *mut *mut SubnetInfo) -> u32;
        fn DhcpEnumSubnetElements(
            server: *const u16,
            subnet: u32,
            kind: u32,
            resume: *mut u32,
            preferred: u32,
            info: *mut *mut ElementArray,
            read: *mut u32,
            total: *mut u32,
        ) -> u32;
    }

    // These RPC buffers are owned by the one-shot query process. Nested pointer
    // allocation boundaries are not inferred from their presence in a struct.
    // No native detail buffer is individually freed; Windows reclaims the entire
    // request arena at worker exit, after the owned Rust result is serialized.

    fn api_error(api: &str, id: Ipv4Addr, code: u32) -> String {
        format!(
            "{api} for {id} failed: {} (code {code})",
            std::io::Error::from_raw_os_error(code as i32)
        )
    }

    unsafe fn copy_name(value: *const u16) -> Result<String, String> {
        if value.is_null() {
            return Ok(String::new());
        }
        // SAFETY: a successful DHCP response guarantees a NUL-terminated
        // Unicode string. Stop at its terminator and copy into Rust-owned storage.
        for length in 0..32768 {
            if unsafe { *value.add(length) } == 0 {
                let units = unsafe { std::slice::from_raw_parts(value, length) };
                return String::from_utf16(units).map_err(|_| "Invalid UTF-16 scope name".into());
            }
        }
        Err("Scope name exceeded the supported Unicode string length".into())
    }

    fn info(server: &[u16], id: Ipv4Addr) -> Result<(String, u32), String> {
        let mut raw = ptr::null_mut();
        eprintln!("DhcpGetSubnetInfo {id}: call begin");
        // SAFETY: server is terminated UTF-16 and raw is a valid out pointer.
        let code = unsafe { DhcpGetSubnetInfo(server.as_ptr(), u32::from(id), &mut raw) };
        eprintln!("DhcpGetSubnetInfo {id}: returned {code}");
        if code != 0 {
            return Err(api_error("DhcpGetSubnetInfo", id, code));
        }
        // SAFETY: a successful call supplies the documented C structure.
        unsafe { copy_info(raw, id) }
    }

    unsafe fn copy_info(raw: *const SubnetInfo, id: Ipv4Addr) -> Result<(String, u32), String> {
        // SAFETY: caller supplies a successful response or a valid test fixture;
        // all pointed-to storage remains alive for this copy. Nothing is freed.
        let info = unsafe { raw.as_ref() }.ok_or("DHCP returned null scope information")?;
        if info.address != u32::from(id) {
            return Err("DHCP returned a different scope ID".into());
        }
        let name = unsafe { copy_name(info.name) }?;
        eprintln!("DhcpGetSubnetInfo {id}: fields copied");
        Ok((name, info.state))
    }

    fn ranges(server: &[u16], id: Ipv4Addr, kind: u32) -> Result<Vec<AddressRange>, String> {
        let mut resume = 0;
        let mut seen = std::collections::HashSet::from([0]);
        let mut result = Vec::new();
        loop {
            let previous = resume;
            let mut raw = ptr::null_mut();
            let (mut read, mut total) = (0, 0);
            eprintln!("DhcpEnumSubnetElements {id}: call begin");
            // SAFETY: initialized output parameters; kind is 0 for allocation
            // ranges or 3 for exclusions, both represented by DHCP_IP_RANGE.
            let code = unsafe {
                DhcpEnumSubnetElements(
                    server.as_ptr(),
                    u32::from(id),
                    kind,
                    &mut resume,
                    65536,
                    &mut raw,
                    &mut read,
                    &mut total,
                )
            };
            eprintln!("DhcpEnumSubnetElements {id}: returned {code}, read={read}");
            match code {
                259 => break,
                0 | 234 => {}
                code => return Err(api_error("DhcpEnumSubnetElements", id, code)),
            }
            result.extend(unsafe { copy_ranges(raw, read, kind, id) }?);
            if code == 0 {
                break;
            }
            if resume == previous || read == 0 || !seen.insert(resume) {
                return Err(format!("Range enumeration made no progress for {id}"));
            }
        }
        Ok(result)
    }

    unsafe fn copy_ranges(
        raw: *const ElementArray,
        read: u32,
        kind: u32,
        id: Ipv4Addr,
    ) -> Result<Vec<AddressRange>, String> {
        let mut result = Vec::new();
        // SAFETY: successful SDK buffers or valid fixtures, retained for the
        // entire copy. Validate the array and union tag before dereferencing.
        unsafe {
            match raw.as_ref() {
                None if read == 0 => {}
                Some(page) if page.count == read && (read == 0 || !page.elements.is_null()) => {
                    for index in 0..read as usize {
                        let element = &*page.elements.add(index);
                        let valid = if kind == 3 {
                            element.kind == 3
                        } else {
                            matches!(element.kind, 0 | 5..=7)
                        };
                        if !valid || element.data.is_null() {
                            return Err(format!("DHCP returned an invalid range element for {id}"));
                        }
                        let range = &*element.data;
                        if range.start > range.end {
                            return Err(format!("DHCP returned a reversed range for {id}"));
                        }
                        result.push(AddressRange {
                            start: scope_address(range.start),
                            end: scope_address(range.end),
                        });
                    }
                }
                _ => return Err(format!("DHCP returned inconsistent range data for {id}")),
            }
        }
        Ok(result)
    }

    pub(super) fn query_ranges(
        server: &str,
        id: Ipv4Addr,
        excluded: bool,
    ) -> Result<Vec<AddressRange>, String> {
        let wide: Vec<u16> = server.encode_utf16().chain(Some(0)).collect();
        ranges(&wide, id, if excluded { 3 } else { 0 })
    }

    pub fn query(server: &str, ids: Vec<Ipv4Addr>) -> Vec<ScopeDetails> {
        let server: Vec<u16> = server.encode_utf16().chain(Some(0)).collect();
        ids.into_iter()
            .map(|id| assemble(id, info(&server, id), ranges(&server, id, 0)))
            .collect()
    }

    #[cfg(all(test, target_pointer_width = "64"))]
    mod tests {
        use super::*;
        use std::mem::{offset_of, size_of};
        #[test]
        fn layouts_match_windows_x64_sdk() {
            assert_eq!(size_of::<HostInfo>(), 24);
            assert_eq!(size_of::<SubnetInfo>(), 56);
            assert_eq!(offset_of!(SubnetInfo, state), 48);
            assert_eq!(offset_of!(SubnetInfo, host), 24);
            assert_eq!(size_of::<Element>(), 16);
            assert_eq!(offset_of!(Element, data), 8);
            assert_eq!(size_of::<ElementArray>(), 16);
            assert_eq!(size_of::<IpRange>(), 8);
        }
        #[test]
        fn copies_unicode_and_empty_names() {
            let name: Vec<u16> = "Büro 東京".encode_utf16().chain(Some(0)).collect();
            assert_eq!(unsafe { copy_name(name.as_ptr()) }.unwrap(), "Büro 東京");
            assert_eq!(unsafe { copy_name(ptr::null()) }.unwrap(), "");
        }
        #[test]
        fn copies_exclusions_with_correct_union_tag_and_bounds() {
            let id = Ipv4Addr::new(192, 0, 2, 0);
            let mut range = IpRange {
                start: 0xC000020A,
                end: 0xC0000213,
            };
            let mut element = Element {
                kind: 3,
                data: &mut range,
            };
            let page = ElementArray {
                count: 1,
                elements: &mut element,
            };
            let result = unsafe { copy_ranges(&page, 1, 3, id) }.unwrap();
            assert_eq!(result[0].start, Ipv4Addr::new(192, 0, 2, 10));
            assert!(unsafe { copy_ranges(&page, 1, 0, id) }.is_err());
            assert!(unsafe { copy_ranges(&page, 2, 3, id) }.is_err());
            range.end = range.start - 1;
            assert!(range.end < range.start);
            assert!(unsafe { copy_ranges(&page, 1, 3, id) }.is_err());
            assert!(unsafe { copy_ranges(std::ptr::null(), 0, 3, id) }
                .unwrap()
                .is_empty());
        }
        #[test]
        fn copies_snapshot_without_owning_nested_storage() {
            // Synthetic graph: several pointers share storage, including an
            // interior pointer. Only the Rust fixture owns that storage.
            let mut text: Vec<u16> = "Example\0shared\0".encode_utf16().collect();
            let interior = unsafe { text.as_mut_ptr().add(8) };
            let raw = Box::new(SubnetInfo {
                address: 0xC0000200,
                mask: 0xFFFFFF00,
                name: text.as_mut_ptr(),
                comment: interior,
                host: HostInfo {
                    address: 0,
                    netbios_name: interior,
                    host_name: interior,
                },
                state: 0,
            });
            let result = unsafe { copy_info(&*raw, Ipv4Addr::new(192, 0, 2, 0)) }.unwrap();
            drop(raw);
            drop(text);
            assert_eq!(result, ("Example".into(), 0));
        }
    }
}

#[cfg(windows)]
pub(crate) fn read_ranges_in_worker(
    server: &str,
    id: Ipv4Addr,
    excluded: bool,
) -> Result<Vec<AddressRange>, String> {
    native::query_ranges(server, id, excluded)
}

fn claim_worker_query(used: &std::sync::atomic::AtomicBool) -> Result<(), String> {
    used.compare_exchange(
        false,
        true,
        std::sync::atomic::Ordering::AcqRel,
        std::sync::atomic::Ordering::Acquire,
    )
    .map(|_| ())
    .map_err(|_| "Scope details require a fresh one-shot worker process.".into())
}

/// Read one scope-detail snapshot in a dedicated, short-lived worker process.
///
/// Native detail buffers deliberately have process lifetime: nested pointers are
/// never treated as independent allocations. Exit the worker after serializing
/// the owned Rust result so Windows reclaims the request arena. Only one query
/// is permitted per process; the GUI starts a fresh child for every query.
pub fn list_scope_details_for_worker(server: &str) -> Result<Vec<ScopeDetails>, String> {
    let server = crate::validate_server(server)?;
    static USED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    claim_worker_query(&USED)?;
    let ids = crate::list_scopes(server)?;
    #[cfg(windows)]
    {
        Ok(native::query(server, ids))
    }
    #[cfg(not(windows))]
    {
        let _ = ids;
        Err("Native DHCP enumeration requires Windows.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_snapshot_is_single_use() {
        let used = std::sync::atomic::AtomicBool::new(false);
        assert!(claim_worker_query(&used).is_ok());
        assert!(claim_worker_query(&used).is_err());
    }
    #[test]
    fn preserves_partial_failures_and_distinguishes_empty_ranges() {
        let id = Ipv4Addr::new(10, 0, 0, 0);
        let scope = assemble(id, Err("access denied".into()), Ok(vec![]));
        assert_eq!(scope.scope_id, id);
        assert!(scope.name.is_none());
        assert!(scope.state.is_none());
        assert_eq!(scope.ranges, Some(vec![]));
        assert_eq!(scope.warnings, ["access denied"]);
        let scope = assemble(
            id,
            Ok(("Office".into(), 1)),
            Err("range query failed".into()),
        );
        assert_eq!(scope.state.as_deref(), Some("Inactive"));
        assert!(scope.ranges.is_none());
        assert_eq!(scope.warnings, ["range query failed"]);
    }
    #[test]
    fn preserves_multiple_disjoint_ranges_and_unknown_states() {
        let first = AddressRange {
            start: Ipv4Addr::new(10, 0, 0, 10),
            end: Ipv4Addr::new(10, 0, 0, 20),
        };
        let second = AddressRange {
            start: Ipv4Addr::new(10, 0, 0, 50),
            end: Ipv4Addr::new(10, 0, 0, 60),
        };
        let scope = assemble(
            Ipv4Addr::new(10, 0, 0, 0),
            Ok(("".into(), 99)),
            Ok(vec![second.clone(), first.clone(), first.clone()]),
        );
        assert_eq!(scope.ranges.unwrap(), [first, second]);
        assert_eq!(scope.state.as_deref(), Some("Unknown (99)"));
        for (value, label) in [
            (0, "Active"),
            (1, "Inactive"),
            (2, "Active (switched)"),
            (3, "Inactive (switched)"),
            (4, "Invalid"),
        ] {
            assert_eq!(state_label(value), label);
        }
    }
}
