// SPDX-License-Identifier: AGPL-3.0-or-later
import { setTimeout } from 'node:timers/promises';
import { queueOrders, queueZonesSql } from '../worker/queue_counts.js';
import { boardCoverageOrderAdapter } from './board_coverage_orders.mjs';

const rollback = new Set(['55P03', '40P01', '40001', '25P02']);
const hex = /^[0-9a-f]{32}$/;
const taskID = /^[1-9][0-9]*$/;
const lock = tx => tx.unsafe("SELECT pg_advisory_xact_lock(hashtextextended('compute-order-import',0))");
const cutOf = tx => tx.unsafe("SELECT compute_order_cut_lock(),coalesce(compute_order_cut_epoch(),'') cut");
const readWindow = tx => tx.unsafe('SELECT * FROM state_compute_order_stock_window');

export const MAX_ACTIVE_ORDERS = 1_000_000;
export function checkActiveOrderLimit(limit) {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > MAX_ACTIVE_ORDERS)
    throw Error(`max-open must be between 1 and ${MAX_ACTIVE_ORDERS}`);
}

const confirmPage = (actual, { first, last, orders }) => {
  if (!actual || Number(actual.first_byte) !== first || Number(actual.next_byte) !== last || actual.orders !== orders)
    throw Error('stock window page changed');
};

export async function stockWindowBounds(reader, { id, rows, maxOpen, limit }) {
  checkActiveOrderLimit(maxOpen);
  const [window] = await readWindow(reader);
  if (window) {
    if (window.import_id !== id) throw Error('resume the earlier stock window before advancing');
    const pages = await reader.unsafe(
      'SELECT * FROM state_compute_order_stock_pages WHERE window_key=$1 ORDER BY first_byte', [window.key]);
    return { pages: pages.map(page => page.orders), reserved: true };
  }
  const [queue] = await reader.unsafe(queueZonesSql);
  if (!queue) return { result: 'queue_unavailable' };
  const available = Math.max(0, Math.min(maxOpen - queueOrders(queue.zones).total, rows * limit));
  if (!available) return { result: 'queue_full' };
  return { pages: Array.from({ length: Math.ceil(available / rows) }, (_, i) => Math.min(rows, available - i * rows)) };
}

export async function refuseLegacyWindow(tx) {
  const [{ installed }] = await tx.unsafe("SELECT to_regclass('state_compute_order_stock_window') IS NOT NULL installed");
  if (installed && (await readWindow(tx)).length) throw Error('resume the reserved stock window before using the serial importer');
}

export async function reserveStockWindow(writer, { id, cut, pages, maxOpen }) {
  checkActiveOrderLimit(maxOpen);
  const first = pages[0]?.first, last = pages.at(-1)?.last;
  const count = pages.reduce((n, page) => n + page.orders, 0);
  if (!count || count > maxOpen ||
      pages.some((page, at) => !Number.isSafeInteger(page.first) || !Number.isSafeInteger(page.last) ||
        !Number.isSafeInteger(page.orders) || page.orders < 1 || page.first < 0 || page.first >= page.last ||
        (at > 0 && page.first !== pages[at - 1].last)))
    throw Error('stock window must be consecutive and fit the configured queue');
  const computations = [...new Set(pages.flatMap(page => page.prepared.items.map(item => item.target.computation)))];
  if (!computations.length || computations.length > count || computations.some(id => !hex.test(id)))
    throw Error('stock window needs canonical computations');
  const key = JSON.stringify([id, first]);
  return writer.begin(async tx => {
    await lock(tx);
    const [{ cut: current }] = await cutOf(tx);
    const [run] = await tx.unsafe(`SELECT i.* FROM state_compute_order_imports i
      JOIN record_compute_order_stock_sources s ON s.import_id=i.id WHERE i.id=$1 FOR UPDATE OF i`, [id]);
    if (!run || current !== cut || run.cut !== cut || Number(run.next_byte) !== first || Number(run.input_bytes) < last)
      throw Error('accepted stock position or order cut changed');
    if (run.paused) return { result: 'paused' };
    const [existing] = await readWindow(tx);
    if (existing) {
      if (existing.key !== key || Number(existing.next_byte) !== last || Number(existing.max_open) !== maxOpen ||
          computations.some(id => !taskID.test(existing.positions[id])) || Object.keys(existing.positions).length !== computations.length)
        throw Error('resume the reserved stock window with its original input and limits');
      const reserved = await tx.unsafe('SELECT * FROM state_compute_order_stock_pages WHERE window_key=$1 ORDER BY first_byte', [key]);
      if (reserved.length !== pages.length) throw Error('stock window pages changed');
      pages.forEach((page, at) => confirmPage(reserved[at], page));
      return { result: 'reserved', key };
    }
    const [queue] = await tx.unsafe(queueZonesSql);
    if (!queue) return { result: 'queue_unavailable' };
    if (queueOrders(queue.zones).total + count > maxOpen) return { result: 'queue_full' };
    await tx.unsafe(`INSERT INTO state_compute_order_stock_window(key,import_id,first_byte,next_byte,max_open,positions)
      SELECT $1,$2,$3,$4,$5,jsonb_object_agg(computation,position::text)
      FROM (SELECT computation,nextval('compute_queue_position') position
        FROM (SELECT computation FROM jsonb_array_elements_text($6::text::jsonb) WITH ORDINALITY
          r(computation,ordinal) ORDER BY ordinal OFFSET 0) ordered) positions`,
    [key, id, first, last, maxOpen, JSON.stringify(computations)]);
    await tx.unsafe(`INSERT INTO state_compute_order_stock_pages
      SELECT $1,first,last,orders FROM jsonb_to_recordset($2::text::jsonb) p(first bigint,last bigint,orders int)`,
    [key, JSON.stringify(pages.map(({ first, last, orders }) => ({ first, last, orders })))]);
    return { result: 'reserved', key };
  });
}

