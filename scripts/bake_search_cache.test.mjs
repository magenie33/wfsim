// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { execFile, execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { access, copyFile, mkdtemp, mkdir, readFile, rm, rmdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, relative, resolve } from 'node:path';
import { test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { runInNewContext } from 'node:vm';
import { gunzipSync } from 'node:zlib';
import { officialSearchRequest, writeSearchCacheReceipt } from './bake_search_cache.mjs';

const site = resolve(process.env.WFSIM_TEST_SITE ?? fileURLToPath(new URL('../site', import.meta.url)));
const cli = fileURLToPath(new URL('./bake_search_cache.mjs', import.meta.url));
const crowd = 'standard_multi_target__aimed_half';
const single = 'standard_single_target__aimed_half';
const json = async (p) => JSON.parse(await readFile(p));
const sha = (v) => createHash('sha256').update(v).digest('hex');
const stable = (v) => Array.isArray(v) ? v.map(stable) : v && typeof v === 'object'
  ? Object.fromEntries(Object.keys(v).sort().map((key) => [key, stable(v[key])])) : v;
const encode = (v) => JSON.stringify(stable(v));
const args = (cmd, flags = {}) => [cli, cmd, '--site', site, ...Object.entries(flags).flatMap(([k, v]) => v === true ? [`--${k}`] : [`--${k}`, v])];
const ok = (cmd, flags) => execFileSync(process.execPath, args(cmd, flags), { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] })
  .trim().split('\n').map(JSON.parse);
const bad = (cmd, flags, message) => {
  const result = spawnSync(process.execPath, args(cmd, flags), { encoding: 'utf8' });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, message);
};

test('official request materializes crowd and uses release defaults', async () => {
  const generation = await json(join(site, 'pkg', 'generation.json'));
  const meta = await json(join(site, 'pkg', generation.names.find((n) => /^meta\./.test(n))));
  const scenario = meta.benchmarks.find((item) => item.id === crowd).scenario;
  const request = await officialSearchRequest({ site, weapon: 'torid', ruler: crowd });
  assert.equal(request.formation.length, 24);
  assert.deepEqual(request.formation, scenario.formation.map((member) => ({ ...member, enemy: member.enemy || scenario.enemy })));
  assert.equal(request.seed, scenario.seed);
  assert.equal(request.runs, scenario.runs);
  assert.equal(request.headshot_pct, 50);
  assert.equal(request.ability_strength, null);
  assert.equal(request.strategy, 'quick');
  assert.equal(request.search_strategy, 'four_elements');
  assert.equal(request.mode, 'cycle');
  assert.deepEqual(request.starts.map((start) => start.slots), [Array(10).fill(null)]);
  for (const field of ['rivens', 'custom_enemies', 'also_acting']) assert.deepEqual(request[field], []);
  const kuva = await officialSearchRequest({ site, weapon: 'kuva_nukor', ruler: single, strategy: 'regular', finalists: 2, candidateRuns: 1 });
  const spec = meta.weapons.find((item) => item.id === 'kuva_nukor').valence;
  assert.deepEqual(kuva.formation, []);
  assert.equal(kuva.valence_element, spec.elements[0]);
  assert.equal(kuva.valence_bonus, spec.max);
  assert.equal(kuva.finalists, 2);
  assert.equal(kuva.candidate_runs, 1);
  await assert.rejects(officialSearchRequest({ site, weapon: 'torid', ruler: 'custom' }), /Unknown official ruler/);
  await assert.rejects(officialSearchRequest({ site, weapon: 'unknown', ruler: crowd }), /Unknown official weapon/);
});

test('alternative training excludes eligible public arcanes only', async () => {
  const excludeArcanes = ['secondary_fortifier', 'secondary_fortifier', 'unknown_arcane'];
  for (const weapon of ['braton', 'kuva_nukor', 'magistar']) {
    const options = { site, weapon, ruler: crowd };
    const expected = await officialSearchRequest(options);
    expected.limits.exclude.arcanes = weapon === 'kuva_nukor' ? ['secondary_fortifier'] : [];
    assert.deepEqual(await officialSearchRequest({ ...options, excludeArcanes }), expected);
  }
  bad('plan', { weapon: 'torid', ruler: crowd, request: '{}' }, /Unknown option/);
});

