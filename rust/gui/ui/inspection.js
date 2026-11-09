"use strict";
(() => {
 let server=null,scope=null,locked=false;
 const views=['options','health'];
 const el=(kind,suffix)=>document.querySelector(`#${kind}-${suffix}`);
 function controls(){for(const kind of views) el(kind,'load').disabled=locked||!server||(kind==='options'&&!scope);}
 function reset(kind){el(kind,'rows').replaceChildren();el(kind,'errors').replaceChildren();el(kind,'errors').hidden=true;el(kind,'status').textContent='Not loaded';el(kind,'context').textContent=server?`${server}${kind==='options'&&scope?' · '+scope:''}`:'';el(kind,'load').textContent=`Load ${kind==='options'?'scope options':'server health'}`;}
 for(const kind of views) el(kind,'load').addEventListener('click',async()=>{
  if(busy||!server||(kind==='options'&&!scope))return;
  const source={server,scopeId:kind==='options'?scope:null};
  reset(kind);setBusy(true);el(kind,'panel').setAttribute('aria-busy','true');el(kind,'status').textContent='Loading…';
  const timer=setTimeout(()=>el(kind,'status').textContent='Still querying the server… Windows controls the current query duration.',12000);
  try {
   const result=await window.__TAURI__.core.invoke('inspect_dhcp',source);
   if(result.server!==source.server||result.scopeId!==source.scopeId)throw new Error('Response did not match the requested source.');
   for(const entry of result.rows){const row=document.createElement('tr');for(const value of [entry.name,entry.kind,entry.values.join('; ')]){const td=document.createElement('td');td.textContent=value;row.append(td);}el(kind,'rows').append(row);}
   for(const warning of result.warnings){const li=document.createElement('li');li.textContent=warning;el(kind,'errors').append(li);}
   el(kind,'errors').hidden=!result.warnings.length;
   el(kind,'status').textContent=`${result.warnings.length?(result.rows.length?'Partial results':'Unavailable'):result.rows.length?'Loaded':kind==='options'?'No explicit standard scope options configured':'No server information returned'} · Updated ${new Date().toLocaleTimeString()}`;
  }catch(error){el(kind,'rows').replaceChildren();const li=document.createElement('li');li.textContent=String(error);el(kind,'errors').append(li);el(kind,'errors').hidden=false;el(kind,'status').textContent='Query failed · Values unavailable';}
  finally{clearTimeout(timer);el(kind,'panel').setAttribute('aria-busy','false');el(kind,'load').textContent='Refresh';setBusy(false);}
 });
 window.InspectionViews={server(value){server=value;scope=null;views.forEach(reset);controls();},scope(value){scope=value;reset('options');controls();},setBusy(value){locked=value;controls();}};
 views.forEach(reset);controls();
})();
