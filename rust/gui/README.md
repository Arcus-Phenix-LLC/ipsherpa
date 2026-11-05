# IPSherpa desktop preview

A Windows Tauri 2 GUI for the read-only Rust scope enumeration experiment.
Enter `DHCP01` (or your server hostname/IP), select **Connect**, and use **Refresh**
to query that same server again. Editing the server field requires Connect to
switch targets. The table shows IPv4 scope IDs, names, configured allocation
ranges, and state. Lease details and DHCP writes are not included.

## Build and launch

Requirements: Windows, Rust 1.93.0, MSVC C++ Build Tools and Windows SDK, and the
Microsoft Edge WebView2 runtime. The native DHCP DLL must also be available.
The current lab baseline has RSAT DHCP tools installed; running without RSAT
has not been validated. No Node.js or frontend bundler is needed to build/run.

From the repository's `rust` directory:

```powershell
rustup toolchain install 1.93.0 --profile minimal
cargo +1.93.0 build --release --locked --manifest-path gui/Cargo.toml
.\gui\target\release\ipsherpa-gui.exe
```

The executable embeds the UI assets. This preview has no installer, automatic
updates, or code signature. It uses the installed WebView2 runtime rather than
downloading one. For later distribution, test an offline installer on the
actual administration workstations before choosing runtime packaging.

## Behavior and boundaries

- Uses the current Windows identity; no passwords, server history, or credentials
  are stored. DHCP server permissions and RPC connectivity are still required.
- Runs native queries on a blocking worker, keeping the UI responsive. One query
  may run at a time, enforced by the backend as well as disabled controls.
- Shows loading, successful results, an empty response, and API errors including
  their numeric Windows code. Previous results are cleared before a new query.
- Names and state come from `DhcpGetSubnetInfo`; configured ranges come from
  `DhcpEnumSubnetElements` with `DhcpIpRanges`. Multiple ranges remain separate;
  exclusions and lease usage are not subtracted. These are not free-IP estimates.
- Per-scope failures keep the scope ID visible with an inline warning. Failed
  range queries show **Unavailable**, while successful empty queries show
  **No ranges configured**. Unknown state codes are shown explicitly.
- A slow-query hint appears after 12 seconds. This is not a cancellation timeout:
  Windows RPC controls how long the underlying operation takes. A timeout or
  reliable cancellation mechanism remains future work.
- The frontend is bundled locally. Its single permitted application command is
  `list_scopes`, scoped to the main window. There are no shell/file/network
  plugins, remote web content, or DHCP mutation commands.
- This GUI is Windows-focused. Tauri's cross-platform support does not make the
  Windows DHCP backend portable.

## Verification

From `rust`:

```powershell
cargo +1.93.0 test --locked
cargo +1.93.0 fmt --check
cargo +1.93.0 fmt --check --manifest-path gui/Cargo.toml
cargo +1.93.0 clippy --locked --manifest-path gui/Cargo.toml -- -D warnings
```

Before accepting the GUI in your lab:

1. Query the same server already checked with the CLI; compare scope IDs,
   names, state, and ranges with the DHCP console. For a PowerShell comparison:
   `Get-DhcpServerv4Scope -ComputerName DHCP01 | Select-Object ScopeId,Name,StartRange,EndRange,State`.
2. Refresh and confirm it returns the same server's current data.
3. Check empty scopes, access denied, and an unreachable server.
4. Confirm the window remains responsive during a slow query.

Optional browser regression tests use Playwright and an installed Chrome. From
`rust`, install the test-only dependency and run:

```powershell
npm install --prefix gui --no-save --package-lock=false playwright@1.64.1
node gui/tests/browser.cjs
```

These mock IPC to cover Unicode names, multiple/empty/unavailable ranges,
partial errors, safe text rendering, loading controls, refresh targeting,
invalid input and a narrow window. They do not establish native API parity.

Local validation passed the Windows x64 release build, Clippy, formatting, and
seven shared-library/CLI tests. The initial GUI's startup and scope query were
confirmed by the user on an RSAT-equipped machine. The new name/range/state
queries still require lab comparison; a test success is not claimed for them.
