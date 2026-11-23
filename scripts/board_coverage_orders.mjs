// SPDX-License-Identifier: AGPL-3.0-or-later
import { createHash } from 'node:crypto';
import { computationCanon } from '../worker/computation_input.js';
import { BOARD_COVERAGE_BATCH_SIZE, prepareBoardCoverageBatch } from './board_coverage.mjs';

const hash = value => createHash('sha256').update(computationCanon(value)).digest('hex');

async function prepareOrders(orders, { id, first, cut, engine }) {
  const manifest = hash([id, first]), namespace = 'coverage-' + manifest.slice(0, 16);
  const native = orders[0].native, unique = new Map();
  for (const order of orders) {
    if (order.native !== native) throw Error('coverage batch contains different Native inputs');
    const { identity, ruler, mode } = order.target;
    const source = [namespace, identity, ruler, mode].join('/');
    const previous = unique.get(source);
    if (previous) {
      if (previous.target.computation !== order.target.computation ||
          computationCanon(previous.plan.flow) !== computationCanon(order.plan.flow))
        throw Error('coverage orders disagree for the same measurement');
      continue;
    }
    unique.set(source, order);
  }
  const items = [...unique].map(([source, order], index) => ({ source,
    target: { ...order.target, source, ordinal: index + 1, engine }, plan: { ...order.plan, from: [source] } }));
  const parts = [];
  for (let next = 0; next < items.length; next += BOARD_COVERAGE_BATCH_SIZE) {
    const group = items.slice(next, next + BOARD_COVERAGE_BATCH_SIZE);
    const checkpoint = { manifest, next, enqueued: 0, target: items.length };
    const rows = group.map(({ target: { engine: _engine, computation: _computation, ...target } }) => target);
    const proposal = { before: hash(checkpoint), after: { ...checkpoint, next: next + rows.length }, rows };
    proposal.id = hash(proposal);
    const sealed = { batch: proposal.id, manifest, namespace, native, engine, items: group };
    parts.push((await prepareBoardCoverageBatch(checkpoint, proposal, sealed, { epoch: cut, engine, native })).batch);
  }
  const batch = hash(parts.map(part => part.batch));
  return { batch: { ...parts[0], batch, last: items.length, groups: parts.flatMap(part => part.groups) }, items, cut };
}

async function admitCoverageOrders(tx, { batch, items, cut }) {
  await tx.unsafe('SELECT board_coverage_admit_import(($1::text)::jsonb,$2,$3)', [JSON.stringify(batch), cut, batch.engine]);
  const received = await tx.unsafe(`SELECT x.target->>'source' source, r.disposition->>'batch' batch,
      r.disposition->>'outcome' outcome,t.id::text task_id
    FROM board_coverage_targets(($1::text)::jsonb) AS x(target)
    CROSS JOIN LATERAL (
      SELECT disposition FROM record_board_coverage_receipts
      WHERE source=x.target->>'source' AND disposition->>'computation'=x.target->>'computation' OFFSET 0
    ) r
    LEFT JOIN LATERAL (
      SELECT t.id FROM entity_compute_tasks t
      WHERE t.id=(r.disposition->>'task_id')::bigint AND t.computation=x.target->>'computation'
        AND t.xmin=pg_current_xact_id()::xid OFFSET 0
    ) t ON true
    ORDER BY (x.target->>'ordinal')::bigint`, [JSON.stringify(batch)]);
  if (received.length !== items.length || received.some((row, index) => row.source !== items[index].source || row.batch !== batch.batch))
    throw Error('coverage admission did not retain each source');
  const enqueued = received.filter(row => row.outcome === 'enqueued');
  if (enqueued.some(row => !row.task_id)) throw Error('coverage admission does not prove its new tasks');
  return [...new Set(enqueued.map(row => row.task_id))];
}

export const boardCoverageOrderAdapter = { prepare: prepareOrders, admit: admitCoverageOrders };
