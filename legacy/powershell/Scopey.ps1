<#
.SYNOPSIS
  Scopey - a cartoony DHCP scope helper for Windows DHCP admins.

.DESCRIPTION
  Scopey manages common IPv4 Microsoft DHCP Server tasks from an admin workstation
  or directly from the DHCP server. It uses the DhcpServer PowerShell module and
  supports remote DHCP servers through the -DhcpServer parameter or a local server
  inventory file.

.PARAMETER DhcpServer
  DHCP server to manage. If omitted, Scopey prompts you to select from the known
  server inventory or type a server manually.

.PARAMETER Credential
  Optional credential used for DHCP Server cmdlets that support -Credential.

.PARAMETER ServerListPath
  Optional path to a JSON file containing known DHCP servers.

.EXAMPLE
  .\Scopey.ps1 -DhcpServer DHCP01

.EXAMPLE
  .\Scopey.ps1 -DhcpServer DHCP01 -Credential (Get-Credential)

.EXAMPLE
  .\Scopey.ps1 -ServerListPath .\Scopey.Servers.json

.NOTES
  Forked from DHCP_Scope_Manager by Flemming Sørvollen Skaret.
  Scopey modernization adds remote server targeting, known DHCP server selection,
  and intelligent IP availability checks.
#>

[CmdletBinding()]
param(
    [Parameter()]
    [string]$DhcpServer,

    [Parameter()]
    [System.Management.Automation.PSCredential]$Credential,

    [Parameter()]
    [string]$ServerListPath = (Join-Path -Path $PSScriptRoot -ChildPath 'Scopey.Servers.json')
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$script:FirstRun = $true
$script:SelectedScope = $null
$script:SelectedScopeId = $null
$script:DhcpServer = $DhcpServer
$script:Credential = $Credential
$script:ServerListPath = $ServerListPath

function Import-ScopeyDhcpModule {
    try {
        Import-Module DhcpServer -ErrorAction Stop
    }
    catch {
        Write-Host 'ERROR: Unable to import the DhcpServer PowerShell module.' -ForegroundColor Red
        Write-Host 'Install RSAT DHCP tools on this workstation or run from a machine with the DHCP Server tools installed.' -ForegroundColor Yellow
        Read-Host 'Press ENTER to exit'
        exit 1
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
        $boundParameters['ComputerName'] = $script:DhcpServer
    }

    if ($script:Credential -and $command.Parameters.ContainsKey('Credential')) {
        $boundParameters['Credential'] = $script:Credential
    }

    & $CommandName @boundParameters
}

function Get-ScopeyServerInventory {
    if (-not (Test-Path -Path $script:ServerListPath)) {
        return @()
    }

    try {
        $servers = Get-Content -Path $script:ServerListPath -Raw | ConvertFrom-Json
        if (-not $servers) { return @() }
        return @($servers)
    }
    catch {
        Write-Host "Unable to read server inventory at $($script:ServerListPath)." -ForegroundColor Yellow
        Write-Host $_.Exception.Message -ForegroundColor DarkYellow
        return @()
    }
}

function Save-ScopeyServerInventory {
    param(
        [Parameter(Mandatory)]
        [object[]]$Servers
    )

    try {
        $folder = Split-Path -Path $script:ServerListPath -Parent
        if ($folder -and -not (Test-Path -Path $folder)) {
            New-Item -Path $folder -ItemType Directory -Force | Out-Null
        }

        $Servers | ConvertTo-Json -Depth 5 | Set-Content -Path $script:ServerListPath -Encoding UTF8
    }
    catch {
        Write-Host "Unable to save server inventory at $($script:ServerListPath)." -ForegroundColor Red
        Write-Host $_.Exception.Message -ForegroundColor DarkYellow
    }
}

function Add-ScopeyKnownServer {
    $name = Read-Host 'DHCP server name or FQDN'
    if (-not $name) {
        Write-Host 'No server entered.' -ForegroundColor Yellow
        Show-ScopeyStartupServerPicker
        return
    }

    $location = Read-Host 'Optional location label, such as HQ, Public Safety, Site 2, Lab'
    $description = Read-Host 'Optional description'

    $servers = Get-ScopeyServerInventory
    $existing = $servers | Where-Object { $_.Name -eq $name }

    if ($existing) {
        Write-Host "Server $name already exists in the inventory." -ForegroundColor Yellow
    }
    else {
        $servers += [PSCustomObject]@{
            Name = $name
            Location = $location
            Description = $description
        }

        Save-ScopeyServerInventory -Servers $servers
        Write-Host "Added $name to the known DHCP server inventory." -ForegroundColor Green
    }

    Show-ScopeyStartupServerPicker
}

function Show-ScopeyStartupServerPicker {
    $servers = Get-ScopeyServerInventory

    Write-Host ''
    Write-Host '+-------------------------------------------------------+' -ForegroundColor Magenta
    Write-Host ' Scopey - Select DHCP Server'
    Write-Host '+-------------------------------------------------------+' -ForegroundColor Magenta

    if ($servers.Count -gt 0) {
        for ($i = 0; $i -lt $servers.Count; $i++) {
            $server = $servers[$i]
            $label = if ($server.Location) { "[$($server.Location)]" } else { '[No location]' }
            Write-Host " $($i + 1) - $($server.Name) $label $($server.Description)"
        }
    }
    else {
        Write-Host ' No known DHCP servers saved yet.' -ForegroundColor Yellow
    }

    Write-Host ' M - Manually type a DHCP server'
    Write-Host ' A - Add a DHCP server to known servers'
    Write-Host ' L - Use local computer'
    Write-Host ' Q - Quit'
    Write-Host '+-------------------------------------------------------+' -ForegroundColor Magenta

    $choice = Read-Host 'Select option'

    if ($choice -match '^\d+$') {
        $index = [int]$choice - 1
        if ($index -ge 0 -and $index -lt $servers.Count) {
            $script:DhcpServer = $servers[$index].Name
            return
        }
    }

    switch ($choice.ToUpperInvariant()) {
        'M' {
            $manualServer = Read-Host 'Enter DHCP server name, FQDN, or IP'
            if ($manualServer) { $script:DhcpServer = $manualServer; return }
        }
        'A' { Add-ScopeyKnownServer; return }
        'L' { $script:DhcpServer = $env:COMPUTERNAME; return }
        'Q' { exit }
    }

    Write-Host 'Invalid selection.' -ForegroundColor Red
    Show-ScopeyStartupServerPicker
}

function Select-ScopeyDhcpServer {
    if ($script:DhcpServer) { return }
    Show-ScopeyStartupServerPicker
}

function Set-ScopeyWindowTitle {
    $scopeText = if ($script:SelectedScopeId) { "[$($script:SelectedScopeId)] $($script:SelectedScope.Name)" } else { 'None selected' }
    $host.UI.RawUI.WindowTitle = "Scopey - DHCP Server: $($script:DhcpServer) - Selected Scope: $scopeText"
}

function Test-ScopeyDhcpServerConnection {
    try {
        Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4Scope -Parameters @{ ErrorAction = 'Stop' } | Out-Null
        Write-Host "Connected to DHCP server: $($script:DhcpServer)" -ForegroundColor Green
    }
    catch {
        Write-Host "ERROR: Unable to query DHCP server '$($script:DhcpServer)'." -ForegroundColor Red
        Write-Host 'Verify network reachability, permissions, firewall rules, and that the DHCP Server service/tools are available.' -ForegroundColor Yellow
        Write-Host "Details: $($_.Exception.Message)" -ForegroundColor DarkYellow
        Read-Host 'Press ENTER to return to server selection'
        $script:DhcpServer = $null
        Select-ScopeyDhcpServer
        Test-ScopeyDhcpServerConnection
    }
}

function Select-ScopeyScope {
    Write-Host ''
    Write-Host "Available scopes on $($script:DhcpServer):" -ForegroundColor Cyan

    try {
        Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4Scope |
            Select-Object ScopeId, Name, StartRange, EndRange, State |
            Format-Table -AutoSize
    }
    catch {
        Write-Host 'Unable to list scopes.' -ForegroundColor Red
    }

    $scopeInput = Read-Host 'Select ScopeID'

    try {
        $scope = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4Scope -Parameters @{ ScopeId = $scopeInput }
        $stats = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4ScopeStatistics -Parameters @{ ScopeId = $scopeInput }

        $script:SelectedScope = $scope
        $script:SelectedScopeId = $scope.ScopeId.IPAddressToString

        Write-Host "Scope [$($script:SelectedScopeId)] $($scope.Name) selected" -ForegroundColor Green
        Write-Host "IP Range: $($scope.StartRange.IPAddressToString) - $($scope.EndRange.IPAddressToString)" -ForegroundColor Yellow
        Write-Host "Subnet Mask: $($scope.SubnetMask.IPAddressToString)" -ForegroundColor Yellow
        Write-Host "Addresses in use: $($stats.AddressesInUse) - $($stats.PercentageInUse)%" -ForegroundColor Yellow
    }
    catch {
        Write-Host 'Unable to select scope.' -ForegroundColor Red
        $script:SelectedScope = $null
        $script:SelectedScopeId = $null
    }
    finally {
        Set-ScopeyWindowTitle
        if ($script:SelectedScopeId) { Show-ScopeyMenu } else { Select-ScopeyScope }
    }
}

function Invoke-ScopeyFailoverReplication {
    try {
        Invoke-ScopeyDhcpCommand -CommandName Invoke-DhcpServerv4FailoverReplication -Parameters @{
            ScopeId = $script:SelectedScopeId
            Force = $true
        } | Out-Null

        $failover = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4Failover
        $partnerServer = ($failover | Select-Object -First 1).PartnerServer
        Write-Host "Scope $($script:SelectedScopeId) replicated successfully to failover partner $partnerServer" -ForegroundColor Green
    }
    catch {
        Write-Host 'An error occurred during replication.' -ForegroundColor Red
        Write-Host $_.Exception.Message -ForegroundColor DarkYellow
    }
    finally { Show-ScopeyMenu }
}

function Show-ScopeyFreeIpAddresses {
    try {
        $stats = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4ScopeStatistics -Parameters @{ ScopeId = $script:SelectedScopeId }

        if ($stats.Free -eq 0) {
            Write-Host 'There are no available IP addresses in this scope.' -ForegroundColor Yellow
        }
        else {
            Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4FreeIPAddress -Parameters @{
                ScopeId = $script:SelectedScopeId
                NumAddress = $stats.Free
            }
        }
    }
    catch {
        Write-Host 'An error occurred while listing free IP addresses.' -ForegroundColor Red
        Write-Host $_.Exception.Message -ForegroundColor DarkYellow
    }
    finally { Show-ScopeyMenu }
}

function Show-ScopeyLeasesByState {
    param(
        [Parameter(Mandatory)]
        [string]$AddressState,

        [Parameter(Mandatory)]
        [string]$EmptyMessage
    )

    try {
        $result = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4Lease -Parameters @{
            ScopeId = $script:SelectedScopeId
            AllLeases = $true
        } | Where-Object AddressState -eq $AddressState | Select-Object IPAddress, ClientId, HostName, AddressState

        if (-not $result) { Write-Host $EmptyMessage -ForegroundColor Yellow }
        else { $result | Format-Table -AutoSize }
    }
    catch {
        Write-Host 'An error occurred while listing leases.' -ForegroundColor Red
        Write-Host $_.Exception.Message -ForegroundColor DarkYellow
    }
    finally { Show-ScopeyMenu }
}

function Add-ScopeyReservation {
    $suggestion = $null

    try {
        $stats = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4ScopeStatistics -Parameters @{ ScopeId = $script:SelectedScopeId }
        if ($stats.Free -ne 0) {
            $freeIp = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4FreeIPAddress -Parameters @{ ScopeId = $script:SelectedScopeId }
            $suggestion = " (example free IP: $freeIp)"
        }
    }
    catch { }

    $ipAddress = Read-Host "Select an IP address$suggestion in range $($script:SelectedScope.StartRange.IPAddressToString) - $($script:SelectedScope.EndRange.IPAddressToString)"
    $clientId = Read-Host "Select a ClientID/MAC for $ipAddress"
    $description = Read-Host 'Optional reservation description'

    try {
        $params = @{
            ScopeId = $script:SelectedScopeId
            IPAddress = $ipAddress
            ClientId = $clientId
        }

        if ($description) { $params['Description'] = $description }

        Invoke-ScopeyDhcpCommand -CommandName Add-DhcpServerv4Reservation -Parameters $params
        Write-Host "The IP address $ipAddress was successfully reserved for $clientId." -ForegroundColor Green
    }
    catch {
        Write-Host 'An error occurred. Make sure the IP and MAC address are free and valid.' -ForegroundColor Red
        Write-Host $_.Exception.Message -ForegroundColor DarkYellow
    }
    finally { Show-ScopeyMenu }
}

function Remove-ScopeyReservationByIp {
    $ipAddress = Read-Host 'Select an IP address in the scope to delete the reservation for'

    try {
        $reservation = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4Reservation -Parameters @{ IPAddress = $ipAddress }

        if ($reservation.ScopeId.IPAddressToString -ne $script:SelectedScopeId) {
            Write-Host 'The selected IP address is not inside the selected scope.' -ForegroundColor Yellow
        }
        else {
            Invoke-ScopeyDhcpCommand -CommandName Remove-DhcpServerv4Reservation -Parameters @{ IPAddress = $ipAddress }
            Write-Host "The reservation of $ipAddress was successfully removed from $($reservation.ClientId)." -ForegroundColor Green
        }
    }
    catch {
        Write-Host 'The selected IP address reservation was not found on this DHCP server.' -ForegroundColor Yellow
        Write-Host $_.Exception.Message -ForegroundColor DarkYellow
    }
    finally { Show-ScopeyMenu }
}

function Remove-ScopeyReservationByClientId {
    $clientId = Read-Host 'Select a ClientID/MAC address in the scope to delete the reservation for'

    try {
        $reservation = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4Reservation -Parameters @{
            ScopeId = $script:SelectedScopeId
            ClientId = $clientId
        }

        Invoke-ScopeyDhcpCommand -CommandName Remove-DhcpServerv4Reservation -Parameters @{ IPAddress = $reservation.IPAddress.IPAddressToString }
        Write-Host "The reservation of $($reservation.IPAddress.IPAddressToString) was successfully removed from $clientId." -ForegroundColor Green
    }
    catch {
        Write-Host 'The selected ClientID was not found in this scope.' -ForegroundColor Yellow
        Write-Host $_.Exception.Message -ForegroundColor DarkYellow
    }
    finally { Show-ScopeyMenu }
}

function Find-ScopeyLeaseByClientId {
    $clientId = Read-Host 'Select a ClientID/MAC address in this scope to find the related lease'

    try {
        Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4Lease -Parameters @{
            ScopeId = $script:SelectedScopeId
            ClientId = $clientId
        } | Select-Object IPAddress, ClientId, HostName, AddressState | Format-Table -AutoSize
    }
    catch {
        Write-Host 'No lease for the specified ClientID was found in this scope.' -ForegroundColor Yellow
    }
    finally { Show-ScopeyMenu }
}

function Find-ScopeyLeaseByIp {
    $ipAddress = Read-Host 'Select an IP address in this scope to find the related lease'

    try {
        $result = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4Lease -Parameters @{ IPAddress = $ipAddress } |
            Where-Object { $_.ScopeId.IPAddressToString -eq $script:SelectedScopeId }

        if ($result) {
            $result | Select-Object IPAddress, ClientId, HostName, AddressState | Format-Table -AutoSize
        }
        else {
            Write-Host 'No lease for the specified IP was found in this scope.' -ForegroundColor Yellow
        }
    }
    catch {
        Write-Host 'No lease for the specified IP was found in this scope.' -ForegroundColor Yellow
    }
    finally { Show-ScopeyMenu }
}

function Test-ScopeyIpAvailability {
    param(
        [Parameter(Mandatory)]
        [string]$IPAddress,

        [Parameter()]
        [int[]]$TcpPorts = @(80, 443, 515, 631, 9100)
    )

    $lease = $null
    $reservation = $null
    $dnsName = $null
    $pingResponded = $false
    $openPorts = @()
    $signals = New-Object System.Collections.Generic.List[string]

    try {
        $lease = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4Lease -Parameters @{ IPAddress = $IPAddress }
    }
    catch { }

    try {
        $reservation = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4Reservation -Parameters @{ IPAddress = $IPAddress }
    }
    catch { }

    try {
        $dnsResult = Resolve-DnsName -Name $IPAddress -ErrorAction Stop | Select-Object -First 1
        if ($dnsResult.NameHost) { $dnsName = $dnsResult.NameHost }
        elseif ($dnsResult.Name) { $dnsName = $dnsResult.Name }
    }
    catch { }

    try {
        $pingResponded = Test-Connection -ComputerName $IPAddress -Count 1 -Quiet -ErrorAction SilentlyContinue
    }
    catch { $pingResponded = $false }

    foreach ($port in $TcpPorts) {
        try {
            $tcp = Test-NetConnection -ComputerName $IPAddress -Port $port -InformationLevel Quiet -WarningAction SilentlyContinue
            if ($tcp) { $openPorts += $port }
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
        IPAddress = $IPAddress
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

function Find-ScopeyLikelyAvailableIp {
    $countInput = Read-Host 'How many likely available addresses should Scopey find? [Default: 5]'
    $desiredCount = if ($countInput -match '^\d+$') { [int]$countInput } else { 5 }

    $scanPortsInput = Read-Host 'Probe common printer/web ports too? This is slower. [Y/N, Default: N]'
    $tcpPorts = if ($scanPortsInput -match '^[Yy]') { @(80, 443, 515, 631, 9100) } else { @() }

    try {
        $stats = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4ScopeStatistics -Parameters @{ ScopeId = $script:SelectedScopeId }
        if ($stats.Free -eq 0) {
            Write-Host 'There are no DHCP-free addresses in this scope.' -ForegroundColor Yellow
            Show-ScopeyMenu
            return
        }

        $candidateCount = [Math]::Min([int]$stats.Free, [Math]::Max($desiredCount * 5, 10))
        $candidates = Invoke-ScopeyDhcpCommand -CommandName Get-DhcpServerv4FreeIPAddress -Parameters @{
            ScopeId = $script:SelectedScopeId
            NumAddress = $candidateCount
        }

        $results = foreach ($candidate in $candidates) {
            Test-ScopeyIpAvailability -IPAddress $candidate.IPAddressToString -TcpPorts $tcpPorts
        }

        $results |
            Sort-Object @{ Expression = 'Confidence'; Descending = $true }, IPAddress |
            Select-Object -First $desiredCount |
            Format-Table IPAddress, Status, Confidence, DNSName, Ping, OpenTcpPorts -AutoSize
    }
    catch {
        Write-Host 'An error occurred while searching for likely available IP addresses.' -ForegroundColor Red
        Write-Host $_.Exception.Message -ForegroundColor DarkYellow
    }
    finally { Show-ScopeyMenu }
}

function Test-ScopeySpecificIp {
    $ipAddress = Read-Host 'Enter IP address to test'
    if (-not $ipAddress) {
        Write-Host 'No IP address entered.' -ForegroundColor Yellow
        Show-ScopeyMenu
        return
    }

    $scanPortsInput = Read-Host 'Probe common printer/web ports too? [Y/N, Default: Y]'
    $tcpPorts = if ($scanPortsInput -match '^[Nn]') { @() } else { @(80, 443, 515, 631, 9100) }

    try {
        Test-ScopeyIpAvailability -IPAddress $ipAddress -TcpPorts $tcpPorts | Format-List
    }
    catch {
        Write-Host 'An error occurred while testing the IP address.' -ForegroundColor Red
        Write-Host $_.Exception.Message -ForegroundColor DarkYellow
    }
    finally { Show-ScopeyMenu }
}

function Switch-ScopeyDhcpServer {
    $script:SelectedScope = $null
    $script:SelectedScopeId = $null
    $script:FirstRun = $true
    $script:DhcpServer = $null

    Show-ScopeyStartupServerPicker
    Test-ScopeyDhcpServerConnection
    Select-ScopeyScope
}

function Show-ScopeyServerInfo {
    Write-Host ''
    Write-Host "Scopey server target: $($script:DhcpServer)" -ForegroundColor Cyan
    Write-Host "Running from: $env:COMPUTERNAME" -ForegroundColor Cyan
    Write-Host "Credential supplied: $([bool]$script:Credential)" -ForegroundColor Cyan
    Write-Host "Server inventory path: $($script:ServerListPath)" -ForegroundColor Cyan
    Show-ScopeyMenu
}

function Show-ScopeyMenu {
    Set-ScopeyWindowTitle

    if (-not $script:FirstRun) {
        Read-Host 'Press ENTER to continue to menu'
    }

    $script:FirstRun = $false

    Write-Host ''
    Write-Host '+-------------------------------------------------------+' -ForegroundColor Magenta
    Write-Host ' Scopey - DHCP Scope Helper'
    Write-Host " DHCP Server: $($script:DhcpServer)"
    Write-Host " Selected Scope: [$($script:SelectedScopeId)] $($script:SelectedScope.Name)"
    Write-Host '+-------------------------------------------------------+' -ForegroundColor Magenta
    Write-Host ' 0  - Exit'
    Write-Host ' 1  - Select a New Scope'
    Write-Host ' 2  - Replicate Scope'
    Write-Host ' 3  - List all free IP addresses'
    Write-Host ' 4  - List all active IP addresses which are not reserved'
    Write-Host ' 5  - List all active IP addresses which are reserved'
    Write-Host ' 6  - List all inactive IP addresses which are reserved'
    Write-Host ' 7  - Reserve IP address'
    Write-Host ' 8  - Remove reservation by IP'
    Write-Host ' 9  - Remove reservation by ClientID'
    Write-Host ' 10 - Find lease by ClientID'
    Write-Host ' 11 - Find lease by IP'
    Write-Host ' 12 - Show server connection info'
    Write-Host ' 13 - Find likely available IP addresses'
    Write-Host ' 14 - Test specific IP availability'
    Write-Host ' 15 - Switch DHCP server'
    Write-Host '+-------------------------------------------------------+' -ForegroundColor Magenta

    $choice = Read-Host 'Select alternative'

    switch ($choice) {
        '0'  { exit }
        '1'  { Select-ScopeyScope }
        '2'  { Invoke-ScopeyFailoverReplication }
        '3'  { Show-ScopeyFreeIpAddresses }
        '4'  { Show-ScopeyLeasesByState -AddressState Active -EmptyMessage 'There are no active unreserved IP addresses in this scope.' }
        '5'  { Show-ScopeyLeasesByState -AddressState ActiveReservation -EmptyMessage 'There are no active reserved IP addresses in this scope.' }
        '6'  { Show-ScopeyLeasesByState -AddressState InactiveReservation -EmptyMessage 'There are no inactive reserved IP addresses in this scope.' }
        '7'  { Add-ScopeyReservation }
        '8'  { Remove-ScopeyReservationByIp }
        '9'  { Remove-ScopeyReservationByClientId }
        '10' { Find-ScopeyLeaseByClientId }
        '11' { Find-ScopeyLeaseByIp }
        '12' { Show-ScopeyServerInfo }
        '13' { Find-ScopeyLikelyAvailableIp }
        '14' { Test-ScopeySpecificIp }
        '15' { Switch-ScopeyDhcpServer }
        default {
            Write-Host 'Invalid choice.' -ForegroundColor Red
            Show-ScopeyMenu
        }
    }
}

Import-ScopeyDhcpModule
Select-ScopeyDhcpServer
Set-ScopeyWindowTitle
Test-ScopeyDhcpServerConnection
Select-ScopeyScope
