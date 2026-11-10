// SPDX-License-Identifier: AGPL-3.0-or-later
// One classic coordinator owns the durable projection, native operator and board.
import postgres from './pg/node_modules/postgres/src/index.js';
import { existsSync, readFileSync, mkdirSync, statSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join, dirname } from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { BOARD_KINDS } from '../worker/board_kinds.js';
import { canon } from '../worker/task_contract.js';
import { inventoryWrite, INVENTORY_MAX_MS } from '../worker/task_inventory.js';
import { openCatalog } from './incremental_inventory.mjs';
import { InventoryRelations } from './inventory_relations.mjs';
import { openInventoryOperator } from './inventory_operator.mjs';
import { readInventoryPage, applyInventoryPage, auditInventoryProjection } from './inventory_projection.mjs';
import { InventoryPublication } from './inventory_publication.mjs';
import { captureEpoch, acknowledgeInventory, saveInventoryState as save, hashInventoryFile as hash,
  collectInventoryFiles as collect, checkInventoryDisk as disk } from './inventory_pipeline.mjs';
import { pipelineMonitor } from './pipeline_monitor.mjs';
import { inventoryDiskBudget } from './inventory_disk_budget.mjs';
import { readInventoryCompatibility, compatibilityKey } from './inventory_compatibility.mjs';

const load = path => JSON.parse(readFileSync(path, 'utf8'));
const progress = (stage, fields = {}) => console.error(JSON.stringify({ event: 'board_inventory', stage, ...fields }));
const fingerprint = path => { const s = statSync(path); return [s.size, s.mtimeMs, s.ctimeMs]; };
const algorithmFiles = ['./inventory_board_pipeline.mjs','./inventory_pipeline.mjs','./inventory_projection.mjs',
  './inventory_publication.mjs','./inventory_publication_io.mjs','./json_array_stream.mjs','./inventory_relations.mjs',
  './incremental_inventory.mjs','./inventory_inputs.mjs','./inventory_operator.mjs','./capture_compute_inventory.mjs',
  './classify_compute_inventory.mjs','./board_heads.mjs','./board_meta.py','./inventory_disk_budget.mjs',
  '../worker/task_contract.js','../worker/task_inventory.js','../worker/board_kinds.js'];
const publicationFiles = ['./inventory_publication.mjs','./inventory_publication_io.mjs','./json_array_stream.mjs',
  './board_heads.mjs','./board_meta.py','../mcp/headless.js'];
const catalogFiles = [...algorithmFiles.filter(path => !publicationFiles.includes(path)), './inventory_compatibility.mjs'];
const catalogDependencies = ['./pg/package.json','./pg/package-lock.json','../worker/pg/schema.sql',
  '../worker/pg/library.sql','../worker/pg/sync.sql','../worker/pg/inventory.sql'];
const filesKey = paths => compatibilityKey(Object.fromEntries(paths.map(path => [path,
  existsSync(new URL(path, import.meta.url)) ? createHash('sha256').update(readFileSync(new URL(path, import.meta.url))).digest('hex') : null])));

