# IPSherpa desktop preview

A Windows Tauri 2 GUI for the read-only Rust scope enumeration experiment.
Enter `DHCP01` (or your server hostname/IP), select **Connect**, and use **Refresh**
to query that same server again. Editing the server field requires Connect to
switch targets. The table shows IPv4 scope IDs, not scope names or lease details.

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

1. Query the same server already checked with the CLI; compare scope IDs.
2. Refresh and confirm it returns the same server's current data.
3. Check empty scopes, access denied, and an unreachable server.
4. Confirm the window remains responsive during a slow query.

Browser checks with mocked IPC cover success, empty results, error display,
loading controls, refresh targeting, invalid input and a narrow window. They do
not establish GUI-to-server parity. User-reported CLI success on an RSAT-equipped
machine establishes the native API baseline, not a completed GUI lab test.

Local validation passed the Windows x64 release build, Clippy, formatting, and
three shared-library/CLI tests. Native desktop launch was blocked by the build
session's approval policy, so actual WebView2 startup and GUI IPC still require
the lab checks above.
