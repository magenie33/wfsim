// SPDX-License-Identifier: AGPL-3.0-or-later
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDeepStrictEqual } from 'node:util';
import { runInNewContext } from 'node:vm';
import { buildOfficialRequest } from './bake_search_cache_request.mjs';

const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
const json = async (path) => JSON.parse(await readFile(path));
const scoped = /^data\/(?:weapons|kitguns|zaws)\/(?:[a-z0-9_]+\/)*[a-z0-9_]+\.yaml$/;
const id = /^[a-z0-9_]+$/;
const hash12 = /^[0-9a-f]{12}$/;
const object = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const fail = (label) => { throw Error(label); };

async function namedBytes(directory, name, expected) {
  const bytes = await readFile(join(directory, name));
  if (!digest(bytes).startsWith(expected)) fail(`Asset SHA-256 mismatch: ${name}`);
  return bytes;
}

function one(names, pattern) {
  const matches = names.filter((name) => typeof name === 'string' && pattern.test(name));
  if (matches.length !== 1) fail(`Missing or ambiguous release asset ${pattern}`);
  return matches[0];
}

function constant(source, name) {
  const matches = [...source.matchAll(new RegExp(`^const ${name} = (.+);$`, 'gm'))];
  if (matches.length !== 1) fail(`Missing or ambiguous release constant ${name}`);
  return JSON.parse(matches[0][1]);
}

async function engineFiles(site, engine) {
  const manifest = await json(join(site, 'engines', `${engine}.json`));
  if (manifest.engine !== engine || !object(manifest.files) || typeof manifest.files.rest !== 'string'
      || !/^[0-9a-f]{16}$/.test(manifest.files.rest)) {
    fail('Invalid engine manifest or rest digest');
  }
  const hash = createHash('sha256');
  for (const name of Object.keys(manifest.files).sort()) {
    const value = manifest.files[name];
    if (name !== 'rest' && (!scoped.test(name) || typeof value !== 'string' || !hash12.test(value))) fail(`Unknown engine file: ${name}`);
    hash.update(name + '\0' + value + '\0');
  }
  if (hash.digest('hex').slice(0, 12) !== engine) fail(`Engine manifest fingerprint mismatch: ${engine}`);
  return manifest.files;
}