async function admitPage(writer, { id, key, cut, page }) {
  return writer.begin(async tx => {
    const [reserved] = await tx.unsafe(`SELECT p.*,w.positions,i.paused FROM state_compute_order_stock_pages p
      JOIN state_compute_order_stock_window w ON w.key=p.window_key
      JOIN state_compute_order_imports i ON i.id=w.import_id
      WHERE w.import_id=$1 AND p.window_key=$2 AND p.first_byte=$3 FOR UPDATE OF p`, [id, key, page.first]);
    confirmPage(reserved, page);
    if (reserved.paused) return 'paused';
    const [{ cut: current }] = await cutOf(tx);
    if (current !== cut) throw Error('stock window order cut changed');
    if ((await tx.unsafe('SELECT 1 FROM record_compute_order_import_batches WHERE (import_id,first_byte)=($1,$2)',
      [id, page.first])).length) return 'committed';
    const ids = await boardCoverageOrderAdapter.admit(tx, page.prepared);
    if (ids.length > page.orders || ids.some(id => !taskID.test(id)) || new Set(ids).size !== ids.length)
      throw Error('invalid physical stock receipt');
    const taskIds = JSON.stringify(ids);
    const held = await tx.unsafe(`WITH physical AS MATERIALIZED (
      SELECT t.id,t.computation,($3::text::jsonb->>t.computation)::bigint position
      FROM jsonb_array_elements_text($1::text::jsonb) a(id) JOIN entity_compute_tasks t ON t.id=a.id::bigint
      WHERE t.xmin=pg_current_xact_id()::xid
    ), placed AS (
      UPDATE state_compute_scheduling q SET zone_position=p.position FROM physical p
      WHERE (q.id,q.task_id)=(p.computation,p.id) AND q.state='open' AND q.zone=3 AND p.position IS NOT NULL
      RETURNING q.task_id
    ) INSERT INTO state_compute_order_stock_holds SELECT task_id,$2 FROM placed RETURNING task_id`,
    [taskIds, key, JSON.stringify(reserved.positions)]);
    if (held.length !== ids.length) throw Error('stock admission failed its position or task check');
    await tx.unsafe(`INSERT INTO record_compute_order_import_batches VALUES($1,$2,$3,$4,
      ARRAY(SELECT value::bigint FROM jsonb_array_elements_text($5::text::jsonb)))`,
    [id, page.first, page.last, page.orders, taskIds]);
    return 'committed';
  });
}

async function verifyPage(reader, id, page) {
  return reader.begin(async tx => {
    await tx.unsafe('SET TRANSACTION READ ONLY');
    const [proof] = await tx.unsafe(`SELECT b.*,cardinality(b.task_ids) admitted,
      (SELECT count(*)::int FROM unnest(b.task_ids) a(id) JOIN entity_compute_tasks t USING(id)) verified
      FROM record_compute_order_import_batches b WHERE (b.import_id,b.first_byte)=($1,$2)`, [id, page.first]);
    if (!proof) return null;
    confirmPage(proof, page);
    if (proof.admitted !== proof.verified) throw Error('stock page lacks physical task evidence');
    return proof.admitted;
  });
}

