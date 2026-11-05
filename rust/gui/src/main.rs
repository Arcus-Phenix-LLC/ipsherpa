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

fn main() {
    diagnostics::init();
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--dhcp-worker") {
        let code = match args.as_slice() {
            [_, mode, server] => worker::child_main(mode, server),
            _ => {
                diagnostics::log("Invalid DHCP worker arguments");
                2
            }
        };
        std::process::exit(code);
    }
    tauri::Builder::default()
        .manage(QueryState::default())
        .invoke_handler(tauri::generate_handler![list_scopes])
        .run(tauri::generate_context!())
        .expect("Failed to start IPSherpa. Check the WebView2 runtime installation.");
}
