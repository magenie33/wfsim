// SPDX-License-Identifier: AGPL-3.0-or-later
// An offline replay never connects to Postgres or publishes a database receipt.
import { existsSync, readFileSync, writeFileSync, mkdirSync, readdirSync, statSync, openSync, closeSync, fsyncSync, renameSync } from 'node:fs';
import { join, resolve, basename, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { openCatalog } from './incremental_inventory.mjs';
import { InventoryRelations } from './inventory_relations.mjs';
import { InventoryPublication } from './inventory_publication.mjs';
import { openInventoryOperator } from './inventory_operator.mjs';
import { readInventoryPage, applyInventoryPage } from './inventory_projection.mjs';
import { fileHash } from './inventory_publication_io.mjs';
import { inventoryDiskBudget } from './inventory_disk_budget.mjs';

const components = ['replay_board_projection.mjs','inventory_relations.mjs','incremental_inventory.mjs','inventory_operator.mjs',
  'inventory_projection.mjs','inventory_publication.mjs','inventory_publication_io.mjs','inventory_disk_budget.mjs','json_array_stream.mjs','board_heads.mjs','board_meta.py'];
function save(path, value) {
  const partial = path + '.partial'; writeFileSync(partial, JSON.stringify(value));
  const fd = openSync(partial, 'r'); try { fsyncSync(fd); } finally { closeSync(fd); }
  renameSync(partial, path);
  const parent = openSync(dirname(path), 'r'); try { fsyncSync(parent); } finally { closeSync(parent); }
}
async function command(program, args) {
  await new Promise((accept, reject) => {
    const child = spawn(program, args, { stdio: ['ignore','inherit','inherit'] });
    child.once('error', reject); child.once('exit', code => code === 0 ? accept() : reject(Error('offline artifact command failed: ' + program)));
  });
}
function artifacts(root, prefix = '') {
  const files = [];
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const name = prefix + entry.name, path = join(root, entry.name);
    if (entry.isDirectory()) files.push(...artifacts(path, name + '/'));
    else if (entry.name.endsWith('.json')) files.push({ name, bytes: statSync(path).size, path });
  }
  return files;
}
export async function replayBoardProjection({ capture, bin, work, manifestSha, diskBudgetBytes, fault = async () => {}, stopAfterBatches = Infinity } = {}) {
  capture = resolve(capture); bin = resolve(bin); work = resolve(work);
  if (!/^[a-f0-9]{64}$/.test(manifestSha || '')) throw Error('the protected capture manifest SHA256 is required');
  if (work === capture || work.startsWith(capture + '/') || work === bin || work.startsWith(bin + '/')) throw Error('an isolated output directory is required');
  mkdirSync(work, { recursive: true });
  const checkpoint = join(work, 'replay.json');
  if (!existsSync(checkpoint) && readdirSync(work).length) throw Error('new replay output must be empty');
  const manifestPath = join(capture, 'capture.json');
  if (await fileHash(manifestPath) !== manifestSha) throw Error('protected capture manifest SHA256 mismatch');
  const manifest = JSON.parse(readFileSync(manifestPath)), names = new Set();
  for (const file of manifest.files) {
    const name = basename(file.path); if (names.has(name)) throw Error('capture filenames are ambiguous'); names.add(name);
    if (await fileHash(join(capture, name)) !== file.sha256) throw Error('protected source input SHA256 mismatch: ' + name);
  }
  const raw = Object.fromEntries(Object.entries(manifest.raw).map(([key, path]) => [key, join(capture, basename(path))]));
  if (Object.values(raw).some(path => !names.has(basename(path)))) throw Error('capture input is not protected by its manifest');
  const snapshot = JSON.parse(readFileSync(raw.meta)), engine = readFileSync(join(bin, 'ENGINE'), 'utf8').trim();
  if (snapshot.engine !== engine || !manifest.bootstrap) throw Error('offline replay requires the matching complete source baseline');
  const disk = inventoryDiskBudget({ work, sources: [manifestPath, ...manifest.files.map(file => join(capture, basename(file.path)))], totalBytes: diskBudgetBytes });
  const algorithm = createHash('sha256');
  for (const name of components) algorithm.update(readFileSync(new URL(name, import.meta.url)));
  const operator = await openInventoryOperator(join(bin, 'wfsim-board'));
  let catalog;
  const context = {};
  try {
    for (const op of ['describe','facts','gate','render','floor','carry','state'])
      if (!operator.contract.operators?.includes(op)) throw Error('offline replay requires native publication operators');
    const contract = { v: 1, algorithm: 'offline-board-projection-v1', engine, manifest_sha: manifestSha,
      adapter: await fileHash(join(bin, 'wfsim-board')), classifier: algorithm.digest('hex'), operator: operator.contract,
      disk_budget_bytes: diskBudgetBytes ?? null };
    const projectionId = createHash('sha256').update(JSON.stringify(contract)).digest('hex');
    let state;
    if (existsSync(checkpoint)) {
      state = JSON.parse(readFileSync(checkpoint));
      if (state.projection_id !== projectionId) throw Error('offline replay code, engine, binary or input fence changed');
    } else {
      state = { v: 1, mode: 'offline_replay', projection_id: projectionId, database_epoch: null, publication_remote: false,
        manifest_sha: manifestSha, source_engine: engine, source_snapshot: snapshot.snapshot, source_started_at: snapshot.started_at,
        contract, phase: 'import', counts: snapshot.counts };
      save(checkpoint, state);
    }
    catalog = openCatalog(join(work, 'projection.sqlite'), { engine, contract, generation: projectionId, create: true, databaseBytes: disk.databaseBytes });
    if (catalog.db.prepare('PRAGMA quick_check').get().quick_check !== 'ok') throw Error('offline catalog integrity check failed');
    const index = new InventoryRelations(catalog, operator.contract), operate = operator.call;
    let batches = 0, lastReport = 0;
    const report = phase => {
      if (Date.now() - lastReport < 1000 && phase !== 'complete') return;
      lastReport = Date.now();
      console.log(JSON.stringify({ event: 'offline_board_projection', phase, input: catalog.meta.input,
        memory: process.memoryUsage(), disk: disk.report(), metrics: index.metrics, native_pid: operator.pid }));
    };
    const files = ['builds','facts','queue','tasks'];
    let input = catalog.meta.input || { file: 0, offset: 0, count: 0 };
    while (input.file < files.length) {
      disk.check();
      const kind = files[input.file], page = await readInventoryPage(raw[kind], input.offset, 256);
      let next = { file: input.file, offset: page.offset, count: input.count + page.rows.length };
      if (page.done) {
        if (next.count !== snapshot.counts[kind === 'builds' ? 'library' : kind]) throw Error('incomplete offline ' + kind);
        next = { file: input.file + 1, offset: 0, count: 0 };
      }
      const before = catalog.meta;
      catalog.db.exec('BEGIN IMMEDIATE');
      try {
        await applyInventoryPage(index, operate, page, kind, true);
        catalog.setMeta({ ...catalog.meta, input: next });
        await fault('before-input-commit', { input, next });
        catalog.db.exec('COMMIT');
      } catch (error) {
        if (catalog.db.isTransaction) catalog.db.exec('ROLLBACK'); catalog.meta = before; throw error;
      }
      input = next;
      await fault('after-input-commit-before-reply', { input }); report('import');
      if (++batches >= stopAfterBatches) return { pending: true, phase: 'import', projection_id: projectionId };
    }
    state.phase = 'classify'; save(checkpoint, state);
    while (index.pending()) {
      disk.check();
      await index.step(operate, { budgetMs: 100 }); report('classify');
      await fault('after-classification-batch', {});
      if (++batches >= stopAfterBatches) return { pending: true, phase: 'classify', projection_id: projectionId };
    }
    state.phase = 'publication'; save(checkpoint, state);
    const publication = new InventoryPublication(index, work, context, fault);
    while ((await publication.step({ projectionId }, operate, { budgetMs: 100 })).pending) {
      disk.check();
      report('publication');
      if (++batches >= stopAfterBatches) return { pending: true, phase: 'publication', projection_id: projectionId };
    }
    const here = dirname(fileURLToPath(import.meta.url));
    await command(process.execPath, [join(here, 'board_heads.mjs'), join(work, 'verified')]);
    await command('python3', [join(here, 'board_meta.py'), join(work, 'verified'), join(work, 'pub/data/board_state.yaml'), join(work, 'verified/meta.json')]);
    const output = artifacts(join(work, 'verified'));
    const oversized = output.filter(file => file.bytes > 32 * 1024 ** 2).map(({ name, bytes }) => ({ name, bytes }));
    disk.check(true);
    state = { ...state, phase: 'complete', summary: catalog.summary(), question_digest: catalog.digest(), disk: disk.report(),
      files: output.length, output_bytes: output.reduce((n, file) => n + file.bytes, 0),
      largest_file: output.map(({ name, bytes }) => ({ name, bytes })).sort((a, b) => b.bytes - a.bytes)[0] || null,
      existing_receiver_limit_bytes: 32 * 1024 ** 2, delivery_eligible: !oversized.length, oversized,
      memory: process.memoryUsage(), published: false };
    save(checkpoint, state); report('complete'); return state;
  } finally { context.writer?.close(); catalog?.close(); await operator.close(); }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const [capture, bin, work, manifestSha, diskBytes] = process.argv.slice(2);
  if (!capture || !bin || !work || !manifestSha || process.argv.length < 6 || process.argv.length > 7)
    throw Error('usage: replay_board_projection.mjs <capture-dir> <bin-dir> <empty-work-dir> <manifest-sha256> [total-disk-bytes]');
  const result = await replayBoardProjection({ capture, bin, work, manifestSha, diskBudgetBytes: diskBytes === undefined ? undefined : Number(diskBytes) });
  console.log(JSON.stringify(result));
  if (result.delivery_eligible === false) process.exitCode = 2;
}
