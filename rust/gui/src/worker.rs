use crate::{diagnostics, ScopeList, ScopeRow};
use std::{
    io::Write,
    process::{Command, Output},
};

pub fn child_main(mode: &str, server: &str) -> i32 {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn SetErrorMode(mode: u32) -> u32;
        }
        // SAFETY: only this short-lived worker's error mode is changed. Native
        // failure must return an exit status, not wait for a crash dialog.
        unsafe {
            SetErrorMode(0x0001 | 0x0002);
        }
    }
    let result = (|| -> Result<ScopeList, String> {
        ipsherpa::validate_server(server)?;
        eprintln!("worker {mode}: query begin");
        let scopes = match mode {
            "details" => ipsherpa::list_scope_details_for_worker(server)?
                .into_iter()
                .map(ScopeRow::from)
                .collect(),
            "ids" => ipsherpa::list_scopes(server)?
                .into_iter()
                .map(|id| ScopeRow {
                    scope_id: id.to_string(),
                    name: None,
                    state: None,
                    ranges: None,
                    warnings: vec![],
                })
                .collect(),
            _ => return Err("Unknown DHCP worker mode".into()),
        };
        eprintln!("worker {mode}: query complete");
        Ok(ScopeList {
            server: server.to_owned(),
            scopes,
            notice: None,
        })
    })();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    if let Err(error) = serde_json::to_writer(&mut stdout, &result)
        .and_then(|_| stdout.flush().map_err(serde_json::Error::io))
    {
        diagnostics::log(&format!("Worker output failed: {error}"));
        return 1;
    }
    0
}

fn decode(mode: &str, output: Output) -> Result<ScopeList, String> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        diagnostics::log(&format!("{mode} worker stages:\n{stderr}"));
    }
    if !output.status.success() {
        let code = output
            .status
            .code()
            .map(|code| format!("0x{:08X}", code as u32))
            .unwrap_or_else(|| output.status.to_string());
        return Err(format!("Native {mode} worker exited with {code}"));
    }
    serde_json::from_slice::<Result<ScopeList, String>>(&output.stdout)
        .map_err(|error| format!("Invalid {mode} worker response: {error}"))?
}

fn run(mode: &str, server: &str) -> Result<ScopeList, String> {
    diagnostics::log(&format!("Starting {mode} worker for {server}"));
    let mut command = Command::new(std::env::current_exe().map_err(|error| error.to_string())?);
    command.args(["--dhcp-worker", mode, server]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let output = command
        .output()
        .map_err(|error| format!("Unable to start {mode} worker: {error}"))?;
    decode(mode, output)
}

fn with_fallback(
    server: &str,
    mut worker: impl FnMut(&str, &str) -> Result<ScopeList, String>,
) -> Result<ScopeList, String> {
    match worker("details", server) {
        Ok(result) => Ok(result),
        Err(detail_error) => {
            diagnostics::log(&format!(
                "Detail query failed: {detail_error}; trying scope IDs"
            ));
            let mut result = worker("ids", server).map_err(|id_error| {
                format!(
                    "{detail_error}. Scope-ID recovery also failed: {id_error}. Diagnostic log: {}",
                    diagnostics::path().display()
                )
            })?;
            result.notice = Some(format!(
                "Scope IDs recovered; names, ranges and state are unavailable. {detail_error}. Diagnostic log: {}",
                diagnostics::path().display()));
            Ok(result)
        }
    }
}

pub fn query(server: &str) -> Result<ScopeList, String> {
    let result = with_fallback(server, run);
    match &result {
        Ok(result) => diagnostics::log(&format!(
            "Query complete: {} scopes; recovery={}",
            result.scopes.len(),
            result.notice.is_some()
        )),
        Err(error) => diagnostics::log(&format!("Query failed: {error}")),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> ScopeList {
        ScopeList {
            server: "DHCP01".into(),
            scopes: vec![ScopeRow {
                scope_id: "10.0.0.0".into(),
                name: None,
                state: None,
                ranges: None,
                warnings: vec![],
            }],
            notice: None,
        }
    }
    #[test]
    fn recovers_ids_after_detail_worker_failure() {
        let mut modes = Vec::new();
        let result = with_fallback("DHCP01", |mode, _| {
            modes.push(mode.to_owned());
            if mode == "details" {
                Err("Native details worker exited with 0xC0000005".into())
            } else {
                Ok(fixture())
            }
        })
        .unwrap();
        assert_eq!(modes, ["details", "ids"]);
        assert_eq!(result.scopes[0].scope_id, "10.0.0.0");
        assert!(result.notice.unwrap().contains("0xC0000005"));
    }
    #[test]
    fn reports_both_failures_instead_of_claiming_empty_success() {
        let error = with_fallback("DHCP01", |mode, _| Err(format!("{mode} failed")))
            .err()
            .unwrap();
        assert!(error.contains("details failed"));
        assert!(error.contains("ids failed"));
    }
    #[test]
    fn successful_details_do_not_retry() {
        let mut calls = 0;
        let result = with_fallback("DHCP01", |mode, _| {
            calls += 1;
            assert_eq!(mode, "details");
            Ok(fixture())
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert!(result.notice.is_none());
    }
    #[cfg(windows)]
    #[test]
    fn classifies_native_crash_and_rejects_invalid_output() {
        use std::os::windows::process::ExitStatusExt;
        let crash = Output {
            status: std::process::ExitStatus::from_raw(0xC0000005),
            stdout: vec![],
            stderr: vec![],
        };
        assert!(decode("details", crash)
            .err()
            .unwrap()
            .contains("0xC0000005"));
        let invalid = Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: b"not JSON".to_vec(),
            stderr: vec![],
        };
        assert!(decode("details", invalid).is_err());
        let valid = Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: serde_json::to_vec(&Ok::<_, String>(fixture())).unwrap(),
            stderr: vec![],
        };
        assert_eq!(decode("details", valid).unwrap().scopes.len(), 1);
    }
}
