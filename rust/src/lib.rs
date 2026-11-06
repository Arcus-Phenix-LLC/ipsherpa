use std::net::Ipv4Addr;

mod capacity;
mod details;
pub use capacity::{scope_capacity_for_worker, CapacitySnapshot, PoolTotals, ScopeStatistics};
mod records;
pub use details::{list_scope_details_for_worker, AddressRange, ScopeDetails};
pub use records::{list_records_for_worker, validate_scope_id, ClientRecord};

fn scope_address(value: u32) -> Ipv4Addr {
    // DHCP_IP_ADDRESS is a host-order DWORD, not a network-order byte buffer.
    Ipv4Addr::from(value)
}

#[cfg(windows)]
mod dhcp {
    use super::scope_address;
    use std::{ffi::c_void, net::Ipv4Addr, ptr};

    #[repr(C)]
    struct IpArray {
        count: u32,
        elements: *mut u32,
    }

    #[link(name = "dhcpsapi")]
    extern "system" {
        fn DhcpEnumSubnets(
            server: *const u16,
            resume: *mut u32,
            preferred: u32,
            info: *mut *mut IpArray,
            read: *mut u32,
            total: *mut u32,
        ) -> u32;
        fn DhcpRpcFreeMemory(buffer: *mut c_void);
    }

    struct Page(*mut IpArray);
    impl Drop for Page {
        fn drop(&mut self) {
            // SAFETY: these pointers belong to the DHCP RPC allocator. Each page
            // owns its elements and outer structure, and releases each once.
            unsafe {
                if !self.0.is_null() {
                    if !(*self.0).elements.is_null() {
                        DhcpRpcFreeMemory((*self.0).elements.cast());
                    }
                    DhcpRpcFreeMemory(self.0.cast());
                }
            }
        }
    }

    pub fn scopes(server: &str) -> Result<Vec<Ipv4Addr>, String> {
        let wide: Vec<u16> = server.encode_utf16().chain(Some(0)).collect();
        let mut resume = 0;
        let mut result = Vec::new();
        loop {
            let previous = resume;
            let mut raw = ptr::null_mut();
            let (mut read, mut total) = (0, 0);
            // SAFETY: input is NUL-terminated UTF-16; output pointers are valid
            // for this synchronous call. Page guards returned allocations.
            let status = unsafe {
                DhcpEnumSubnets(
                    wide.as_ptr(),
                    &mut resume,
                    65536,
                    &mut raw,
                    &mut read,
                    &mut total,
                )
            };
            let page = Page(raw);
            match status {
                259 => break, // ERROR_NO_MORE_ITEMS
                0 | 234 => {}, // ERROR_SUCCESS / ERROR_MORE_DATA
                code => return Err(format!(
                    "DhcpEnumSubnets on {server} failed: {} (code {code}). Check account permissions, server name, DHCP service and RPC/firewall access.",
                    std::io::Error::from_raw_os_error(code as i32))),
            }
            if raw.is_null() {
                if read != 0 {
                    return Err("DHCP returned a null page with a nonzero count".into());
                }
            } else {
                // SAFETY: successful API call supplies a DHCP_IP_ARRAY. Validate
                // counts and nullability before reading its allocated elements.
                unsafe {
                    let info = &*page.0;
                    if info.count != read || (read > 0 && info.elements.is_null()) {
                        return Err("DHCP returned inconsistent scope data".into());
                    }
                    if read > 0 {
                        result.extend(
                            std::slice::from_raw_parts(info.elements, read as usize)
                                .iter()
                                .copied()
                                .map(scope_address),
                        );
                    }
                }
            }
            if status == 0 {
                break;
            }
            if resume == previous || read == 0 {
                return Err("DHCP enumeration made no progress; refusing to loop".into());
            }
        }
        result.sort_unstable();
        result.dedup();
        Ok(result)
    }
}

/// Validate a target before passing it to the Windows UTF-16 API.
pub fn validate_server(server: &str) -> Result<&str, String> {
    let server = server.trim();
    if server.is_empty()
        || server.len() > 253
        || server.starts_with('-')
        || server.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(
            "Enter a DHCP server hostname or IP address (no spaces or control characters).".into(),
        );
    }
    Ok(server)
}

/// Read scope IDs using the current Windows identity. Never changes the server.
pub fn list_scopes(server: &str) -> Result<Vec<Ipv4Addr>, String> {
    let server = validate_server(server)?;
    #[cfg(windows)]
    {
        dhcp::scopes(server)
    }
    #[cfg(not(windows))]
    {
        let _ = server;
        Err("Native DHCP enumeration requires Windows.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn converts_host_order_dhcp_addresses() {
        assert_eq!(scope_address(0xC0A80A00).to_string(), "192.168.10.0");
        assert_eq!(scope_address(0x0A000000).to_string(), "10.0.0.0");
    }
    #[test]
    fn rejects_invalid_server_before_ffi() {
        for value in ["", "  ", "a\0b", "bad server", "a\nb", "--server"] {
            assert!(validate_server(value).is_err(), "{value:?}");
            assert!(list_scopes(value).is_err());
        }
        assert_eq!(validate_server(" DHCP01 ").unwrap(), "DHCP01");
        assert!(validate_server(&"a".repeat(254)).is_err());
        assert!(validate_server("dhcp01.example.test").is_ok());
        assert!(validate_server("192.168.10.1").is_ok());
    }
}
