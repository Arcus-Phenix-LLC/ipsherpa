"use strict";
(() => {
 const $=id=>document.getElementById(`reservation-${id}`),dialog=$('dialog'),form=$('form');
 let source=null,token=null,pending=false,finished=false;
 function status(message){$('status').textContent=message;}
 function edit(){token=null;$('confirm').checked=false;$('confirm').disabled=false;$('fields').hidden=false;$('review').hidden=true;$('preview').hidden=false;$('create').hidden=true;$('edit').hidden=true;$('fields').disabled=false;status('');}
 function controls(){document.getElementById('new-reservation').disabled=busy||!selectedScope;for(const id of ['cancel','edit','preview'])$(id).disabled=pending;$('create').disabled=pending||!token||!$('confirm').checked;}
 function close(){if(!pending)dialog.close();}
 document.getElementById('new-reservation').addEventListener('click',()=>{
  if(busy||!selectedScope||!connectedServer)return;
  source={server:connectedServer,scopeId:selectedScope.scopeId};form.reset();finished=false;edit();$('cancel').textContent='Cancel';$('source').textContent=`${source.server} · ${source.scopeId}`;controls();dialog.showModal();
 });
 $('cancel').addEventListener('click',close);dialog.addEventListener('cancel',event=>{if(pending)event.preventDefault();});
 $('edit').addEventListener('click',()=>{edit();controls();});$('confirm').addEventListener('change',controls);
 form.addEventListener('submit',async event=>{
  event.preventDefault();if(pending||finished||token)return;
  const request={...source,ipAddress:$('ip').value.trim(),mac:$('mac').value.trim(),name:$('name').value.trim(),description:$('description').value.trim()};
  pending=true;setBusy(true);$('fields').disabled=true;status('Checking current DHCP data…');
  try{
   const result=await window.__TAURI__.core.invoke('preview_reservation',{request});
   if(result.request.server!==source.server||result.request.scopeId!==source.scopeId)throw new Error('Review source did not match the selected scope.');
   status(result.outcome.message);
   if(result.outcome.status==='ready'&&result.token){token=result.token;$('summary').replaceChildren();for(const [label,value] of [['Server',result.request.server],['Scope',result.request.scopeId],['IP',result.request.ipAddress],['MAC',result.request.mac],['Name',result.request.name],['Description',result.request.description||'(none)'],['Type','DHCP only']]){const dt=document.createElement('dt'),dd=document.createElement('dd');dt.textContent=label;dd.textContent=value;$('summary').append(dt,dd);}$('fields').hidden=true;$('review').hidden=false;$('preview').hidden=true;$('create').hidden=false;$('edit').hidden=false;}
  }catch(error){status(String(error));}
  finally{pending=false;$('fields').disabled=false;setBusy(false);controls();}
 });
 $('create').addEventListener('click',async()=>{
  if(pending||!token||!$('confirm').checked||finished)return;
  const reviewed=token;token=null;pending=true;setBusy(true);status('Rechecking conflicts, creating and verifying…');
  let result;
  try{result=await window.__TAURI__.core.invoke('create_reservation',{token:reviewed});}
  catch(error){result={status:'unknown',message:`Outcome unknown. Inspect DHCP Manager before trying again. ${String(error)}`};}
  finally{pending=false;finished=true;recordCache={};recordsLoaded=false;recordData=[];recordBody.replaceChildren();recordStatus.textContent="Reservation operation completed. Refresh records to reload server data.";emptyRecords("Refresh required", "The reservation table snapshot has been cleared. Use Refresh records to load current data.");window.ScopeCapacity?.selectionChanged(connectedServer,selectedScope);setBusy(false);}
  status(result.message);$('create').hidden=true;$('edit').hidden=true;$('preview').hidden=true;$('cancel').textContent='Close';$('confirm').disabled=true;controls();
 });
 window.ReservationUI={setBusy(){controls();}};controls();
})();
