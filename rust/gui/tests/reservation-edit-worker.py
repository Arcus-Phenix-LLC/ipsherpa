# All cmdlets below are synthetic; no module import, network or live write.
import pathlib,subprocess,json,os,runpy
base=runpy.run_path('gui/tests/reservation-worker.py') # also runs all 15 creation regressions
script=base['script'];exe=base['exe']
fixtures=r'''
$global:writes=0
function Import-Module {param($Name,$ErrorAction)}
function Get-DhcpServerv4Scope {param($ComputerName,$ScopeId,$ErrorAction) [pscustomobject]@{ScopeId='192.0.2.0';SubnetMask='255.255.255.0';StartRange='192.0.2.10';EndRange='192.0.2.200'}}
function Get-DhcpServerv4ExclusionRange {param($ComputerName,$ScopeId,$ErrorAction)}
function Get-DhcpServerv4Lease {param($ComputerName,$ScopeId,[switch]$AllLeases,$ErrorAction) if($env:TEST_CASE -eq 'leaseconflict'){[pscustomobject]@{IPAddress='192.0.2.25';ClientId='02-00-00-00-00-01';AddressState='Active'}}}
function Get-DhcpServerv4Reservation {param($ComputerName,$ScopeId,$ErrorAction)
 if($global:writes -gt 0){if($env:TEST_CASE -eq 'verifyfail'){throw 'Synthetic verification failed'};$name=$script:updatedName;if($env:TEST_CASE -eq 'mismatch'){$name='Mismatch'};[pscustomobject]@{IPAddress='192.0.2.25';ClientId='02-AA-BB-CC-DD-EE';Name=$name;Description=$script:updatedDescription;Type='Dhcp'};return}
 if($env:TEST_CASE -eq 'missing'){return}
 $id='02-AA-BB-CC-DD-EE';$name='Before';$description='Previous';$type='Dhcp'
 if($env:TEST_CASE -eq 'identity'){$id='02-00-00-00-00-01'}
 if($env:TEST_CASE -eq 'stale'){$name='Another admin'}
 if($env:TEST_CASE -eq 'descstale'){$description='Another description'}
 if($env:TEST_CASE -eq 'type'){$type='Both'}
 [pscustomobject]@{IPAddress='192.0.2.25';ClientId=$id;Name=$name;Description=$description;Type=$type}
 if($env:TEST_CASE -eq 'duplicate'){[pscustomobject]@{IPAddress='192.0.2.99';ClientId=$id;Name='Other';Description='';Type='Dhcp'}}
}
function Add-DhcpServerv4Reservation {throw 'Edit must never call Add'}
function Set-DhcpServerv4Reservation {param($ComputerName,$IPAddress,$Name,$Description,$Confirm,$ErrorAction) $global:writes++;$script:updatedName=$Name;$script:updatedDescription=$Description;if($env:TEST_CASE -eq 'writefail'){throw 'Synthetic write error'}}
'''
request={'server':'EXAMPLE-DHCP','scopeId':'192.0.2.0','ipAddress':'192.0.2.25','mac':'02-AA-BB-CC-DD-EE','name':'Updated 東京','description':'New $() ` "','original':{'name':'Before','description':'Previous','clientType':'DHCP'}}
cases=[('ok','preview','ready',0),('ok','create','updated',1),('missing','create','blocked',0),('identity','create','blocked',0),('stale','preview','blocked',0),('stale','create','blocked',0),('descstale','create','blocked',0),('type','create','blocked',0),('duplicate','create','blocked',0),('leaseconflict','create','blocked',0),('writefail','create','unknown',1),('verifyfail','create','unknown',1),('mismatch','create','unknown',1),('noop','create','blocked',0),('clear','create','updated',1)]
for case,phase,status,writes in cases:
 r=json.loads(json.dumps(request));
 if case=='noop':r['name']='Before';r['description']='Previous'
 if case=='clear':r['description']=''
 env=os.environ.copy();env['TEST_CASE']=case
 command=fixtures+'\n$result=& {\n'+script+'\n}\n@{result=($result|ConvertFrom-Json);writes=$global:writes}|ConvertTo-Json -Depth 5 -Compress'
 p=subprocess.run([str(exe),'-NoProfile','-NonInteractive','-Command',command],input=json.dumps({'phase':phase,'request':r},ensure_ascii=False).encode(),capture_output=True,env=env,creationflags=subprocess.CREATE_NO_WINDOW,timeout=15)
 assert p.returncode==0,(case,p.stderr.decode(errors='replace'))
 result=json.loads(p.stdout.decode('utf-8-sig'));assert result['result']['status']==status,(case,result);assert result['writes']==writes,(case,result)
print('15 synthetic edit cases passed, plus 15 creation regressions')
