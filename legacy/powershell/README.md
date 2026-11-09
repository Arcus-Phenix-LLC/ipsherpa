> Archived legacy PowerShell documentation. The primary Rust desktop guide is [here](../../README.md). This historical guide describes the preserved scripts, not feature parity with the Rust application.

<p align="center">
  <img src="../../ipsherpa-mascot.png" alt="IPSherpa Mascot" width="450">
</p>

<h1 align="center">IPSherpa</h1>

<p align="center">
<b>Find. Verify. Reserve.</b><br>
Open-source IP & DHCP administration toolkit for Windows administrators.
</p>

# IPSherpa

**IPSherpa** is an open-source Microsoft DHCP and IP address administration toolkit for Windows administrators.

Originally forked from **DHCP Scope Manager** by Flemming Sørvollen Skaret, IPSherpa expands the original idea into a broader network administration assistant. The goal is simple: help administrators find, verify, and reserve IP addresses with more confidence.

IPSherpa can be run from an RSAT-enabled administrator workstation instead of requiring an interactive session directly on the DHCP server.

> **Tagline:** Find. Verify. Reserve.

---

## IPSherpa Mascot

IPSherpa's mascot represents the purpose of the project: guiding administrators safely through IP space.

Just as a mountain sherpa helps climbers navigate difficult terrain, IPSherpa helps network administrators navigate complex DHCP environments by helping them:

- 🔍 Discover available addresses
- ✅ Verify they are truly unused
- 📌 Create reservations safely
- 🌐 Manage multiple DHCP servers
- 🧭 Navigate networks with confidence

---

## Project Status

IPSherpa is currently in early modernization.

Some script filenames still use the earlier working name **Scopey** while the project transitions to the new IPSherpa branding.

Current entry points:

```powershell
.\Scopey.ps1
.\Scopey-ReserveDevice.ps1
```

Future releases may rename these to IPSherpa-native filenames while preserving compatibility aliases.

---

## Features

### Current Features

- Remote Microsoft DHCP Server management
- Known DHCP server inventory
- Interactive DHCP server selection
- DHCP scope selection
- DHCP failover replication
- List free IP addresses
- View active and reserved leases
- Search leases by IP address
- Search leases by MAC / Client ID
- Create DHCP reservations
- Remove DHCP reservations
- Intelligent IP availability detection
- Generic device reservation wizard

---

## Requirements

- Windows PowerShell 5.1 or newer
- Microsoft DHCP PowerShell module
- RSAT DHCP Server Tools **or** Windows DHCP Server Role
- Administrative permissions to the DHCP server
- Network connectivity to the DHCP server

If running IPSherpa from an administrator workstation, install the RSAT DHCP tools first.

---

## How to Use

### Start IPSherpa

Run the interactive DHCP helper:

```powershell
.\Scopey.ps1
```

Connect directly to a specific DHCP server:

```powershell
.\Scopey.ps1 -DhcpServer DHCP01
```

Use alternate credentials:

```powershell
.\Scopey.ps1 -DhcpServer DHCP01 -Credential (Get-Credential)
```

Use a custom known-server inventory file:

```powershell
.\Scopey.ps1 -ServerListPath .\Scopey.Servers.json
```

---

## Managing Multiple DHCP Servers

IPSherpa supports multiple DHCP servers through a simple JSON inventory file.

Create a file named:

```text
Scopey.Servers.json
```

Example:

```json
[
  {
    "Name": "DHCP01.contoso.local",
    "Location": "Headquarters",
    "Description": "Primary DHCP Server"
  },
  {
    "Name": "DHCP02.contoso.local",
    "Location": "Branch Office",
    "Description": "Secondary DHCP Server"
  },
  {
    "Name": "DHCP-LAB.contoso.local",
    "Location": "Testing Lab",
    "Description": "Lab DHCP Server"
  }
]
```

When IPSherpa starts, choose the desired DHCP server from the list.

You can also switch DHCP servers from inside the application without restarting.

---

## Selecting a Scope

After connecting to a DHCP server, IPSherpa displays available IPv4 scopes.

Select the Scope ID to begin managing that subnet.

Example:

```text
192.168.10.0
```

