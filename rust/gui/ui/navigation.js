"use strict";
(() => {
 const $=id=>document.getElementById(id);
 let screen='server',scopeView='overview',serverView='scopes',fromSearch=false;
 const scopeTabs=[...document.querySelectorAll('.scope-tab')];
 function context(){ $('navigation-context').textContent=connectedServer?`${connectedServer}${screen==='scope'&&selectedScope?' · '+selectedScope.scopeId:''}`:'No server connected'; }
 function show(next,focus=true){
  screen=next;
  for(const view of ['server','scope','search']){$(`screen-${view}`).hidden=view!==screen;const button=$(`nav-${view}`);if(view===screen)button.setAttribute('aria-current','page');else button.removeAttribute('aria-current');}
  $('screen-label').textContent={server:'SERVER OVERVIEW',scope:'SELECTED SCOPE',search:'CROSS-SERVER SEARCH'}[screen];
  $('back-to-search').hidden=screen!=='scope'||!fromSearch;
  document.body.classList.remove('navigation-open');$('navigation-toggle').setAttribute('aria-expanded','false');
  context();if(focus){$('screen-label').tabIndex=-1;$('screen-label').focus();}
 }
 function renderScope(){
  for(const button of scopeTabs){const active=button.id===`tab-${scopeView}`;button.setAttribute('aria-selected',String(active));button.tabIndex=active?0:-1;}
  $('scope-overview').hidden=scopeView!=='overview';$('scope-options').hidden=scopeView!=='options';
  $('records-results').hidden=!['leases','reservations'].includes(scopeView);
  $('records-refresh').hidden=!['leases','reservations'].includes(scopeView);
 }
 function chooseScope(view){if(busy)return;scopeView=view;renderScope();}
 function serverTab(view){serverView=view;for(const name of ['scopes','health']){const selected=view===name;$(`server-${name}`).hidden=!selected;const button=$(`server-tab-${name}`);button.setAttribute('aria-selected',String(selected));button.tabIndex=selected?0:-1;}}
 function keyboard(list,event){if(!['ArrowLeft','ArrowRight','Home','End'].includes(event.key))return;if(busy)return;event.preventDefault();const index=list.indexOf(document.activeElement);const next=event.key==='Home'?0:event.key==='End'?list.length-1:(index+(event.key==='ArrowRight'?1:list.length-1))%list.length;list[next].focus();list[next].click();}
 for(const button of scopeTabs)button.addEventListener('click',()=>chooseScope(button.id.replace('tab-','')));
 document.querySelector('.tabs').addEventListener('keydown',event=>keyboard(scopeTabs,event));
 const serverTabs=[...document.querySelectorAll('.server-tab')];
 for(const button of serverTabs)button.addEventListener('click',()=>{if(!busy)serverTab(button.id.replace('server-tab-',''));});
 document.querySelector('.server-tabs').addEventListener('keydown',event=>keyboard(serverTabs,event));
 for(const name of ['server','scope','search'])$(`nav-${name}`).addEventListener('click',()=>show(name));
 $('back-to-search').addEventListener('click',()=>show('search'));
 $('navigation-toggle').addEventListener('click',()=>{const open=document.body.classList.toggle('navigation-open');$('navigation-toggle').setAttribute('aria-expanded',String(open));if(open)$('nav-server').focus();});
 document.addEventListener('keydown',event=>{if(event.key==='Escape'&&document.body.classList.contains('navigation-open')){document.body.classList.remove('navigation-open');$('navigation-toggle').setAttribute('aria-expanded','false');$('navigation-toggle').focus();}});
 $("connection-form").addEventListener("submit",()=>show("server",false));
 window.Navigation={
  selected(scope,kind){fromSearch=screen==='search';scopeView=kind||'overview';const summary=$('selected-scope-summary');summary.replaceChildren();for(const [label,value] of [['Scope ID',scope.scopeId],['State',scope.state||'Unavailable'],['Configured allocation ranges',scope.ranges===null?'Unavailable':scope.ranges.map(range=>`${range.start} – ${range.end}`).join('; ')||'None configured']]){const item=document.createElement('div'),dt=document.createElement('dt'),dd=document.createElement('dd');dt.textContent=label;dd.textContent=value;item.append(dt,dd);summary.append(item);}$('no-scope').hidden=true;$('nav-scope').disabled=false;renderScope();show('scope');},
  cleared(){scopeView='overview';$('nav-scope').disabled=true;$('no-scope').hidden=false;fromSearch=false;renderScope();if(screen==='scope')show('server',false);context();},
  connected(){context();serverTab('scopes');},
  recordsChanged(){scopeView=activeTab;renderScope();},
  setBusy(value){for(const button of [...scopeTabs,...serverTabs])button.disabled=value;},
 };
 renderScope();show('server',false);
})();
