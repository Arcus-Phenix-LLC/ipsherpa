const {navigate,openMenu}=require('./navigation-helpers.cjs');
// Synthetic IPC fixtures only. No live DHCP data or diagnostic logs belong here.
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
      window.recordMode = 'success';
      window.recordCalls = [];
      window.__TAURI__ = { core: { invoke: async (command, args) => {
        if (command === 'list_scopes') return { server: args.server, notice: null, scopes: [
          { scopeId: '192.0.2.0', name: 'Example office', state: 'Active', ranges: [], warnings: [] },
          { scopeId: '198.51.100.0', name: 'Example devices', state: 'Inactive', ranges: [], warnings: [] }
        ] };
        if (command !== 'list_records') throw 'Unexpected command';
        window.recordCalls.push(args);
        if (window.recordMode === 'pending') return new Promise(resolve => { window.finishRecords = resolve; });
        if (window.recordMode === 'error') throw 'Read denied (code 5)';
        const fixture = {
          ipAddress: '192.0.2.10', name: 'Printer 東京', clientId: '01-02-03-04-05-06-07',
          description: '<img src=x onerror="window.injected=true">', clientType: 'DHCP / BOOTP',
          state: 'Active', expiresUnixSeconds: 0, expiryLabel: null, isReservation: true, optionsPresent: true
        };
        return { server: window.recordMode === 'mismatch' ? 'OTHER' : args.server, scopeId: args.scopeId, kind: args.kind,
          records: window.recordMode === 'empty' ? [] : args.kind === 'leases'
            ? Array.from({ length: 201 }, (_, i) => ({ ...fixture, ipAddress: `192.0.2.${i + 1}` })) : [fixture] };
      } } };
    });
    await page.goto(pathToFileURL(path.join(__dirname, '../ui/index.html')).href);
    assert.equal(await page.locator('#records-panel').isHidden(), true);
    await page.locator('#server').fill('EXAMPLE-DHCP');
    await page.locator('#connect').click();
    const scope = page.getByRole('button', { name: 'Browse scope 192.0.2.0', exact: true });
    await scope.waitFor();
    await scope.focus();
    await page.keyboard.press('Enter');
    await page.locator('#tab-leases').click();
    await page.waitForFunction(() => document.querySelector('#records-status').textContent.includes('201 leases'));
    assert.equal(await page.locator('#records-rows tr').count(), 100);
    assert.equal(await page.locator('#records-rows td:nth-child(2)').first().textContent(), 'Printer 東京');
    assert.equal(await page.locator('#records-rows td:nth-child(3)').first().textContent(), '01-02-03-04-05-06-07');
    assert.match(await page.locator('#records-rows td:nth-child(4)').first().textContent(), /Reserved/);
    const epoch = await page.evaluate(() => new Date(0).toLocaleString(undefined, { timeZoneName: 'short' }));
    assert.equal(await page.locator('#records-rows td:nth-child(5)').first().textContent(), epoch);
    assert.equal(await page.locator('#records-rows img').count(), 0);
    assert.equal(await page.evaluate(() => window.injected), undefined);
    await page.locator('#records-next').click();
    assert.equal(await page.locator('#records-rows td').first().textContent(), '192.0.2.101');
    await page.locator('#records-next').click();
    assert.equal(await page.locator('#records-rows tr').count(), 1);
    assert.equal(await page.locator('#records-next').isDisabled(), true);
    assert.equal(await page.evaluate(() => window.recordCalls.length), 1);
    await page.locator('#records-prev').click();
    await page.locator('#tab-leases').focus();
    await page.keyboard.press('ArrowRight');
    await page.waitForFunction(() => document.querySelector('#records-status').textContent.includes('1 reservation'));
    assert.equal(await page.locator('#tab-reservations').getAttribute('aria-selected'), 'true');
    assert.equal(await page.locator('#records-rows td:nth-child(4)').textContent(), 'DHCP / BOOTP');
    assert.equal(await page.locator('#records-rows td:nth-child(5)').textContent(), 'Yes');
    assert.equal(await page.locator('#records-pager').isHidden(), true);
    if (process.env.IPSHERPA_RECORDS_SCREENSHOT) await page.screenshot({ path: process.env.IPSHERPA_RECORDS_SCREENSHOT, fullPage: true });

    await page.locator('#server').fill('EDITED-SERVER');
    await page.locator('#records-refresh').click();
    await page.waitForFunction(() => !document.querySelector('#records-refresh').disabled);
    assert.deepEqual(await page.evaluate(() => window.recordCalls.at(-1)), { server: 'EXAMPLE-DHCP', scopeId: '192.0.2.0', kind: 'reservations' });

    await page.evaluate(() => window.recordMode = 'pending');
    await navigate(page,'server');
    await page.getByRole('button', { name: 'Browse scope 198.51.100.0', exact: true }).click();
    await page.locator('#tab-leases').click();
    assert.equal(await page.locator('#records-results').getAttribute('aria-busy'), 'true');
    for (const selector of ['#connect', '#refresh', '#tab-reservations', '#records-refresh']) {
      assert.equal(await page.locator(selector).isDisabled(), true);
    }
    assert.equal(await page.locator('#records-rows tr').count(), 0);
    await page.evaluate(() => window.finishRecords({ server: 'EXAMPLE-DHCP', scopeId: '198.51.100.0', kind: 'leases', records: [] }));
    await page.waitForFunction(() => document.querySelector('#records-status').textContent.includes('0 leases'));
    assert.equal(await page.locator('#records-empty h3').textContent(), 'No leases found');
    assert.match(await page.locator('#records-context').textContent(), /198\.51\.100\.0/);

    await page.evaluate(() => window.recordMode = 'error');
    await page.locator('#tab-reservations').click();
    await page.locator('#records-error').waitFor({ state: 'visible' });
    assert.match(await page.locator('#records-error').textContent(), /code 5/);
    assert.equal(await page.locator('#records-table-wrapper').isHidden(), true);
    assert.equal(await page.locator('#records-refresh').isEnabled(), true);
    await page.evaluate(() => window.recordMode = 'mismatch');
    await page.locator('#records-refresh').click();
    await page.waitForFunction(() => document.querySelector('#records-error').textContent.includes('did not match'));
    assert.equal(await page.locator('#records-rows tr').count(), 0);

    await page.evaluate(() => window.recordMode = 'success');
    await page.locator('#records-refresh').click();
    await page.waitForFunction(() => !document.querySelector('#records-table-wrapper').hidden);
    await page.setViewportSize({ width: 620, height: 560 });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth), false);
    assert.equal(await page.locator('#records-table-wrapper').evaluate(el => el.scrollWidth > el.clientWidth), true);
    await navigate(page,'server');
    await page.locator('#refresh').click();
    await page.waitForFunction(() => !document.querySelector('#refresh').disabled);
    assert.equal(await page.locator('#records-panel').isHidden(), true);
    assert.deepEqual(errors, []);
    console.log('PASS: selection, keyboard tabs, lease/reservation columns, full UIDs, local expiry, paging, refresh target, busy controls, empty/error/mismatched replies, safe text, narrow layout and selection reset.');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
