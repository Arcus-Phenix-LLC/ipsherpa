# IPSherpa 0.10.0 — first primary Rust release

Validated Rust/Tauri functionality is promoted to `master` on 2026-11-09. The prior PowerShell implementation remains available in `legacy/powershell` and at tag `legacy-powershell-2026-11-09`.

## Included

- Native IPv4 scope, lease, reservation, range, exclusion, standard scope-option and basic server-statistics reads.
- Saved server names, cross-server searches, per-tab filters and paging, persistent navigation and source context.
- Explicitly reviewed DHCP-only reservation creation.
- Explicitly reviewed name/description edits preserving DHCP, BOOTP or DHCP/BOOTP reservation type.
- Native write workers replacing the PowerShell bridge used in versions 0.8 and 0.9. No PowerShell execution or DhcpServer-module import is used by the Rust application.
- Single-use expiring reviews, fresh checks, stale-row detection, read-back verification and uncertain-outcome handling.

## Deployment

Windows x64, RSAT DHCP Server Tools, WebView2, Windows DHCP API components, appropriate Windows identity permissions and DHCP RPC connectivity are prerequisites. RSAT-free deployment has not been validated. The distribution is a portable ZIP with the executable, MIT license, third-party notices and a quick-start README. `SHA256SUMS.txt` identifies the downloadable ZIP.

## Compatibility boundaries

This is the first primary Rust release, not complete legacy feature parity. Reservation deletion, alternate credentials, failover replication, DNS/ICMP/TCP static-device availability probes, extended-client-ID writes and reservation address/client-ID changes remain outside the GUI. Scope options exclude inherited/policy/class-specific values. Health is basic management API/counter visibility. Scope checks are conservative and are not an atomic server transaction or proof that an address is unused on the network.

Creation is multi-step. If a write or verification has an uncertain result, inspect DHCP Manager before trying again. No automatic retry, delete/recreate or rollback is performed.

## Validation

The maintainer confirmed the native reservation creation and DHCP/BOOTP metadata-editing milestone after validating the read-only and navigation milestones. Release checks use synthetic fixtures: 43 Rust tests, browser regression suites, SDK layout/ownership fixtures, Clippy, formatting and five packaged native-worker validation cases that make no DHCP calls.

No runtime logs, real server information, credentials or saved profiles are included in the source commit or release package.
