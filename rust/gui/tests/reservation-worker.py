# Synthetic cmdlet fixtures. No RSAT module is imported and no network is used.
import pathlib,subprocess,json,os
script=pathlib.Path('gui/src/reservation.ps1').read_text(encoding='utf-8')
exe=pathlib.Path(os.environ['SystemRoot'])/'System32/WindowsPowerShell/v1.0/powershell.exe'
request={'server':'EXAMPLE-DHCP','scopeId':'192.0.2.0','ipAddress':'192.0.2.25','mac':'02-AA-BB-CC-DD-EE','name':'Example 東京','description':'Synthetic $() ` quote "'}
fixtures=r'''
$global:writes=0
function Import-Module {param($Name,$ErrorAction)}
function Get-DhcpServerv4Scope {param($ComputerName,$ScopeId,$ErrorAction) if($env:TEST_CASE -eq 'denied'){throw 'Synthetic denied'};[pscustomobject]@{ScopeId='192.0.2.0';SubnetMask='255.255.255.0';StartRange='192.0.2.10';EndRange='192.0.2.200'}}
function Get-DhcpServerv4ExclusionRange {param($ComputerName,$ScopeId,$ErrorAction) if($env:TEST_CASE -eq 'excluded'){[pscustomobject]@{StartRange='192.0.2.20';EndRange='192.0.2.30'}}}
function Get-DhcpServerv4Reservation {param($ComputerName,$ScopeId,$ErrorAction)
 if($global:writes -gt 0){if($env:TEST_CASE -eq 'verifyfail'){throw 'Synthetic readback failed'};[pscustomobject]@{IPAddress=$r.ipAddress;ClientId=$r.mac;Name=$r.name;Description=$r.description;Type='Dhcp'}}
 elseif($env:TEST_CASE -eq 'existing'){[pscustomobject]@{IPAddress='192.0.2.25';ClientId='02-00-00-00-00-01'}}
 elseif($env:TEST_CASE -eq 'duplicate'){[pscustomobject]@{IPAddress='192.0.2.99';ClientId='02-AA-BB-CC-DD-EE'}}
}
function Get-DhcpServerv4Lease {param($ComputerName,$ScopeId,$AllLeases,$ErrorAction)
 if($env:TEST_CASE -eq 'leaseconflict'){[pscustomobject]@{IPAddress='192.0.2.25';ClientId='02-00-00-00-00-01';AddressState='Active'}}
 elseif($env:TEST_CASE -eq 'badlease'){[pscustomobject]@{IPAddress='192.0.2.25';ClientId='02-AA-BB-CC-DD-EE';AddressState='Declined'}}
 elseif($env:TEST_CASE -eq 'move'){[pscustomobject]@{IPAddress='192.0.2.99';ClientId='02-AA-BB-CC-DD-EE';AddressState='Active'}}
 elseif($env:TEST_CASE -eq 'samelease'){[pscustomobject]@{IPAddress='192.0.2.25';ClientId='02-AA-BB-CC-DD-EE';AddressState='Active'}}
}
function Add-DhcpServerv4Reservation {param($ComputerName,$ScopeId,$IPAddress,$ClientId,$Name,$Description,$Type,$Confirm,$ErrorAction) $global:writes++;if($env:TEST_CASE -eq 'writefail'){throw 'Synthetic write failed'}}
'''.replace('$AllLeases,','[switch]$AllLeases,')
# The fixed production script runs inside a child scope; mocks use its request
# parameters for readback via script scope rather than real server state.
fixtures=fixtures.replace('$r.ipAddress','$script:requested.ipAddress').replace('$r.mac','$script:requested.mac').replace('$r.name','$script:requested.name').replace('$r.description','$script:requested.description')
fixtures=fixtures.replace('$global:writes++;',"$script:requested=[pscustomobject]@{ipAddress=$IPAddress;mac=$ClientId;name=$Name;description=$Description};$global:writes++;")
for case,phase,expected,writes in [('ok','preview','ready',0),('ok','create','created',1),('samelease','create','created',1),('excluded','create','blocked',0),('existing','create','blocked',0),('duplicate','create','blocked',0),('leaseconflict','create','blocked',0),('badlease','create','blocked',0),('move','create','blocked',0),('outside','create','blocked',0),('network','create','blocked',0),('broadcast','create','blocked',0),('denied','create','blocked',0),('writefail','create','unknown',1),('verifyfail','create','unknown',1)]:
 request['ipAddress']={'outside':'192.0.2.250','network':'192.0.2.0','broadcast':'192.0.2.255'}.get(case,'192.0.2.25')
 env=os.environ.copy();env['TEST_CASE']=case
 command=fixtures+'\n$result=& {\n'+script+'\n}\n@{result=($result|ConvertFrom-Json);writes=$global:writes}|ConvertTo-Json -Depth 5 -Compress'
 p=subprocess.run([str(exe),'-NoProfile','-NonInteractive','-Command',command],input=json.dumps({'phase':phase,'request':request},ensure_ascii=False).encode(),capture_output=True,env=env,creationflags=subprocess.CREATE_NO_WINDOW,timeout=15)
 assert p.returncode==0,(case,p.stderr.decode(errors='replace'))
 result=json.loads(p.stdout.decode('utf-8-sig'))
 assert result['result']['status']==expected,(case,result)
 assert result['writes']==writes,(case,result)
print('15 synthetic PowerShell reservation checks passed')
