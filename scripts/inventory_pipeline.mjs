// SPDX-License-Identifier: AGPL-3.0-or-later
// One frozen inventory cut over the existing ranker's command pipes.
import postgres from './pg/node_modules/postgres/src/index.js';
import { DatabaseSync } from 'node:sqlite';
import { createReadStream, createWriteStream, appendFileSync, closeSync, existsSync, fsyncSync, mkdirSync,
  openSync, readFileSync, readdirSync, lstatSync, renameSync, rmSync, statfsSync, writeFileSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { once } from 'node:events';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { BOARD_KINDS } from '../worker/board_kinds.js';
import { canon } from '../worker/task_contract.js';
import { inventoryWrite } from '../worker/task_inventory.js';
import { captureInventory } from './capture_compute_inventory.mjs';
import { openCatalog, readInventoryRows } from './incremental_inventory.mjs';
import { shardInventory, auditInventoryInputs } from './inventory_inputs.mjs';
import { pipelineMonitor } from './pipeline_monitor.mjs';

const MAX_CAPTURE_MS = 45 * 60000, RESERVE = 8 * 1024 ** 3, LIMIT = 256;
const progress = (stage, fields = {}) => console.error(JSON.stringify({ event: 'incremental_inventory', stage, ...fields }));
const load = path => JSON.parse(readFileSync(path, 'utf8'));
function sync(path) { const fd = openSync(path, 'r'); try { fsyncSync(fd); } finally { closeSync(fd); } }
function save(path, value) {
  const partial = `${path}.partial-${process.pid}`;
  writeFileSync(partial, JSON.stringify(value)); sync(partial); renameSync(partial, path); sync(dirname(path));
}
function disk(path) {
  const stat = statfsSync(path);
  if (stat.bavail * stat.bsize < RESERVE) throw new Error('inventory needs 8GiB of filesystem reserve');
}
function collect(root, state) {
  const catalogs = new Set([state.catalog, state.previous_catalog].filter(Boolean));
  const pending = state.pending ? dirname(state.pending.capture) : null;
  for (const name of readdirSync(root)) {
    const path = join(root, name);
    if (lstatSync(path).isSymbolicLink()) continue;
    if (/^capture-[0-9]+$/.test(name) && path !== pending) rmSync(path, { recursive: true, force: true });
    if (/^generation-[0-9]+\.sqlite(?:-wal|-shm)?$/.test(name)
      && !catalogs.has(path.replace(/-(?:wal|shm)$/, ''))) rmSync(path, { force: true });
  }
}
async function hashFile(path) {
  const hash = createHash('sha256');
  for await (const data of createReadStream(path)) hash.update(data);
  return hash.digest('hex');
}
async function writeRows(path, input, report = () => {}) {
  const stream = createWriteStream(path); let failed = null, count = 0;
  stream.on('error', error => { failed = error; });
  try {
    for await (const row of input) {
      if (failed) throw failed;
      if (!stream.write(JSON.stringify(row) + '\n')) await once(stream, 'drain');
      count++;
      if (count % 20000 === 0) report(count);
    }
    if (failed) throw failed;
    stream.end(); await once(stream, 'finish');
    if (failed) throw failed;
    return count;
  } finally { stream.destroy(); }
}
async function* batches(input, limit = LIMIT) {
  let batch = [];
  for await (const row of readInventoryRows(input)) {
    batch.push(row);
    if (batch.length === limit) { yield batch; batch = []; }
  }
  if (batch.length) yield batch;
}
async function* outputPaths(path) {
  if (existsSync(path)) for await (const row of readInventoryRows(path)) yield row.path;
}
async function* cursor(tx, name, query, args = []) {
  await tx.unsafe(`DECLARE ${name} NO SCROLL CURSOR FOR ${query}`, args);
  try {
    for (;;) {
      const rows = await tx.unsafe(`FETCH FORWARD 5000 FROM ${name}`);
      yield* rows;
      if (rows.length < 5000) break;
    }
  } finally { await tx.unsafe(`CLOSE ${name}`); }
}
export function rankerChannel() {
  const reader = createInterface({ input: process.stdin, crlfDelay: Infinity }), replies = reader[Symbol.asyncIterator]();
  return { close: () => { reader.close(); process.stdin.pause(); },
    call: async (request, output, timeout = MAX_CAPTURE_MS) => {
      const path = `${output}.request.json`;
      save(path, request);
      process.stdout.write(`inventory ${path} ${output}\n`);
      let timer;
      try {
        const reply = await Promise.race([replies.next(), new Promise((_, reject) => {
          timer = setTimeout(() => reject(new Error('inventory ranker command timed out')), timeout);
        })]);
        if (reply.done || !reply.value.startsWith('ok inventory ')) throw new Error(`inventory ranker refused: ${reply.value || 'closed pipe'}`);
      } finally { clearTimeout(timer); }
    } };
}
const common = receipt => ({ v: 1, engine: receipt.engine, contract: receipt.contract,
  generation: receipt.generation, epoch: receipt.epoch, snapshot: receipt.snapshot });

export async function captureEpoch({ sql, catalog, generation, dir, bootstrap, call, baseEpoch, relational = false, report = progress }) {
  mkdirSync(dir, { recursive: true }); disk(dir);
  const paths = [], started = Date.now(), covers = JSON.stringify(BOARD_KINDS), captureMs = bootstrap ? MAX_CAPTURE_MS : 15 * 60000;
  const check = () => { disk(dir); if (Date.now() - started > captureMs) throw new Error('inventory input capture exceeded its budget'); };
  const file = name => { const path = join(dir, name); paths.push(path); return path; };
  const raw = { library: file('library.json'), builds: file('builds.ndjson'), facts: file('facts.ndjson'),
    queue: file('queue.ndjson'), tasks: file('tasks.ndjson'), meta: file('snapshot.json') };
  return sql.begin('isolation level repeatable read', async transaction => {
    const tx = { unsafe: async (...args) => { check(); return transaction.unsafe(...args); } };
    await tx.unsafe("SET LOCAL statement_timeout='180s'");
    await tx.unsafe("SET LOCAL idle_in_transaction_session_timeout='60s'");
    await tx.unsafe("SET LOCAL lock_timeout='3s'");
    const [{ receipt }] = await tx.unsafe('SELECT inventory_epoch_begin($1::text) receipt', [generation]);
    const changes = file('changes.ndjson');
    report('capture-changes', { epoch: receipt.epoch, rows: 0 });
    const changeCount = await writeRows(changes, cursor(tx, 'inventory_tokens', 'SELECT * FROM inventory_epoch_changes($1::text)', [receipt.epoch]),
      rows => report('capture-changes', { epoch: receipt.epoch, rows }));
    report('captured-changes', { epoch: receipt.epoch, rows: changeCount });
    const result = { receipt, raw, changes, bootstrap, applies: [], scopes: [], engine_epoch: baseEpoch };
    let heartbeatError = null;
    const heartbeat = setInterval(() => { transaction.unsafe('SELECT 1').catch(error => { heartbeatError = error; }); }, 20000);
    try {
      if (bootstrap) {
        const meta = await captureInventory({ begin: async (_options, fn) => fn(tx) }, raw, receipt.engine, BOARD_KINDS, report);
        if (meta.snapshot !== receipt.snapshot || meta.started_at !== receipt.started_at) throw new Error('inventory bootstrap snapshot changed');
        result.meta = meta;
      } else if (relational) {
        result.delta = file('delta.ndjson');
        async function* changedInputs() {
          for await (const chunk of batches(changes, 64)) {
            const [{ input }] = await tx.unsafe('SELECT inventory_inputs($1::text,$2::text,$3::text) input',
              [receipt.engine, covers, JSON.stringify(chunk)]);
            if (input.snapshot !== receipt.snapshot || input.generation !== receipt.generation) throw new Error('inventory input fence changed');
            for (const row of input.builds) yield { kind: 'reset', row: { id: row.id } };
            for (const kind of ['builds', 'facts', 'queue', 'tasks']) for (const row of input[kind]) yield { kind, row };
          }
        }
        await writeRows(result.delta, changedInputs());
      } else {
        const index = new DatabaseSync(join(dir, 'scopes.sqlite'));
        index.exec('PRAGMA cache_size=-16384;PRAGMA temp_store=FILE;CREATE TABLE scopes(weapon TEXT PRIMARY KEY);CREATE TABLE aliases(weapon TEXT,id TEXT,PRIMARY KEY(weapon,id)) WITHOUT ROWID;');
        const scopePut = index.prepare('INSERT OR IGNORE INTO scopes VALUES(?)'), aliasPut = index.prepare('INSERT OR IGNORE INTO aliases VALUES(?,?)');
        let number = 0;
        try {
          for await (const chunk of batches(changes, 64)) {
            const [{ input }] = await tx.unsafe('SELECT inventory_inputs($1::text,$2::text,$3::text) input', [receipt.engine, covers, JSON.stringify(chunk)]);
            if (input.snapshot !== receipt.snapshot || input.generation !== receipt.generation) throw new Error('inventory input fence changed');
            const prefix = `apply-${number++}`, builds = file(`${prefix}.builds`), facts = file(`${prefix}.facts`), tasks = file(`${prefix}.tasks`);
            await writeRows(builds, input.builds.map(row => ({ ...row, record: row.deleted ? null : row.record })));
            await writeRows(facts, input.facts); await writeRows(tasks, input.tasks);
            for (const row of input.builds) {
              const old = catalog.owner(row.id);
              if (old?.weapon) { scopePut.run(old.weapon); aliasPut.run(old.weapon, row.id); }
              for (const owner of catalog.statement(`SELECT DISTINCT o.weapon FROM owners o JOIN records r ON r.id=o.candidate WHERE r.build_id=? AND o.weapon IS NOT NULL`).iterate(row.id)) {
                scopePut.run(owner.weapon); aliasPut.run(owner.weapon, row.id);
              }
            }
            let owners = null;
            if (input.builds.length) {
              owners = file(`${prefix}.owners`);
              await call({ ...common(receipt), op: 'apply', base_epoch: result.engine_epoch, builds, facts,
                fact_ids: input.builds.map(row => row.id), counts: { builds: input.builds.length, facts: input.facts.length } }, owners, Math.max(1, captureMs - (Date.now() - started)));
              result.engine_epoch = receipt.epoch;
              for await (const owner of readInventoryRows(owners)) if (owner.type === 'owner' && owner.weapon) {
                scopePut.run(owner.weapon);
                if (owner.build_id) aliasPut.run(owner.weapon, owner.build_id);
              }
            }
            result.applies.push({ builds, facts, tasks, owners });
          }
          for (const scope of index.prepare('SELECT weapon FROM scopes ORDER BY weapon').iterate()) {
            for (const row of catalog.statement(`SELECT DISTINCT r.build_id id FROM owners o JOIN records r ON r.id=o.candidate
              WHERE o.weapon=? AND (r.stored IS NULL OR json_extract(r.stored,'$.weapon')<>o.weapon)`).iterate(scope.weapon)) aliasPut.run(scope.weapon, row.id);
            const key = createHash('sha256').update(scope.weapon).digest('hex').slice(0, 16), prefix = `scope-${key}`;
            const files = { builds: file(`${prefix}.builds`), facts: file(`${prefix}.facts`), queue: file(`${prefix}.queue`) };
            const streams = Object.fromEntries(Object.entries(files).map(([kind, path]) => [kind, createWriteStream(path)]));
            const counts = { builds: 0, facts: 0, queue: 0 }; let streamError = null;
            for (const stream of Object.values(streams)) stream.on('error', error => { streamError = error; });
            const collect = async specification => {
              for await (const row of cursor(tx, 'inventory_scope', 'SELECT * FROM inventory_cohort_rows($1::text,$2::text,$3::text)', [receipt.engine, covers, JSON.stringify(specification)])) {
                if (streamError) throw streamError;
                if (!streams[row.kind]) throw new Error('unknown inventory cohort input');
                if (!streams[row.kind].write(JSON.stringify(row.body) + '\n')) await once(streams[row.kind], 'drain');
                counts[row.kind]++;
              }
            };
            try {
              await collect({ weapons: [scope.weapon], identities: [] });
              let aliases = [];
              for (const alias of index.prepare('SELECT id FROM aliases WHERE weapon=? ORDER BY id').iterate(scope.weapon)) {
                aliases.push(alias.id);
                if (aliases.length === LIMIT) { await collect({ weapons: [], identities: aliases }); aliases = []; }
              }
              if (aliases.length) await collect({ weapons: [], identities: aliases });
              for (const stream of Object.values(streams)) { stream.end(); await once(stream, 'finish'); }
              if (streamError) throw streamError;
            } finally { for (const stream of Object.values(streams)) stream.destroy(); }
            result.scopes.push({ key, scope: { weapon: scope.weapon }, ...files, counts });
          }
        } finally { index.close(); }
      }
      if (heartbeatError) throw heartbeatError;
      const manifest = [];
      for (const path of paths) if (existsSync(path)) { check(); sync(path); manifest.push({ path, sha256: await hashFile(path) }); }
      const manifestPath = join(dir, 'capture.json'); save(manifestPath, { ...result, files: manifest });
      const digest = await hashFile(manifestPath);
      const [{ sealed }] = await tx.unsafe('SELECT inventory_epoch_seal($1::text,$2::bigint,$3::text) sealed', [receipt.epoch, changeCount, digest]);
      report('captured', { epoch: sealed.epoch, changes: changeCount, scopes: result.scopes.length });
      return { ...result, receipt: sealed, manifest_sha: digest, capture: manifestPath };
    } finally { clearInterval(heartbeat); }
  });
}

export { save as saveInventoryState, hashFile as hashInventoryFile, collect as collectInventoryFiles, disk as checkInventoryDisk };

export async function runInventoryCycle({ work, bin, instance, sql, call, budgetMs = 15000, report = progress }) {
  const root = join(work, 'inventory'); mkdirSync(root, { recursive: true }); disk(root);
  const statePath = join(root, 'state.json'), source = readFileSync(join(bin, 'VERSION'), 'utf8').trim();
  const engine = readFileSync(join(bin, 'ENGINE'), 'utf8').trim();
  const algorithm = createHash('sha256');
  for (const file of ['./classify_compute_inventory.mjs', './incremental_inventory.mjs', './inventory_inputs.mjs',
    './inventory_pipeline.mjs', '../worker/task_contract.js']) algorithm.update(readFileSync(new URL(file, import.meta.url)));
  const contract = { v: 1, algorithm: 'board-inventory-v1', covers: BOARD_KINDS, adapter: await hashFile(join(bin, 'wfsim-board')),
    classifier: algorithm.digest('hex') };
  let state = existsSync(statePath) ? load(statePath) : null;
  report('generation');
  const [{ status }] = await sql.unsafe('SELECT inventory_status() status');
  if (!state || state.instance !== instance || state.generation !== status?.generation || canon(status.contract) !== canon(contract) || status.engine !== engine) {
    const [{ generation }] = await sql.unsafe('SELECT inventory_begin($1::text,$2::text,$3::text) generation', [engine, JSON.stringify(contract), status?.generation || null]);
    const previous = state?.published_at ? state.catalog : state?.previous_catalog;
    state = { generation: generation.generation, instance, engine, contract, catalog: join(root, `generation-${generation.generation}.sqlite`),
      previous_catalog: previous, engine_epoch: null, pending: null };
    save(statePath, state);
  }
  collect(root, state);
  report('catalog', { generation: state.generation });
  const catalog = openCatalog(state.catalog, { engine, contract, generation: state.generation, create: true });
  const started = Date.now();
  try {
    if (!state.pending && state.published_at && Date.now() - Date.parse(state.published_at) < 60000)
      return { pending: false, engine_epoch: state.engine_epoch };
    if (!state.pending) {
      const dir = join(root, `capture-${Date.now()}`);
      report('capture');
      state.pending = await captureEpoch({ sql, catalog, generation: state.generation, dir, bootstrap: !catalog.meta.complete, call, baseEpoch: state.engine_epoch, report });
      state.engine_epoch = state.pending.engine_epoch; state.pending.phase = 'preparing';
      save(statePath, state);
    }
    const cut = state.pending, receipt = cut.receipt, dir = dirname(cut.capture);
    catalog.beginEpoch(receipt);
    if (cut.phase === 'preparing' && catalog.meta.epoch.phase !== 'preparing') {
      if (cut.bootstrap) {
        report('bootstrap', { epoch: receipt.epoch });
        const shards = load(join(dir, 'shards/manifest.json'));
        cut.scopes = shards.scopes.map(scope => ({ ...scope, scope: scope.scope || { weapon: scope.weapon, riven: scope.riven } }));
        cut.canonical = shards.canonical; state.engine_epoch = receipt.epoch;
      }
      cut.phase = 'processing'; save(statePath, state);
    }
    if (cut.phase === 'preparing') {
      if (cut.bootstrap) {
        report('bootstrap', { epoch: receipt.epoch });
        await catalog.importRecords(cut.raw.builds); await catalog.importLifecycle(cut.raw.tasks, { bootstrap: true });
        const owners = join(dir, 'bootstrap.owners');
        await call({ ...common(receipt), op: 'bootstrap', builds: cut.raw.builds, facts: cut.raw.facts,
          counts: { builds: cut.meta.counts.library, facts: cut.meta.counts.facts } }, owners);
        state.engine_epoch = receipt.epoch;
        await catalog.applyOwners(owners, { op: 'bootstrap' });
        const shards = await shardInventory({ ...cut.raw, owners, out: join(dir, 'shards'), ...common(receipt) });
        cut.scopes = shards.scopes.map(scope => ({ ...scope, scope: scope.scope || { weapon: scope.weapon, riven: scope.riven } }));
        cut.canonical = shards.canonical;
      } else {
        for (const apply of cut.applies) {
          await catalog.importRecords(apply.builds); await catalog.importLifecycle(apply.tasks);
          if (apply.owners) await catalog.applyOwners(apply.owners, { op: 'apply' });
        }
      }
      for (const scope of cut.scopes) catalog.queueJob(scope.key, scope.scope);
      catalog.freezeEpoch(); cut.phase = 'processing'; save(statePath, state);
    }
    let chunks = 0;
    while (catalog.jobs(1).length && (chunks === 0 || Date.now() - started < budgetMs)) {
      disk(root);
      const job = catalog.jobs(1)[0], scope = cut.scopes.find(scope => scope.key === job.key);
      if (!scope) throw new Error('inventory cohort manifest is unavailable');
      const output = join(dir, `members-${job.key}-${job.after_owner || 'start'}.ndjson`);
      report('cohort', { epoch: receipt.epoch, chunks });
      await call({ ...common(receipt), op: 'cohort', scope: job.scope, facts: scope.facts, queue: scope.queue,
        counts: { facts: scope.counts.facts, queue: scope.counts.queue }, after_owner: job.after_owner, limit: 64 }, output);
      appendFileSync(join(dir, 'outputs.ndjson'), JSON.stringify({ path: output }) + '\n'); sync(join(dir, 'outputs.ndjson'));
      await catalog.replaceOwners(output, { job, snapshot: receipt.snapshot }); chunks++;
    }
    if (catalog.jobs(1).length) { report('processing', { epoch: receipt.epoch, chunks }); return { pending: true, engine_epoch: state.engine_epoch }; }
    if (!catalog.meta.complete) {
      report('audit', { epoch: receipt.epoch });
      const rulers = execFileSync(join(bin, 'wfsim-board'), ['--rulers'], { encoding: 'utf8' }).trim().split(/\s+/);
      const audit = await auditInventoryInputs({ ...cut.raw, meta: cut.meta, rulers, engineOutputs: outputPaths(join(dir, 'outputs.ndjson')),
        contract, generation: receipt.generation, epoch: receipt.epoch, out: join(dir, 'audit') });
      catalog.seal({ snapshot: receipt.snapshot, started_at: receipt.started_at, expected: { owners: cut.meta.counts.library,
        records: cut.meta.counts.library, tasks: cut.meta.counts.tasks, rows: audit.plan.rows, queued: audit.plan.queued, digest: audit.plan.question_digest } });
      report('audited', { epoch: receipt.epoch, rows: audit.plan.rows });
    }
    report('acknowledging', { epoch: receipt.epoch });
    for await (const changes of batches(cut.changes)) {
      catalog.recordReceipts(changes);
      await sql.unsafe('SELECT inventory_ack($1::text,$2::text)', [state.generation, JSON.stringify(changes)]);
      catalog.acknowledge(changes);
    }
    const plan = { ...catalog.finishEpoch(), source }, body = JSON.parse(inventoryWrite(plan).params[1]);
    report('publishing', { epoch: receipt.epoch });
    const [{ published }] = await sql.unsafe('SELECT inventory_publish_cut($1::text,$2::text) published',
      [receipt.epoch, JSON.stringify({ ...body, generation: state.generation, contract })]);
    catalog.activate(published); save(join(root, 'last-publication.json'), { receipt: published, capture: load(cut.capture) });
    state.published_at = published.at; state.pending = null; save(statePath, state); collect(root, state);
    report('published', { epoch: receipt.epoch, total: published.total, started_at: published.started_at });
    return { pending: false, published, engine_epoch: state.engine_epoch };
  } finally { catalog.close(); }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const [work, bin, instance] = process.argv.slice(2);
  if (!work || !bin || !instance || /\s/.test(work + bin) || !process.env.PG_URL) throw new Error('usage: inventory_pipeline.mjs <work> <bin> <ranker-instance> (PG_URL required; paths without spaces)');
  const sql = postgres(process.env.PG_URL, { max: 1, fetch_types: false, connect_timeout: 15, onnotice: () => {}, connection: { application_name: 'wfsim-inventory-incremental' } });
  const engine = rankerChannel();
  const monitor = pipelineMonitor(join(work, 'diagnostics'), 'inventory', {
    source: readFileSync(join(bin, 'VERSION'), 'utf8').trim(), engine: readFileSync(join(bin, 'ENGINE'), 'utf8').trim() });
  try {
    const result = await runInventoryCycle({ work: resolve(work), bin: resolve(bin), instance, sql, call: engine.call, report: monitor.report });
    save(join(work, 'inventory/status.json'), result); monitor.finish(result.pending ? 'pending' : 'complete');
  } catch (error) { monitor.finish('failed', error); process.exitCode = 1; }
  finally { engine.close(); await sql.end({ timeout: 5 }); }
}
