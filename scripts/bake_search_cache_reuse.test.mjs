// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { access, copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { loadEngine } from './wasm_engine.mjs';
import { officialSearchRequest, writeSearchCacheReceipt } from './bake_search_cache.mjs';

const site = fileURLToPath(new URL('../site/', import.meta.url));
const scripts = fileURLToPath(new URL('./', import.meta.url));
const cli = join(scripts, 'bake_search_cache.mjs');
const sha = (v) => createHash('sha256').update(v).digest('hex');
const tag = (v) => sha(v).slice(0, 12);
const json = async (p) => JSON.parse(await readFile(p));
const put = (p, value) => writeFile(p, JSON.stringify(value));
const arbucep = 'weapons/archgun/arbucep.yaml';
const braton = 'weapons/primary/braton.yaml';

async function baseRelease() {
  const release = await json(join(site, 'release.json'));
  const engine = await json(join(site, 'engines', `${release.engine}.json`));
  const generation = await json(join(site, 'pkg', 'generation.json'));
  const assets = await json(join(site, 'asset', 'generation.json'));
  const app = assets.names.find((n) => n.startsWith('app.'));
  const worker = assets.names.find((n) => n.startsWith('worker.'));
  const meta = generation.names.find((n) => n.startsWith('meta.'));
  const data = release.packs.find((n) => n.startsWith('pack.data.'));
  return { release, engine, generation, assets, app, worker, meta, data,
    raw: await readFile(join(site, 'pkg', data)), source: await readFile(join(site, 'asset', app), 'utf8'),
    html: await readFile(join(site, 'index.html'), 'utf8') };
}

async function changedRelease(next, base, { paths = [arbucep], changed = ['arbucep'], rest, extra, textOnly = false, appEdit } = {}) {
  const r = structuredClone(base.release), m = structuredClone(base.engine), pack = JSON.parse(base.raw);
  const computed = paths.filter((p) => p.startsWith('weapons/') && !textOnly);
  for (const p of paths) {
    assert.equal(typeof pack.files[p], 'string');
    if (computed.includes(p)) {
      const updated = pack.files[p].replace(/^  fire_rate: [\d.]+/m, '  fire_rate: 3.125');
      assert.notEqual(updated, pack.files[p]);
      pack.files[p] = updated;
    } else pack.files[p] += 'description: changed prose\n';
  }
  if (computed.length) {
    const code = 'import sys, json, hashlib;sys.path.insert(0,sys.argv[1]);import build_site_app as b;'
      + 'print(json.dumps([hashlib.sha256(b.without_prose(t).encode()).hexdigest()[:12] for t in json.load(sys.stdin)]))';
    const hashes = JSON.parse(execFileSync(process.env.WFSIM_PYTHON || (process.platform === 'win32' ? 'python' : 'python3'),
      ['-I', '-B', '-X', 'utf8', '-c', code, scripts], { input: JSON.stringify(computed.map((p) => pack.files[p])), encoding: 'utf8' }));
    computed.forEach((p, i) => { m.files['data/' + p] = hashes[i]; });
  }
  if (rest) m.files.rest = rest;
  const bytes = paths.length ? Buffer.from(JSON.stringify(pack)) : base.raw;
  m.engine = tag(Object.keys(m.files).sort().map((p) => `${p}\0${m.files[p]}\0`).join(''));
  r.engine = m.engine; r.data = tag(bytes); r.release = tag('fixture:' + r.engine + r.data);
  const data = `pack.data.${r.data}.json`;
  r.packs = r.packs.map((n) => n === base.data ? data : n);
  r.accepted = [{ engine: base.release.engine, changed, until: '2099-01-01T00:00:00.000Z' }];
  Object.assign(r, extra);
  let source = base.source.replace(base.release.release, r.release).replace(base.release.engine, r.engine).replaceAll(base.data, data);
  if (appEdit) source = appEdit(source);
  const app = `app.${tag(source)}.js`;
  const names = base.generation.names.map((n) => n === base.data ? data : n);
  const assetNames = base.assets.names.map((n) => n === base.app ? app : n);
  await writeFile(join(next, 'pkg', data), bytes);
  await writeFile(join(next, 'asset', app), source);
  await writeFile(join(next, 'index.html'), base.html.replace(base.app, app).replaceAll(base.data, data));
  await put(join(next, 'pkg/generation.json'), { names });
  await put(join(next, 'asset/generation.json'), { names: assetNames });
  await put(join(next, 'engines', `${m.engine}.json`), m);
  await put(join(next, 'release.json'), r);
  return r;
}

