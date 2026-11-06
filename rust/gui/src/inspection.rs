use serde::{Deserialize, Serialize};
use std::{collections::HashSet, net::Ipv4Addr, ptr};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub server: String,
    pub scope_id: Option<String>,
    pub rows: Vec<Entry>,
    pub warnings: Vec<String>,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct Entry {
    pub name: String,
    pub kind: String,
    pub values: Vec<String>,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Binary {
    length: u32,
    data: *const u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
union Value {
    byte: u8,
    word: u16,
    dword: u32,
    pair: [u32; 2],
    text: *const u16,
    binary: Binary,
}
#[repr(C)]
struct Element {
    kind: u32,
    value: Value,
}
#[repr(C)]
struct Data {
    count: u32,
    elements: *const Element,
}
#[repr(C)]
struct OptionValue {
    id: u32,
    data: Data,
}
#[repr(C)]
struct Options {
    count: u32,
    values: *const OptionValue,
}
#[repr(C)]
struct Scope {
    kind: u32,
    subnet: usize,
}
#[repr(C)]
struct Mib {
    counters: [u32; 7],
    start: [u32; 2],
    quarantine: [u32; 7],
    delayed: u32,
    delayed_scopes: u32,
    count: u32,
    scopes: *const ScopeMib,
}
#[repr(C)]
struct ScopeMib {
    subnet: u32,
    used: u32,
    free: u32,
    pending: u32,
}
#[link(name = "dhcpsapi")]
extern "system" {
    fn DhcpEnumOptionValues(
        server: *const u16,
        scope: *const Scope,
        resume: *mut u32,
        max: u32,
        result: *mut *mut Options,
        read: *mut u32,
        total: *mut u32,
    ) -> u32;
    fn DhcpGetVersion(server: *const u16, major: *mut u32, minor: *mut u32) -> u32;
    fn DhcpGetMibInfoV5(server: *const u16, result: *mut *mut Mib) -> u32;
}
fn failure(api: &str, code: u32) -> String {
    format!(
        "{api}: {} (code {code})",
        std::io::Error::from_raw_os_error(code as i32)
    )
}
unsafe fn array<'a, T>(p: *const T, n: u32) -> Result<&'a [T], String> {
    if n > 1_000_000 || (n > 0 && p.is_null()) {
        return Err("Invalid native array length or pointer".into());
    }
    if n == 0 {
        Ok(&[])
    } else {
        Ok(unsafe { std::slice::from_raw_parts(p, n as usize) })
    }
}
unsafe fn text(p: *const u16) -> Result<String, String> {
    if p.is_null() {
        return Ok(String::new());
    }
    for n in 0..65536 {
        if unsafe { *p.add(n) } == 0 {
            return Ok(String::from_utf16_lossy(unsafe {
                std::slice::from_raw_parts(p, n)
            }));
        }
    }
    Err("Unterminated option string".into())
}
unsafe fn element(e: &Element) -> Result<String, String> {
    // The union is read only according to the SDK discriminator. All pointers
    // remain owned by the native API until this one-shot worker exits.
    unsafe {
        Ok(match e.kind {
            0 => e.value.byte.to_string(),
            1 => e.value.word.to_string(),
            2 => e.value.dword.to_string(),
            3 => format!("DWORD1={} DWORD2={}", e.value.pair[0], e.value.pair[1]),
            4 => Ipv4Addr::from(e.value.dword).to_string(),
            5 | 8 => text(e.value.text)?,
            6 | 7 => array(e.value.binary.data, e.value.binary.length)?
                .iter()
                .map(|v| format!("{v:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
            other => format!("Unsupported native type {other}"),
        })
    }
}
unsafe fn copy_options(raw: *const Options, read: u32) -> Result<Vec<Entry>, String> {
    if raw.is_null() {
        return if read == 0 {
            Ok(vec![])
        } else {
            Err("Null option page".into())
        };
    }
    let page = unsafe { &*raw };
    if page.count != read {
        return Err("Option page count mismatch".into());
    }
    unsafe { array(page.values, page.count) }?
        .iter()
        .map(|row| {
            let mut kinds = Vec::new();
            let mut values = Vec::new();
            for item in unsafe { array(row.data.elements, row.data.count) }? {
                kinds.push(match item.kind {
                    0 => "Byte",
                    1 => "Word",
                    2 => "DWORD",
                    3 => "DWORD pair",
                    4 => "IPv4",
                    5 => "String",
                    6 => "Binary",
                    7 => "Encapsulated",
                    8 => "IPv6",
                    _ => "Unknown",
                });
                values.push(unsafe { element(item) }?);
            }
            Ok(Entry {
                name: format!("Option {}", row.id),
                kind: kinds.join(", "),
                values,
            })
        })
        .collect()
}
pub fn query(server: &str, scope: Option<&str>) -> Result<Inspection, String> {
    let server = ipsherpa::validate_server(server)?;
    let mut output = Inspection {
        server: server.into(),
        scope_id: None,
        rows: vec![],
        warnings: vec![],
    };
    let wide: Vec<u16> = server.encode_utf16().chain(Some(0)).collect();
    if let Some(scope) = scope {
        let id = ipsherpa::validate_scope_id(scope)?;
        output.scope_id = Some(id.to_string());
        let scope = Scope {
            kind: 2,
            subnet: u32::from(id) as usize,
        };
        let mut resume = 0;
        let mut seen = HashSet::new();
        loop {
            let (mut raw, mut read, mut total) = (ptr::null_mut(), 0, 0);
            let code = unsafe {
                DhcpEnumOptionValues(
                    wide.as_ptr(),
                    &scope,
                    &mut resume,
                    65536,
                    &mut raw,
                    &mut read,
                    &mut total,
                )
            };
            if code == 259 {
                break;
            }
            if code != 0 && code != 234 {
                return Err(failure("DhcpEnumOptionValues", code));
            }
            output.rows.extend(unsafe { copy_options(raw, read) }?);
            if code == 0 {
                break;
            }
            if read == 0 || !seen.insert(resume) {
                return Err("Option enumeration made no progress".into());
            }
        }
    } else {
        let (mut major, mut minor) = (0, 0);
        let code = unsafe { DhcpGetVersion(wide.as_ptr(), &mut major, &mut minor) };
        if code == 0 {
            output.rows.push(Entry {
                name: "DHCP management API".into(),
                kind: "Response".into(),
                values: vec![format!("Responding · version {major}.{minor}")],
            });
        } else {
            output.warnings.push(failure("DhcpGetVersion", code));
        }
        let mut raw = ptr::null_mut();
        let code = unsafe { DhcpGetMibInfoV5(wide.as_ptr(), &mut raw) };
        if code != 0 {
            output.warnings.push(failure("DhcpGetMibInfoV5", code));
        } else if raw.is_null() {
            output.warnings.push("Null server statistics".into());
        } else {
            output.rows.extend(unsafe { copy_health(&*raw) }?);
        }
    }
    Ok(output)
}
unsafe fn copy_health(mib: &Mib) -> Result<Vec<Entry>, String> {
    let scopes = unsafe { array(mib.scopes, mib.count) }?;
    let mut rows = Vec::new();
    for (name, value) in [
        "Discovers",
        "Offers",
        "Requests",
        "Acknowledgments",
        "Negative acknowledgments",
        "Declines",
        "Releases",
    ]
    .iter()
    .zip(mib.counters)
    {
        rows.push(Entry {
            name: (*name).into(),
            kind: "Server counter".into(),
            values: vec![value.to_string()],
        });
    }
    rows.push(Entry {
        name: "Scopes reported".into(),
        kind: "Statistics".into(),
        values: vec![mib.count.to_string()],
    });
    for (name, value) in [
        (
            "Addresses in use",
            scopes.iter().map(|x| u64::from(x.used)).sum::<u64>(),
        ),
        ("DHCP free", scopes.iter().map(|x| u64::from(x.free)).sum()),
        (
            "Pending offers",
            scopes.iter().map(|x| u64::from(x.pending)).sum(),
        ),
    ] {
        rows.push(Entry {
            name: name.into(),
            kind: "Across reported scopes".into(),
            values: vec![value.to_string()],
        });
    }
    Ok(rows)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sdk_layouts() {
        use std::mem::{offset_of, size_of};
        assert_eq!(size_of::<Scope>(), 16);
        assert_eq!(size_of::<Element>(), 24);
        assert_eq!(offset_of!(Element, value), 8);
        assert_eq!(size_of::<OptionValue>(), 24);
        assert_eq!(size_of::<Options>(), 16);
        assert_eq!(size_of::<Mib>(), 88);
    }
    #[test]
    fn copies_typed_options() {
        let name: Vec<u16> = "Example 東京\0".encode_utf16().collect();
        let bytes = [0, 255];
        let elements = [
            Element {
                kind: 4,
                value: Value { dword: 0xC0000201 },
            },
            Element {
                kind: 5,
                value: Value {
                    text: name.as_ptr(),
                },
            },
            Element {
                kind: 6,
                value: Value {
                    binary: Binary {
                        length: 2,
                        data: bytes.as_ptr(),
                    },
                },
            },
        ];
        let option = OptionValue {
            id: 3,
            data: Data {
                count: 3,
                elements: elements.as_ptr(),
            },
        };
        let page = Options {
            count: 1,
            values: &option,
        };
        let rows = unsafe { copy_options(&page, 1) }.unwrap();
        assert_eq!(rows[0].values, vec!["192.0.2.1", "Example 東京", "00 FF"]);
        assert!(unsafe { copy_options(&page, 2) }.is_err());
        assert!(unsafe { copy_options(ptr::null(), 1) }.is_err());
        assert!(unsafe { array::<Element>(ptr::null(), 1) }.is_err());
    }
    #[test]
    fn health_sums_large_counts() {
        let scopes = [
            ScopeMib {
                subnet: 0,
                used: u32::MAX,
                free: 0,
                pending: 0,
            },
            ScopeMib {
                subnet: 1,
                used: u32::MAX,
                free: 0,
                pending: 0,
            },
        ];
        let mib = Mib {
            counters: [0; 7],
            start: [0; 2],
            quarantine: [0; 7],
            delayed: 0,
            delayed_scopes: 0,
            count: 2,
            scopes: scopes.as_ptr(),
        };
        let rows = unsafe { copy_health(&mib) }.unwrap();
        assert_eq!(rows[8].values, vec!["8589934590"]);
    }
}
