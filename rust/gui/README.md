# IPSherpa desktop preview

A Windows Tauri 2 GUI for DHCP browsing and reviewed reservation creation.
Enter `DHCP01` (or your server hostname/IP), select **Connect**, and use **Refresh**
to query that same server again. Editing the server field requires Connect to
switch targets. The table shows IPv4 scope IDs, names, configured allocation
ranges, and state. Select a scope to browse Overview, Leases, Reservations and
Options. Browsing remains read-only; **New reservation** is the explicit write
workflow added in 0.8.0 and requires RSAT DHCP tools.

## Read-only record browsing (0.2.0)

- Leases show IP, hostname, the complete client ID, server-reported state,
  lease expiry in local time, and description. Reserved clients are marked.
- Reservations show IP, name, full client ID, allowed client type, whether
  reservation options are present, and description. No lease is required for a
  configured reservation to appear.
- **Refresh records** queries the selected scope and tab. It uses the connected
  server even if the editable server field has been changed. Connect switches
  servers; refreshing scopes clears selection and record results.
- Native enumeration retrieves all pages. The UI renders 100 records per page
  and pages locally without additional network queries. A later native-page
  error is reported as an error; partial results are not presented as complete.
- Empty, loading, denied/unsupported, worker failure and unexpected-response
  states are distinct. No prior record results remain visible after a new query.
- Lease expiry sentinels show **Never** or **Not set**. Native state labels may
  differ from PowerShell's derived labels. Client IDs are not shortened to a
  presumed six-byte MAC address.
- Native APIs: `DhcpEnumSubnetClientsVQ` (Windows Server 2008 R2+) and
  `DhcpV4EnumSubnetReservations` (Windows Server 2012+). Unsupported servers
  return a visible error. These reads use the same process-lifetime buffers and
  short-lived hidden workers as scope details, with no nested native frees.

Compare a selected scope in the lab with the DHCP console and:

```powershell
Get-DhcpServerv4Lease -ComputerName EXAMPLE-DHCP -ScopeId 192.0.2.0 -AllLeases
Get-DhcpServerv4Reservation -ComputerName EXAMPLE-DHCP -ScopeId 192.0.2.0
```

Replace the synthetic target and scope with your lab values. Compare counts,
addresses, names, full client IDs and expiry; test empty results and a denied
query. Reservation reads must include reservations without current leases.

## Search and filters (0.3.0)

Use **Search records** to find an IP substring/prefix, hostname/name, or client
ID. **Search in** restricts matching to one field. Matching is case-insensitive
and handles Unicode names; client IDs can be entered with hyphens, colons,
dots, spaces, or no separators, without shortening variable-length IDs.

On Leases, **Lease state** combines with the text search. Options reflect the
server-reported states in the snapshot; reserved-client badges do not change
the underlying state. Reservations do not have a lease-state filter.

Filtering covers every loaded record before 100-row table paging. It performs
no additional DHCP query and preserves the connected server and selected scope.
The summary shows matches versus total records. **No matching records** is
distinct from a successful empty server response or a query error.

Each tab retains its own filters in memory. Refresh records preserves them,
including a selected state that is no longer present. Changing scope or
refreshing the scope list resets both tabs' filters. **Clear filters** resets
only the current tab. Search terms are neither saved nor sent to the backend.

For a lab check, search for a known record from a later table page, combine a
lease-state filter with its hostname, and try a client ID with alternate MAC
separators. Confirm counts and that Clear filters restores the full snapshot.

## Multiple servers and cross-scope search (0.4.0)

Use **Save typed server** to save the hostname/IP in the connection field.
Selecting a saved server fills that field; **Connect** switches the per-scope
browser. Saved names persist in this computer's WebView storage. No passwords
or credentials are stored. **Remove saved server** removes the saved name;
it does not disconnect a server that is currently being browsed. If storage is
unavailable, the list remains usable for the current session with a warning.

Expand **Search across servers**, check target servers, enter an IP, name or
client-ID search, and choose leases, reservations, or both. Saved servers and
the currently connected server are available as targets. Queries start only
when **Search servers** is selected, not while typing. Search reuses the same
case-insensitive and separator-insensitive matching as per-scope browsing.

The search enumerates every scope per selected server, then reads the selected
record types sequentially through existing isolated workers. Results arrive as
queries complete, with a server, scope and record type on every row. Equal IPs
on different servers/scopes/types remain separate results. Hostname case
variants are deduplicated, but aliases/IPs for the same physical server are not.

Failures are listed by server/scope/type and do not discard successful matches.
Stopped or failed searches are labeled **Partial results**; a zero-match partial
search does not claim that all selected servers have no matching records.
Scope metadata warnings are distinguished from failed record queries. The
summary reports discovered scopes, completed record queries and full server
coverage. Windows RPC still controls the current query duration.