async function snapshot(site) {
  const metadata = await json(join(site, 'release.json'));
  const { release, engine, data, wasm, packs, accepted, commit, ...wire } = metadata;
  if (![release, engine, data, wasm].every((n) => typeof n === 'string' && hash12.test(n))
      || !Array.isArray(packs) || wire.data_format !== 1 || !Number.isSafeInteger(wire.protocol)
      || wire.protocol <= 0 || !Array.isArray(wire.protocols_served) || !wire.protocols_served.includes(wire.protocol)) {
    fail('Invalid release fingerprint or wire contract');
  }
  const pkg = join(site, 'pkg'), asset = join(site, 'asset');
  const pkgNames = (await json(join(pkg, 'generation.json'))).names;
  const assetNames = (await json(join(asset, 'generation.json'))).names;
  const wasmName = `wfsim_wasm_bg.${wasm}.wasm`, glueName = `wfsim_wasm.${wasm}.js`, dataName = `pack.data.${data}.json`;
  const appName = one(assetNames, /^app\.[0-9a-f]{12}\.js$/);
  const workerName = one(assetNames, /^worker\.[0-9a-f]{12}\.js$/);
  const metaName = one(pkgNames, /^meta\.[0-9a-f]{12}\.json$/);
  const generatedPacks = pkgNames.filter((name) => name.startsWith('pack.')).sort();
  if (one(pkgNames, /^wfsim_wasm_bg\..+\.wasm$/) !== wasmName || one(pkgNames, /^wfsim_wasm\..+\.js$/) !== glueName
      || one(pkgNames, /^pack\.data\..+\.json$/) !== dataName || !isDeepStrictEqual(generatedPacks, [...packs].sort())) {
    fail('Package generation does not match release.json');
  }
  for (const name of generatedPacks) {
    if (name === dataName) continue;
    const match = /^pack\.[a-z0-9_]+\.([0-9a-f]{12})\.json$/.exec(name);
    if (!match) fail('Invalid release pack name');
    await namedBytes(pkg, name, match[1]);
  }
  const app = (await namedBytes(asset, appName, appName.split('.')[1])).toString('utf8');
  const worker = (await namedBytes(asset, workerName, workerName.split('.')[1])).toString('utf8');
  const html = await readFile(join(site, 'index.html'), 'utf8');
  const scripts = [...html.matchAll(/<script\s+[^>]*?src=["']\/(asset\/app\.[0-9a-f]+\.js)["']/g)];
  if (scripts.length !== 1 || scripts[0][1] !== `asset/${appName}` || constant(app, 'RELEASE_ID') !== release
      || constant(app, 'ENGINE_ID') !== engine || constant(app, 'ENGINE_WASM') !== `/pkg/${wasmName}`
      || !isDeepStrictEqual(constant(app, 'ENGINE_PACKS'), [`/pkg/${dataName}`])
      || constant(app, 'ENGINE_VIEWS')['/api/meta'] !== `/pkg/${metaName}`
      || !app.includes(`new Worker("/asset/${workerName}")`) || !worker.includes(`importScripts("/pkg/${glueName}")`)) {
    fail('Page, worker and release manifest refer to different assets');
  }
  const packBytes = await namedBytes(pkg, dataName, data);
  const pack = JSON.parse(packBytes);
  if (pack.format !== wire.data_format || pack.pack !== 'data' || !object(pack.files)
      || !isDeepStrictEqual(Object.keys(pack).sort(), ['files', 'format', 'pack'])) fail('Invalid official data pack');
  const files = await engineFiles(site, engine);
  const scopes = Object.keys(pack.files).map((p) => `data/${p}`).filter((p) => scoped.test(p)).sort();
  if (!scopes.length || !isDeepStrictEqual(scopes, Object.keys(files).filter((p) => p !== 'rest').sort())) {
    fail('Incomplete engine manifest for official battle data');
  }
  return { site, identity: { release, engine, data, wasm }, accepted, wire, files, app, worker, pack, packBytes,
    meta: JSON.parse(await namedBytes(pkg, metaName, metaName.split('.')[1])),
    wasm: await namedBytes(pkg, wasmName, wasm), glue: await readFile(join(pkg, glueName)) };
}

const difference = (before, after) => [...new Set([...Object.keys(before), ...Object.keys(after)])]
  .filter((key) => before[key] !== after[key]).sort();

// Read ownership from both snapshots. An inheritance or family link can disappear in the newer one.
const dependencies = String.raw`
import sys, json, hashlib, functools
sys.path.insert(0, sys.argv[1])
import build_site_app as build
old, new, paths = json.load(sys.stdin)
paths = set(paths)
near, affected, covered = {}, set(), set()
chambers = {p.rsplit('/', 1)[-1][:-5]: p for p in paths if p.startswith('data/kitguns/chambers/')}
kitgun_parts = {p for p in paths if p.startswith('data/kitguns/') and p not in chambers.values()}
zaw_parts = {p for p in paths if p.startswith('data/zaws/')}
@functools.lru_cache(None)
def axes(text):
    w = build.yload(text)
    return w['id'], w.get('inherits'), w.get('transform_group'), w.get('riven_family'), w.get('kitgun'), bool(w.get('zaw'))
for data, manifest in (old, new):
    if tuple(build.yload(data['prose.yaml'])['keys']) != build.prose_keys():
        raise ValueError('unrecognized release prose schema')
    for p in paths:
        if p[5:] in data:
            actual = hashlib.sha256(build.without_prose(data[p[5:]]).encode()).hexdigest()[:12]
            if actual != manifest[p]:
                raise ValueError('scoped manifest digest mismatch: ' + p)
    for p, text in data.items():
        if not p.startswith('weapons/'):
            continue
        weapon, parent, group, family, kitgun, zaw = axes(text)
        node = ('w', weapon)
        owned = {'data/' + p} & paths
        if kitgun:
            owned.update(kitgun_parts)
            if kitgun in chambers:
                owned.add(chambers[kitgun])
        if zaw:
            owned.update(zaw_parts)
        if owned:
            covered.update(owned)
            affected.add(node)
        for kind, key in [('w', parent), ('t', group), ('r', family)]:
            if key:
                other = (kind, key)
                near.setdefault(node, set()).add(other)
                near.setdefault(other, set()).add(node)
if covered != paths:
    raise ValueError('changed weapon data has no known owner: ' + ', '.join(sorted(paths - covered)))
todo = list(affected)
while todo:
    for linked in near.get(todo.pop(), ()):
        if linked not in affected:
            affected.add(linked)
            todo.append(linked)
print(json.dumps(sorted(key for kind, key in affected if kind == 'w')))
`;

function affectedWeapons(old, next, paths) {
  const input = JSON.stringify([[old.pack.files, old.files], [next.pack.files, next.files], paths]);
  const scripts = fileURLToPath(new URL('./', import.meta.url));
  const python = process.env.WFSIM_PYTHON || (process.platform === 'win32' ? 'python' : 'python3');
  try {
    return JSON.parse(execFileSync(python, ['-I', '-B', '-X', 'utf8', '-c', dependencies, scripts],
      { input, encoding: 'utf8', stdio: 'pipe', windowsHide: true, timeout: 60_000, maxBuffer: 4 * 1024 ** 2 }));
  } catch (error) {
    const reason = error.stderr?.trim().split('\n').at(-1) || error.code || 'invalid dependency data';
    fail(`Weapon dependency check failed: ${reason}`);
  }
}

async function compatible(old, next, weapon) {
  if (!old.wasm.equals(next.wasm) || !old.glue.equals(next.glue) || old.worker !== next.worker) {
    fail('WASM, bindings or worker wire changed');
  }
  if (!isDeepStrictEqual(old.wire, next.wire)) fail('Release protocol, data format or declared schema changed');
  if (old.files.rest !== next.files.rest) fail('Shared engine, RNG, scoring or gas inputs changed (rest)');
  const paths = difference(old.files, next.files);
  const dataPaths = difference(old.pack.files, next.pack.files).map((name) => `data/${name}`);
  if (old.identity.engine === next.identity.engine && old.identity.data === next.identity.data) {
    if (paths.length || !old.packBytes.equals(next.packBytes)) fail('Equal fingerprints with different inputs');
    return [];
  }
  const grants = next.accepted?.filter((entry) => entry?.engine === old.identity.engine);
  const grant = grants?.length === 1 && grants[0];
  if (!grant || (Object.hasOwn(grant, 'join') && grant.join !== false)
      || Object.keys(grant).some((k) => !['engine', 'changed', 'until', 'join'].includes(k))
      || !Array.isArray(grant.changed) || !grant.changed.length
      || !grant.changed.every((v) => typeof v === 'string' && id.test(v))
      || !(Date.parse(grant.until) > Date.now())) fail('No current unambiguous weapon-scoped compatibility grant');
  if (grant.changed.includes(weapon)) fail(`Release marks ${weapon} changed`);
  if (!paths.length || !isDeepStrictEqual(paths, dataPaths) || !paths.every((p) => scoped.test(p))) {
    fail('Battle data differences are not exactly the changed weapon files in the manifest');
  }
  if (!isDeepStrictEqual(await engineFiles(next.site, old.identity.engine), old.files)) fail('Retained old engine manifest mismatch');
  const affected = affectedWeapons(old, next, paths);
  const granted = new Set(grant.changed);
  if (!affected.length || affected.includes(weapon) || affected.some((w) => !granted.has(w))) {
    fail('Compatibility grant omits an affected weapon or dependency');
  }
  return paths;
}

function normalizer(snap) {
  // Freeze the task with already verified bytes. Only this non-simulating endpoint is called.
  const binding = runInNewContext(`${snap.glue}\nwasm_bindgen.initSync({module: bytes}); wasm_bindgen;`,
    { WebAssembly, TextEncoder, TextDecoder, bytes: snap.wasm });
  const error = binding.install_data_bytes(snap.packBytes);
  if (error) fail(`Cannot install release data: ${error}`);
  return (request) => {
    const reply = JSON.parse(binding.api('/api/computation/input', JSON.stringify({ kind: 'optimization', request })));
    if (reply.ok !== true || !object(reply.request) || !reply.schema) fail('Official request normalization failed');
    return reply;
  };
}

async function scenarioHash(snap, frozen, ruler, hostSource) {
  let source = hostSource;
  if (snap.app.includes('function searchWarmIdentity(')) {
    const start = snap.app.search(/^const searchWarm(?:MiB|Limits) =/m);
    const end = snap.app.indexOf('\nfunction searchWarmReceipt', start);
    if (start < 0 || end < 0) fail('Unrecognized release cache identity implementation');
    source = snap.app.slice(start, end);
  }
  const opts = { ...snap.identity, official: ruler };
  const result = await runInNewContext(`${source}\nsearchWarmIdentity(JSON.parse(body), JSON.parse(options));`, {
    body: JSON.stringify(frozen), options: JSON.stringify(opts), crypto: globalThis.crypto, TextEncoder,
  });
  return result?.scenario;
}

export async function proveSearchReuse(oldSite, newSite, receipt, templates, canonical) {
  const old = await snapshot(oldSite), next = await snapshot(newSite);
  if (!isDeepStrictEqual(receipt.job.release, old.identity)) fail('Receipt does not name the old release');
  const { weapon, ruler, template } = receipt.job;
  const changedPaths = await compatible(old, next, weapon);
  const oldNormalize = normalizer(old), newNormalize = normalizer(next);
  const cacheSource = await readFile(new URL('../web/src/static/app/07-warm-search-cache.js', import.meta.url), 'utf8');
  for (const [name, settings] of Object.entries(templates)) {
    if (settings.excludeMods && name !== template) continue;
    const opts = { weapon, ruler, ...settings };
    const before = buildOfficialRequest(old.meta, old.app, opts), after = buildOfficialRequest(next.meta, next.app, opts);
    if (!isDeepStrictEqual(before, after)) fail(`Official raw request changed for ${name}`);
    const a = oldNormalize(before), b = newNormalize(after);
    if (!isDeepStrictEqual(a, b)) fail(`Frozen task, seed or request schema changed for ${name}`);
    if (name === template) {
      if (digest(canonical(before)) !== receipt.requestSha256) fail('Official request SHA-256 mismatch');
      for (const [snap, frozen] of [[old, a.request], [next, b.request]]) {
        if (await scenarioHash(snap, frozen, ruler, cacheSource) !== receipt.pack.identity.scenario) {
          fail('Official frozen scenario digest mismatch');
        }
      }
    }
  }
  return { release: next.identity, changedPaths, wasmSha256: digest(next.wasm) };
}