export async function admitStockPage(writer, reader, request, signal) {
  if (writer === reader) throw Error('stock page requires independent readback');
  for (let attempt = 0; ; attempt++) {
    signal?.throwIfAborted();
    let result, failure;
    try { result = await admitPage(writer, request); } catch (error) { failure = error; }
    if (result === 'paused') return { result };
    const admitted = await verifyPage(reader, request.id, request.page);
    if (admitted !== null) return { result: 'committed', admitted };
    if (!rollback.has(failure?.code) || attempt >= 2)
      throw failure || Error('stock page commit lacks independent readback');
    await setTimeout(150 * (attempt + 1), undefined, { signal });
  }
}

export function admitStockPages(pages, admit) {
  const latest = new Map();
  return Promise.allSettled(pages.map((page, lane) => {
    const builds = new Set(page.prepared.items.map(({ target }) => target.identity));
    const predecessors = new Set([...builds].map(build => latest.get(build)).filter(Boolean));
    const result = Promise.all(predecessors).then(outcomes => {
      if (outcomes.some(outcome => outcome.result === 'paused')) return { result: 'paused' };
      return admit(page, lane);
    });
    // The database takes a build lock for every identity. Shared pages follow source order.
    for (const build of builds) latest.set(build, result);
    return result;
  }));
}

export async function publishStockWindow(writer, reader, { id, key, cut, pages }) {
  if (writer === reader) throw Error('stock window requires independent readback');
  const first = pages[0].first, last = pages.at(-1).last;
  const witnesses = await Promise.all(pages.map(page => verifyPage(reader, id, page)));
  if (witnesses.some(n => n === null)) throw Error('stock window cannot pass an unresolved page');
  const admitted = witnesses.reduce((sum, n) => sum + n, 0);
  const processed = pages.reduce((sum, page) => sum + page.orders, 0);
  let result, failure;
  try {
    result = await writer.begin(async tx => {
      await lock(tx);
      const [{ cut: current }] = await cutOf(tx);
      if (current !== cut) throw Error('stock window order cut changed');
      const [run] = await tx.unsafe('SELECT * FROM state_compute_order_imports WHERE id=$1 FOR UPDATE', [id]);
      if (!run || Number(run.next_byte) !== first) throw Error('stock cursor changed');
      if (run.paused) return 'paused';
      const [window] = await readWindow(tx);
      if (!window || window.key !== key || window.import_id !== id || Number(window.first_byte) !== first ||
          Number(window.next_byte) !== last) throw Error('stock window changed');
      const [{ matching }] = await tx.unsafe(`SELECT count(DISTINCT a.id)::int matching
        FROM state_compute_order_stock_pages p
        JOIN record_compute_order_import_batches b ON (b.import_id,b.first_byte,b.next_byte,b.orders)=($2,p.first_byte,p.next_byte,p.orders)
        CROSS JOIN LATERAL unnest(b.task_ids) a(id)
        JOIN state_compute_order_stock_holds h ON (h.task_id,h.window_key)=(a.id,p.window_key)
        WHERE p.window_key=$1`, [key, id]);
      if (matching !== admitted) throw Error('stock order holds do not match the physical receipts');
      const released = await tx.unsafe('DELETE FROM state_compute_order_stock_holds WHERE window_key=$1 RETURNING task_id', [key]);
      if (released.length !== admitted) throw Error('stock publication released an unverified task');
      await tx.unsafe(`UPDATE state_compute_order_imports SET next_byte=$2,processed=processed+$3,admitted=admitted+$4 WHERE id=$1`,
        [id, last, processed, admitted]);
      await tx.unsafe('DELETE FROM state_compute_order_stock_window WHERE key=$1', [key]);
      return 'published';
    });
  } catch (error) { failure = error; }
  if (result === 'paused') return { result };
  const [proof] = await reader.unsafe(`SELECT i.next_byte,
    EXISTS(SELECT FROM state_compute_order_stock_window w WHERE w.key=$2) pending,
    EXISTS(SELECT FROM state_compute_order_stock_holds h WHERE h.window_key=$2) held
    FROM state_compute_order_imports i WHERE i.id=$1`, [id, key]);
  if (!proof || Number(proof.next_byte) !== last || proof.pending || proof.held)
    throw failure || Error('stock window publication lacks independent readback');
  return { result: 'published', admitted, processed, next: last };
}