async function capture(base, weapon, file, { template, excludeArcanes } = {}) {
  const ruler = 'standard_multi_target__aimed_half';
  const request = await officialSearchRequest({ site, weapon, ruler, excludeArcanes });
  const frozen = loadEngine().api('/api/computation/input', { kind: 'optimization', request });
  assert.equal(frozen.ok, true);
  const { engine, data, wasm } = base.release;
  const reader = await readFile(new URL('../web/src/static/app/08-checkpoint-api.js', import.meta.url), 'utf8');
  const identity = await runInNewContext(`${reader}\nsearchWarmIdentity(JSON.parse(body), opts)`, {
    body: JSON.stringify(frozen.request), opts: { official: ruler, engine, data, wasm }, crypto: globalThis.crypto, TextEncoder });
  const key = 'q1:' + sha(weapon).slice(0, 32);
  const shard = { runs: 1, gas: 12100, warm_key: key, index: [{ hi: 0, lo: 123, v: 7 }], ttks: [],
    by_body: [], by_combatant: [], seat_counters: [], series: { effective: [], kill_progress: [] } };
  const answer = { result: { ok: true }, wallMs: 17, pack: { version: 1, identity,
    entries: [[key, 1, JSON.stringify([1, [401, 8], shard])]] } };
  await writeSearchCacheReceipt({ site, request, answer, weapon, ruler, template, out: file });
  return sha(await readFile(file));
}

