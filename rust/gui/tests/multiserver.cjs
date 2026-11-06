// All servers, scopes and records are synthetic. No network queries are issued.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1040, height: 900 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(() => {
      window.calls = [];
      window.mode = 'success';
      window.__TAURI__ = { core: { invoke: async (command, args) => {
        window.calls.push({ command, args });
        if (args.server === 'EXAMPLE-DOWN') throw 'RPC unavailable (code 1722)';
        if (command === 'list_scopes') return { server: args.server, notice: null, scopes: [
          { scopeId: '192.0.2.0', name: `Office ${args.server}`, ranges: [], state: 'Active', warnings: [] },
          { scopeId: '198.51.100.0', name: 'Lab', ranges: [], state: 'Active', warnings: [] }
        ] };
        if (window.mode === 'pending') return new Promise(resolve => { window.finish = resolve; window.pendingArgs = args; });
        const record = { ipAddress: '192.0.2.10', name: 'Printer 東京', clientId: '01-AA-BB-CC-DD-EE-FF', description: '',
          clientType: 'DHCP', state: 'Active', expiresUnixSeconds: null, expiryLabel: 'Never', isReservation: true, optionsPresent: false };
        return { ...args, records: window.mode === 'empty' ? [] : window.mode === 'many'
          ? Array.from({ length: 120 }, (_, index) => ({ ...record, name: `Printer ${index}` })) : [record] };
      } } };
    });
    await page.goto(pathToFileURL(path.join(__dirname, '../ui/index.html')).href);
    for (const server of ['EXAMPLE-A', 'EXAMPLE-B']) {
      await page.locator('#server').fill(server);
      await page.locator('#save-server').click();
    }
    assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('ipsherpa.serverProfiles.v1')).length), 2);
    await page.reload();
    assert.equal(await page.locator('#server-profile option').count(), 3);
    await page.locator('#server-profile').selectOption('EXAMPLE-A');
    assert.equal(await page.locator('#server').inputValue(), 'EXAMPLE-A');
    assert.equal(await page.evaluate(() => window.calls.length), 0);
    await page.locator('#connect').click();
    await page.waitForFunction(() => !document.querySelector('#connect').disabled);
    await page.locator('#cross-panel summary').click();
    await page.locator('#cross-query').fill('printer');
    await page.locator('#cross-search').click();
    await page.waitForFunction(() => document.querySelector('#cross-status').textContent.startsWith('Search complete'));
    assert.equal(await page.locator('#cross-rows tr').count(), 8);
    assert.deepEqual([...new Set(await page.locator('#cross-rows td:first-child').allTextContents())], ['EXAMPLE-A', 'EXAMPLE-B']);
    assert.equal(await page.locator('#cross-rows td:nth-child(4)').first().textContent(), '192.0.2.10');
    assert.match(await page.locator('#cross-status').textContent(), /2\/2 servers complete/);
    if (process.env.IPSHERPA_MULTI_SCREENSHOT) await page.screenshot({ path: process.env.IPSHERPA_MULTI_SCREENSHOT, fullPage: true });
    await page.getByRole('button', { name: 'Open reservations for scope 192.0.2.0 on EXAMPLE-B', exact: true }).click();
    await page.waitForFunction(() => document.querySelector('#records-status').textContent.includes('1 reservation'));
    assert.match(await page.locator('#records-context').textContent(), /EXAMPLE-B · 192\.0\.2\.0/);
    assert.equal(await page.locator('#tab-reservations').getAttribute('aria-selected'), 'true');

    await page.locator('#server').fill('EXAMPLE-DOWN');
    await page.locator('#save-server').click();
    await page.locator('#cross-search').click();
    await page.waitForFunction(() => document.querySelector('#cross-status').textContent.startsWith('Partial results'));
    assert.equal(await page.locator('#cross-rows tr').count(), 8);
    assert.match(await page.locator('#cross-issues').textContent(), /EXAMPLE-DOWN/);
    await page.locator('#cross-targets input').evaluateAll(inputs => inputs.forEach(input => { input.checked = input.value === 'EXAMPLE-DOWN'; }));
    await page.locator('#cross-search').click();
    await page.waitForFunction(() => !document.querySelector('#cross-search').disabled);
    assert.equal(await page.locator('#cross-rows tr').count(), 0);
    assert.match(await page.locator('#cross-empty').textContent(), /incomplete/);

    await page.locator('#cross-targets input').evaluateAll(inputs => inputs.forEach(input => { input.checked = input.value === 'EXAMPLE-A'; }));
    await page.evaluate(() => window.mode = 'pending');
    await page.locator('#cross-search').click();
    await page.waitForFunction(() => typeof window.finish === 'function');
    const before = await page.evaluate(() => window.calls.length);
    await page.locator('#cross-stop').click();
    assert.equal(await page.locator('#cross-stop').isDisabled(), true);
    await page.evaluate(() => window.finish({ ...window.pendingArgs, records: [] }));
    await page.waitForFunction(() => !document.querySelector('#cross-search').disabled);
    assert.match(await page.locator('#cross-status').textContent(), /Stopped/);
    assert.equal(await page.evaluate(() => window.calls.length), before);
    assert.equal(await page.locator('#cross-stop').textContent(), 'Stop after current query');

    await page.evaluate(() => window.mode = 'many');
    await page.locator('#cross-kind').selectOption('leases');
    await page.locator('#cross-search').click();
    await page.waitForFunction(() => !document.querySelector('#cross-search').disabled);
    assert.equal(await page.locator('#cross-rows tr').count(), 100);
    const calls = await page.evaluate(() => window.calls.length);
    await page.locator('#cross-next').click();
    assert.equal(await page.locator('#cross-rows tr').count(), 100);
    await page.locator('#cross-next').click();
    assert.equal(await page.locator('#cross-rows tr').count(), 40);
    assert.equal(await page.evaluate(() => window.calls.length), calls);
    await page.setViewportSize({ width: 620, height: 560 });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth), false);
    await page.locator('#server-profile').selectOption('EXAMPLE-DOWN');
    await page.locator('#remove-server').click();
    assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('ipsherpa.serverProfiles.v1')).includes('EXAMPLE-DOWN')), false);
    assert.deepEqual(errors, []);
    console.log('PASS: saved servers, reload/removal, multi-server/multi-scope source labels, duplicate IPs, drill-down, partial/all failures, stop-after-current, paging and narrow layout.');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
