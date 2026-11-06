// Synthetic browser fixtures; no actual network or diagnostic data is used.
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
      window.withoutDeclined = false;
      window.__TAURI__ = { core: { invoke: async (command, args) => {
        if (command === 'list_scopes') return { server: args.server, notice: null, scopes: [
          { scopeId: '192.0.2.0', name: 'Example office', state: 'Active', ranges: [], warnings: [] },
          { scopeId: '198.51.100.0', name: 'Example lab', state: 'Active', ranges: [], warnings: [] }
        ] };
        window.calls.push(args);
        if (window.mode === 'error') throw 'Read denied (code 5)';
        if (window.mode === 'pending') return new Promise(resolve => { window.finish = resolve; });
        const base = { name: 'Camera', ipAddress: '192.0.2.10', clientId: 'AA-00-BB-11-CC-22', description: '',
          clientType: 'DHCP', state: null, expiresUnixSeconds: null, expiryLabel: 'Never', isReservation: true, optionsPresent: false };
        return { server: args.server, scopeId: args.scopeId, kind: args.kind,
          records: window.mode === 'empty' ? [] : args.kind === 'reservations' ? [base]
            : Array.from({ length: 205 }, (_, index) => ({ ...base,
              ipAddress: `192.0.2.${index + 1}`, name: index === 200 ? 'Büro-Printer' : `Host ${index + 1}`,
              clientId: index === 200 ? '01-AA-BB-CC-DD-EE-FF' : `00-11-22-33-44-${index.toString(16).padStart(2,'0')}`,
              state: index === 200 && !window.withoutDeclined ? 'Declined' : 'Active'
            })) };
      } } };
    });
    await page.goto(pathToFileURL(path.join(__dirname, '../ui/index.html')).href);
    await page.locator('#server').fill('EXAMPLE-DHCP');
    await page.locator('#connect').click();
    await page.getByRole('button', { name: 'Browse scope 192.0.2.0', exact: true }).click();
    await page.waitForFunction(() => !document.querySelector('#records-search').disabled);
    await page.locator('#records-next').click();
    await page.locator('#records-next').click();
    assert.equal(await page.locator('#records-rows tr').count(), 5);
    await page.locator('#records-search-field').selectOption('name');
    await page.locator('#records-search').fill('BÜRO');
    assert.equal(await page.locator('#records-rows tr').count(), 1);
    assert.equal(await page.locator('#records-rows td').first().textContent(), '192.0.2.201');
    assert.equal(await page.locator('#filter-summary').textContent(), '1 of 205 leases shown');
    assert.equal(await page.locator('#records-pager').isHidden(), true);
    await page.locator('#records-state').selectOption('Active');
    assert.equal(await page.locator('#records-empty h3').textContent(), 'No matching leases');
    await page.locator('#records-state').selectOption('Declined');
    assert.equal(await page.locator('#records-rows tr').count(), 1);
    await page.locator('#records-search-field').selectOption('ip');
    await page.locator('#records-search').fill('192.0.2.201');
    assert.equal(await page.locator('#records-rows tr').count(), 1);
    await page.locator('#records-search-field').selectOption('clientId');
    for (const query of ['01aabbccddeeff', '01:aa:bb:cc:dd:ee:ff', '01aa.bbcc.ddee.ff']) {
      await page.locator('#records-search').fill(query);
      assert.equal(await page.locator('#records-rows tr').count(), 1);
    }
    assert.equal(await page.evaluate(() => window.calls.length), 1);
    assert.match(await page.locator('#records-context').textContent(), /EXAMPLE-DHCP · 192\.0\.2\.0/);
    assert.equal(await page.getByRole('button', { name: 'Browse scope 192.0.2.0', exact: true }).getAttribute('aria-pressed'), 'true');
    await page.locator('#clear-filters').click();
    assert.equal(await page.locator('#records-search').inputValue(), '');
    assert.equal(await page.locator('#records-state').inputValue(), '');
    assert.equal(await page.locator('#records-search-field').inputValue(), 'all');
    assert.equal(await page.locator('#records-rows td').first().textContent(), '192.0.2.1');
    assert.equal(await page.locator('#filter-summary').textContent(), '205 of 205 leases shown');

    await page.locator('#records-search').fill('büro');
    await page.locator('#records-state').selectOption('Declined');
    await page.locator('#records-refresh').click();
    await page.waitForFunction(() => !document.querySelector('#records-search').disabled);
    assert.equal(await page.locator('#records-search').inputValue(), 'büro');
    assert.equal(await page.locator('#records-state').inputValue(), 'Declined');
    assert.equal(await page.locator('#records-rows tr').count(), 1);
    await page.locator('#tab-reservations').click();
    await page.waitForFunction(() => !document.querySelector('#records-search').disabled);
    assert.equal(await page.locator('#records-search').inputValue(), '');
    assert.equal(await page.locator('#lease-state-control').isHidden(), true);
    await page.locator('#records-search').fill('camera');
    await page.locator('#tab-leases').click();
    await page.waitForFunction(() => !document.querySelector('#records-search').disabled);
    assert.equal(await page.locator('#records-search').inputValue(), 'büro');
    assert.equal(await page.locator('#records-state').inputValue(), 'Declined');
    if (process.env.IPSHERPA_SEARCH_SCREENSHOT) await page.screenshot({ path: process.env.IPSHERPA_SEARCH_SCREENSHOT, fullPage: true });
    await page.evaluate(() => window.withoutDeclined = true);
    await page.locator('#records-refresh').click();
    await page.waitForFunction(() => !document.querySelector('#records-search').disabled);
    assert.equal(await page.locator('#records-state').inputValue(), 'Declined');
    assert.equal(await page.locator('#filter-summary').textContent(), '0 of 205 leases shown');
    await page.getByRole('button', { name: 'Browse scope 198.51.100.0', exact: true }).click();
    await page.waitForFunction(() => !document.querySelector('#records-search').disabled);
    assert.equal(await page.locator('#records-search').inputValue(), '');
    assert.equal(await page.locator('#records-state').inputValue(), '');
    await page.evaluate(() => window.mode = 'error');
    await page.locator('#records-refresh').click();
    await page.locator('#records-error').waitFor({ state: 'visible' });
    assert.equal(await page.locator('#records-search').isDisabled(), true);
    assert.match(await page.locator('#records-status').textContent(), /query failed/);
    await page.evaluate(() => window.mode = 'pending');
    await page.locator('#records-refresh').click();
    assert.equal(await page.locator('#records-state').isDisabled(), true);
    await page.evaluate(() => window.finish({ server: 'EXAMPLE-DHCP', scopeId: '198.51.100.0', kind: 'leases', records: [] }));
    await page.waitForFunction(() => !document.querySelector('#records-search').disabled);
    assert.equal(await page.locator('#records-empty h3').textContent(), 'No leases found');
    assert.equal(await page.locator('#filter-summary').textContent(), '0 of 0 leases shown');
    await page.setViewportSize({ width: 620, height: 560 });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth), false);
    assert.deepEqual(errors, []);
    console.log('PASS: cross-page searches, UID separators, combined state filters, counts, clear, per-tab state, refresh persistence, scope reset, absent states, error/loading guards and narrow layout.');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
