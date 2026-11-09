<p align="center"><img src="./ipsherpa-mascot.png" alt="IPSherpa mascot" width="280"></p>

# IPSherpa

**Find. Verify. Reserve.** A Rust/Tauri desktop application for Microsoft DHCP administrators on Windows.

The Rust application is now the primary implementation. Version **0.10.0** uses native Windows DHCP APIs for browsing, reservation creation and metadata editing. It does not launch PowerShell or import the DhcpServer PowerShell module.

## Download and run

Download `IPSherpa-GUI-0.10.0-Windows-x64.zip` and `SHA256SUMS.txt` from the [v0.10.0 release](https://github.com/Arcus-Phenix-LLC/ipsherpa/releases/tag/v0.10.0). Extract the ZIP and run `ipsherpa-gui.exe`; keep the license and third-party notices with it.

Prerequisites for this first Rust release:

- Windows x64 with RSAT DHCP Server Tools installed.
- Microsoft Edge WebView2 Runtime and the Windows DHCP API components.
- Connectivity to the target DHCP server over the Windows DHCP management RPC path.
- Read access for browsing and suitable DHCP administrative rights for writes, under your current Windows identity.

RSAT remains a deployment prerequisite even though IPSherpa does not execute PowerShell. Installation without RSAT is a future validation milestone. No alternate credentials are stored or supplied by the app.

To compare the downloaded ZIP with the published checksum:

```powershell
Get-FileHash .\IPSherpa-GUI-0.10.0-Windows-x64.zip -Algorithm SHA256
```

## Current features

| Area | Functionality |
|---|---|
| Navigation | Persistent server context, saved server names, dedicated server/scope/search screens and scope tabs |
| Browsing | IPv4 scopes, allocation ranges, leases, reservations, utilization and exclusions |
| Search | IP, hostname, client ID/MAC and lease-state filters; multi-server, cross-scope searches |
| Options | Explicit standard scope options; inherited and policy/class-specific values are not included |
| Server overview | DHCP management API response/version and server/scope counters |
| Create reservations | Reviewed DHCP-only creation with a six-byte unicast MAC, conflict checks and read-back verification |
| Edit reservations | Reviewed name/description edits for DHCP, BOOTP and DHCP/BOOTP reservations; existing IP, client ID, type and options are preserved |

Connect to a server, select a scope and use **Overview**, **Leases**, **Reservations** or **Options**. Loading a panel or switching tabs does not silently refresh its snapshots. Use the panel's refresh control for current data.

For writes, choose **New reservation** or **Edit**, then **Check & review**. Verify the server, scope and proposed values, check the confirmation box and explicitly select **Create reservation** or **Apply changes**. Reviews are single-use and expire after two minutes.

## Scope and limitations

- Checks cover the selected server/scope. They do not establish that an address is unused by statically configured devices, trigger failover replication, or lock out concurrent administrators.
- Writes are not atomic multi-step transactions. A reservation can be partially created if a later metadata or verification step fails. On an uncertain outcome, inspect DHCP Manager before another attempt; the app does not retry or roll back automatically.
- Six-byte unicast MACs are supported. Arbitrary extended DHCP client IDs and reservation IP/client-ID changes are not supported by the write workflow.
- Allocation-range, exclusion and lease checks apply conservatively to creation and editing. Metadata edits outside the current usable pool are blocked.
- Reservation deletion, alternate credentials, static-device/DNS/ICMP/TCP availability probes and failover replication are legacy features that have not been ported to the Rust GUI.
- API health counters are snapshots, not a comprehensive service, failover or database health assessment. Windows controls RPC duration; the application does not impose a native RPC timeout.

See the [desktop guide](rust/gui/README.md) and [release notes](RELEASE_NOTES.md) for the write model and validation boundaries.

## Legacy PowerShell application

The former scripts and their original documentation are archived under [legacy/powershell](legacy/powershell/README.md). The pre-transition `master` is preserved by the [`legacy-powershell-2026-11-09` tag](https://github.com/Arcus-Phenix-LLC/ipsherpa/tree/legacy-powershell-2026-11-09).

This release promotes the validated Rust feature set; it does not claim complete parity with every legacy menu item. Use the archived PowerShell application when an unported legacy function is required. Its RSAT PowerShell module requirements still apply. Saved GUI server profiles do not automatically import the legacy JSON inventory.

## Build from source

Install Rust 1.93 or newer with the MSVC toolchain, Visual Studio C++ Build Tools and the Windows SDK (including the DHCP import library). From the repository root:

```powershell
cargo build --release --locked --manifest-path rust/gui/Cargo.toml
.\rust\gui\target\release\ipsherpa-gui.exe
```

The independent read-only CLI can be built with `cargo build --release --locked --manifest-path rust/Cargo.toml`. Native workers are isolated child processes; returned RPC buffers live until worker exit.

## Validation and privacy

The Rust GUI's browsing, navigation, native reservation creation and DHCP/BOOTP metadata-editing milestones have been confirmed in the development lab. Automated checks cover 43 Rust tests, browser regressions, ABI fixtures, formatting, Clippy and packaged-worker input validation. Synthetic tests do not replace deployment-specific validation.

Runtime diagnostics stay local to `%LOCALAPPDATA%\IPSherpa\logs`. They can contain target server/scope information. Do not commit runtime logs, real server inventories, saved profiles or environment-specific diagnostic data.

## Credits and license

IPSherpa builds on **DHCP Scope Manager**, created by **Flemming Sørvollen Skaret**, and preserves the original project's copyright and license notices. Licensed under [MIT](LICENSE). Packaged third-party license notices accompany the executable.
