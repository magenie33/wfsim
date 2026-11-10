// SPDX-License-Identifier: AGPL-3.0-or-later
// Durable incremental maintenance, independent of a ranker's process lifetime.
import postgres from './pg/node_modules/postgres/src/index.js';
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { BOARD_KINDS } from '../worker/board_kinds.js';
import { canon } from '../worker/task_contract.js';
import { inventoryWrite, INVENTORY_MAX_MS } from '../worker/task_inventory.js';
import { openCatalog } from './incremental_inventory.mjs';
import { InventoryRelations } from './inventory_relations.mjs';
import { openInventoryOperator } from './inventory_operator.mjs';
import { readInventoryPage, applyInventoryPage, auditInventoryProjection } from './inventory_projection.mjs';
import { pipelineMonitor } from './pipeline_monitor.mjs';
import { captureEpoch, saveInventoryState as save, hashInventoryFile, collectInventoryFiles as collect,
  checkInventoryDisk as disk } from './inventory_pipeline.mjs';

const load = path => JSON.parse(readFileSync(path, 'utf8'));
const progress = (stage, fields) => console.error(JSON.stringify({ event: 'relational_inventory', stage, ...fields }));

export async function runRelationalInventory({ work, bin, sql, budgetMs = 15000, operator: supplied, report = progress }) {
  const root = join(work, 'inventory'), statePath = join(root, 'state.json'); mkdirSync(root, { recursive: true }); disk(root);
  const source = readFileSync(join(bin, 'VERSION'), 'utf8').trim(), engine = readFileSync(join(bin, 'ENGINE'), 'utf8').trim();
  const algorithm = createHash('sha256');
  for (const name of ['./inventory_relations.mjs', './inventory_relational_pipeline.mjs', './incremental_inventory.mjs',
    './inventory_inputs.mjs', './inventory_pipeline.mjs', './inventory_operator.mjs', './capture_compute_inventory.mjs',
    './classify_compute_inventory.mjs', '../worker/task_contract.js', '../worker/task_inventory.js', '../worker/board_kinds.js'])
    algorithm.update(readFileSync(new URL(name, import.meta.url)));
  const operator = supplied || await openInventoryOperator(join(bin, 'wfsim-board'));
  let catalog;
  try {
    const contract = { v: 1, algorithm: 'board-inventory-relations-v1', covers: BOARD_KINDS,
      adapter: await hashInventoryFile(join(bin, 'wfsim-board')), classifier: algorithm.digest('hex'), operator: operator.contract };
    let state = existsSync(statePath) ? load(statePath) : null;
    report('generation');
    const [{ status }] = await sql.unsafe('SELECT inventory_status() status');
    if (!state || state.generation !== status?.generation || canon(status.contract) !== canon(contract) || status.engine !== engine) {
      const [{ generation }] = await sql.unsafe('SELECT inventory_begin($1::text,$2::text,$3::text) generation',
        [engine, JSON.stringify(contract), status?.generation || null]);
      state = { generation: generation.generation, engine, contract, catalog: join(root, `generation-${generation.generation}.sqlite`),
        previous_catalog: state?.published_at ? state.catalog : state?.previous_catalog, pending: null };
      save(statePath, state);
    }
    collect(root, state);
    catalog = openCatalog(state.catalog, { engine, contract, generation: state.generation, create: true });
    const index = new InventoryRelations(catalog, operator.contract), operate = operator.call;
    const until = performance.now() + budgetMs;
    if (!state.pending && state.published_at && Date.now() - Date.parse(state.published_at) < 60000) return { pending: false };
    if (!state.pending) {
      report('capture', { generation: state.generation });
      state.pending = await captureEpoch({ sql, catalog, generation: state.generation, dir: join(root, `capture-${Date.now()}`),
        bootstrap: !catalog.meta.complete, relational: true, report });
      save(statePath, state);
    }
    const cut = state.pending, receipt = cut.receipt, directory = dirname(cut.capture);
    catalog.beginEpoch(receipt);
    if (catalog.meta.epoch.phase === 'preparing') {
      let input = catalog.meta.input?.epoch === receipt.epoch ? catalog.meta.input : { epoch: receipt.epoch, file: 0, offset: 0, count: 0, limit: 16 };
      const files = cut.bootstrap ? ['builds', 'facts', 'queue', 'tasks'].map(kind => [kind, cut.raw[kind]]) : [['delta', cut.delta]];
      while (input.file < files.length) {
        const [kind, path] = files[input.file], start = performance.now();
        const page = await readInventoryPage(path, input.offset, input.limit);
        await applyInventoryPage(index, operate, page, kind, cut.bootstrap);
        const elapsed = Math.max(1, performance.now() - start), estimate = Math.floor(input.limit * 100 / elapsed);
        input = { ...input, offset: page.offset, count: input.count + page.rows.length,
          limit: Math.max(1, Math.min(256, input.limit * 2, estimate)) };
        const processed = input.count;
        if (page.done) {
          const expected = cut.bootstrap && cut.meta.counts[kind === 'builds' ? 'library' : kind];
          if (cut.bootstrap && input.count !== expected) throw Error(`incomplete relational ${kind}`);
          input = { ...input, file: input.file + 1, offset: 0, count: 0 };
        }
        catalog.setMeta({ ...catalog.meta, input });
        report(`input-${kind}`, { epoch: receipt.epoch, rows: processed });
        if (performance.now() >= until) break;
      }
      if (input.file === files.length) catalog.freezeEpoch();
      if (catalog.meta.epoch.phase === 'preparing') return { pending: true, stage: 'inputs', input };
    }
    if (index.pending()) {
      const result = await index.step(operate, { budgetMs: Math.max(0, until - performance.now()) });
      report('processing', { epoch: receipt.epoch, ...result.metrics });
      if (result.pending) return { pending: true, stage: 'processing', metrics: result.metrics };
    }
    if (!catalog.meta.complete) { report('audit', { epoch: receipt.epoch }); await auditInventoryProjection(index, cut, directory); report('audited', { epoch: receipt.epoch }); }
    report('acknowledging', { epoch: receipt.epoch });
    for (let offset = catalog.meta.ack?.epoch === receipt.epoch ? catalog.meta.ack.offset : 0;;) {
      const page = await readInventoryPage(cut.changes, offset, 256);
      if (page.rows.length) {
        catalog.recordReceipts(page.rows);
        await sql.unsafe('SELECT inventory_ack($1::text,$2::text)', [state.generation, JSON.stringify(page.rows)]);
        catalog.acknowledge(page.rows);
      }
      catalog.setMeta({ ...catalog.meta, ack: { epoch: receipt.epoch, offset: page.offset } });
      if (page.done) break;
      if (performance.now() >= until) return { pending: true, stage: 'acknowledgments' };
      offset = page.offset;
    }
    const plan = { ...catalog.finishEpoch(), source };
    const refresh = reason => {
      state.pending = null; state.published_at = null; save(statePath, state); collect(root, state);
      return { pending: true, stage: reason };
    };
    if (Date.now() - Date.parse(receipt.started_at) >= INVENTORY_MAX_MS) {
      return refresh('refresh-expired-cut');
    }
    const body = JSON.parse(inventoryWrite(plan).params[1]);
    let published;
    report('publishing', { epoch: receipt.epoch });
    try {
      [{ published }] = await sql.unsafe('SELECT inventory_publish_cut($1::text,$2::text) published',
        [receipt.epoch, JSON.stringify({ ...body, generation: state.generation, contract })]);
    } catch (error) {
      if (error.code === 'P0001' && error.message === 'inventory cut expired') return refresh('refresh-expired-cut');
      if (error.code === 'P0001' && error.message === 'a newer inventory cut is already published') return refresh('refresh-superseded-cut');
      throw error;
    }
    catalog.activate(published); save(join(root, 'last-publication.json'), { receipt: published, capture: load(cut.capture) });
    state.published_at = published.at; state.pending = null; save(statePath, state); collect(root, state);
    report('published', { epoch: receipt.epoch, total: published.total });
    return { pending: false, published, metrics: index.metrics };
  } finally { catalog?.close(); if (!supplied) await operator.close(); }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const [work, bin] = process.argv.slice(2);
  if (!work || !bin || !process.env.PG_URL) throw Error('usage: inventory_relational_pipeline.mjs <work> <bin> (PG_URL required)');
  const sql = postgres(process.env.PG_URL, { max: 1, fetch_types: false, connect_timeout: 15, onnotice: () => {},
    connection: { application_name: 'wfsim-relational-inventory' } });
  const monitor = pipelineMonitor(join(work, 'diagnostics'), 'relational-inventory', {
    source: readFileSync(join(bin, 'VERSION'), 'utf8').trim(), engine: readFileSync(join(bin, 'ENGINE'), 'utf8').trim() });
  try {
    const result = await runRelationalInventory({ work: resolve(work), bin: resolve(bin), sql, report: monitor.report });
    monitor.finish(result.pending ? 'pending' : 'complete'); console.log(JSON.stringify(result));
  } catch (error) { monitor.finish('failed', error); process.exitCode = 1; }
  finally { await sql.end({ timeout: 5 }); }
}
