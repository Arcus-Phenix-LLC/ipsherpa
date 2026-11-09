use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    net::Ipv4Addr,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub server: String,
    pub scope_id: String,
    pub ip_address: String,
    pub mac: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub original: Option<Original>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Original {
    pub name: String,
    pub description: String,
    pub client_type: String,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct Outcome {
    pub status: String,
    pub message: String,
}
#[derive(Serialize)]
pub struct Preview {
    pub request: Request,
    pub outcome: Outcome,
    pub token: Option<String>,
}
struct Review {
    token: String,
    request: Request,
    expires: Instant,
}
#[derive(Default)]
pub struct Reviews(Mutex<Option<Review>>);
pub fn validate(mut r: Request) -> Result<Request, String> {
    r.server = ipsherpa::validate_server(r.server.trim())?.to_owned();
    r.scope_id = ipsherpa::validate_scope_id(r.scope_id.trim())?.to_string();
    let ip: Ipv4Addr = r
        .ip_address
        .trim()
        .parse()
        .map_err(|_| "Enter a valid IPv4 address.")?;
    if ip.is_unspecified() || ip.is_broadcast() || ip.is_multicast() || ip.is_loopback() {
        return Err("Enter a usable unicast IPv4 address.".into());
    }
    r.ip_address = ip.to_string();
    let text = r.mac.trim();
    if !text
        .chars()
        .all(|c| c.is_ascii_hexdigit() || "-:.".contains(c))
    {
        return Err("Enter a six-byte MAC address.".into());
    }
    let hex: String = text.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 12 {
        return Err("Enter exactly six bytes for the MAC address. Extended DHCP client IDs are not supported in this workflow.".into());
    }
    let bytes: Vec<u8> = (0..6)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
        .collect();
    if bytes[0] & 1 != 0 || bytes.iter().all(|b| *b == 0) {
        return Err("MAC must be a nonzero unicast address.".into());
    }
    r.mac = bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join("-");
    r.name = r.name.trim().into();
    r.description = r.description.trim().into();
    if r.name.is_empty()
        || r.name.encode_utf16().count() > 255
        || r.description.encode_utf16().count() > 255
        || r.name
            .chars()
            .chain(r.description.chars())
            .any(char::is_control)
    {
        return Err("Name is required; name and description must each be at most 255 characters without control characters.".into());
    }
    if let Some(original) = &r.original {
        if !matches!(
            original.client_type.as_str(),
            "DHCP" | "BOOTP" | "DHCP / BOOTP"
        ) {
            return Err(
                "Editing supports DHCP, BOOTP and DHCP/BOOTP reservations with six-byte unicast MACs."
                    .into(),
            );
        }
        if original.name.encode_utf16().count() > 255
            || original.description.encode_utf16().count() > 255
            || original
                .name
                .chars()
                .chain(original.description.chars())
                .any(char::is_control)
        {
            return Err("Existing metadata is unsupported by this editing workflow.".into());
        }
        if original.name == r.name && original.description == r.description {
            return Err("Change the name or description before reviewing.".into());
        }
    }
    Ok(r)
}
fn run(phase: &str, r: &Request) -> Result<Outcome, String> {
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command.args(["--reservation-worker", phase]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Unable to start native reservation worker: {e}"))?;
    // User input is JSON data on stdin, never interpolated into executable code.
    let data = serde_json::to_vec(&serde_json::json!({"phase":phase,"request":r}))
        .map_err(|e| e.to_string())?;
    let written = child
        .stdin
        .take()
        .ok_or("Worker stdin unavailable.")?
        .write_all(&data);
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    written.map_err(|e| e.to_string())?;
    if !output.status.success() {
        let code = output
            .status
            .code()
            .map(|v| format!("0x{:08X}", v as u32))
            .unwrap_or_else(|| output.status.to_string());
        return Err(format!(
            "Native DHCP worker exited {code}. Check server connectivity and permissions."
        ));
    }
    let result: Outcome = serde_json::from_slice(&output.stdout)
        .map_err(|_| "Invalid native DHCP worker response.".to_owned())?;
    if !matches!(
        result.status.as_str(),
        "ready" | "blocked" | "created" | "updated" | "unknown"
    ) {
        return Err("Unrecognized reservation result.".into());
    }
    Ok(result)
}
impl Reviews {
    pub fn preview(&self, r: Request) -> Result<Preview, String> {
        *self.0.lock().map_err(|_| "Review state unavailable")? = None;
        let r = validate(r)?;
        let outcome = run("preview", &r)?;
        let token = if outcome.status == "ready" {
            static NEXT: AtomicU64 = AtomicU64::new(1);
            let token = format!("review-{}", NEXT.fetch_add(1, Ordering::Relaxed));
            *self.0.lock().map_err(|_| "Review state unavailable")? = Some(Review {
                token: token.clone(),
                request: r.clone(),
                expires: Instant::now() + Duration::from_secs(120),
            });
            Some(token)
        } else {
            None
        };
        Ok(Preview {
            request: r,
            outcome,
            token,
        })
    }
    pub fn create(&self, token: &str) -> Result<Outcome, String> {
        let review = match self.take(token) {
            Ok(review) => review,
            Err(message) => {
                return Ok(Outcome {
                    status: "blocked".into(),
                    message,
                })
            }
        };
        // Once dispatched a write can have an indeterminate outcome. Never retry it.
        Ok(run("create", &review).unwrap_or_else(|e| Outcome {
            status: "unknown".into(),
            message: format!(
                "The reservation may have changed. Inspect DHCP Manager before trying again. {e}"
            ),
        }))
    }
    fn take(&self, token: &str) -> Result<Request, String> {
        let review = self
            .0
            .lock()
            .map_err(|_| "Review state unavailable")?
            .take()
            .ok_or("Review the reservation before creating it.")?;
        if review.token != token || Instant::now() >= review.expires {
            return Err("Review expired or changed. Review the reservation again.".into());
        }
        Ok(review.request)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Request {
        Request {
            server: "EXAMPLE-DHCP".into(),
            scope_id: "192.0.2.0".into(),
            ip_address: "192.0.2.25".into(),
            mac: "02:aa:bb:cc:dd:ee".into(),
            name: "Example 東京".into(),
            description: "Synthetic".into(),
            original: None,
        }
    }
    #[test]
    fn validates_and_normalizes() {
        assert_eq!(validate(request()).unwrap().mac, "02-AA-BB-CC-DD-EE");
        for mac in [
            "00:00:00:00:00:00",
            "ff-ff-ff-ff-ff-ff",
            "01aabbccddee",
            "01-02-AA-BB-CC-DD-EE",
            "aa$()bb",
        ] {
            let mut r = request();
            r.mac = mac.into();
            assert!(validate(r).is_err());
        }
    }
    #[test]
    fn rejects_bad_input() {
        for ip in [
            "0.0.0.0",
            "127.0.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "bad",
        ] {
            let mut r = request();
            r.ip_address = ip.into();
            assert!(validate(r).is_err());
        }
        let mut r = request();
        r.name = "\n".into();
        assert!(validate(r).is_err());
    }
    #[test]
    fn validates_edit_baseline_and_rejects_noop_or_unsupported_type() {
        let mut r = request();
        r.original = Some(Original {
            name: r.name.clone(),
            description: r.description.clone(),
            client_type: "DHCP".into(),
        });
        assert!(validate(r.clone()).is_err());
        r.name = "Updated".into();
        assert!(validate(r.clone()).is_ok());
        for kind in ["BOOTP", "DHCP / BOOTP"] {
            r.original.as_mut().unwrap().client_type = kind.into();
            assert!(validate(r.clone()).is_ok());
        }
        r.original.as_mut().unwrap().client_type = "Unknown (99)".into();
        assert!(validate(r).is_err());
    }
    #[test]
    fn review_preserves_before_and_after_values() {
        let reviews = Reviews::default();
        let mut r = request();
        r.original = Some(Original {
            name: "Before".into(),
            description: "Previous".into(),
            client_type: "DHCP".into(),
        });
        *reviews.0.lock().unwrap() = Some(Review {
            token: "edit".into(),
            request: r.clone(),
            expires: Instant::now() + Duration::from_secs(120),
        });
        assert_eq!(reviews.take("edit").unwrap(), r);
        assert!(reviews.take("edit").is_err());
    }
    #[test]
    fn review_is_bound_expiring_and_single_use() {
        let reviews = Reviews::default();
        *reviews.0.lock().unwrap() = Some(Review {
            token: "synthetic".into(),
            request: request(),
            expires: Instant::now() + Duration::from_secs(120),
        });
        assert_eq!(reviews.take("synthetic").unwrap(), request());
        assert!(reviews.take("synthetic").is_err());
        *reviews.0.lock().unwrap() = Some(Review {
            token: "old".into(),
            request: request(),
            expires: Instant::now(),
        });
        assert!(reviews.take("old").is_err());
        assert!(reviews.take("other").is_err());
    }
}