**Stop after current query** prevents the next query from starting. It cannot
interrupt an active RPC. Result pages contain 100 matches and page locally.
Select a result's scope link to connect to its source and open the corresponding
Leases/Reservations tab. Existing per-scope search/filter controls remain.
Changing target selections or the search term does not alter the provenance of
an existing result snapshot; run a new search to update it.

Lab acceptance: save two readable servers, search both across multiple scopes,
compare known matches with their consoles, open a result on the second server,
and test an unreadable server alongside a readable one. Confirm that partial
results remain usable and that Stop prevents subsequent queries. Saved names
and all diagnostic data stay local and must not be added to the repository.

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
- Version 0.1.1 runs all native DHCP queries in a separate, hidden child process.
  If that process fails, the GUI retries scope IDs in another child and displays
  a recovery warning. If both queries fail, it shows an error rather than an
  empty result. Native access violations cannot be caught as ordinary Rust errors.
- Version 0.1.2 gives native detail-query buffers process lifetime. It copies
  the requested fields into owned Rust values without freeing nested native
  pointers. The one-shot worker exits after serialization, and Windows reclaims
  its native buffers. A single-use backend guard prevents repeated snapshots
  inside the same process; Refresh always starts a new worker.
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
- The frontend is bundled locally. Its permitted application commands are
  `list_scopes` and `list_records`, scoped to the main window. There are no shell/file/network
  plugins, remote web content, or DHCP mutation commands. The Rust backend starts
  only its own executable in the internal `--dhcp-worker` mode, without a shell.
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
node gui/tests/records.cjs
node --test gui/tests/filters.cjs
node gui/tests/search.cjs
node --test gui/tests/fleet.cjs
node gui/tests/multiserver.cjs
```

These mock IPC to cover Unicode names, multiple/empty/unavailable ranges,
partial errors, safe text rendering, loading controls, refresh targeting,
invalid input and a narrow window. They do not establish native API parity.

The scope-detail baseline was confirmed on an RSAT-equipped machine. Lease and
reservation browsing was also confirmed in the lab; search/filter changes need
a separate acceptance check; mocked results and ABI fixtures
do not establish live DHCP parity. Local checks cover the Windows release build,
Clippy, formatting, native pointer-array copies, FILETIME conversion, pagination,
worker payloads and browser interactions. Fixtures contain synthetic data only.

## Diagnosing the scope-detail crash

The detail build had unsafe assumptions about independent ownership of nested
native buffers. Version 0.1.2 removes that cleanup from detail queries entirely;
their memory is reclaimed when the query worker exits. This avoids relying on
undocumented nested allocation boundaries. The process isolation and scope-ID
fallback remain available. Live server comparison is still required for a new
build; synthetic regression fixtures do not establish native API parity.

Logs are local to `%LOCALAPPDATA%\IPSherpa\logs\ipsherpa.log`. They record the
target server, scope IDs, API stages, failures, and Rust panics; no passwords or
credentials. An optional `IPSHERPA_LOG_DIR` environment variable overrides the
directory for testing. Logs are appended across runs.

After retrying the failing query, inspect the last entries:

```powershell
Get-Content "$env:LOCALAPPDATA\IPSherpa\logs\ipsherpa.log" -Tail 60
```

Local worker tests cover native error exit codes, malformed replies, successful
details, scope-ID recovery and failure of both workers. A packaged headless
worker smoke test validated structured stdout and diagnostic file creation.
Native buffer tests use only synthetic data, including aliased and interior
pointers, and verify that the copied snapshot outlives its source storage.
Runtime logs and environment-specific diagnostic data must not be committed.

## Scope utilization and exclusions (0.5.0)

Select a scope, expand **Utilization & exclusions**, then choose **Load overview**.
The overview refreshes separately from lease and reservation tables. It reads
DHCP-reported in-use, free, and pending-offer counters using DhcpGetMibInfoV5.
Utilization is in-use / (in-use + free); a zero denominator is unavailable.
Pending offers remain a separate counter.

Configured range totals merge overlapping intervals. Excluded totals count only
addresses inside configured ranges, once each. Range addresses after exclusions
are not free-address discovery, and exclusions are not subtracted again from the
DHCP free counter. Empty exclusions mean none configured; unavailable exclusions
indicate a query failure. Independent failures preserve other available sections.

These read-only calls run in a disposable native worker. As with scope details,
native result buffers live until that worker exits. Scope options and server
health remain later milestones.

For lab comparison, use synthetic placeholders below with your own target:

```powershell
Get-DhcpServerv4ScopeStatistics -ComputerName EXAMPLE-DHCP -ScopeId 192.0.2.0
Get-DhcpServerv4ExclusionRange -ComputerName EXAMPLE-DHCP -ScopeId 192.0.2.0
```

Browser tests mock responses and do not establish live Windows DHCP parity.
Runtime logs, saved profiles, and environment-specific diagnostics are excluded
from repository content.

## Scope options and server health (0.6.0)

After connecting, expand **Server health** and choose **Load server health**.
This reads DhcpGetVersion and DhcpGetMibInfoV5 independently: management API
response/version, packet counters, reported scope count, and aggregate scope
in-use/free/pending counts. Partial failures retain available values. These
snapshots are not packet rates or an assurance of authorization, database
integrity, failover state, service uptime, or client-path connectivity.

Select a scope, expand **Scope options**, then choose **Load scope options**.
DhcpEnumOptionValues enumerates explicitly configured standard scope options.
Each row shows the numeric option ID, native type and values. IPv4 addresses,
strings, integer types, IPv6 strings and binary/encapsulated hex values are
preserved. Unknown native types are labeled unsupported. Server inheritance,
reservation overrides, vendor/user classes and policy options are not included;
an empty list means no explicit standard scope values, not no client options.
Both views load on demand, identify their source and clear old values on target
changes. Queries share the existing busy lock and disposable native worker.
Native result buffers are retained only until the child exits; no nested frees.

Lab acceptance: compare the scope view with Scope Options in DHCP Manager and
`Get-DhcpServerv4OptionValue -ComputerName EXAMPLE-DHCP -ScopeId 192.0.2.0`.
Compare server counters with `Get-DhcpServerv4Statistics -ComputerName
EXAMPLE-DHCP`. Test an empty scope-option list and an unavailable server.
API version is the DHCP management API version, not the Windows product build.
Synthetic tests do not establish live-server parity. No runtime logs or real
server diagnostics are included in the repository.

## Workspace navigation (0.7.0)

The sidebar contains saved servers, connection controls and navigation to Server
overview, Selected scope and Search across servers. Server overview separates
Scopes and Health. Selecting a scope opens Overview; Leases, Reservations and
Options each have their own tab. Overview combines configured allocation ranges,
utilization and exclusions. Queries remain read-only and load on demand.

Switching views keeps loaded snapshots, record filters and record pagination.
Successful lease/reservation snapshots are cached only for the selected scope
and current connection. Switching record tabs reuses that snapshot; use Refresh
records to query again. Selecting a different scope or querying the server again
clears the selected scope's snapshots. Loaded timestamps remain visible.

Cross-server search has its own screen. Opening a result goes to its source
scope and record tab; Back to search restores the existing search results.
The current server/scope header stays visible while active content scrolls.
Narrow windows use the Menu button; Escape closes the navigation menu.
Tabs support arrow keys, Home and End. Navigation itself does not query DHCP.

Browser regression checks cover view switching, record-cache reuse, filters,
pagination, source context, search return, keyboard navigation, loading/errors,
and narrow layout. All fixtures are synthetic; no runtime logs are committed.

## Reviewed reservation creation (0.8.0)

Select a scope, choose **New reservation**, and enter an IPv4 address, six-byte
unicast MAC, name and optional description. **Check & review** performs read-only
checks, then displays normalized values with the exact server and scope. Check
the confirmation box and choose **Create reservation** to write. Editing or
closing the dialog does not write. The backend binds a single-use review token
to the normalized request; reviews expire after two minutes.

This first write workflow uses Windows PowerShell and the RSAT DhcpServer module
under the current Windows identity. It calls Add-DhcpServerv4Reservation with
Type Dhcp. The existing native read-only workers remain unchanged. Input is
passed as JSON on stdin to a fixed embedded script, never interpolated into code.
No execution-policy changes or credential storage are required.

Preview and Create each re-read the selected scope, exclusions, reservations
and all lease states. Addresses outside its usable allocation range or inside
exclusions are blocked. Existing reservation IPs/MACs, conflicting lease IPs,
non-active lease states, or a same-MAC lease at another address are blocked.
An active same-MAC lease at the requested address may become a reservation.
No existing lease, reservation or exclusion is removed or overwritten.
Extended client IDs, BOOTP, edits, deletes and automatic lease moves are deferred.

After Add returns, the reservation is read back and compared with the requested
IP, MAC, name, description and DHCP type. A write error or failed/mismatched
readback has an unknown outcome: inspect DHCP Manager before trying again.
There is no automatic retry or rollback. Record caches and capacity snapshots
are invalidated after the attempt; refresh records for current data.

Checks apply only to this scope/server. They cannot detect statically configured
devices, guarantee network conflict freedom, or atomically exclude changes by
another administrator between checking and writing. Failover replication and
other servers are not checked or triggered by this workflow.

Validation covers normalized input, single-use/expired reviews, synthetic
PowerShell cmdlet failures/conflicts/readback, and browser confirmation flow.
No live reservation writes are performed by automated checks. Use an authorized
lab scope, create a disposable reservation, and compare it with DHCP Manager.
Runtime logs and real server data must not be committed.
