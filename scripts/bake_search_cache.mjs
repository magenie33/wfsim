// SPDX-License-Identifier: AGPL-3.0-or-later
import { createHash, randomUUID } from 'node:crypto';
import { link, mkdir, open, readdir, rename, rm, rmdir, stat, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { gzipSync } from 'node:zlib';
import { officialSearchRequest } from './bake_search_cache_request.mjs';

export { officialSearchRequest };

const version = 1;
const MiB = 1024 ** 2;
const maxGzipBytes = 24 * MiB;
const maxRawBytes = 128 * MiB;
const maxManifestBytes = 128 * 1024;
const maxEntries = 100_000;
const maxCaptureEntries = 500_000;
const id = /^[a-z0-9_]{1,100}$/;
const hash = /^[a-f0-9]{64}$/;
const fingerprint = /^[a-f0-9]{12,64}$/;
const sha = (value) => createHash('sha256').update(value).digest('hex');
const compare = (a, b) => a < b ? -1 : a > b ? 1 : 0;
const tuple = (key, runs) => `${key}:${runs}`;
const pair = ({ weapon, ruler }) => `${weapon}/${ruler}`;
const fourElementsV1 = Object.freeze({ strategy: 'four_elements', finalists: 5, candidateRuns: 10 });
const templates = Object.freeze({
  four_elements_v1: fourElementsV1,
  four_elements_alt_arcane_v1: Object.freeze({ ...fourElementsV1, excludeArcanes: Object.freeze(['secondary_fortifier']) }),
  regular_v1: Object.freeze({ strategy: 'regular', finalists: 5, candidateRuns: 10 }),
});

function canonical(value, depth = 0) {
  if (depth > 64) throw Error('JSON nesting exceeds the limit');
  if (Array.isArray(value)) return `[${value.map((item) => canonical(item, depth + 1)).join(',')}]`;
  if (value && typeof value === 'object') {
    return `{${Object.keys(value).sort().map((k) => `${JSON.stringify(k)}:${canonical(value[k], depth + 1)}`).join(',')}}`;
  }
  if (value === undefined || (typeof value === 'number' && !Number.isFinite(value))) throw Error('Invalid JSON value');
  return JSON.stringify(value);
}

function exact(value, keys, label) {
  if (!value || typeof value !== 'object' || Array.isArray(value)
      || Object.keys(value).sort().join(',') !== keys.slice().sort().join(',')) throw Error(`Invalid ${label} fields`);
}

function integer(value, min, max, label) {
  if (!Number.isSafeInteger(value) || value < min || value > max) throw Error(`Invalid ${label}`);
  return value;
}

async function readBytes(file, max = 192 * MiB) {
  const fd = await open(file, 'r');
  try {
    const stat = await fd.stat();
    if (!stat.isFile() || stat.size > max) throw Error(`Invalid file size: ${file}`);
    const bytes = await fd.readFile();
    if (bytes.length > max) throw Error(`Invalid file size: ${file}`);
    return bytes;
  } finally { await fd.close(); }
}

async function readJson(file, max) {
  return JSON.parse(await readBytes(file, max));
}

async function writeAtomic(file, content, immutable = false) {
  await mkdir(dirname(file), { recursive: true });
  const temp = `${file}.${randomUUID()}.tmp`;
  try {
    await writeFile(temp, content, { flag: 'wx' });
    if (!immutable) await rename(temp, file);
    else {
      try { await link(temp, file); }
      catch (error) {
        if (error.code !== 'EEXIST' || !(await readBytes(file)).equals(content)) throw error;
      }
    }
  } finally { await rm(temp, { force: true }); }
}

async function releaseAt(site) {
  const source = await readJson(join(site, 'release.json'), 1024 * 1024);
  const { release, engine, data, wasm } = source;
  for (const value of [release, engine, data, wasm]) {
    if (typeof value !== 'string' || !fingerprint.test(value)) throw Error('Invalid site release fingerprints');
  }
  return { release, engine, data, wasm };
}

function jobSpec(release, weapon, ruler, template) {
  if (![weapon, ruler, template].every((value) => typeof value === 'string' && id.test(value))
      || !Object.hasOwn(templates, template)) throw Error('Invalid official weapon, ruler or template ID');
  const spec = { version, release, weapon, ruler, template };
  return { ...spec, jobId: sha(canonical(spec)) };
}

function checkedJob(job, release) {
  exact(job, ['version', 'release', 'weapon', 'ruler', 'template', 'jobId'], 'job');
  const expected = jobSpec(release, job.weapon, job.ruler, job.template);
  if (canonical(job) !== canonical(expected)) throw Error('Job ID or release mismatch');
  return expected;
}

function normalizePack(pack, release, job) {
  exact(pack, ['version', 'identity', 'entries'], 'pack');
  if (pack.version !== version || !pack.identity || typeof pack.identity !== 'object'
      || Array.isArray(pack.identity) || !Array.isArray(pack.entries)) throw Error('Invalid pack envelope');
  exact(pack.identity, ['engine', 'data', 'wasm', 'weapon', 'official', 'scenario'], 'pack identity');
  for (const field of ['engine', 'data', 'wasm']) {
    if (pack.identity[field] !== release[field]) throw Error(`Pack ${field} mismatch`);
  }
  if (pack.identity.weapon !== job.weapon || pack.identity.official !== job.ruler
      || typeof pack.identity.scenario !== 'string' || !hash.test(pack.identity.scenario)) {
    throw Error('Pack weapon/ruler mismatch');
  }
  if (!pack.entries.length || pack.entries.length > maxCaptureEntries) throw Error('Invalid capture entry count');
  const seen = new Set();
  for (const entry of pack.entries) {
    if (!Array.isArray(entry) || entry.length !== 3) throw Error('Invalid pack entry shape');
    const [key, runs, packed] = entry;
    if (typeof key !== 'string' || !/^q1:[0-9a-f]{32}$/.test(key)
        || typeof packed !== 'string' || !packed.length || packed.length > 128 * 1024) throw Error('Invalid packed shard');
    integer(runs, 1, 0xffffffff, 'entry runs');
    shardGas(key, runs, packed);
    const entryId = tuple(key, runs);
    if (seen.has(entryId)) throw Error('Duplicate key/runs');
    seen.add(entryId);
  }
  return { version, identity: pack.identity, entries: pack.entries.slice().sort((a, b) => compare(a[0], b[0]) || a[1] - b[1]) };
}

function shardGas(key, runs, packed) {
  const receipt = JSON.parse(packed);
  if (!Array.isArray(receipt) || receipt.length !== 3 || receipt[0] !== version) throw Error('Invalid packed v1 receipt');
  const [, widths, data] = receipt;
  if (!Array.isArray(widths) || widths.length !== 2 || widths[0] !== 401 || widths[1] !== 8
      || data?.warm_key !== key || data.runs !== runs || !Number.isInteger(data.gas) || data.gas < 0
      || !Array.isArray(data.index) || data.index.length !== runs
      || !Array.isArray(data.ttks) || data.ttks.length > runs
      || !Array.isArray(data.by_body) || data.by_body.length > widths[0]
      || !Array.isArray(data.by_combatant) || data.by_combatant.length > widths[1]
      || !Array.isArray(data.seat_counters) || data.seat_counters.length > widths[1]
      || !Array.isArray(data.series?.kill_progress) || data.series.kill_progress.length
      || !Array.isArray(data.series?.effective) || data.series.effective.length) {
    throw Error('Packed shard key, runs or shape mismatch');
  }
  return data.gas;
}

function normalizeHeat(heat, pack) {
  if (!Array.isArray(heat) || heat.length > pack.entries.length) throw Error('Invalid heat observations');
  const known = new Set(pack.entries.map(([key, runs]) => tuple(key, runs)));
  const rows = new Map();
  for (const row of heat) {
    if (!Array.isArray(row) || row.length !== 4) throw Error('Invalid heat observation');
    const [key, runs, hits, work] = row;
    const entryId = tuple(key, runs);
    if (!known.has(entryId) || rows.has(entryId)) throw Error('Unknown or duplicate heat tuple');
    if (!Number.isSafeInteger(hits) || hits < 1 || !Number.isFinite(work) || work <= 0 || !Number.isFinite(hits * work)) {
      throw Error('Invalid observed work or hits');
    }
    rows.set(entryId, row);
  }
  return [...rows.values()].sort((a, b) => compare(a[0], b[0]) || a[1] - b[1]);
}

function receipt(job, requestSha256, answer) {
  if (!answer?.result?.ok || !Number.isFinite(answer.wallMs) || answer.wallMs < 0) throw Error('Runner did not finish a successful capture');
  const pack = normalizePack(answer.pack, job.release, job);
  const heat = normalizeHeat(answer.stats?.heat ?? [], pack);
  return { version, job, requestSha256, packSha256: sha(canonical(pack)), pack, heat, wallMs: answer.wallMs };
}

export async function writeSearchCacheReceipt({ site, weapon, ruler, template = 'four_elements_v1', request, answer, out }) {
  const job = jobSpec(await releaseAt(site), weapon, ruler, template);
  const expected = await officialTemplateRequest(site, job);
  if (canonical(request) !== canonical(expected)) throw Error('Capture does not use the official template request');
  const result = receipt(job, sha(canonical(expected)), answer);
  await writeAtomic(resolve(out), canonical(result) + '\n');
  return result;
}

export function officialTemplateRequest(site, { weapon, ruler, template }) {
  if (!Object.hasOwn(templates, template)) throw Error('Unknown official training template');
  return officialSearchRequest({ site, weapon, ruler, ...templates[template] });
}

function checkedReceipt(value, release) {
  exact(value, ['version', 'job', 'requestSha256', 'packSha256', 'pack', 'heat', 'wallMs'], 'receipt');
  const job = checkedJob(value.job, release);
  if (value.version !== version || !hash.test(value.requestSha256) || !hash.test(value.packSha256)
      || !Number.isFinite(value.wallMs) || value.wallMs < 0) throw Error('Invalid receipt');
  const pack = normalizePack(value.pack, release, job);
  if (value.packSha256 !== sha(canonical(pack))) throw Error('Receipt SHA-256 mismatch');
  return { ...value, job, pack, heat: normalizeHeat(value.heat, pack) };
}

async function reuseCapture(values) {
  const allowed = ['old-site', 'site', 'receipt', 'trusted-sha256', 'out'];
  if (Object.keys(values).some((key) => !allowed.includes(key))) throw Error('Unexpected reuse option');
  const oldSite = resolve(values['old-site']), newSite = resolve(values.site);
  const source = resolve(values.receipt), out = resolve(values.out);
  if (source === out) throw Error('Use a separate output receipt');
  const bytes = await readBytes(source);
  const trusted = values['trusted-sha256'].toLowerCase();
  if (!hash.test(trusted) || sha(bytes) !== trusted) throw Error('Receipt does not match the trusted host SHA-256');
  const prior = checkedReceipt(JSON.parse(bytes), await releaseAt(oldSite));
  const { proveSearchReuse } = await import('./bake_search_cache_reuse.mjs');
  const { release, changedPaths, wasmSha256 } = await proveSearchReuse(oldSite, newSite, prior, templates, canonical);
  const { weapon, ruler, template } = prior.job;
  const job = jobSpec(release, weapon, ruler, template);
  const { engine, data, wasm } = release;
  const pack = { ...prior.pack, identity: { ...prior.pack.identity, engine, data, wasm } };
  const result = { ...prior, job, pack, packSha256: sha(canonical(pack)) };
  const encoded = Buffer.from(canonical(result) + '\n');
  await writeAtomic(out, encoded, true);
  console.log(canonical({ jobId: job.jobId, fromJobId: prior.job.jobId, sourceReceiptSha256: trusted,
    receiptSha256: sha(encoded), packSha256: result.packSha256, entries: pack.entries.length, wasmSha256, changedPaths }));
}

async function warmCacheRoot(path) {
  if (!path.trim() || /^(?:[\\/]{2}|[a-z][a-z\d+.-]*:\/\/|file:)/i.test(path)) {
    throw Error('--warm-from requires a local cache directory');
  }
  const dir = resolve(path);
  try {
    if (!(await stat(dir)).isDirectory()) throw Error('--warm-from requires a directory containing the staged pack');
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  return dir;
}

async function runCapture(runSearchCache, request, job, options, phase, cacheRoot) {
  const { site, workers, wasm, bind } = options;
  return receipt(job, sha(canonical(request)), await runSearchCache({
    site, request, official: job.ruler, workers, mode: 'capture', wasm, bind,
    ...(cacheRoot && { cacheRoot }),
    storageOptions: { remote: Boolean(cacheRoot), indexedDB: null },
    onProgress(progress) { console.error(JSON.stringify({ jobId: job.jobId, phase, progress })); },
  }));
}

function verifyCandidate(candidate, independent) {
  if (candidate.requestSha256 !== independent.requestSha256
      || canonical(candidate.pack.identity) !== canonical(independent.pack.identity)) throw Error('Independent request or pack identity mismatch');
  const entries = new Map(independent.pack.entries.map((row) => [tuple(...row), row]));
  const verified = new Map();
  for (const [key, runs, packed] of candidate.pack.entries) {
    const entryId = tuple(key, runs);
    const row = entries.get(entryId);
    if (!row || row[2] !== packed) throw Error('Independent shard mismatch');
    verified.set(entryId, row);
  }
  return { ...independent, pack: { ...independent.pack, entries: [...verified.values()] },
    heat: independent.heat.filter(([key, runs]) => verified.has(tuple(key, runs))) };
}

function gzip(raw) {
  const bytes = gzipSync(raw, { level: 9 });
  bytes[9] = 255;
  return bytes;
}

function budgetedPack(identity, ranked, maxBytes, rawLimit, full) {
  function encode(n) {
    const entries = ranked.slice(0, n).map((item) => item.row)
      .sort((a, b) => compare(a[0], b[0]) || a[1] - b[1]);
    const raw = Buffer.from(canonical({ version, identity, entries }));
    if (raw.length > rawLimit) return null;
    const bytes = gzip(raw);
    return bytes.length <= maxBytes ? { bytes, rawBytes: raw.length, entries: n } : null;
  }
  const ceiling = Math.min(ranked.length, maxEntries);
  const all = !full || ceiling === ranked.length ? encode(ceiling) : null;
  if (all || full) return all;
  let low = 1, high = ceiling - 1, best = null;
  while (low <= high) {
    const mid = Math.floor((low + high) / 2);
    const candidate = encode(mid);
    if (candidate) { best = candidate; low = mid + 1; }
    else high = mid - 1;
  }
  return best;
}

function mergeReceipts(receipts) {
  const groups = new Map();
  const seen = new Set();
  for (const { job, pack, heat } of receipts.sort((a, b) => compare(a.job.jobId, b.job.jobId))) {
    if (seen.has(job.jobId)) throw Error('Duplicate job receipt');
    seen.add(job.jobId);
    const name = pair(job);
    if (!groups.has(name)) groups.set(name, { weapon: job.weapon, ruler: job.ruler, identity: pack.identity, entries: new Map() });
    const group = groups.get(name);
    if (canonical(group.identity) !== canonical(pack.identity)) throw Error(`Conflicting identities for ${name}`);
    const byKey = new Map(heat.map((row) => [tuple(...row), row]));
    for (const row of pack.entries) {
      const [key, runs, packed] = row;
      const entryId = tuple(key, runs);
      const previous = group.entries.get(entryId);
      if (previous && previous.row[2] !== packed) throw Error(`Conflicting shards for ${name}`);
      // Shard gas is metered on the actual run. Match engine work_of as the fallback cost estimate.
      const [, , hits, work] = byKey.get(entryId) || [key, runs, 1,
        Math.floor(shardGas(key, runs, packed) / 20) + 400000 * runs];
      group.entries.set(entryId, { row, hits: (previous?.hits || 0) + hits, work: Math.max(previous?.work || 0, work) });
    }
  }
  return groups;
}

function rankGroups(receipts, onlyFull) {
  const groups = [...mergeReceipts(receipts).entries()].sort(([a], [b]) => compare(a, b));
  return groups.map(([name, group]) => {
    const ranked = [...group.entries.values()].map((item) => ({ ...item, score: onlyFull ? 0
      : item.hits * item.work / gzip(Buffer.from(canonical(item.row))).length }));
    ranked.sort((a, b) => b.score - a.score || compare(a.row[0], b.row[0]) || a.row[1] - b.row[1]);
    return [name, group, ranked];
  });
}

async function readStagedManifest(path) {
  let bytes;
  try { bytes = await readBytes(path, maxManifestBytes); }
  catch (error) { if (error.code === 'ENOENT') return; throw error; }
  const manifest = JSON.parse(bytes);
  exact(manifest, ['version', 'packs'], 'staged manifest');
  if (manifest.version !== version || !manifest.packs || typeof manifest.packs !== 'object' || Array.isArray(manifest.packs)) {
    throw Error('Invalid staged search manifest');
  }
  for (const [identity, asset] of Object.entries(manifest.packs)) {
    exact(asset, ['bytes', 'sha256'], 'staged manifest entry');
    if (!hash.test(identity) || typeof asset.sha256 !== 'string' || !hash.test(asset.sha256)) throw Error('Invalid staged manifest SHA-256');
    integer(asset.bytes, 18, maxGzipBytes, 'staged gzip size');
  }
  return { bytes, packs: manifest.packs };
}

async function commitIndex(out, changes, updateIndex, totalBytes) {
  const path = join(out, 'manifest.json'), lock = join(out, '.manifest.lock');
  await mkdir(out, { recursive: true });
  const deadline = Date.now() + 10_000;
  for (;;) {
    try { await mkdir(lock); break; }
    catch (error) {
      if (error.code !== 'EEXIST') throw error;
      if (Date.now() >= deadline) throw Error(`Manifest writer lock busy: ${lock}`);
      await delay(50);
    }
  }
  try {
    const prior = updateIndex ? await readStagedManifest(path) : undefined;
    const packs = { ...prior?.packs, ...changes };
    const contents = Buffer.from(canonical({ version, packs }) + '\n');
    if (contents.length > maxManifestBytes) throw Error('Manifest exceeds the browser size limit');
    if (Object.values(packs).reduce((sum, asset) => sum + asset.bytes, 0) > totalBytes) throw Error('Combined index exceeds --total-bytes');
    if (!prior?.bytes.equals(contents)) await writeAtomic(path, contents);
    return { path, indexEntries: Object.keys(packs).length };
  } finally { await rmdir(lock); }
}

async function writeSearchCache({ out, groups, packBytes, totalBytes, rawBytes, full, updateIndex }) {
  const changes = {};
  const summary = [];
  let remaining = totalBytes;
  for (const [name, group, ranked] of groups) {
    const pack = budgetedPack(group.identity, ranked, Math.min(packBytes, remaining), rawBytes, full);
    if (full && !pack) throw Error(`Full pack exceeds the configured bounds: ${name}`);
    if (!pack) continue;
    const sha256 = sha(pack.bytes);
    const identitySha256 = sha(canonical(group.identity));
    const bytes = pack.bytes.length;
    await writeAtomic(join(out, `${sha256}.json.gz`), pack.bytes, true);
    changes[identitySha256] = { bytes, sha256 };
    summary.push({ weapon: group.weapon, ruler: group.ruler, identitySha256, sha256, bytes,
      rawBytes: pack.rawBytes, entries: pack.entries, availableEntries: ranked.length });
    remaining -= pack.bytes.length;
  }
  if (groups.length && !summary.length) throw Error('No pack fits the budget');
  const index = await commitIndex(out, changes, updateIndex, totalBytes);
  return { ...index, cacheRoot: out, packs: summary };
}

const options = Object.fromEntries(['site', 'weapon', 'ruler', 'template', 'job', 'out', 'receipts', 'jobs', 'runner',
  'workers', 'wasm', 'bind', 'pack-bytes', 'total-bytes', 'raw-bytes', 'old-site', 'receipt', 'trusted-sha256', 'warm-from']
  .map((name) => [name, { type: 'string' }]));
options['update-index'] = { type: 'boolean' };
options['replace-index'] = { type: 'boolean' };
const usage = `node scripts/bake_search_cache.mjs plan --site SITE --weapon ID --ruler ID [--template ID] [--out JOB]
node scripts/bake_search_cache.mjs bake --site SITE --job JOB --out RECEIPT [--workers N] [--runner MODULE] [--warm-from TRUSTED_CACHE_DIR]
node scripts/bake_search_cache.mjs reuse --old-site OLD --site NEW --receipt RECEIPT --trusted-sha256 HOST_SHA256 --out NEW_RECEIPT
node scripts/bake_search_cache.mjs pack --site SITE --receipts DIR --out ROOT [--pack-bytes N[,N,full]] [--total-bytes N] [--update-index | --replace-index]
node scripts/bake_search_cache.mjs assemble --site SITE --jobs DIR --receipts DIR --out ROOT [--workers N] [--runner MODULE]`;

async function eachJson(dir, validate, allowEmpty = false) {
  const names = (await readdir(dir, { withFileTypes: true })).filter((item) => item.isFile() && item.name.endsWith('.json'));
  if (!names.length && !allowEmpty) throw Error(`No JSON files in ${dir}`);
  const result = [];
  for (const { name } of names.sort((a, b) => compare(a.name, b.name))) result.push(validate(await readJson(join(dir, name))));
  return result;
}

async function main() {
  const args = process.argv.slice(2);
  if (!args.length || args.includes('--help')) { console.log(usage); return; }
  const { values, positionals } = parseArgs({ args, options, allowPositionals: true });
  const [command] = positionals;
  if (positionals.length !== 1 || !['plan', 'bake', 'reuse', 'pack', 'assemble'].includes(command)) throw Error(usage);
  if (values['warm-from'] !== undefined && command !== 'bake') throw Error(`--warm-from is only valid for bake, received ${command}`);
  const updateIndex = values['update-index'], replaceIndex = values['replace-index'];
  if ((updateIndex || replaceIndex) && command !== 'pack') throw Error('Index flags are only valid for pack');
  if (updateIndex && replaceIndex) throw Error('Choose either --update-index or --replace-index');
  const required = { plan: ['site', 'weapon', 'ruler'], bake: ['site', 'job', 'out'],
    reuse: ['old-site', 'site', 'receipt', 'trusted-sha256', 'out'], pack: ['site', 'receipts', 'out'], assemble: ['site', 'jobs', 'receipts', 'out'] };
  for (const field of required[command]) if (!values[field]) throw Error(`Missing --${field}`);
  if (command === 'reuse') {
    try { return await reuseCapture(values); }
    catch (error) { throw Error(`Reuse failed; rebake required: ${error.message}`); }
  }
  const site = resolve(values.site);
  const release = await releaseAt(site);
  const makeRequest = (job) => officialTemplateRequest(site, job);
  if (command === 'plan') {
    const job = jobSpec(release, values.weapon, values.ruler, values.template ?? 'four_elements_v1');
    await makeRequest(job);
    if (values.out) await writeAtomic(resolve(values.out), canonical(job) + '\n');
    console.log(canonical(job));
    return;
  }
  const workers = integer(Number(values.workers ?? 10), 1, 10, 'worker count');
  const loadRunner = async () => {
    const runner = values.runner ? pathToFileURL(resolve(values.runner)) : new URL('./run_search_cache.mjs', import.meta.url);
    const { runSearchCache } = await import(runner);
    if (typeof runSearchCache !== 'function') throw Error('Expected runSearchCache export');
    return runSearchCache;
  };
  const runnerOptions = { site, workers, wasm: values.wasm && resolve(values.wasm), bind: values.bind && resolve(values.bind) };
  if (command === 'bake') {
    const cacheRoot = values['warm-from'] === undefined ? undefined : await warmCacheRoot(values['warm-from']);
    const job = checkedJob(await readJson(values.job, 16 * 1024), release);
    const request = await makeRequest(job);
    const result = await runCapture(await loadRunner(), request, job, runnerOptions, 'bake', cacheRoot);
    const encoded = Buffer.from(canonical(result) + '\n');
    await writeAtomic(resolve(values.out), encoded);
    console.log(canonical({ jobId: job.jobId, entries: result.pack.entries.length, wallMs: result.wallMs,
      packSha256: result.packSha256, receiptSha256: sha(encoded) }));
    return;
  }
  const budgets = [...new Set((values['pack-bytes'] ?? '524288').split(',').map((part) => part === 'full'
    ? part : integer(Number(part), 1, maxGzipBytes, 'per-pack budget')))];
  if (updateIndex && budgets.length !== 1) throw Error('--update-index requires a single --pack-bytes budget');
  const candidates = await eachJson(values.receipts, (value) => checkedReceipt(value, release), replaceIndex);
  const totalBytes = integer(Number(values['total-bytes'] ?? Number.MAX_SAFE_INTEGER), 1, Number.MAX_SAFE_INTEGER, 'total budget');
  const rawBytes = integer(Number(values['raw-bytes'] ?? maxRawBytes), 1, maxRawBytes, 'uncompressed budget');
  const jobs = command === 'assemble' ? await eachJson(values.jobs, (job) => checkedJob(job, release)) : null;
  const allowed = new Set(jobs?.map((job) => job.jobId));
  const seen = new Set();
  for (const candidate of candidates) {
    const { job, requestSha256 } = candidate;
    if (seen.has(job.jobId)) throw Error('Duplicate job receipt');
    seen.add(job.jobId);
    if (jobs && !allowed.has(job.jobId)) throw Error('Receipt was not assigned by this host');
    if (sha(canonical(await makeRequest(job))) !== requestSha256) throw Error('Official request fingerprint mismatch');
  }
  const receipts = [];
  if (jobs) {
    const runner = await loadRunner();
    for (const candidate of candidates) {
      const verified = await runCapture(runner, await makeRequest(candidate.job), candidate.job, runnerOptions, 'verify');
      receipts.push(verifyCandidate(candidate, verified));
    }
  } else receipts.push(...candidates);
  const basePath = `/search-cache/v${version}/${release.release}/`;
  const groups = rankGroups(receipts, budgets.length === 1 && budgets[0] === 'full');
  for (const budget of budgets) {
    const siteRoot = resolve(values.out, budgets.length > 1 ? String(budget) : '.');
    const out = join(siteRoot, 'search-cache', `v${version}`, release.release);
    const full = budget === 'full';
    const result = await writeSearchCache({ out, groups, packBytes: full ? maxGzipBytes : budget, totalBytes, rawBytes, full, updateIndex });
    console.log(canonical({ release, budget, siteRoot, basePath, ...result }));
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