---

## Viewing Available Addresses

Menu option:

```text
3 - List all free IP addresses
```

IPSherpa displays addresses currently available for assignment inside the selected DHCP scope.

---

## Intelligent IP Discovery

One of IPSherpa's primary features is **Intelligent IP Discovery**.

Traditional DHCP tools only show what DHCP knows. That can miss statically configured devices, stale DNS records, or devices that were never DHCP clients.

IPSherpa checks multiple signals before recommending an address:

- DHCP leases
- DHCP reservations
- DNS records
- ICMP ping
- Optional TCP port probes

Example result:

```text
IPAddress:     192.168.10.210
DHCP Lease:    No
Reservation:   No
DNS Record:    No
Ping:          No
Open Ports:    None

Confidence:    5/5
Status:        Likely Available
```

This helps reduce the chance of assigning an IP address already used by a statically configured device.

---

## Testing a Specific Address

Menu option:

```text
14 - Test specific IP availability
```

This runs Intelligent IP Discovery against one IP address.

Use this before assigning or reserving an address manually.

---

## Device Reservation Wizard

IPSherpa includes a generic device reservation wizard.

Run:

```powershell
.\Scopey-ReserveDevice.ps1
```

Connect to a specific DHCP server and scope:

```powershell
.\Scopey-ReserveDevice.ps1 -DhcpServer DHCP01 -ScopeId 192.168.10.0
```

Reserve a specific device type:

```powershell
.\Scopey-ReserveDevice.ps1 -DhcpServer DHCP01 -ScopeId 192.168.10.0 -DeviceType Camera
```

Supported device profiles:

- Printer
- Server
- Switch
- Camera
- AccessPoint
- Phone
- Workstation
- IoT
- Other

Each profile uses device-appropriate TCP port probes to improve confidence that an address is actually unused.

| Device Type | Typical Ports Checked |
|---|---|
| Printer | 80, 443, 515, 631, 9100 |
| Switch | 22, 23, 80, 443, 161, 830 |
| Server | 22, 80, 135, 139, 443, 445, 3389, 5985, 5986 |
| Camera | 80, 443, 554, 8000, 8080 |
| AccessPoint | 22, 80, 443, 8080, 8443 |
| Phone | 80, 443, 5060, 5061 |
| Workstation | 135, 139, 445, 3389, 5985 |
| IoT | 80, 443, 1883, 5683, 8080 |

The wizard walks through:

1. Selecting a DHCP scope
2. Choosing a device profile
3. Finding likely available addresses
4. Testing candidate addresses
5. Creating the DHCP reservation

---

## Why IPSherpa?

DHCP administration often requires administrators to:

- RDP into a DHCP server
- Search leases manually
- Compare reservations manually
- Ping addresses one at a time
- Check DNS separately
- Guess whether a static device already owns an address

IPSherpa is designed to guide administrators through that terrain.

It brings DHCP administration to the workstation while adding network intelligence that the built-in DHCP console does not provide.

---

## Roadmap

Planned future improvements:

- Rename scripts from Scopey working names to IPSherpa-native names
- Search across all DHCP scopes
- Multi-server dashboard
- Automatic device placement
- Reservation templates
- Duplicate IP detection
- DHCP health dashboard
- DHCP utilization graphs
- CSV / Excel reporting
- Search by hostname
- Search by MAC address across all scopes
- VLAN awareness
- DNS integration
- Active Directory integration
- Interactive console improvements
- Windows GUI
- PowerShell 7 compatibility investigation

---

## Mascot Concept

IPSherpa's mascot concept is a friendly network guide: a sherpa carrying Ethernet cables, a clipboard of IP addresses, and a compass shaped like a subnet map.

The mascot reinforces the project's purpose:

> Guiding administrators safely through IP space.

---

## Credits

Original project:

**DHCP Scope Manager**

Created by Flemming Sørvollen Skaret.

IPSherpa is an independent open-source fork that modernizes and expands the original project while preserving the original spirit of simplifying Microsoft DHCP administration.

---

## License

This project remains licensed under the MIT License unless otherwise noted.

The original copyright and license notices from the upstream project should be preserved.