async function verifyInputs(cut, sql, context) {
  const keys = ['epoch','generation','engine','contract','snapshot','started_at','manifest_sha','change_count','status'];
  const [{ receipt }] = await sql.unsafe('SELECT inventory_epoch_status($1::text) receipt', [cut.receipt.epoch]);
  if (keys.some(key => canon(receipt?.[key]) !== canon(cut.receipt[key]))) throw Error('sealed inventory receipt fence changed');
  const manifest = load(cut.capture), paths = [cut.capture, ...manifest.files.map(file => file.path)];
  const held = context.verifiedInputs;
  if (held?.epoch === cut.receipt.epoch && paths.every(path => canon(held.files[path]) === canon(fingerprint(path)))) return;
  if (await hash(cut.capture) !== cut.receipt.manifest_sha) throw Error('sealed capture manifest changed');
  for (const key of ['raw','changes','delta','bootstrap','meta'])
    if (canon(manifest[key]) !== canon(cut[key])) throw Error('sealed capture paths changed');
  for (const file of manifest.files) if (await hash(file.path) !== file.sha256) throw Error('sealed capture input changed');
  context.verifiedInputs = { epoch: cut.receipt.epoch, files: Object.fromEntries(paths.map(path => [path, fingerprint(path)])) };
}
async function commitPage(index, cut, input, page, kind, next, fault) {
  const catalog = index.catalog, before = catalog.meta;
  catalog.db.exec('BEGIN IMMEDIATE');
  try {
    const started = performance.now();
    await applyInventoryPage(index, index.operate, page, kind, cut.bootstrap);
    next.limit = Math.max(1, Math.min(256, input.limit * 2, Math.floor(input.limit * 100 / Math.max(1, performance.now() - started))));
    catalog.setMeta({ ...catalog.meta, input: next });
    await fault('before-input-commit', { input, next });
    catalog.db.exec('COMMIT');
  } catch (error) {
    if (catalog.db.isTransaction) catalog.db.exec('ROLLBACK');
    catalog.meta = before; catalog.symbolIds.clear(); catalog.symbolNames.clear(); throw error;
  }
  await fault('after-input-commit-before-reply', { input, next });
}
async function ackPage(sql, catalog, generation, cut, offset, fault, report) {
  const groups = []; let next = offset, done = false, bytes = 0;
  while (groups.length < 128) {
    const page = await readInventoryPage(cut.changes, next, 256);
    const size = Buffer.byteLength(JSON.stringify(page.rows));
    if (groups.length && bytes + size > 4 * 1024 ** 2) break;
    if (page.rows.length) { groups.push(page.rows); bytes += size; }
    next = page.offset; done = page.done;
    if (done) break;
  }
  await acknowledgeInventory(sql, catalog, generation, groups, fields => report('acknowledging', fields));
  await fault('after-ack-before-offset', { offset, next });
  catalog.setMeta({ ...catalog.meta, ack: { epoch: cut.receipt.epoch, offset: next } });
  return { offset: next, done };
}

