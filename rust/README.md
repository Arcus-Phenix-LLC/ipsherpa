# IPSherpa Rust experiment

Rust transition experiment on `rust-transition`. The CLI lists IPv4 scope IDs
using `DhcpEnumSubnets` directly and remains read-only without PowerShell.
Uses the current Windows process identity, with no stored credentials.

The [Tauri GUI](gui/README.md) reuses the same native API through `src/lib.rs`.
It adds server/scope browsing, search and scope details. GUI 0.8.0 also adds
reviewed DHCP-only reservation creation (0.8.0) and name/description editing
(0.9.0) through Windows PowerShell and the RSAT
DhcpServer module. See its README for checks, confirmation and limitations. The CLI remains
available independently and requires no Tauri dependencies.

## Build and run

From this directory, use a Rust MSVC toolchain with Visual Studio C++ Build Tools
and the Windows SDK (including the DHCP import library):

```powershell
cargo build --release --offline
.\target\release\ipsherpa.exe --help
.\target\release\ipsherpa.exe scopes --server DHCP01
```

The runtime needs the Windows `Dhcpsapi.dll` component, permission to read the
target DHCP server, and RPC connectivity. This prototype links that DLL at load
time; a missing DLL may prevent even `--help` from starting. Do not copy Windows
DLLs from another machine to work around deployment requirements.

Microsoft lists no supported client OS for this API. Windows workstation/RSAT
deployment remains experimental and must be verified separately. Start on a
Windows Server lab admin host. This code does not implement an RPC timeout or
alternate credentials; interrupt a stalled invocation with Ctrl+C.

## Validation

```powershell
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
```

Before accepting native integration, run against a lab DHCP server and compare
the returned scope IDs with `Get-DhcpServerv4Scope -ComputerName DHCP01`.
Check an empty server, multiple scopes, denied access, and unreachable targets.
Record OS, installed DHCP tools, identity permissions, and results. Unit tests
cover CLI input and address conversion; they do not establish live API parity.

## API references

- https://learn.microsoft.com/en-us/windows/win32/api/dhcpsapi/nf-dhcpsapi-dhcpenumsubnets
- https://learn.microsoft.com/en-us/windows/win32/api/dhcpsapi/ns-dhcpsapi-dhcp_ip_array
- https://learn.microsoft.com/en-us/windows/win32/api/dhcpsapi/nf-dhcpsapi-dhcprpcfreememory

The FFI boundary uses C layout and the Windows system calling convention.
Scope-ID pages and their element arrays are released with DhcpRpcFreeMemory.
The CLI lists scope IDs. The GUI additionally reads names, configured ranges,
and state through `list_scope_details_for_worker`. It also browses leases and
reservations via `list_records_for_worker`. These APIs are single-use within
short-lived workers; native buffers are reclaimed by Windows at worker exit.
The scope-detail baseline was confirmed on an RSAT-equipped machine. New record
reads need a separate lab comparison. DHCP writes remain future work.
