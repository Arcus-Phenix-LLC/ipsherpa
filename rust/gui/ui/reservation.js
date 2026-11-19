"use strict";
(() => {
 const $=id=>document.getElementById(`reservation-${id}`),dialog=$('dialog'),form=$('form');
 let source=null,token=null,pending=false,finished=false,original=null,sourceLease=null;
 function status(message){$('status').textContent=message;}
 function edit(){token=null;$('confirm').checked=false;$('confirm').disabled=false;$('fields').hidden=false;$('review').hidden=true;$('preview').hidden=false;$('create').hidden=true;$('edit').hidden=true;$('fields').disabled=false;status('');}
 function controls(){document.getElementById('new-reservation').disabled=busy||!selectedScope;for(const id of ['cancel','edit','preview'])$(id).disabled=pending;$('create').disabled=pending||!token||!$('confirm').checked;}
 function close(){if(!pending)dialog.close();}
 function open(record=null,fromLease=false){
  if(busy||!selectedScope||!connectedServer)return;
  source={server:connectedServer,scopeId:selectedScope.scopeId};form.reset();sourceLease=fromLease?{clientId:record.clientId}:null;original=record&&!fromLease?{name:record.name,description:record.description,clientType:record.clientType}:null;finished=false;edit();
  $('ip').readOnly=Boolean(record);$('mac').readOnly=Boolean(record);
  if(record){$('ip').value=record.ipAddress;$('mac').value=fromLease?record.clientId:reservationMac(record.clientId)||record.clientId;$('name').value=record.name;$('description').value=record.description;}
  $('title').textContent=fromLease?'Add lease to reservation':record?'Edit reservation metadata':'New DHCP reservation';
  $('review-title').textContent=fromLease?'Review reservation from selected lease':record?'Review name and description changes':'Review DHCP-only reservation';
  $('confirm-label').textContent=original?'I confirm these changes on the server and scope shown above.':'I confirm this reservation on the server and scope shown above.';
  $('create').textContent=original?'Apply changes':'Create reservation';
  $('help').textContent=fromLease?'Uses this lease’s current address and full DHCP client ID. Both are fixed. Name and description are prefilled and can be adjusted. The lease must still be active with the same identity when you review and create.':record?'Only name and description can change. IP, MAC, DHCP type and reservation options stay fixed. The current reservation is checked against the loaded values before review and again before applying.':'Checks apply to the selected scope on this server. They do not detect static devices, other servers or failover replication. No existing leases, reservations or exclusions will be changed.';
  $('identity-label').textContent=fromLease?'DHCP client ID (from lease)':'MAC address (six bytes)';
  $('cancel').textContent='Cancel';$('source').textContent=`${source.server} · ${source.scopeId}`;controls();dialog.showModal();
 }
 document.getElementById('new-reservation').addEventListener('click',()=>open());
 $('cancel').addEventListener('click',close);dialog.addEventListener('cancel',event=>{if(pending)event.preventDefault();});
 $('edit').addEventListener('click',()=>{edit();controls();});$('confirm').addEventListener('change',controls);
 form.addEventListener('submit',async event=>{
  event.preventDefault();if(pending||finished||token)return;
  const request={...source,ipAddress:$('ip').value.trim(),mac:$('mac').value.trim(),name:$('name').value.trim(),description:$('description').value.trim(),...(original?{original}: {}),...(sourceLease?{sourceLease}: {})};
  pending=true;setBusy(true);$('fields').disabled=true;status('Checking current DHCP data…');
  try{
   const result=await window.__TAURI__.core.invoke('preview_reservation',{request});
   if(result.request.server!==source.server||result.request.scopeId!==source.scopeId)throw new Error('Review source did not match the selected scope.');
   status(result.outcome.message);
   if(result.outcome.status==='ready'&&result.token){token=result.token;$('summary').replaceChildren();for(const [label,value] of [['Server',result.request.server],['Scope',result.request.scopeId],['IP',result.request.ipAddress],[sourceLease?'DHCP client ID':'MAC',result.request.mac],...(original?[['Name before',result.request.original.name||'(none)'],['Name after',result.request.name],['Description before',result.request.original.description||'(none)'],['Description after',result.request.description||'(none)']]:[['Name',result.request.name],['Description',result.request.description||'(none)']]),['Type',original?result.request.original.clientType:'DHCP only']]){const dt=document.createElement('dt'),dd=document.createElement('dd');dt.textContent=label;dd.textContent=value;$('summary').append(dt,dd);}$('fields').hidden=true;$('review').hidden=false;$('preview').hidden=true;$('create').hidden=false;$('edit').hidden=false;}
  }catch(error){status(String(error));}
  finally{pending=false;$('fields').disabled=false;setBusy(false);controls();}
 });
 $('create').addEventListener('click',async()=>{
  if(pending||!token||!$('confirm').checked||finished)return;
  const reviewed=token;token=null;pending=true;setBusy(true);status(original?'Rechecking current reservation, applying and verifying…':'Rechecking conflicts, creating and verifying…');
  let result;
  try{result=await window.__TAURI__.core.invoke('create_reservation',{token:reviewed});}
  catch(error){result={status:'unknown',message:`Outcome unknown. Inspect DHCP Manager before trying again. ${String(error)}`};}
  finally{pending=false;finished=true;recordCache={};recordsLoaded=false;recordData=[];recordBody.replaceChildren();recordStatus.textContent="Reservation operation completed. Refresh records to reload server data.";emptyRecords("Refresh required", "The record snapshots have been cleared. Use Refresh records to load current data.");window.ScopeCapacity?.selectionChanged(connectedServer,selectedScope);setBusy(false);}
  status(result.message);$('create').hidden=true;$('edit').hidden=true;$('preview').hidden=true;$('cancel').textContent='Close';$('confirm').disabled=true;controls();
 });
 const menu=document.getElementById('lease-menu'),menuAction=document.getElementById('lease-menu-reserve'),menuNote=document.getElementById('lease-menu-note');
 let menuRecord=null,menuFocus=null;
 function hideMenu(restore=false){menu.hidden=true;if(restore)menuFocus?.focus();menuRecord=null;}
 function contextMenu(event,record,row){
  event.preventDefault();if(busy)return;
  menuRecord=record;menuFocus=document.activeElement;
  const problem=leaseReservationProblem(record);menuAction.disabled=Boolean(problem);menuNote.textContent=problem;menuNote.hidden=!problem;
  menu.hidden=false;
  const rect=row.getBoundingClientRect(),bounds=menu.getBoundingClientRect();
  const x=event.type==='keydown'?rect.left+16:event.clientX,y=event.type==='keydown'?rect.top+20:event.clientY;
  menu.style.left=Math.max(8,Math.min(x,innerWidth-bounds.width-8))+'px';menu.style.top=Math.max(8,Math.min(y,innerHeight-bounds.height-8))+'px';
  if(problem)menu.focus();else menuAction.focus();
 }
 menuAction.addEventListener('click',()=>{const record=menuRecord;hideMenu();if(record)open(record,true);});
 menu.addEventListener('keydown',event=>{if(['Escape','Tab'].includes(event.key)){if(event.key==='Escape')event.preventDefault();hideMenu(event.key==='Escape');}});
 document.addEventListener('pointerdown',event=>{if(!menu.contains(event.target))hideMenu();});
 document.addEventListener('scroll',()=>hideMenu(),true);window.addEventListener('resize',()=>hideMenu());
 window.ReservationUI={open,fromLease(record){if(!leaseReservationProblem(record))open(record,true);},contextMenu,setBusy(){if(busy)hideMenu();controls();}};controls();
})();