export async function runBoardInventoryCycle({ work, bin, sql, operator: supplied, context = {}, budgetMs = 15000,
  report = progress, fault = async () => {}, onBoard, beforeBoard, now = Date.now, diskBudgetBytes } = {}) {
  const root = join(work, 'inventory'), statePath = join(root, 'state.json'); mkdirSync(root, { recursive: true }); disk(root);
  const source = readFileSync(join(bin, 'VERSION'), 'utf8').trim(), engine = readFileSync(join(bin, 'ENGINE'), 'utf8').trim();
  const adapter = await hash(join(bin, 'wfsim-board'));
  if (context.operatorAdapter && context.operatorAdapter !== adapter)
    throw Object.assign(Error('loaded inventory operator binary changed'), { code: 'INVENTORY_OPERATOR_CHANGED' });
  const operator = supplied || await openInventoryOperator(join(bin, 'wfsim-board'));
  let catalog;
  try {
    if (!['describe','facts','gate','render','floor','carry','state'].every(op => operator.contract.operators.includes(op)))
      throw Error('native board publication operator is unavailable');
    context.operatorAdapter = adapter;
    const sidecar = join(bin, 'inventory-compat.json'), compatibilityStamp = existsSync(sidecar) ? fingerprint(sidecar) : null;
    const cached = context.compatibility;
    const proof = cached?.adapter === adapter && canon(cached.stamp) === canon(compatibilityStamp)
      ? cached.value : readInventoryCompatibility(bin, adapter);
    context.compatibility = { adapter, stamp: compatibilityStamp, value: proof };
    const compatible = catalogDependencies.every(path => existsSync(new URL(path, import.meta.url)))
      ? proof : { mode: 'strict-binary', identity: { binary_sha256: adapter } };
    const contract = { v: 1, algorithm: 'board-inventory-disk-v2', covers: BOARD_KINDS, native: compatible.identity,
      classifier: filesKey([...new Set([...(compatible.mode === 'semantic' ? catalogFiles : [...algorithmFiles, ...publicationFiles, './inventory_compatibility.mjs']),
        ...catalogDependencies])]), operator: compatibilityKey(operator.contract) };
    const publicationKey = compatibilityKey({ native: compatible.identity, runtime: filesKey(publicationFiles) });
    let state = existsSync(statePath) ? load(statePath) : null;
    const same = state && state.engine === engine && canon(state.contract) === canon(contract);
    if (state) collect(root, state);
    inventoryDiskBudget({ work, databasePath: same ? state.catalog : undefined, totalBytes: diskBudgetBytes });
    report('generation');
    const [{ status }] = await sql.unsafe('SELECT inventory_status() status');
    if (same && state.generation !== status?.generation) throw Error('inventory coordinator generation was fenced');
    if (!same || canon(status?.contract) !== canon(contract) || status?.engine !== engine) {
      const [{ generation }] = await sql.unsafe('SELECT inventory_begin($1::text,$2::text,$3::text) generation',
        [engine, JSON.stringify(contract), status?.generation || null]);
      state = { generation: generation.generation, engine, contract, catalog: join(root, `generation-${generation.generation}.sqlite`),
        previous_catalog: state?.published_at ? state.catalog : state?.previous_catalog, pending: null };
      save(statePath, state); context.verifiedInputs = null;
    }
    if (state.runtime?.adapter !== adapter || state.runtime?.publication !== publicationKey) {
      state.runtime = { adapter, publication: publicationKey, compatibility: compatible.mode }; save(statePath, state);
    }
    collect(root, state);
    let space = inventoryDiskBudget({ work, databasePath: state.catalog, totalBytes: diskBudgetBytes });
    catalog = openCatalog(state.catalog, { engine, contract, generation: state.generation, create: true, databaseBytes: space.databaseBytes });
    if (context.checkedCatalog !== state.catalog) {
      const check = catalog.db.prepare('PRAGMA quick_check').all();
      if (check.length !== 1 || check[0].quick_check !== 'ok') throw Error('inventory catalog integrity check failed');
      context.checkedCatalog = state.catalog;
    }
    const index = new InventoryRelations(catalog, operator.contract), operate = operator.call;
    index.operate = operate;
    const until = performance.now() + budgetMs;
    const publication = new InventoryPublication(index, work, context, fault);
    publication.compatibility(publicationKey);
    if (catalog.meta.publication_refresh && !state.pending && catalog.meta.complete) {
      space.check();
      if (beforeBoard && !await beforeBoard()) return { pending: true, stage: 'prior-board-delivery' };
      const refreshed = await publication.step({ projectionId: 'publication:' + publicationKey + ':' + catalog.meta.epoch.epoch }, operate,
        { budgetMs: Math.max(0, until - performance.now()) });
      if (refreshed.pending) return { pending: true, stage: 'publication-refresh' };
      if (onBoard) await onBoard({ work, catalog: state.catalog, forceHeads: true });
      catalog.setMeta({ ...catalog.meta, publication_refresh: null });
      return { pending: false, stage: 'publication-refreshed' };
    }
    if (!state.pending && state.published_at && now() - Date.parse(state.published_at) < 60000) return { pending: false };
    if (!state.pending) {
      report('capture');
      state.pending = await captureEpoch({ sql, catalog, generation: state.generation, dir: join(root, `capture-${Date.now()}`),
        bootstrap: !catalog.meta.complete, relational: true, legacyLibrary: false,
        report: (stage, fields) => { space.check(); report(stage, fields); } });
      save(statePath, state); context.verifiedInputs = null;
    }
    if (diskBudgetBytes !== undefined) {
      space = inventoryDiskBudget({ work, databasePath: state.catalog, totalBytes: diskBudgetBytes });
      const pageSize = catalog.db.prepare('PRAGMA page_size').get().page_size, pages = Math.floor(space.databaseBytes / pageSize);
      if (catalog.db.prepare('PRAGMA max_page_count=' + pages).get().max_page_count !== pages)
        throw Error('inventory database already exceeds its explicit disk budget');
      report('disk', space.report());
    }
    const cut = state.pending, receipt = cut.receipt;
    await verifyInputs(cut, sql, context);
    catalog.beginEpoch(receipt);
    if (catalog.meta.epoch.phase === 'preparing') {
      let input = catalog.meta.input?.epoch === receipt.epoch ? catalog.meta.input : { epoch: receipt.epoch, file: 0, offset: 0, count: 0, limit: 16 };
      const files = cut.bootstrap ? ['builds','facts','queue','tasks'].map(kind => [kind, cut.raw[kind]]) : [['delta', cut.delta]];
      while (input.file < files.length) {
        space.check();
        const [kind, path] = files[input.file], started = performance.now(), page = await readInventoryPage(path, input.offset, input.limit);
        const elapsed = Math.max(1, performance.now() - started);
        let next = { ...input, offset: page.offset, count: input.count + page.rows.length,
          limit: Math.max(1, Math.min(256, input.limit * 2, Math.floor(input.limit * 100 / elapsed))) };
        if (page.done) {
          const expected = cut.bootstrap && cut.meta.counts[kind === 'builds' ? 'library' : kind];
          if (cut.bootstrap && next.count !== expected) throw Error('incomplete board inventory ' + kind);
          next = { ...next, file: input.file + 1, offset: 0, count: 0 };
        }
        await commitPage(index, cut, input, page, kind, next, fault);
        input = next; report('input-' + kind, { epoch: receipt.epoch, rows: next.count });
        if (performance.now() >= until) break;
      }
      if (input.file === files.length) catalog.freezeEpoch();
      if (catalog.meta.epoch.phase === 'preparing') return { pending: true, stage: 'inputs', input };
    }
    if (index.pending()) {
      space.check();
      const result = await index.step(operate, { budgetMs: Math.max(0, until - performance.now()) });
      report('processing', { epoch: receipt.epoch, ...result.metrics });
      if (result.pending) return { pending: true, stage: 'processing' };
    }
    if (catalog.meta.publication?.round !== receipt.epoch && beforeBoard && !await beforeBoard())
      return { pending: true, stage: 'prior-board-delivery' };
    space.check();
    const board = await publication.step(receipt, operate, { budgetMs: Math.max(0, until - performance.now()) });
    report('board', { epoch: receipt.epoch });
    if (board.pending) return { pending: true, stage: 'board' };
    if (!catalog.meta.publication.announced && now() - Date.parse(receipt.started_at) < INVENTORY_MAX_MS && onBoard) {
      await onBoard({ epoch: receipt.epoch, work, catalog: state.catalog, forceHeads: Boolean(catalog.meta.publication_refresh) });
      await fault('after-board-before-announced', { epoch: receipt.epoch });
      catalog.setMeta({ ...catalog.meta, publication_refresh: null, publication: { ...catalog.meta.publication, announced: true } });
      return { pending: true, stage: 'board-ready' };
    }
    if (!catalog.meta.complete) {
      report('audit', { epoch: receipt.epoch }); await auditInventoryProjection(index, cut, dirname(cut.capture));
      report('audited', { epoch: receipt.epoch });
    }
    let ack = { offset: catalog.meta.ack?.epoch === receipt.epoch ? catalog.meta.ack.offset : 0, done: false };
    do {
      ack = await ackPage(sql, catalog, state.generation, cut, ack.offset, fault, report);
      if (!ack.done && performance.now() >= until) return { pending: true, stage: 'acknowledgments' };
    } while (!ack.done);
    const plan = { ...catalog.finishEpoch(), source };
    const refresh = reason => {
      state.pending = null; state.published_at = null; save(statePath, state); context.verifiedInputs = null; collect(root, state);
      return { pending: true, stage: reason };
    };
    if (now() - Date.parse(receipt.started_at) >= INVENTORY_MAX_MS) return refresh('refresh-expired-cut');
    const body = JSON.parse(inventoryWrite(plan).params[1]);
    let published;
    try {
      [{ published }] = await sql.unsafe('SELECT inventory_publish_cut($1::text,$2::text) published',
        [receipt.epoch, JSON.stringify({ ...body, generation: state.generation, contract })]);
    } catch (error) {
      if (error.code === 'P0001' && error.message === 'inventory cut expired') return refresh('refresh-expired-cut');
      if (error.code === 'P0001' && error.message === 'a newer inventory cut is already published') return refresh('refresh-superseded-cut');
      throw error;
    }
    catalog.activate(published); save(join(root, 'last-publication.json'), { receipt: published, capture: load(cut.capture) });
    state.published_at = published.at; state.pending = null; save(statePath, state); context.verifiedInputs = null; collect(root, state);
    report('published', { epoch: receipt.epoch, total: published.total });
    return { pending: false, published };
  } finally { catalog?.close(); if (!supplied) await operator.close(); }
}

