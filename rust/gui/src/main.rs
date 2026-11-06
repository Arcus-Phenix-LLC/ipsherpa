#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod diagnostics;
mod worker;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Default, Clone)]
struct QueryState(Arc<AtomicBool>);

struct QueryGuard(Arc<AtomicBool>);
impl Drop for QueryGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScopeList {
    server: String,
    scopes: Vec<ScopeRow>,
    #[serde(default)]
    notice: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScopeRow {
    scope_id: String,
    name: Option<String>,
    state: Option<String>,
    ranges: Option<Vec<RangeRow>>,
    warnings: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct RangeRow {
    start: String,
    end: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordList {
    server: String,
    scope_id: String,
    kind: String,
    records: Vec<RecordRow>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordRow {
    ip_address: String,
    name: String,
    client_id: String,
    description: String,
    client_type: String,
    state: Option<String>,
    expires_unix_seconds: Option<i64>,
    expiry_label: Option<String>,
    is_reservation: bool,
    options_present: Option<bool>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapacityRow {
    server: String,
    scope_id: String,
    statistics: Option<StatisticsRow>,
    configured_ranges: Option<Vec<RangeRow>>,
    exclusions: Option<Vec<RangeRow>>,
    totals: Option<TotalsRow>,
    warnings: Vec<String>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatisticsRow {
    in_use: u32,
    free: u32,
    pending_offers: u32,
    utilization_percent: Option<f64>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TotalsRow {
    configured: u64,
    excluded: u64,
    after_exclusions: u64,
}
impl CapacityRow {
    fn new(server: &str, scope: &str, snapshot: ipsherpa::CapacitySnapshot) -> Self {
        let ranges = |ranges: Option<Vec<ipsherpa::AddressRange>>| {
            ranges.map(|ranges| {
                ranges
                    .into_iter()
                    .map(|range| RangeRow {
                        start: range.start.to_string(),
                        end: range.end.to_string(),
                    })
                    .collect()
            })
        };
        Self {
            server: server.into(),
            scope_id: scope.into(),
            statistics: snapshot.statistics.map(|s| StatisticsRow {
                in_use: s.in_use,
                free: s.free,
                pending_offers: s.pending_offers,
                utilization_percent: s.utilization_percent(),
            }),
            configured_ranges: ranges(snapshot.configured_ranges),
            exclusions: ranges(snapshot.exclusions),
            totals: snapshot.totals.map(|t| TotalsRow {
                configured: t.configured,
                excluded: t.excluded,
                after_exclusions: t.after_exclusions,
            }),
            warnings: snapshot.warnings,
        }
    }
}

impl From<ipsherpa::ClientRecord> for RecordRow {
    fn from(record: ipsherpa::ClientRecord) -> Self {
        Self {
            ip_address: record.address.to_string(),
            name: record.name,
            client_id: record.client_id,
            description: record.description,
            client_type: record.client_type,
            state: record.state,
            expires_unix_seconds: record.expires_unix_seconds,
            expiry_label: record.expiry_label,
            is_reservation: record.is_reservation,
            options_present: record.options_present,
        }
    }
}

impl From<ipsherpa::ScopeDetails> for ScopeRow {
    fn from(scope: ipsherpa::ScopeDetails) -> Self {
        Self {
            scope_id: scope.scope_id.to_string(),
            name: scope.name,
            state: scope.state,
            ranges: scope.ranges.map(|ranges| {
                ranges
                    .into_iter()
                    .map(|range| RangeRow {
                        start: range.start.to_string(),
                        end: range.end.to_string(),
                    })
                    .collect()
            }),
            warnings: scope.warnings,
        }
    }
}

#[tauri::command]
async fn list_scopes(
    server: String,
    state: tauri::State<'_, QueryState>,
) -> Result<ScopeList, String> {
    let server = ipsherpa::validate_server(&server)?.to_owned();
    if state
        .0
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("A scope query is already running. Wait for it to finish.".into());
    }
    // Move the guard into the blocking worker: abandoning an IPC request must
    // not release the query lock while the Windows RPC is still running.
    let guard = QueryGuard(Arc::clone(&state.0));
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        worker::query(&server)
    })
    .await
    .map_err(|error| format!("The scope query worker failed: {error}"))?
}

#[tauri::command]
async fn list_records(
    server: String,
    scope_id: String,
    kind: String,
    state: tauri::State<'_, QueryState>,
) -> Result<RecordList, String> {
    let server = ipsherpa::validate_server(&server)?.to_owned();
    let scope_id = ipsherpa::validate_scope_id(&scope_id)?.to_string();
    if !matches!(kind.as_str(), "leases" | "reservations") {
        return Err("Unknown record kind.".into());
    }
    if state
        .0
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("Another query is running. Wait for it to finish.".into());
    }
    let guard = QueryGuard(Arc::clone(&state.0));
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        worker::records(&server, &scope_id, &kind)
    })
    .await
    .map_err(|error| format!("The record query worker failed: {error}"))?
}

#[tauri::command]
async fn scope_capacity(
    server: String,
    scope_id: String,
    state: tauri::State<'_, QueryState>,
) -> Result<CapacityRow, String> {
    let server = ipsherpa::validate_server(&server)?.to_owned();
    let scope = ipsherpa::validate_scope_id(&scope_id)?.to_string();
    if state
        .0
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("Another query is running. Wait for it to finish.".into());
    }
    let guard = QueryGuard(Arc::clone(&state.0));
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        worker::capacity(&server, &scope)
    })
    .await
    .map_err(|error| format!("Capacity worker failed: {error}"))?
}

fn main() {
    diagnostics::init();
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--dhcp-worker") {
        let code = match args.as_slice() {
            [_, mode, server] => worker::child_main(mode, server, None),
            [_, mode, server, scope] => worker::child_main(mode, server, Some(scope)),
            _ => {
                diagnostics::log("Invalid DHCP worker arguments");
                2
            }
        };
        std::process::exit(code);
    }
    tauri::Builder::default()
        .manage(QueryState::default())
        .invoke_handler(tauri::generate_handler![
            list_scopes,
            list_records,
            scope_capacity
        ])
        .run(tauri::generate_context!())
        .expect("Failed to start IPSherpa. Check the WebView2 runtime installation.");
}