test('reuse admits only host-pinned capture with a proven release delta', async (t) => {
  const base = await baseRelease();
  const root = await mkdtemp(join(tmpdir(), 'wfsim-reuse-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const next = join(root, 'next'), receipt = join(root, 'old.json'), reused = join(root, 'reused');
  await mkdir(reused);
  const out = join(reused, 'torid.json');
  for (const name of ['asset', 'pkg', 'engines']) await mkdir(join(next, name), { recursive: true });
  for (const name of [base.meta, ...base.release.packs.filter((n) => n !== base.data),
    `wfsim_wasm.${base.release.wasm}.js`, `wfsim_wasm_bg.${base.release.wasm}.wasm`]) {
    await copyFile(join(site, 'pkg', name), join(next, 'pkg', name));
  }
  await copyFile(join(site, 'asset', base.worker), join(next, 'asset', base.worker));
  await copyFile(join(site, 'engines', `${base.release.engine}.json`), join(next, 'engines', `${base.release.engine}.json`));
  const pin = await capture(base, 'torid', receipt);
  const before = await json(receipt);
  const original = await readFile(receipt);
  const invoke = (dest, trust = pin, input = receipt) => spawnSync(process.execPath,
    [cli, 'reuse', '--old-site', site, '--site', next, '--receipt', input, '--trusted-sha256', trust, '--out', dest], { encoding: 'utf8' });
  const reject = async (pattern, trust, input) => {
    const dest = join(root, 'rejected.json'), result = invoke(dest, trust, input);
    assert.equal(result.status, 1, result.stdout);
    assert.match(result.stderr, /rebake required/);
    assert.match(result.stderr, pattern);
    await assert.rejects(access(dest), { code: 'ENOENT' });
  };
  const accepted = await changedRelease(next, base);
  const first = invoke(out);
  assert.equal(first.status, 0, first.stderr);
  const proof = JSON.parse(first.stdout), after = await json(out);
  assert.equal(proof.receiptSha256, sha(await readFile(out)));
  assert.deepEqual(proof.changedPaths, ['data/' + arbucep]);
  assert.deepEqual(after.pack.entries, before.pack.entries);
  assert.equal(after.pack.identity.scenario, before.pack.identity.scenario);
  assert.equal(after.requestSha256, before.requestSha256);
  assert.equal(after.pack.identity.data, accepted.data);
  assert.equal(after.pack.identity.engine, accepted.engine);
  assert.notEqual(after.job.jobId, before.job.jobId);
  assert.notEqual(after.packSha256, before.packSha256);
  assert.equal(invoke(out).stdout, first.stdout);
  assert.deepEqual(await readFile(receipt), original);
  const template = 'four_elements_alt_arcane_v1', alt = join(root, 'alt.json');
  await assert.rejects(capture(base, 'ocucor', alt, { template }), /official template request/);
  const altPin = await capture(base, 'ocucor', alt, { template, excludeArcanes: ['secondary_fortifier'] });
  const altOut = join(root, 'alt-reused.json'), altReuse = invoke(altOut, altPin, alt);
  assert.equal(altReuse.status, 0, altReuse.stderr);
  assert.equal((await json(altOut)).job.template, template);
  assert.notEqual((await json(altOut)).requestSha256, after.requestSha256);
  const packed = JSON.parse(execFileSync(process.execPath, [cli, 'pack', '--site', next, '--receipts', reused,
    '--out', join(root, 'staging'), '--pack-bytes', 'full'], { encoding: 'utf8' }));
  assert.equal(packed.basePath, `/search-cache/v1/${accepted.release}/`);
  assert.equal(packed.packs[0].entries, after.pack.entries.length);
  await reject(/trusted host SHA/, before.packSha256);
  const foreign = join(root, 'returned.json');
  await put(foreign, { ...before, heat: [[before.pack.entries[0][0], 1, 150000, 1000000]] });
  await reject(/trusted host SHA/, pin, foreign);
  await put(foreign, { ...before, requestSha256: '0'.repeat(64) });
  await reject(/request SHA-256/, sha(await readFile(foreign)), foreign);
  for (const acceptedRows of [[], [{ ...accepted.accepted[0], join: true }], [...accepted.accepted, ...accepted.accepted],
    [{ ...accepted.accepted[0], until: '2000-01-01T00:00:00Z' }]]) {
    await put(join(next, 'release.json'), { ...accepted, accepted: acceptedRows });
    await reject(/compatibility grant/);
  }
  await put(join(next, 'release.json'), { ...accepted, accepted: [{ ...accepted.accepted[0], changed: ['torid', 'arbucep'] }] });
  await reject(/marks torid changed/);
  for (const extra of [{ rng_version: 2 }, { score_schema: 2 }, { gas_schema: 2 }, { protocol: 42, protocols_served: [42] }]) {
    await put(join(next, 'release.json'), { ...accepted, ...extra });
    await reject(/protocol, data format or declared schema/);
  }
  await put(join(next, 'release.json'), accepted);
  const wasm = `pkg/wfsim_wasm_bg.${base.release.wasm}.wasm`;
  await writeFile(join(next, wasm), 'changed');
  await reject(/Asset SHA-256 mismatch/);
  await copyFile(join(site, wasm), join(next, wasm));
  await put(join(next, 'engines', `${base.release.engine}.json`), { ...base.engine, files: { ...base.engine.files, rest: '0'.repeat(16) } });
  await reject(/Engine manifest fingerprint/);
  await put(join(next, 'engines', `${base.release.engine}.json`), base.engine);
  await changedRelease(next, base, { rest: '0'.repeat(16) });
  await reject(/rest/);
  await changedRelease(next, base, { paths: [braton], changed: ['braton'] });
  const family = join(root, 'braton-prime.json'), familyPin = await capture(base, 'braton_prime', family);
  await reject(/affected weapon or dependency/, familyPin, family);
  await changedRelease(next, base, { paths: [arbucep, 'mods/rifle/serration.yaml'] });
  await reject(/data differences/);
  await changedRelease(next, base, { textOnly: true });
  await reject(/data differences/);
  await changedRelease(next, base, { paths: [], appEdit: (s) => s.replace('finalists: optRun.finalists,\n    strategy:',
    'finalists: opt.search_strategy === "regular" ? 91 : optRun.finalists,\n    strategy:') });
  await reject(/Official raw request changed for regular_v1/);
  await changedRelease(next, base, { paths: [] });
  const pageOnly = invoke(join(root, 'page-only.json'));
  assert.equal(pageOnly.status, 0, pageOnly.stderr);
  assert.deepEqual(JSON.parse(pageOnly.stdout).changedPaths, []);
});