test('request matches the clean shipping page', { skip: !process.env.WFSIM_CHECK_BAKE_PAGE }, async () => {
  const { openApp } = await import('./cdp.mjs');
  const app = await openApp({ root: site });
  try {
    for (const [weapon, ruler] of [['torid', crowd], ['kuva_nukor', single], ['catchmoon_secondary', crowd], ['magistar', crowd]]) {
      const params = JSON.stringify({ weapon, ruler });
      const shown = await app.evaluate(`(({weapon, ruler}) => {
        switchWeapon(weapon);
        activeScenario = ruler;
        applyScenario(scenarioNamed(ruler).state);
        applyOptState(blankOpt());
        setOptStrategy('four_elements');
        setOptSizes({finalists: 5, candidate_runs: 10});
        return nameableOptimizeBody(optimizeBody());
      })(${params})`);
      const raw = await officialSearchRequest({ site, weapon, ruler });
      assert.deepEqual(shown, raw, weapon);
      const frozen = await app.evaluate(`api('/api/computation/input', ${JSON.stringify({ kind: 'optimization', request: raw })})`);
      assert.equal(frozen.ok, true, JSON.stringify(frozen));
      assert.equal(frozen.request.weapon, weapon);
      assert.deepEqual(frozen.request.formation, raw.formation);
    }
  } finally { await app.close(); }
});

