// SPDX-License-Identifier: AGPL-3.0-or-later
// Each timed arm starts in a fresh Chromium profile and runs through final verification.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { officialTemplateRequest, writeSearchCacheReceipt } from './bake_search_cache.mjs';
import { runSearchCache } from './run_search_cache.mjs';

const { values } = parseArgs({ options: {
  site: { type: 'string', default: 'site' }, control: { type: 'string' },
  weapon: { type: 'string' }, ruler: { type: 'string' }, out: { type: 'string' },
  template: { type: 'string', default: 'four_elements_v1' }, workers: { type: 'string', default: '10' },
  budget: { type: 'string', default: '8388608' }, ban: { type: 'string' },
  start: { type: 'string' }, lock: { type: 'boolean' }, pool: { type: 'string' },
  local: { type: 'boolean' }, resume: { type: 'boolean' },
} });
const { weapon, ruler, template, budget } = values;
if (!weapon || !ruler || !values.out || (values.lock && !values.start)) {
  throw Error('Provide --weapon ID --ruler ID --out DIR and a supported template. --lock requires --start MOD.');
}
const base = resolve(values.out);
const site = resolve(values.site);
const controlSite = values.control ? resolve(values.control) : site;
const workers = Number(values.workers);
const release = async dir => JSON.parse(await readFile(join(dir, 'release.json')));
const rel = await release(site);
const request = await officialTemplateRequest(site, { weapon, ruler, template });
const pathFor = label => join(base, label + '.json');
const save = (label, data) => writeFile(pathFor(label), JSON.stringify(data));
const digest = data => createHash('sha256').update(JSON.stringify(data)).digest('hex');
const simulationResult = ({ cpu_ms, work, fights, ...result }) => result;
function compare(cold, warm) {
  assert.deepEqual(simulationResult(warm.result), simulationResult(cold.result));
  assert.equal(cold.scoreDigest, warm.scoreDigest);
}
const probe = structuredClone(request);
if (values.ban) probe.limits.exclude.mods.push(...values.ban.split(','));
if (values.start) {
  probe.starts[0].slots[0] = values.start;
  if (values.lock) probe.starts[0].fixed.push({ kind: 'mods', idx: 0, placement: 'slot' });
}
if (values.pool) {
  const mods = values.pool.split(',');
  probe.mods = Object.fromEntries(mods.map(id => [id, 'search']));
  probe.limits.mods = Math.min(3, mods.length);
  probe.limits.exilus = false;
  probe.limits.arcane_seats = [false];
}
await mkdir(join(base, 'receipt'), { recursive: true });
async function measure(label, query, options) {
  const fingerprint = await release(options.site || site);
  const manifest = options.cacheRoot ? await readFile(join(options.cacheRoot, 'manifest.json'), 'utf8') : null;
  const inputDigest = digest({ query, workers, mode: options.mode, official: Object.hasOwn(options, 'official') ? options.official : ruler, manifest, localRepeat: !!options.localRepeat });
  if (values.resume) {
    try {
      const old = JSON.parse(await readFile(pathFor(label)));
      if (old.engine === fingerprint.engine && old.wasm === fingerprint.wasm && old.data === fingerprint.data && old.inputDigest === inputDigest) return old;
    } catch (error) { if (error.code !== 'ENOENT') throw error; }
  }
  const begin = performance.now();
  console.log('start', weapon, ruler, label);
  const report = await runSearchCache({ site, official: ruler, workers, request: query, audit: true, ...options,
    onProgress(p) { console.log(weapon, label, Math.round(p.ms / 1000), p.steps, p.builds, p.fights, p.phase); },
  });
  const { pack, ...row } = report;
  if (pack) {
    await writeSearchCacheReceipt({ site, weapon, ruler, template, request, answer: report, out: join(base, 'receipt', 'host.json') });
    row.captureWithExportMs = performance.now() - begin;
  }
  row.rankDigest = digest(simulationResult(row.result));
  row.inputDigest = inputDigest;
  await save(label, row);
  console.log('finished', weapon, label, Math.round(row.wallMs), row.result.fights, row.stats.hits);
  return row;
}
const cold = await measure('cold', request, { mode: 'cold', site: controlSite });
const captured = await measure('capture', request, { mode: 'capture', storageOptions: { remote: false, indexedDB: null } });
compare(cold, captured);

const packStart = performance.now();
const cli = fileURLToPath(new URL('./bake_search_cache.mjs', import.meta.url));
const packArgs = [cli, 'pack', '--site', site, '--receipts', join(base, 'receipt'), '--out', join(base, 'published'), '--pack-bytes', budget];
const outputs = execFileSync(process.execPath, packArgs, { encoding: 'utf8', maxBuffer: 4 * 1024 ** 2 });
const published = outputs.trim().split(/\r?\n/).map(JSON.parse);
const packMs = performance.now() - packStart;
const checks = [];
for (const pub of published) {
  const official = await measure('official-' + pub.budget, request, { mode: 'official', cacheRoot: pub.cacheRoot });
  compare(cold, official);
  assert(official.stats.hits > 0 && official.requests.some(r => r.bytes && r.key.endsWith('.gz')));
  checks.push({ budget: pub.budget, pack: pub.packs[0], ms: official.wallMs, speedup: 1 - official.wallMs / cold.wallMs });
}
if (values.ban || values.start || values.pool) {
  const heldCold = await measure('holdout-cold', probe, { mode: 'cold', site: controlSite });
  const heldWarm = await measure('holdout-official', probe, { mode: 'official', cacheRoot: published.at(-1).cacheRoot });
  compare(heldCold, heldWarm);
  checks.push({ holdout: true, coldMs: heldCold.wallMs, warmMs: heldWarm.wallMs, hit: heldWarm.stats.hits, lookups: heldWarm.stats.lookups });
}
if (values.local) {
  const local = await measure('custom-local', request, { mode: 'local', official: null, localRepeat: true });
  compare(cold, local);
  compare(cold, local.repeat);
  assert(local.repeat.stats.hits > 0);
  assert(![...local.requests, ...local.repeat.requests].some(r => r.key.includes('manifest')));
  checks.push({ customFirstMs: local.wallMs, customRepeatMs: local.repeat.wallMs, customRepeatHits: local.repeat.stats.hits });
}
await save('summary', { weapon, ruler, template, engine: rel.engine, wasm: rel.wasm, workers,
  coldMs: cold.wallMs, captureMs: captured.wallMs, bakeMs: captured.captureWithExportMs + packMs, packMs, checks });
