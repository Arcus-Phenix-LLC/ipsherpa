<#
.SYNOPSIS
  Scopey device reservation wizard for Microsoft DHCP Server.

.DESCRIPTION
  Finds likely available IPv4 addresses in a selected DHCP scope, checks for common
  signs of existing use, then optionally creates a DHCP reservation for a device.

  Supports device profiles such as Printer, Switch, Server, Camera, AccessPoint,
  Phone, Workstation, IoT, and Other. Profiles control suggested TCP probes and
  reservation description formatting.

.PARAMETER DhcpServer
  DHCP server to manage. Defaults to the local computer.

.PARAMETER ScopeId
  DHCP scope ID, such as 192.168.10.0.

.PARAMETER DeviceType
  Device profile to reserve.

.PARAMETER DeviceName
  Friendly device name or hostname.

.PARAMETER ClientId
  MAC address / DHCP client ID.

.PARAMETER IPAddress
  Specific IP address to reserve. If omitted, Scopey suggests likely available IPs.

.PARAMETER Credential
  Optional credential used for DHCP Server cmdlets that support -Credential.

.EXAMPLE
  .\Scopey-ReserveDevice.ps1 -DhcpServer DHCP01 -ScopeId 192.168.10.0 -DeviceType Printer

.EXAMPLE
  .\Scopey-ReserveDevice.ps1 -DhcpServer DHCP01 -ScopeId 192.168.30.0 -DeviceType Camera -DeviceName CAM-FRONT-01 -ClientId A1-B2-C3-D4-E5-F6
#>

