const { test } = require('node:test');
const assert = require('node:assert/strict');
const { scan, uniqueServers } = require('../ui/fleet.js');
const scope = { scopeId: '192.0.2.0', name: 'Example', warnings: [] };
const record = { ipAddress: '192.0.2.10', name: 'Example printer', clientId: '01-AA-BB-CC-DD-EE-FF', state: 'Active' };
function mock() {
  return async (command, args) => command === 'list_scopes'
    ? { server: args.server, scopes: [scope] }
    : { ...args, records: [record] };
}
test('deduplicates only equivalent target names and preserves duplicate addresses across sources', async () => {
  assert.deepEqual(uniqueServers([' EXAMPLE-A ', 'example-a', 'EXAMPLE-B', 'bad server', null]), ['EXAMPLE-A', 'EXAMPLE-B']);
  const result = await scan({ targets: ['EXAMPLE-A', 'example-a', 'EXAMPLE-B'], query: 'printer', invoke: mock() });
  assert.equal(result.matches.length, 4);
  assert.equal(result.completedQueries, 4);
  assert.equal(result.serversCompleted, 2);
  assert.deepEqual(result.matches.map(row => [row.server, row.scopeId, row.kind]), [
    ['EXAMPLE-A', '192.0.2.0', 'leases'], ['EXAMPLE-A', '192.0.2.0', 'reservations'],
    ['EXAMPLE-B', '192.0.2.0', 'leases'], ['EXAMPLE-B', '192.0.2.0', 'reservations']
  ]);
});
test('continues after a server or record query fails and does not claim empty success', async () => {
  const invoke = async (command, args) => {
    if (args.server === 'EXAMPLE-DOWN') throw 'RPC unavailable';
    if (command === 'list_records' && args.kind === 'reservations') throw 'Read denied';
    return mock()(command, args);
  };
  const result = await scan({ targets: ['EXAMPLE-DOWN', 'EXAMPLE-A'], query: 'printer', invoke });
  assert.equal(result.matches.length, 1);
  assert.equal(result.failedQueries, 2);
  assert.equal(result.completedQueries, 1);
  assert.equal(result.serversCompleted, 0);
  assert.equal(result.issues[0].server, 'EXAMPLE-DOWN');
  assert.equal(result.issues[1].scopeId, '192.0.2.0');
  const denied = await scan({ targets: ['EXAMPLE-DOWN'], query: 'missing', invoke });
  assert.equal(denied.matches.length, 0);
  assert.equal(denied.failedQueries, 1);
});
test('stops before the next query, while retaining a completed in-flight result', async () => {
  let stopped = false;
  let count = 0;
  const invoke = async (command, args) => {
    count++;
    const result = await mock()(command, args);
    if (command === 'list_records') stopped = true;
    return result;
  };
  const result = await scan({ targets: ['EXAMPLE-A', 'EXAMPLE-B'], query: 'printer', invoke, shouldStop: () => stopped });
  assert.equal(count, 2);
  assert.equal(result.matches.length, 1);
  assert.equal(result.stopped, true);
});
test('matches normalized client IDs and rejects a response from the wrong source', async () => {
  const result = await scan({ targets: ['EXAMPLE-A'], query: '01aabbccddeeff', field: 'clientId', kind: 'reservations', invoke: mock() });
  assert.equal(result.matches.length, 1);
  const wrong = await scan({ targets: ['EXAMPLE-A'], query: 'printer', invoke: async (command, args) => command === 'list_scopes'
    ? mock()(command, args) : { ...args, scopeId: '198.51.100.0', records: [record] } });
  assert.equal(wrong.matches.length, 0);
  assert.equal(wrong.failedQueries, 2);
  await assert.rejects(scan({ targets: [], query: 'printer', invoke: mock() }));
});
test('successful empty scopes and metadata warnings are distinct from failed coverage', async () => {
  const empty = await scan({ targets: ['EXAMPLE-A'], query: 'printer', invoke: async () => ({ server: 'EXAMPLE-A', scopes: [] }) });
  assert.equal(empty.serversCompleted, 1);
  assert.equal(empty.failedQueries, 0);
  const warnings = await scan({ targets: ['EXAMPLE-A'], query: 'printer', kind: 'leases', invoke: async (command, args) => command === 'list_scopes'
    ? { server: args.server, scopes: [scope], notice: 'Names unavailable' } : mock()(command, args) });
  assert.equal(warnings.matches.length, 1);
  assert.equal(warnings.issues[0].warning, true);
  assert.equal(warnings.failedQueries, 0);
});
