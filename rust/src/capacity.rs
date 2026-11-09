use crate::AddressRange;
use std::{
    net::Ipv4Addr,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, PartialEq)]
pub struct ScopeStatistics {
    pub in_use: u32,
    pub free: u32,
    pub pending_offers: u32,
}
impl ScopeStatistics {
    pub fn utilization_percent(&self) -> Option<f64> {
        let total = u64::from(self.in_use) + u64::from(self.free);
        (total > 0).then(|| f64::from(self.in_use) * 100.0 / total as f64)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct PoolTotals {
    pub configured: u64,
    pub excluded: u64,
    pub after_exclusions: u64,
}
#[derive(Debug)]
pub struct CapacitySnapshot {
    pub statistics: Option<ScopeStatistics>,
    pub configured_ranges: Option<Vec<AddressRange>>,
    pub exclusions: Option<Vec<AddressRange>>,
    pub totals: Option<PoolTotals>,
    pub warnings: Vec<String>,
}

fn union(ranges: &[AddressRange]) -> Result<Vec<(u64, u64)>, String> {
    let mut sorted = Vec::new();
    for range in ranges {
        let (start, end) = (
            u64::from(u32::from(range.start)),
            u64::from(u32::from(range.end)),
        );
        if start > end {
            return Err("A configured range has its start after its end.".into());
        }
        sorted.push((start, end));
    }
    sorted.sort_unstable();
    let mut merged: Vec<(u64, u64)> = Vec::new();
    for (start, end) in sorted {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 + 1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end));
    }
    Ok(merged)
}
pub fn pool_totals(
    configured: &[AddressRange],
    exclusions: &[AddressRange],
) -> Result<PoolTotals, String> {
    let configured = union(configured)?;
    let exclusions = union(exclusions)?;
    let total = configured
        .iter()
        .map(|(start, end)| end - start + 1)
        .sum::<u64>();
    let (mut i, mut j, mut excluded) = (0, 0, 0);
    while i < configured.len() && j < exclusions.len() {
        let (a, b) = configured[i];
        let (c, d) = exclusions[j];
        let (start, end) = (a.max(c), b.min(d));
        if start <= end {
            excluded += end - start + 1;
        }
        if b < d {
            i += 1;
        } else {
            j += 1;
        }
    }
    Ok(PoolTotals {
        configured: total,
        excluded,
        after_exclusions: total - excluded,
    })
}
fn assemble(
    stats: Result<ScopeStatistics, String>,
    ranges: Result<Vec<AddressRange>, String>,
    exclusions: Result<Vec<AddressRange>, String>,
) -> CapacitySnapshot {
    let mut warnings = Vec::new();
    let statistics = stats
        .map_err(|error| warnings.push(format!("Statistics: {error}")))
        .ok();
    let configured_ranges = ranges
        .map_err(|error| warnings.push(format!("Allocation ranges: {error}")))
        .ok();
    let exclusions = exclusions
        .map_err(|error| warnings.push(format!("Exclusions: {error}")))
        .ok();
    let totals = match (&configured_ranges, &exclusions) {
        (Some(ranges), Some(exclusions)) => pool_totals(ranges, exclusions)
            .map_err(|error| warnings.push(error))
            .ok(),
        _ => None,
    };
    CapacitySnapshot {
        statistics,
        configured_ranges,
        exclusions,
        totals,
        warnings,
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::ptr;
    #[repr(C)]
    struct DateTime {
        low: u32,
        high: u32,
    }
    #[repr(C)]
    struct ScopeMib {
        subnet: u32,
        in_use: u32,
        free: u32,
        pending: u32,
    }
    #[repr(C)]
    struct Mib {
        counters: [u32; 7],
        start: DateTime,
        quarantine: [u32; 7],
        delayed: u32,
        delayed_scopes: u32,
        count: u32,
        scopes: *mut ScopeMib,
    }
    #[link(name = "dhcpsapi")]
    extern "system" {
        fn DhcpGetMibInfoV5(server: *const u16, info: *mut *mut Mib) -> u32;
    }
    unsafe fn copy_stats(raw: *const Mib, id: Ipv4Addr) -> Result<ScopeStatistics, String> {
        // SAFETY: successful SDK response or live test fixture. ScopeInfo is a
        // contiguous array (unlike the lease/reservation pointer arrays).
        let mib = unsafe { raw.as_ref() }.ok_or("DHCP returned null statistics.")?;
        if mib.count > 0 && mib.scopes.is_null() {
            return Err("DHCP returned a null statistics array.".into());
        }
        for index in 0..mib.count as usize {
            let scope = unsafe { &*mib.scopes.add(index) };
            if scope.subnet == u32::from(id) {
                return Ok(ScopeStatistics {
                    in_use: scope.in_use,
                    free: scope.free,
                    pending_offers: scope.pending,
                });
            }
        }
        Err("The selected scope is absent from server statistics.".into())
    }
    fn stats(server: &str, id: Ipv4Addr) -> Result<ScopeStatistics, String> {
        let wide: Vec<u16> = server.encode_utf16().chain(Some(0)).collect();
        let mut raw = ptr::null_mut();
        eprintln!("DhcpGetMibInfoV5: call begin");
        // SAFETY: terminated UTF-16 input and initialized output. All native
        // allocations have process lifetime and are reclaimed at worker exit.
        let code = unsafe { DhcpGetMibInfoV5(wide.as_ptr(), &mut raw) };
        eprintln!("DhcpGetMibInfoV5: returned {code}");
        if code != 0 {
            return Err(format!(
                "DhcpGetMibInfoV5 failed: {} (code {code})",
                std::io::Error::from_raw_os_error(code as i32)
            ));
        }
        unsafe { copy_stats(raw, id) }
    }
    pub fn query(server: &str, id: Ipv4Addr) -> CapacitySnapshot {
        assemble(
            stats(server, id),
            crate::details::read_ranges_in_worker(server, id, false),
            crate::details::read_ranges_in_worker(server, id, true),
        )
    }
    #[cfg(all(test, target_pointer_width = "64"))]
    mod tests {
        use super::*;
        use std::mem::{offset_of, size_of};
        #[test]
        fn sdk_statistics_layout_and_scope_lookup() {
            assert_eq!(size_of::<Mib>(), 88);
            assert_eq!(offset_of!(Mib, count), 72);
            assert_eq!(offset_of!(Mib, scopes), 80);
            assert_eq!(size_of::<ScopeMib>(), 16);
            let mut rows = [
                ScopeMib {
                    subnet: 0xC0000200,
                    in_use: 1,
                    free: 9,
                    pending: 0,
                },
                ScopeMib {
                    subnet: 0xC6336400,
                    in_use: 7,
                    free: 3,
                    pending: 2,
                },
            ];
            let raw = Mib {
                counters: [0; 7],
                start: DateTime { low: 0, high: 0 },
                quarantine: [0; 7],
                delayed: 0,
                delayed_scopes: 0,
                count: 2,
                scopes: rows.as_mut_ptr(),
            };
            assert_eq!(
                unsafe { copy_stats(&raw, Ipv4Addr::new(198, 51, 100, 0)) }
                    .unwrap()
                    .in_use,
                7
            );
            assert!(unsafe { copy_stats(&raw, Ipv4Addr::new(203, 0, 113, 0)) }.is_err());
            assert!(unsafe { copy_stats(ptr::null(), Ipv4Addr::new(192, 0, 2, 0)) }.is_err());
        }
    }
}

/// One-shot, read-only capacity snapshot. Exit the worker after serializing the
/// owned result; native buffers intentionally remain alive until process exit.
pub fn scope_capacity_for_worker(server: &str, scope: &str) -> Result<CapacitySnapshot, String> {
    let server = crate::validate_server(server)?;
    let id = crate::validate_scope_id(scope)?;
    static USED: AtomicBool = AtomicBool::new(false);
    if USED.swap(true, Ordering::AcqRel) {
        return Err("Capacity requires a fresh one-shot worker.".into());
    }
    #[cfg(windows)]
    {
        Ok(native::query(server, id))
    }
    #[cfg(not(windows))]
    {
        let _ = (server, id);
        Err("Native DHCP capacity queries require Windows.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn range(start: u32, end: u32) -> AddressRange {
        AddressRange {
            start: Ipv4Addr::from(start),
            end: Ipv4Addr::from(end),
        }
    }
    #[test]
    fn merges_ranges_and_clips_exclusions_without_double_counting() {
        let totals = pool_totals(
            &[range(10, 20), range(18, 25), range(40, 49)],
            &[range(0, 12), range(12, 15), range(22, 42), range(99, 100)],
        )
        .unwrap();
        assert_eq!(
            totals,
            PoolTotals {
                configured: 26,
                excluded: 13,
                after_exclusions: 13
            }
        );
        assert_eq!(pool_totals(&[], &[]).unwrap().after_exclusions, 0);
        assert_eq!(
            pool_totals(&[range(0, u32::MAX)], &[]).unwrap().configured,
            4_294_967_296
        );
        assert!(pool_totals(&[range(20, 10)], &[]).is_err());
    }
    #[test]
    fn statistics_percent_is_explicit_and_handles_zero_and_large_counts() {
        assert_eq!(
            ScopeStatistics {
                in_use: 0,
                free: 0,
                pending_offers: 1
            }
            .utilization_percent(),
            None
        );
        assert_eq!(
            ScopeStatistics {
                in_use: 0,
                free: 10,
                pending_offers: 1
            }
            .utilization_percent(),
            Some(0.0)
        );
        assert_eq!(
            ScopeStatistics {
                in_use: 10,
                free: 0,
                pending_offers: 0
            }
            .utilization_percent(),
            Some(100.0)
        );
        assert_eq!(
            ScopeStatistics {
                in_use: u32::MAX,
                free: u32::MAX,
                pending_offers: 1
            }
            .utilization_percent(),
            Some(50.0)
        );
    }
    #[test]
    fn preserves_partial_failures_and_distinguishes_empty_exclusions() {
        let result = assemble(Err("denied".into()), Ok(vec![range(10, 20)]), Ok(vec![]));
        assert!(result.statistics.is_none());
        assert_eq!(result.exclusions, Some(vec![]));
        assert_eq!(result.totals.unwrap().after_exclusions, 11);
        assert_eq!(result.warnings.len(), 1);
        let result = assemble(
            Ok(ScopeStatistics {
                in_use: 1,
                free: 9,
                pending_offers: 0,
            }),
            Ok(vec![]),
            Err("denied".into()),
        );
        assert!(result.statistics.is_some());
        assert!(result.exclusions.is_none());
        assert!(result.totals.is_none());
    }
}
