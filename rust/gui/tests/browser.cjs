// Optional UI regression checks. Requires Playwright and an installed Chrome.
// DHCP responses are mocked; this does not establish native API parity.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const path = require('node:path');
const { pathToFileURL } = require('node:url');

(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1040, height: 760 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(() => {
      window.calls = [];
      window.mode = 'success';
      window.fixture = [
        { scopeId: '10.0.0.0', name: 'Office', state: 'Active', warnings: [], ranges: [
          { start: '10.0.0.10', end: '10.0.0.20' },
          { start: '10.0.0.50', end: '10.0.0.60' }
        ] },
        { scopeId: '192.168.10.0', name: 'Büro 東京', state: 'Inactive', warnings: [], ranges: [] }
      ];
      window.__TAURI__ = { core: { invoke: async (command, args) => {
        window.calls.push({ command, args });
        if (window.mode === 'pending') return new Promise(resolve => { window.finish = resolve; });
        if (window.mode === 'error') throw 'Access denied (code 5). Check account permissions.';
        return { server: args.server, scopes: window.mode === 'empty' ? [] : window.fixture,
          notice: window.mode === 'recovery' ? 'Scope IDs recovered; native worker exited with 0xC0000005. Diagnostic log: ipsherpa.log' : null };
      } } };
    });
    await page.goto(pathToFileURL(path.join(__dirname, '../ui/index.html')).href);
    assert.equal(await page.locator('#refresh').isDisabled(), true);
    await page.locator('#server').fill('DHCP01');
    await page.locator('#connect').click();
    await page.waitForFunction(() => document.querySelector('#scope-count').textContent === '2');
    assert.deepEqual(await page.locator('#scope-rows td:nth-child(1)').allTextContents(), ['10.0.0.0', '192.168.10.0']);
    assert.deepEqual(await page.locator('#scope-rows td:nth-child(2)').allTextContents(), ['Office', 'Büro 東京']);
    assert.deepEqual(await page.locator('#scope-rows td:nth-child(3)').allTextContents(), [
      '10.0.0.10 – 10.0.0.20\n10.0.0.50 – 10.0.0.60', 'No ranges configured'
    ]);
    assert.deepEqual(await page.locator('#scope-rows td:nth-child(4)').allTextContents(), ['Active', 'Inactive']);
    if (process.env.IPSHERPA_SCREENSHOT) {
      await page.screenshot({ path: process.env.IPSHERPA_SCREENSHOT, fullPage: true });
    }
    await page.locator('#server').fill('DHCP02');
    await page.locator('#refresh').click();
    await page.waitForFunction(() => window.calls.length === 2 && !document.querySelector('#refresh').disabled);
    assert.equal(await page.evaluate(() => window.calls[1].args.server), 'DHCP01');

    await page.evaluate(() => {
      window.fixture = [
        { scopeId: '10.2.0.0', name: null, state: null, ranges: null,
          warnings: ['Access denied <img src=x onerror="window.injected=true"> (code 5)'] },
        { scopeId: '10.3.0.0', name: '', state: 'Unknown (99)', ranges: [], warnings: [] }
      ];
    });
    await page.locator('#refresh').click();
    await page.waitForFunction(() => document.querySelectorAll('.scope-warning').length === 1);
    assert.deepEqual(await page.locator('#scope-rows tr:not(.scope-warning) td:nth-child(2)').allTextContents(), ['Unavailable', '(Unnamed)']);
    assert.equal(await page.locator('#scope-rows tr:not(.scope-warning) td:nth-child(3)').first().textContent(), 'Unavailable');
    assert.equal(await page.locator('#scope-rows tr:not(.scope-warning) td:nth-child(4)').last().textContent(), 'Unknown (99)');
    assert.match(await page.locator('#status').textContent(), /Details unavailable for 1 scope/);
    assert.equal(await page.locator('#scope-rows img').count(), 0);
    assert.equal(await page.evaluate(() => window.injected), undefined);
    assert.equal(await page.locator('#scope-count').textContent(), '2');
    assert.equal(await page.locator('#error').isHidden(), true);

    await page.evaluate(() => window.mode = 'recovery');
    await page.locator('#refresh').click();
    await page.locator('#error').waitFor({ state: 'visible' });
    assert.match(await page.locator('#error').textContent(), /Scope IDs recovered/);
    assert.equal(await page.locator('#table-wrapper').isVisible(), true);
    assert.equal(await page.locator('#refresh').isEnabled(), true);
    await page.evaluate(() => window.mode = 'pending');
    await page.locator('#refresh').click();
    assert.equal(await page.locator('#server').isDisabled(), true);
    assert.equal(await page.locator('#refresh').isDisabled(), true);
    assert.equal(await page.locator('#scope-panel').getAttribute('aria-busy'), 'true');
    await page.evaluate(() => window.finish({ server: 'DHCP01', scopes: [] }));
    await page.waitForFunction(() => document.querySelector('#scope-count').textContent === '0');
    assert.equal(await page.locator('#empty h3').textContent(), 'No IPv4 scopes found');

    await page.evaluate(() => window.mode = 'error');
    await page.locator('#refresh').click();
    await page.locator('#error').waitFor({ state: 'visible' });
    assert.match(await page.locator('#error').textContent(), /Access denied/);
    assert.equal(await page.locator('#scope-rows tr').count(), 0);
    assert.equal(await page.locator('#refresh').isDisabled(), true);
    await page.locator('#server').fill('bad server');
    const before = await page.evaluate(() => window.calls.length);
    await page.locator('#connect').click();
    assert.equal(await page.evaluate(() => window.calls.length), before);

    await page.evaluate(() => window.mode = 'success');
    await page.locator('#server').fill('DHCP01');
    await page.locator('#connect').click();
    await page.waitForFunction(() => !document.querySelector('#table-wrapper').hidden);
    await page.setViewportSize({ width: 620, height: 560 });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth), false);
    assert.equal(await page.locator('#table-wrapper').evaluate(el => el.scrollWidth > el.clientWidth), true);
    assert.deepEqual(errors, []);
    console.log('PASS: details, Unicode, multiple/empty/unavailable ranges, partial errors, safe text, refresh target, loading, empty scopes, errors, input validation, narrow layout.');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
