$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
[Console]::InputEncoding = [System.Text.UTF8Encoding]::new($false)
$stage='checking'
function Reply($status,$message) { @{status=$status;message=$message} | ConvertTo-Json -Compress }
function IPNumber($value) { $b=([ipaddress]$value).GetAddressBytes(); [uint64]$b[0]*16777216+[uint64]$b[1]*65536+[uint64]$b[2]*256+[uint64]$b[3] }
function Client($value) { ([string]$value -replace '[-:.]','').ToUpperInvariant() }
try {
 $inputData=[Console]::In.ReadToEnd() | ConvertFrom-Json
 Import-Module DhcpServer -ErrorAction Stop
 $r=$inputData.request
 $scope=Get-DhcpServerv4Scope -ComputerName $r.server -ScopeId $r.scopeId -ErrorAction Stop
 $ip=IPNumber $r.ipAddress
 $mask=IPNumber $scope.SubnetMask
 $network=IPNumber $scope.ScopeId
 $broadcast=$network + (4294967295 - $mask)
 if ($ip -le $network -or $ip -ge $broadcast -or $ip -lt (IPNumber $scope.StartRange) -or $ip -gt (IPNumber $scope.EndRange)) { throw 'Address must be a usable address inside the selected scope allocation range.' }
 $excluded=@(Get-DhcpServerv4ExclusionRange -ComputerName $r.server -ScopeId $r.scopeId -ErrorAction Stop)
 foreach($range in $excluded) { if($ip -ge (IPNumber $range.StartRange) -and $ip -le (IPNumber $range.EndRange)){throw 'Address is in an exclusion range. This workflow does not change exclusions.'} }
 $reservations=@(Get-DhcpServerv4Reservation -ComputerName $r.server -ScopeId $r.scopeId -ErrorAction Stop)
 foreach($existing in $reservations) {
  if((IPNumber $existing.IPAddress) -eq $ip){throw 'A reservation already exists at this address. No changes were made.'}
  if((Client $existing.ClientId) -eq (Client $r.mac)){throw 'This MAC already has a reservation in this scope. No changes were made.'}
 }
 $leases=@(Get-DhcpServerv4Lease -ComputerName $r.server -ScopeId $r.scopeId -AllLeases -ErrorAction Stop)
 foreach($lease in $leases){
  $sameIP=(IPNumber $lease.IPAddress) -eq $ip
  $sameClient=(Client $lease.ClientId) -eq (Client $r.mac)
  if($sameIP -and (!$sameClient -or [string]$lease.AddressState -notin @('Active','ActiveReservation'))){throw 'The requested address has a conflicting or non-active lease record.'}
  if($sameClient -and !$sameIP){throw 'This MAC has a lease at another address in this scope. Automatic lease moves are not supported.'}
 }
 if($inputData.phase -eq 'preview'){Reply 'ready' 'Scope range, exclusions, reservations and all lease states were checked. No DHCP changes made.';return}
 if($inputData.phase -ne 'create'){throw 'Unknown reservation operation.'}
 $stage='writing'
 Add-DhcpServerv4Reservation -ComputerName $r.server -ScopeId $r.scopeId -IPAddress $r.ipAddress -ClientId $r.mac -Name $r.name -Description $r.description -Type Dhcp -Confirm:$false -ErrorAction Stop | Out-Null
 $stage='verifying'
 $check=@(Get-DhcpServerv4Reservation -ComputerName $r.server -ScopeId $r.scopeId -ErrorAction Stop | Where-Object { (IPNumber $_.IPAddress) -eq $ip })
 if($check.Count -ne 1 -or (Client $check[0].ClientId) -ne (Client $r.mac) -or [string]$check[0].Name -cne [string]$r.name -or [string]$check[0].Description -cne [string]$r.description -or [string]$check[0].Type -ne 'Dhcp'){throw 'Read-back did not match the requested reservation.'}
 Reply 'created' 'Reservation created and verified on the selected server. Refresh reservations to load the current table.'
} catch {
 if($stage -eq 'checking'){Reply 'blocked' $_.Exception.Message}
 else {Reply 'unknown' ('The write may have succeeded. Inspect DHCP Manager before trying again. '+$_.Exception.Message)}
}