function child(command, args, options = {}) {
  const process = spawn(command, args, { stdio: ['ignore','inherit','inherit'], ...options });
  const done = new Promise((resolve, reject) => {
    process.once('error', reject); process.once('exit', code => code === 0 ? resolve() : reject(Error(command + ' failed')));
  });
  return { process, done };
}
export async function boardDeliveryPending(work) {
  const stamp = join(work, 'verified/meta.json'), ledger = join(work, 'pushed.json');
  if (!existsSync(stamp)) return false;
  if (!existsSync(ledger)) return true;
  const pushed = load(ledger);
  if (!pushed || Array.isArray(pushed) || typeof pushed !== 'object') throw Error('invalid pushed ledger');
  return pushed['meta.json'] !== await hash(stamp);
}
export async function serveBoardInventory(work, bin) {
  if (!process.env.PG_URL) throw Error('PG_URL required');
  const sql = postgres(process.env.PG_URL, { max: 1, fetch_types: false, connect_timeout: 15, onnotice: () => {},
    connection: { application_name: 'wfsim-inventory-incremental' } });
  const context = {}, directory = join(work, 'diagnostics');
  let operator = null, stopping = false, push = null, pushOwed = false;
  const stop = () => { stopping = true; }; process.on('SIGTERM', stop); process.on('SIGINT', stop);
  const here = dirname(fileURLToPath(import.meta.url));
  const configuredDisk = process.env.WFSIM_INVENTORY_DISK_BYTES;
  const diskBudgetBytes = configuredDisk === undefined ? undefined : Number(configuredDisk);
  try {
    while (!stopping) {
      const monitor = pipelineMonitor(directory, 'inventory', { source: readFileSync(join(bin, 'VERSION'), 'utf8').trim(),
        engine: readFileSync(join(bin, 'ENGINE'), 'utf8').trim() });
      let pending = true;
      try {
        operator ||= await openInventoryOperator(join(bin, 'wfsim-board'));
        const result = await runBoardInventoryCycle({ work, bin, sql, operator, context, report: monitor.report, diskBudgetBytes,
          beforeBoard: async () => !push && !await boardDeliveryPending(work),
          onBoard: async ({ forceHeads }) => {
            await child(process.execPath, [join(here, 'board_heads.mjs'), join(work, 'verified'), ...(forceHeads ? ['--force'] : [])]).done;
            await child('python3', [join(here, 'board_meta.py'), join(work, 'verified'), join(work, 'pub/data/board_state.yaml'), join(work, 'verified/meta.json')]).done;
            pushOwed = true;
          } });
        pending = result.pending; monitor.finish(pending ? 'pending' : 'complete');
      } catch (error) {
        monitor.finish('failed', error);
        if (operator?.healthy === false || error.code === 'INVENTORY_OPERATOR_CHANGED') {
          await operator?.close(); operator = null; context.operatorAdapter = null;
        }
      }
      try { pushOwed ||= await boardDeliveryPending(work); }
      catch (error) { console.error('live: delivery checkpoint refused: ' + error.message); }
      if (pushOwed && !push) {
        pushOwed = false;
        push = child(process.execPath, ['--max-old-space-size=64', join(here, 'live_publish.mjs'), 'push', work, join(work, 'verified')]);
        push.done.catch(() => { pushOwed = true; }).finally(() => { push = null; });
      }
      if (process.env.ONCE === '1') break;
      if (!stopping) await new Promise(resolve => setTimeout(resolve, pending ? 1000 : 20000));
    }
  } finally {
    process.off('SIGTERM', stop); process.off('SIGINT', stop);
    push?.process.kill('SIGTERM'); context.writer?.close(); await operator?.close(); await sql.end({ timeout: 5 });
  }
}
