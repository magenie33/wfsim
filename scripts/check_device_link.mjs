// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { deviceLinkRoute, DEVICE_LINK_INTERVAL, DEVICE_LINK_SECONDS } from '../worker/device_link.js';
import { sha256 } from '../worker/accounts.js';
import { testDb } from './pg/test_db.mjs';

const db = await testDb('accounts');
const env = { ACCOUNTS: db, AUTH_SECRET: 'device-link-test' };
const prefix = '/api/account/devices/link/';
const verifier = '0123456789abcdef01234567';
let checks = 0;
const check = (name, ok) => { assert(ok, name); console.log(`  ok ${name}`); checks++; };
const call = async (action, body, account = '', origin = 'https://wfsim.app', method = 'POST') => {
  const path = prefix + action;
  const r = await deviceLinkRoute(new Request(`https://wfsim.app${path}`, { method,
    headers: { 'content-type': 'application/json', origin, ...(account ? { cookie: `wfsim_session=${account}` } : {}) },
    ...(method === 'POST' ? { body: JSON.stringify(body) } : {}) }), env, path);
  return { http: r.status, ...(await r.json()) };
};
for (const name of ['alice', 'bob']) {
  await db.run("INSERT INTO entity_accounts(id, created_at, username) VALUES(?, '2026-01-01', ?)", name, name);
  await db.run("INSERT INTO state_login_sessions(token_hash, account, expires_at) VALUES(?, ?, '2999-01-01')", await sha256(name), name);
}
check('unrelated paths pass through', await deviceLinkRoute(new Request('https://wfsim.app/api/other'), env, '/api/other') === null);
check('mutations reject GET', (await call('start', {}, '', undefined, 'GET')).http === 405);
check('cross-origin starts cannot authorize devices', (await call('start', {}, '', 'https://attacker.example')).http === 403);
check('only recognized persistent device identities start a claim', (await call('start', { verifier: 'x'.repeat(24), label: 'device' })).http === 400);
check('labels reject control characters', (await call('start', { verifier, label: 'bad\nname' })).http === 400);
const start = await call('start', { verifier, label: 'My server' });
check('anonymous start returns a code and a separate polling credential', start.ok && /^[A-Z2-9]{4}-[A-Z2-9]{4}$/.test(start.user_code)
  && start.device_code.length === 43 && start.interval === DEVICE_LINK_INTERVAL && start.expires_in === DEVICE_LINK_SECONDS);
check('no full verifier appears in a reply or authorization URL', !JSON.stringify(start).includes(verifier) && !start.verification_uri_complete.includes(start.device_code));
const stored = await db.get('SELECT * FROM state_device_claim_challenges');
check('server stores credential hashes, not the user code or polling secret', stored.token_hash !== start.device_code && stored.code_hash !== start.user_code);
check('poll begins pending', (await call('poll', { device_code: start.device_code })).status === 'pending');
check('a user code cannot poll', (await call('poll', { device_code: start.user_code })).http === 400);
check('approval requires a signed-in account', (await call('approve', { user_code: start.user_code })).http === 401);
const review = await call('review', { user_code: start.user_code.toLowerCase() }, 'alice');
check('browser review exposes only the label and short device id', review.ok && review.label === 'My server' && review.id === verifier.slice(0,6) && !JSON.stringify(review).includes(verifier));
check('review does not claim a device', !(await db.get('SELECT * FROM relation_account_compute_devices WHERE verifier=?', verifier)));
check('a browser explicitly approves its displayed code', (await call('approve', { user_code: start.user_code }, 'alice')).ok);
check('first claim includes previously unclaimed history through the existing span rule', (await db.get('SELECT * FROM record_compute_device_ownership_intervals WHERE verifier=?', verifier)).from_hour === '');
check('poll confirms without exposing account identity', JSON.stringify(await call('poll', { device_code: start.device_code })) === JSON.stringify({ http: 200, ok: true, status: 'claimed' }));
check('approval retry is idempotent', (await call('approve', { user_code: start.user_code }, 'alice')).ok && (await db.all('SELECT * FROM record_compute_device_ownership_intervals WHERE verifier=?', verifier)).length === 1);
check('another account cannot reuse an approved code', (await call('approve', { user_code: start.user_code }, 'bob')).http === 404);
const second = await call('start', { verifier, label: 'Same machine' });
check('another explicit login may claim future work', (await call('approve', { user_code: second.user_code }, 'bob')).ok);
const spans = await db.all('SELECT * FROM record_compute_device_ownership_intervals WHERE verifier=? ORDER BY from_hour', verifier);
check('changing accounts preserves old ownership and starts at the existing boundary', spans.length === 2 && spans[0].account === 'alice'
  && spans[0].until_hour && spans[1].account === 'bob' && spans[1].from_hour === spans[0].until_hour);
const reserved = await call('start', { verifier: 'a'.repeat(24), label: 'Reserved device' });
await db.run('UPDATE state_device_claim_challenges SET account=? WHERE token_hash=?', 'alice', await sha256(reserved.device_code));
check('an interrupted approval can resume only for its reserving account', (await call('approve', { user_code: reserved.user_code }, 'bob')).http === 404
  && (await call('approve', { user_code: reserved.user_code }, 'alice')).ok);
await db.run("UPDATE state_device_claim_challenges SET expires_at='2000-01-01' WHERE token_hash=?", await sha256(second.device_code));
check('expired codes cannot approve or poll as claimed', (await call('approve', { user_code: second.user_code }, 'bob')).http === 404
  && (await call('poll', { device_code: second.device_code })).status === 'expired');
const keys = [];
env.AUTH_LIMIT = { limit: async ({ key }) => { keys.push(key); return { success: true }; } };
await call('poll', { device_code: start.device_code }); await call('poll', { device_code: reserved.device_code });
check('parallel clients behind one network have separate poll rate buckets', keys[0] !== keys[1]);
env.AUTH_LIMIT = { limit: async () => ({ success: false }) };
check('excessive authorization requests are limited', (await call('start', { verifier, label: 'test' })).http === 429);
const source = readFileSync(new URL('../worker/index.js', import.meta.url), 'utf8');
check('account maintenance guards the public device flow', source.indexOf('ACCOUNTS_PAUSED === "1"') < source.indexOf('await deviceLinkRoute('));
await db.close();
console.log(`Device linking: ${checks} checks passed.`);