[CmdletBinding(SupportsShouldProcess)]
param(
    [Parameter()]
    [string]$DhcpServer = $env:COMPUTERNAME,

    [Parameter()]
    [string]$ScopeId,

    [Parameter()]
    [ValidateSet('Printer','Switch','Server','Camera','AccessPoint','Phone','Workstation','IoT','Other')]
    [string]$DeviceType,

    [Parameter()]
    [string]$DeviceName,

    [Parameter()]
    [string]$ClientId,

    [Parameter()]
    [string]$IPAddress,

    [Parameter()]
    [System.Management.Automation.PSCredential]$Credential,

    [Parameter()]
    [int]$Suggestions = 5
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

function Import-ScopeyDhcpModule {
    try {
        Import-Module DhcpServer -ErrorAction Stop
    }
    catch {
        throw 'Unable to import DhcpServer module. Install RSAT DHCP tools or run from a DHCP-capable admin host.'
    }
}

function Invoke-ScopeyDhcpCommand {
    param(
        [Parameter(Mandatory)]
        [string]$CommandName,

        [Parameter()]
        [hashtable]$Parameters = @{}
    )

    $command = Get-Command -Name $CommandName -ErrorAction Stop
    $boundParameters = @{} + $Parameters

    if ($command.Parameters.ContainsKey('ComputerName')) {
        $boundParameters['ComputerName'] = $DhcpServer
    }

    if ($Credential -and $command.Parameters.ContainsKey('Credential')) {
        $boundParameters['Credential'] = $Credential
    }

    & $CommandName @boundParameters
}

function Get-ScopeyDeviceProfile {
    param(
        [Parameter(Mandatory)]
        [string]$Type
    )

    $profiles = @{
        Printer = [PSCustomObject]@{
            Type = 'Printer'
            Prefix = 'PRN'
            Ports = @(80,443,515,631,9100)
            DescriptionHint = 'Printer reservation'
        }
        Switch = [PSCustomObject]@{
            Type = 'Switch'
            Prefix = 'SW'
            Ports = @(22,23,80,443,161,830)
            DescriptionHint = 'Network switch management reservation'
        }
        Server = [PSCustomObject]@{
            Type = 'Server'
            Prefix = 'SRV'
            Ports = @(22,80,135,139,443,445,3389,5985,5986)
            DescriptionHint = 'Server reservation'
        }
        Camera = [PSCustomObject]@{
            Type = 'Camera'
            Prefix = 'CAM'
            Ports = @(80,443,554,8000,8080)
            DescriptionHint = 'IP camera reservation'
        }
        AccessPoint = [PSCustomObject]@{
            Type = 'AccessPoint'
            Prefix = 'AP'
            Ports = @(22,80,443,8080,8443)
            DescriptionHint = 'Wireless access point reservation'
        }
        Phone = [PSCustomObject]@{
            Type = 'Phone'
            Prefix = 'PHN'
            Ports = @(80,443,5060,5061)
            DescriptionHint = 'VoIP phone reservation'
        }
        Workstation = [PSCustomObject]@{
            Type = 'Workstation'
            Prefix = 'WKST'
            Ports = @(135,139,445,3389,5985)
            DescriptionHint = 'Workstation reservation'
        }
        IoT = [PSCustomObject]@{
            Type = 'IoT'
            Prefix = 'IOT'
            Ports = @(80,443,1883,5683,8080)
            DescriptionHint = 'IoT device reservation'
        }
        Other = [PSCustomObject]@{
            Type = 'Other'
            Prefix = 'DEV'
            Ports = @(22,80,443,445,3389,8080)
            DescriptionHint = 'Device reservation'
        }
    }

    $profiles[$Type]
}

function Select-ScopeyDeviceType {
    Write-Host ''
    Write-Host 'Device profiles:' -ForegroundColor Cyan
    Write-Host ' 1 - Printer'
    Write-Host ' 2 - Switch'
    Write-Host ' 3 - Server'
    Write-Host ' 4 - Camera'
    Write-Host ' 5 - AccessPoint'
    Write-Host ' 6 - Phone'
    Write-Host ' 7 - Workstation'
    Write-Host ' 8 - IoT'
    Write-Host ' 9 - Other'

    $choice = Read-Host 'Select device type'

    switch ($choice) {
        '1' { 'Printer' }
        '2' { 'Switch' }
        '3' { 'Server' }
        '4' { 'Camera' }
        '5' { 'AccessPoint' }
        '6' { 'Phone' }
        '7' { 'Workstation' }
        '8' { 'IoT' }
        '9' { 'Other' }
        default { 'Other' }
    }
}

function Test-ScopeyIpAvailability {
    param(
        [Parameter(Mandatory)]
        [string]$Address,

        [Parameter()]
        [int[]]$TcpPorts = @()
    )

    $lease = $null
    $reservation = $null
    $dnsName = $null
    $pingResponded = $false
    $openPorts = @()
    $signals = New-Object System.Collections.Generic.List[string]

    try { $lease = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4Lease -Parameters @{ IPAddress = $Address } }
    catch { }

    try { $reservation = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4Reservation -Parameters @{ IPAddress = $Address } }
    catch { }

    try {
        $dnsResult = Resolve-DnsName -Name $Address -ErrorAction Stop | Select-Object -First 1
        if ($dnsResult.NameHost) { $dnsName = $dnsResult.NameHost }
        elseif ($dnsResult.Name) { $dnsName = $dnsResult.Name }
    }
    catch { }

    try { $pingResponded = Test-Connection -ComputerName $Address -Count 1 -Quiet -ErrorAction SilentlyContinue }
    catch { $pingResponded = $false }

    foreach ($port in $TcpPorts) {
        try {
            if (Test-NetConnection -ComputerName $Address -Port $port -InformationLevel Quiet -WarningAction SilentlyContinue) {
                $openPorts += $port
            }
        }
        catch { }
    }

    if ($reservation) { $signals.Add('DHCP reservation exists') }
    if ($lease) { $signals.Add("DHCP lease exists: $($lease.AddressState)") }
    if ($dnsName) { $signals.Add("DNS record exists: $dnsName") }
    if ($pingResponded) { $signals.Add('Ping responded') }
    if ($openPorts.Count -gt 0) { $signals.Add("Open TCP ports: $($openPorts -join ', ')") }

    $status = 'Likely Available'
    $confidence = 5

    if ($reservation) {
        $status = 'Reserved'
        $confidence = 0
    }
    elseif ($lease) {
        $status = 'In Use - DHCP Lease Found'
        $confidence = 0
    }
    elseif ($pingResponded -or $openPorts.Count -gt 0) {
        $status = 'Probably In Use - Network Response Found'
        $confidence = 1
    }
    elseif ($dnsName) {
        $status = 'Possibly In Use - DNS Record Found'
        $confidence = 3
    }

    [PSCustomObject]@{
        IPAddress = $Address
        Status = $status
        Confidence = $confidence
        DhcpLease = [bool]$lease
        Reservation = [bool]$reservation
        DNSName = $dnsName
        Ping = $pingResponded
        OpenTcpPorts = ($openPorts -join ',')
        Signals = ($signals -join '; ')
    }
}

function Get-ScopeyCandidateAddresses {
    param(
        [Parameter(Mandatory)]
        [string]$TargetScopeId,

        [Parameter(Mandatory)]
        [int]$Count,

        [Parameter()]
        [int[]]$TcpPorts = @()
    )

    $stats = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4ScopeStatistics -Parameters @{ ScopeId = $TargetScopeId }

    if ($stats.Free -eq 0) {
        return @()
    }

    $candidateCount = [Math]::Min([int]$stats.Free, [Math]::Max($Count * 5, 10))
    $freeAddresses = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4FreeIPAddress -Parameters @{
        ScopeId = $TargetScopeId
        NumAddress = $candidateCount
    }

    foreach ($address in $freeAddresses) {
        Test-ScopeyIpAvailability -Address $address.IPAddressToString -TcpPorts $TcpPorts
    }
}

function New-ScopeyReservationDescription {
    param(
        [Parameter(Mandatory)]
        [object]$Profile,

        [Parameter(Mandatory)]
        [string]$Name
    )

    "$($Profile.DescriptionHint): $Name"
}

Import-ScopeyDhcpModule

if (-not $ScopeId) {
    Write-Host "Available scopes on $DhcpServer" -ForegroundColor Cyan
    Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4Scope |
        Select-Object ScopeId, Name, StartRange, EndRange, State |
        Format-Table -AutoSize

    $ScopeId = Read-Host 'Scope ID to reserve from'
}

if (-not $DeviceType) {
    $DeviceType = Select-ScopeyDeviceType
}

$profile = Get-ScopeyDeviceProfile -Type $DeviceType

if (-not $DeviceName) {
    $DeviceName = Read-Host "Device name/hostname [$($profile.Prefix)-NAME]"
}

if (-not $ClientId) {
    $ClientId = Read-Host 'Client ID / MAC address'
}

if (-not $IPAddress) {
    Write-Host ''
    Write-Host "Finding likely available addresses for $DeviceType using profile ports: $($profile.Ports -join ', ')" -ForegroundColor Cyan

    $candidates = @(Get-ScopeyCandidateAddresses -TargetScopeId $ScopeId -Count $Suggestions -TcpPorts $profile.Ports |
        Sort-Object @{ Expression = 'Confidence'; Descending = $true }, IPAddress |
        Select-Object -First $Suggestions)

    if ($candidates.Count -eq 0) {
        throw "No DHCP-free addresses were found in scope $ScopeId."
    }

    $candidates | Format-Table IPAddress, Status, Confidence, DNSName, Ping, OpenTcpPorts -AutoSize

    $IPAddress = Read-Host 'Enter IP address to reserve from the list above, or type another IP'
}
else {
    Write-Host ''
    Write-Host "Testing requested IP address $IPAddress" -ForegroundColor Cyan
    Test-ScopeyIpAvailability -Address $IPAddress -TcpPorts $profile.Ports | Format-List
}

$description = New-ScopeyReservationDescription -Profile $profile -Name $DeviceName

Write-Host ''
Write-Host 'Reservation summary' -ForegroundColor Cyan
Write-Host " DHCP Server : $DhcpServer"
Write-Host " Scope ID    : $ScopeId"
Write-Host " Device Type : $DeviceType"
Write-Host " Device Name : $DeviceName"
Write-Host " IP Address  : $IPAddress"
Write-Host " Client ID   : $ClientId"
Write-Host " Description : $description"

$confirm = Read-Host 'Create this DHCP reservation? [Y/N]'

if ($confirm -notmatch '^[Yy]') {
    Write-Host 'Reservation cancelled.' -ForegroundColor Yellow
    return
}

$params = @{
    ScopeId = $ScopeId
    IPAddress = $IPAddress
    ClientId = $ClientId
    Name = $DeviceName
    Description = $description
}

if ($PSCmdlet.ShouldProcess("$IPAddress for $DeviceName", 'Create DHCP reservation')) {
    Invoke-ScopeyDhcpCommand -CommandName Add-DhcpServerv4Reservation -Parameters $params
    Write-Host "Reservation created for $DeviceName at $IPAddress." -ForegroundColor Green
}
