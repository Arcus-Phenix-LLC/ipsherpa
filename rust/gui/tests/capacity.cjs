// Synthetic responses only; no live DHCP calls.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const { pathToFileURL } = require('node:url');
const path = require('node:path');
(async () => {
 const browser = await chromium.launch({channel:'chrome',headless:true});
 try {
  const page = await browser.newPage(); const errors=[];
  page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(() => {
   window.calls=[]; window.mode='ok';
   window.__TAURI__={core:{invoke:async(command,args)=>{
    window.calls.push(command);
    if(command==='list_scopes') return {server:args.server,notice:null,scopes:[{scopeId:'192.0.2.0',name:'Example',state:'Active',warnings:[],ranges:[]}]};
    if(command!=='scope_capacity') return {...args,records:[]};
    if(window.mode==='failure') throw 'Synthetic failure';
    if(window.mode==='pending') return new Promise(resolve=>window.finish=resolve);
    const result={...args,statistics:{inUse:0,free:90,pendingOffers:2,utilizationPercent:0},configuredRanges:[],exclusions:[{start:'192.0.2.10',end:'192.0.2.19'}],totals:{configured:100,excluded:10,afterExclusions:90},warnings:[]};
    if(window.mode==='empty') result.exclusions=[];
    if(window.mode==='partial'){result.statistics=null;result.warnings=['Statistics unavailable'];}
    if(window.mode==='unavailable'){result.statistics=null;result.configuredRanges=null;result.exclusions=null;result.totals=null;result.warnings=['Synthetic unavailable'];}
    if(window.mode==='zero') result.statistics={inUse:0,free:0,pendingOffers:0,utilizationPercent:null};
    if(window.mode==='full') result.statistics={inUse:90,free:0,pendingOffers:0,utilizationPercent:100};
    if(window.mode==='mismatch') result.server='OTHER';
    return result;
   }}};
  });
  await page.goto(pathToFileURL(path.join(__dirname,'../ui/index.html')).href);
  await page.locator('#server').fill('EXAMPLE-DHCP'); await page.locator('#connect').click();
  await page.locator('.scope-row').first().click();
  await page.waitForFunction(()=>!document.querySelector('#connect').disabled);
  assert.equal(await page.evaluate(()=>window.calls.includes('scope_capacity')),false);
  await page.locator('#capacity-panel summary').click();
  const load=async(mode)=>{await page.evaluate(mode=>window.mode=mode,mode);await page.locator('#capacity-refresh').click();await page.waitForFunction(()=>!document.querySelector('#capacity-refresh').disabled);};
  await load('ok');
  assert.equal(await page.locator('#capacity-percent').textContent(),'0.0%');
  assert.equal(await page.locator('#capacity-offers').textContent(),'2');
  assert.equal(await page.locator('#capacity-usable').textContent(),'90');
  assert.equal(await page.locator('#exclusion-rows tr').count(),1);
  assert.equal(await page.locator('#exclusion-rows td').last().textContent(),'10');
  await load('empty'); assert.match(await page.locator('#exclusion-empty').textContent(),/No exclusion/);
  await load('partial'); assert.equal(await page.locator('#capacity-in-use').textContent(),'Unavailable'); assert.equal(await page.locator('#exclusion-rows tr').count(),1);
  await load('zero'); assert.equal(await page.locator('#capacity-percent').textContent(),'Unavailable');
  await load('full'); assert.equal(await page.locator('#capacity-percent').textContent(),'100.0%');
  await load('unavailable'); assert.match(await page.locator('#capacity-status').textContent(),/Overview unavailable/);
  await load('failure'); assert.match(await page.locator('#capacity-status').textContent(),/query failed/);
  await load('mismatch'); assert.match(await page.locator('#capacity-warnings').textContent(),/did not match/);
  await page.evaluate(()=>window.mode='pending'); await page.locator('#capacity-refresh').click();
  assert.equal(await page.locator('#connect').isDisabled(),true);
  await page.evaluate(()=>window.finish({server:'EXAMPLE-DHCP',scopeId:'192.0.2.0',statistics:null,configuredRanges:[],exclusions:[],totals:null,warnings:[]}));
  await page.waitForFunction(()=>!document.querySelector('#connect').disabled);
  await page.setViewportSize({width:390,height:844});
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth),true);
  assert.deepEqual(errors,[]); console.log('Capacity browser checks passed');
 } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
