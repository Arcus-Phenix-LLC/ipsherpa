use std::{net::Ipv4Addr, process::ExitCode};

fn server_argument(args: impl IntoIterator<Item = String>) -> Result<Option<String>, String> {
    let args: Vec<_> = args.into_iter().collect();
    match args.as_slice() {
        [help] if help == "--help" || help == "-h" => Ok(None),
        [command, flag, server]
            if command == "scopes"
                && flag == "--server"
                && !server.trim().is_empty()
                && !server.contains('\0')
                && !server.starts_with('-') =>
        {
            Ok(Some(server.clone()))
        }
        _ => Err("Usage: ipsherpa scopes --server <hostname-or-IP>".into()),
    }
}

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

fn run() -> Result<(), String> {
    let Some(server) = server_argument(std::env::args().skip(1))? else {
        println!("Usage: ipsherpa scopes --server <hostname-or-IP>\nRead-only scope IDs; uses the current Windows identity.");
        return Ok(());
    };
    #[cfg(windows)]
    {
        let scopes = dhcp::scopes(&server)?;
        println!("DHCP scopes on {server}: {}", scopes.len());
        for scope in scopes {
            println!("{scope}");
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = server;
        Err("Native DHCP enumeration requires Windows.".into())
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<Option<String>, String> {
        server_argument(args.iter().map(|s| s.to_string()))
    }
    #[test]
    fn requires_explicit_target_and_known_command() {
        assert_eq!(
            parse(&["scopes", "--server", "DHCP01"]).unwrap(),
            Some("DHCP01".into())
        );
        for args in [
            vec![],
            vec!["scopes"],
            vec!["delete", "--server", "DHCP01"],
            vec!["scopes", "--server", ""],
            vec!["scopes", "--server", "a\0b"],
        ] {
            assert!(parse(&args).is_err());
        }
        assert_eq!(parse(&["--help"]).unwrap(), None);
    }
    #[test]
    fn converts_host_order_dhcp_addresses() {
        assert_eq!(scope_address(0xC0A80A00).to_string(), "192.168.10.0");
        assert_eq!(scope_address(0x0A000000).to_string(), "10.0.0.0");
    }
}
