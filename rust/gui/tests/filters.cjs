const { test } = require('node:test');
const assert = require('node:assert/strict');
const { matches } = require('../ui/filters.js');
const record = { ipAddress: '192.0.2.10', name: 'Büro-Printer', clientId: '01-AA-BB-CC-DD-EE-FF', state: 'Active' };
const filter = overrides => ({ query: '', field: 'all', state: '', ...overrides });
test('searches IP prefixes and Unicode names without changing records', () => {
  const before = JSON.stringify(record);
  assert(matches(record, filter({ query: '192.0.2.' }), 'leases'));
  assert(matches(record, filter({ query: 'BÜRO', field: 'name' }), 'leases'));
  assert(matches(record, filter({ query: 'Bu\u0308ro', field: 'name' }), 'leases'));
  assert(!matches(record, filter({ query: '192.0.2.', field: 'name' }), 'leases'));
  assert.equal(JSON.stringify(record), before);
});
test('matches full and partial client IDs across MAC separators', () => {
  for (const query of ['01aabbccddeeff', '01:aa:bb:cc:dd:ee:ff', '01aa.bbcc.ddee.ff', '01 aa bb cc dd ee ff', 'aa:bb:cc']) {
    assert(matches(record, filter({ query, field: 'clientId' }), 'leases'), query);
  }
  assert(!matches(record, filter({ query: 'office' }), 'leases'));
  assert(!matches({ ...record, clientId: '' }, filter({ query: '---' }), 'leases'));
});
test('combines search and lease state, while reservations ignore lease state', () => {
  assert(matches(record, filter({ query: 'printer', state: 'Active' }), 'leases'));
  assert(!matches(record, filter({ query: 'printer', state: 'Declined' }), 'leases'));
  assert(matches(record, filter({ query: 'printer', state: 'Declined' }), 'reservations'));
  assert(matches({ ...record, state: null }, filter({ state: 'Unknown' }), 'leases'));
  assert(!matches({ ...record, state: 'Active · Deleted' }, filter({ state: 'Active' }), 'leases'));
});
