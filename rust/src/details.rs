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
    use std::{ffi::c_void, net::Ipv4Addr, ptr};

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
        fn DhcpRpcFreeMemory(buffer: *mut c_void);
    }

    unsafe fn free<T>(value: *mut T) {
        if !value.is_null() {
            // SAFETY: callers pass a DHCP RPC allocation, each freed once.
            unsafe {
                DhcpRpcFreeMemory(value.cast());
            }
        }
    }

    struct InfoGuard(*mut SubnetInfo);
    impl Drop for InfoGuard {
        fn drop(&mut self) {
            // SAFETY: the API supplies this structure and its independently
            // allocated strings; the guard is its sole owner.
            unsafe {
                if let Some(info) = self.0.as_ref() {
                    eprintln!("DhcpGetSubnetInfo cleanup: name");
                    free(info.name);
                    eprintln!("DhcpGetSubnetInfo cleanup: comment");
                    free(info.comment);
                    eprintln!("DhcpGetSubnetInfo cleanup: NetBIOS name");
                    free(info.host.netbios_name);
                    eprintln!("DhcpGetSubnetInfo cleanup: hostname");
                    free(info.host.host_name);
                    eprintln!("DhcpGetSubnetInfo cleanup: outer structure");
                    free(self.0);
                    eprintln!("DhcpGetSubnetInfo cleanup: complete");
                }
            }
        }
    }

    struct RangeGuard(*mut ElementArray);
    impl Drop for RangeGuard {
        fn drop(&mut self) {
            // SAFETY: this is a DHCP-allocated array of pointer-valued union
            // arms. Range arms have no further allocations. The API is only
            // called with DhcpIpRanges, never reservations or host records.
            unsafe {
                if let Some(page) = self.0.as_ref() {
                    eprintln!("DhcpEnumSubnetElements cleanup: {} elements", page.count);
                    if !page.elements.is_null() {
                        for index in 0..page.count as usize {
                            eprintln!("DhcpEnumSubnetElements cleanup: range {index}");
                            free((*page.elements.add(index)).data);
                        }
                        eprintln!("DhcpEnumSubnetElements cleanup: element array");
                        free(page.elements);
                    }
                    eprintln!("DhcpEnumSubnetElements cleanup: outer structure");
                    free(self.0);
                    eprintln!("DhcpEnumSubnetElements cleanup: complete");
                }
            }
        }
    }

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
        // Unicode string. Stop at its terminator, copying before freeing it.
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
        let guard = InfoGuard(raw);
        if code != 0 {
            return Err(api_error("DhcpGetSubnetInfo", id, code));
        }
        // SAFETY: a successful call supplies the documented C structure.
        let info = unsafe { guard.0.as_ref() }.ok_or("DHCP returned null scope information")?;
        if info.address != u32::from(id) {
            return Err("DHCP returned a different scope ID".into());
        }
        let name = unsafe { copy_name(info.name) }?;
        eprintln!("DhcpGetSubnetInfo {id}: fields copied");
        Ok((name, info.state))
    }

    fn ranges(server: &[u16], id: Ipv4Addr) -> Result<Vec<AddressRange>, String> {
        let mut resume = 0;
        let mut result = Vec::new();
        loop {
            let previous = resume;
            let mut raw = ptr::null_mut();
            let (mut read, mut total) = (0, 0);
            eprintln!("DhcpEnumSubnetElements {id}: call begin");
            // SAFETY: all out parameters are initialized. kind=0 requests
            // DhcpIpRanges, not exclusions, leases or inferred subnet bounds.
            let code = unsafe {
                DhcpEnumSubnetElements(
                    server.as_ptr(),
                    u32::from(id),
                    0,
                    &mut resume,
                    65536,
                    &mut raw,
                    &mut read,
                    &mut total,
                )
            };
            eprintln!("DhcpEnumSubnetElements {id}: returned {code}, read={read}");
            let page = RangeGuard(raw);
            match code {
                259 => break,
                0 | 234 => {}
                code => return Err(api_error("DhcpEnumSubnetElements", id, code)),
            }
            // SAFETY: the successful API response owns count valid elements.
            // Check nullability/counts and the union tag before dereferencing.
            unsafe {
                match page.0.as_ref() {
                    None if read == 0 => {}
                    Some(page) if page.count == read && (read == 0 || !page.elements.is_null()) => {
                        for index in 0..read as usize {
                            let element = &*page.elements.add(index);
                            if !matches!(element.kind, 0 | 5..=7) || element.data.is_null() {
                                return Err(format!(
                                    "DHCP returned an invalid range element for {id}"
                                ));
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
            if code == 0 {
                break;
            }
            if resume == previous || read == 0 {
                return Err(format!("Range enumeration made no progress for {id}"));
            }
        }
        Ok(result)
    }

    pub fn query(server: &str, ids: Vec<Ipv4Addr>) -> Vec<ScopeDetails> {
        let server: Vec<u16> = server.encode_utf16().chain(Some(0)).collect();
        ids.into_iter()
            .map(|id| assemble(id, info(&server, id), ranges(&server, id)))
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
    }
}

/// List every scope, preserving per-scope detail errors instead of dropping rows.
pub fn list_scope_details(server: &str) -> Result<Vec<ScopeDetails>, String> {
    let server = crate::validate_server(server)?;
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