test('CLI emits deterministic browser packs and independently rejects untrusted shards', async (t) => {
  const root = await mkdtemp(join(tmpdir(), 'wfsim-bake-'));
  try {
    const jobs = join(root, 'jobs'), receipts = join(root, 'receipts');
    await Promise.all([mkdir(jobs), mkdir(receipts)]);
    const runner = join(root, 'fixture.mjs');
    await writeFile(runner, `import assert from 'node:assert/strict';
      import {createHash} from 'node:crypto';
      import {readFile} from 'node:fs/promises';
      import {join} from 'node:path';
      const sha = value => createHash('sha256').update(value).digest('hex');
      export async function runSearchCache(opts, expectedRoot) {
        const {site, request, official, workers, mode, cacheRoot, storageOptions, onProgress} = opts;
        assert.equal(mode, 'capture'); assert.equal(cacheRoot, expectedRoot); assert.equal(workers, 1);
        assert.deepEqual(storageOptions, {remote: expectedRoot !== undefined, indexedDB: null});
        assert.deepEqual(request.rivens, []);
        const {engine, data, wasm} = JSON.parse(await readFile(join(site, 'release.json')));
        const weapon = request.weapon;
        const identity = {engine, data, wasm, weapon, official, scenario: sha(official)};
        const entries = Array.from({length: 60}, (_, i) => {
          const key = 'q1:' + sha(weapon + i).slice(0, 32);
          return [key, 2, JSON.stringify([1, [401, 8], {runs: 2, gas: i === 0 ? 1e9 : i + 100,
            warm_key: key, index: [[0, 8], [1, 9]], ttks: [], by_body: [], by_combatant: [], seat_counters: [],
            series: {kill_progress: [], effective: []}, marker: sha('value:' + i)}])];
        });
        onProgress({phase: 'scoring', builds: 60});
        return {result: {ok: true}, wallMs: 17, pack: {version: 1, identity, entries}};
      }`);
    for (const [name, weapon, ruler] of [['a', 'torid', crowd], ['b', 'kuva_nukor', single]]) {
      const job = join(jobs, `${name}.json`), out = join(receipts, `${name}.json`);
      ok('plan', { weapon, ruler, out: job });
      const baked = spawnSync(process.execPath, args('bake', { job, out, workers: '1', runner }), { encoding: 'utf8' });
      assert.equal(baked.status, 0, baked.stderr);
      assert.deepEqual(JSON.parse(baked.stderr.trim()), { jobId: (await json(job)).jobId, phase: 'bake', progress: { phase: 'scoring', builds: 60 } });
      assert.equal(JSON.parse(baked.stdout).entries, 60);
    }
    const first = ok('pack', { receipts, out: join(root, 'staging'), 'pack-bytes': '850,1100,full' });
    const second = ok('pack', { receipts, out: join(root, 'staging'), 'pack-bytes': '850,1100,full' });
    assert.deepEqual(first, second);
    await t.test('only a host bake can warm a capture from a local staging leaf', async () => {
      const job = join(root, 'alt.json'), out = join(root, 'alt-receipt.json'), warmRunner = join(root, 'warm-runner.mjs');
      ok('plan', { weapon: 'kuva_nukor', ruler: crowd, template: 'four_elements_alt_arcane_v1', out: job });
      for (const leaf of [first[0].cacheRoot, join(root, 'missing-leaf')]) {
        await writeFile(warmRunner, `import {runSearchCache as capture} from './fixture.mjs';
          export const runSearchCache = (opts) => capture(opts, ${JSON.stringify(leaf)});`);
        const baked = ok('bake', { job, out, workers: '1', runner: warmRunner, 'warm-from': relative(process.cwd(), leaf) });
        assert.equal(baked[0].entries, 60);
        assert.equal((await json(out)).job.template, 'four_elements_alt_arcane_v1');
      }
      bad('bake', { job, out, runner, 'warm-from': 'https://example.invalid/cache/' }, /local cache directory/);
      bad('bake', { job, out, runner, 'warm-from': first[0].path }, /requires a directory/);
      bad('assemble', { jobs, receipts, out, runner, 'warm-from': first[0].cacheRoot }, /--warm-from is only valid for bake/);
    });
    await t.test('concurrent single-budget updates retain other weapons and explicit replacement withdraws packs', async () => {
      const out = join(root, 'index'), release = first[0].release.release;
      const dir = join(out, 'search-cache', 'v1', release), path = join(dir, 'manifest.json'), lock = join(dir, '.manifest.lock');
      const left = join(root, 'torid-receipts'), right = join(root, 'nukor-receipts');
      await mkdir(left); await mkdir(right);
      await copyFile(join(receipts, 'a.json'), join(left, 'a.json'));
      await copyFile(join(receipts, 'b.json'), join(right, 'b.json'));
      const update = { out, 'update-index': true };
      const small = { ...update, receipts: left, 'pack-bytes': '850' }, full = { ...update, receipts: right, 'pack-bytes': 'full' };
      const a = first[0].packs.find(p => p.weapon === 'torid'), b = first.at(-1).packs.find(p => p.weapon === 'kuva_nukor');
      assert.ok(a.entries < a.availableEntries);
      assert.equal(b.entries, b.availableEntries);
      const otherRelease = join(dir, '..', 'other-release');
      await mkdir(otherRelease, { recursive: true });
      const otherManifest = encode({ version: 1, packs: { [sha('other release')]: { bytes: 123, sha256: sha('other pack') } } });
      await writeFile(join(otherRelease, 'manifest.json'), otherManifest);
      // Both writers stage their immutable blobs before the shared manifest lock is released.
      await mkdir(lock, { recursive: true });
      const writers = Promise.allSettled([small, full].map(flags => promisify(execFile)(process.execPath, args('pack', flags))));
      try {
        const deadline = Date.now() + 5000;
        for (const item of [a, b]) {
          const file = join(dir, `${item.sha256}.json.gz`);
          while (!await access(file).then(() => true, () => false)) {
            assert.ok(Date.now() < deadline, 'writers could not stage the expected packs');
            await delay(20);
          }
        }
        await assert.rejects(access(path), { code: 'ENOENT' });
      } finally { await rmdir(lock); await writers; }
      for (const writer of await writers) assert.equal(writer.status, 'fulfilled', writer.reason?.message);
      const both = await readFile(path);
      const entry = p => ({ bytes: p.bytes, sha256: p.sha256 });
      assert.deepEqual(JSON.parse(both).packs, { [a.identitySha256]: entry(a), [b.identitySha256]: entry(b) });
      assert.equal(ok('pack', small)[0].indexEntries, 2);
      assert.deepEqual(await readFile(path), both);
      bad('pack', { ...small, 'pack-bytes': '850,full' }, /single --pack-bytes/);
      bad('pack', { ...small, 'replace-index': true }, /Choose either/);
      bad('pack', { ...small, 'total-bytes': String(a.bytes) }, /Combined index exceeds/);
      assert.deepEqual(await readFile(path), both);
      const [larger] = ok('pack', { ...small, 'pack-bytes': 'full' });
      assert.ok(larger.packs[0].bytes > a.bytes);
      assert.deepEqual((await json(path)).packs, { [a.identitySha256]: entry(larger.packs[0]), [b.identitySha256]: entry(b) });
      const [replaced] = ok('pack', { out, receipts: right, 'pack-bytes': 'full', 'replace-index': true });
      assert.equal(replaced.indexEntries, 1);
      assert.deepEqual((await json(path)).packs, { [b.identitySha256]: entry(b) });
      const empty = join(root, 'empty'); await mkdir(empty);
      bad('pack', { out, receipts: empty }, /No JSON files/);
      const nearLimit = { version: 1, packs: {} }, meta = entry(a), limit = 128 * 1024;
      const perKey = Buffer.byteLength(`"${a.identitySha256}":${encode(meta)},`);
      const capacity = Math.floor((limit - Buffer.byteLength(encode(nearLimit) + '\n') + 1) / perKey);
      for (let i = 0; i < capacity; i++) nearLimit.packs[sha(`fixture-${i}`)] = meta;
      const prior = encode(nearLimit) + '\n';
      assert.ok(Buffer.byteLength(prior) <= limit && Buffer.byteLength(prior) + perKey > limit);
      await writeFile(path, prior);
      bad('pack', small, /Manifest exceeds the browser size limit/);
      assert.equal(await readFile(path, 'utf8'), prior);
      await writeFile(path, '{}');
      bad('pack', small, /Invalid staged manifest/);
      assert.equal(await readFile(path, 'utf8'), '{}');
      assert.equal(ok('pack', { out, receipts: empty, 'replace-index': true })[0].indexEntries, 0);
      assert.deepEqual((await json(path)).packs, {});
      assert.equal(await readFile(join(otherRelease, 'manifest.json'), 'utf8'), otherManifest);
    });
    const browserReader = await readFile(new URL('../web/src/static/app/08-checkpoint-api.js', import.meta.url), 'utf8');
    for (const output of first) {
      const releaseId = output.release.release;
      assert.equal(output.siteRoot, join(root, 'staging', String(output.budget)));
      assert.equal(output.basePath, `/search-cache/v1/${releaseId}/`);
      assert.equal(output.cacheRoot, join(output.siteRoot, 'search-cache', 'v1', releaseId));
      assert.equal(output.path, join(output.cacheRoot, 'manifest.json'));
      const manifest = await json(output.path);
      assert.deepEqual(Object.keys(manifest).sort(), ['packs', 'version']);
      assert.equal(Object.keys(manifest.packs).length, 2);
      for (const pack of output.packs) {
        const info = manifest.packs[pack.identitySha256];
        assert.deepEqual(info, { bytes: pack.bytes, sha256: pack.sha256 });
        const gzip = await readFile(join(output.cacheRoot, `${info.sha256}.json.gz`));
        assert.equal(gzip.length, info.bytes);
        assert.equal(sha(gzip), info.sha256);
        assert.equal(gzip[8], 2); // gzip XFL for maximum compression
        const wire = JSON.parse(gunzipSync(gzip));
        assert.equal(sha(encode(wire.identity)), pack.identitySha256);
        assert.equal(wire.entries.length, pack.entries);
        assert.ok(wire.entries.some(([key]) => key === 'q1:' + sha(pack.weapon + 0).slice(0, 32)));
        if (output.budget === 'full') assert.equal(pack.entries, 60);
        else assert.ok(pack.entries < 60 && pack.bytes <= output.budget);
      }
      const urls = [];
      const origin = 'http://127.0.0.1';
      let fault;
      const fetchLocal = async (url) => {
        urls.push(url);
        const { pathname } = new URL(url);
        assert.ok(pathname.startsWith(output.basePath));
        const bytes = await readFile(join(output.siteRoot, pathname.slice(1)));
        if (pathname.endsWith('.json.gz')) {
          if (fault === 'missing') return new Response(null, { status: 404 });
          if (fault === 'corrupt') bytes[10] ^= 1;
          if (fault === 'encoding') return new Response(bytes, { headers: { 'content-encoding': 'gzip' } });
        }
        return new Response(bytes, { headers: { 'content-length': bytes.length } });
      };
      const asset = output.packs[0];
      const download = () => runInNewContext(`${browserReader}\nsearchWarmDownload(fetchLocal, searchWarmBase(), id, signal);`, {
        location: { origin }, RELEASE_ID: releaseId, fetchLocal, id: asset.identitySha256, signal: new AbortController().signal,
        URL, Uint8Array, TextEncoder, TextDecoder, DecompressionStream, Blob, crypto: globalThis.crypto,
      });
      const base = new URL(output.basePath, origin);
      const loaded = await download();
      assert.equal(loaded.entries.size, asset.entries);
      assert.deepEqual(urls, [new URL('manifest.json', base).href, new URL(`${asset.sha256}.json.gz`, base).href]);
      for (fault of ['missing', 'corrupt', 'encoding']) assert.equal(await download(), null, fault);
    }
    const assembled = ok('assemble', { jobs, receipts, out: join(root, 'verified'), workers: '1', runner, 'pack-bytes': 'full' })[0];
    assert.equal(assembled.siteRoot, join(root, 'verified'));
    assert.equal(assembled.cacheRoot, join(root, 'verified', 'search-cache', 'v1', assembled.release.release));
    assert.equal(assembled.path, join(assembled.cacheRoot, 'manifest.json'));
    assert.deepEqual(await json(assembled.path), await json(first.at(-1).path));
    const old = await json(join(receipts, 'a.json'));
    const capture = { result: { ok: true }, wallMs: old.wallMs, pack: old.pack };
    const request = await officialSearchRequest({ site, weapon: 'torid', ruler: crowd });
    const reuse = { site, weapon: 'torid', ruler: crowd, answer: capture, out: join(root, 'reuse.json') };
    await assert.rejects(writeSearchCacheReceipt({ ...reuse, request: { ...request, rivens: [{ id: 'private' }] } }), /official template/);
    assert.deepEqual(await writeSearchCacheReceipt({ ...reuse, request }), old);
    bad('pack', { receipts, out: join(root, 'oversized'), 'pack-bytes': 'full', 'raw-bytes': '1' }, /Full pack exceeds/);
    const altered = structuredClone(old);
    const data = JSON.parse(altered.pack.entries[0][2]);
    data[2].gas++;
    altered.pack.entries[0][2] = JSON.stringify(data);
    const target = join(receipts, 'a.json');
    await writeFile(target, encode(altered));
    bad('pack', { receipts, out: join(root, 'invalid') }, /Receipt SHA-256 mismatch/);
    altered.packSha256 = sha(encode(altered.pack));
    await writeFile(target, encode(altered));
    bad('assemble', { jobs, receipts, out: join(root, 'invalid'), workers: '1', runner }, /Independent shard mismatch/);
  } finally { await rm(root, { recursive: true, force: true }); }
});
