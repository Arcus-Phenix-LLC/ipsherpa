#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ScopeList {
    server: String,
    scope_ids: Vec<String>,
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
        let scope_ids = ipsherpa::list_scopes(&server)?
            .into_iter()
            .map(|id| id.to_string())
            .collect();
        Ok(ScopeList { server, scope_ids })
    })
    .await
    .map_err(|error| format!("The scope query worker failed: {error}"))?
}

fn main() {
    tauri::Builder::default()
        .manage(QueryState::default())
        .invoke_handler(tauri::generate_handler![list_scopes])
        .run(tauri::generate_context!())
        .expect("Failed to start IPSherpa. Check the WebView2 runtime installation.");
}
