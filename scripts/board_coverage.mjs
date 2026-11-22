// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createInterface } from 'node:readline';
import { pathToFileURL } from 'node:url';
import { computationCanon, computationId } from '../worker/computation_input.js';

export const BOARD_COVERAGE_BATCH_SIZE = 32;
const sha256 = value => createHash('sha256').update(computationCanon(value)).digest('hex');
const nonAdmittingOutcomes = ['skipped_prior_work', 'already_measured', 'baseline_changed', 'rejected_identity', 'rejected_retired'];

function expandRequest(flow, node) {
  assert(Number.isSafeInteger(node.template) && Number.isSafeInteger(node.patch));
  const template = flow.templates[node.template], patch = flow.patches[node.patch];
  assert(template && patch?.set && Array.isArray(patch.remove));
  const request = { ...template, ...patch.set };
  for (const key of patch.remove) delete request[key];
  return request;
}

async function captureGroup(items, pins, namespace) {
  const first = items[0];
  const { identity, mode, weapon } = first.target;
  const { record, at, deadlines } = first.plan;
  const targets = [];
  const nodes = [];
  const boards = [];
  const patches = [];
  const distinctPatches = new Map();
  let template;

  for (const item of items) {
    const { target, plan, source } = item;
    const flow = plan.flow;
    assert.equal(target.identity, identity);
    assert.equal(target.mode, mode);
    assert.equal(target.weapon, weapon);
    assert.equal(target.engine, pins.engine);
    assert.equal(target.source, source);
    assert.equal(source, [namespace, identity, target.ruler, mode].join('/'));
    assert.equal(plan.id, identity);
    assert.equal(plan.record.weapon, weapon);
    assert.deepEqual(plan.record, record);
    assert.deepEqual(plan.from, [source]);
    assert.equal(plan.initiator, 'system');
    assert.equal(plan.derived, false);
    for (const key of ['zone', 'observation', 'system_source', 'flow_source', 'family_flow', 'family_root', 'family_creation', 'recovery_of']) {
      assert(plan[key] == null);
    }
    assert.equal(flow.schema, 1);
    assert.deepEqual(flow.builds, [{ id: identity, record }]);
    assert.deepEqual(flow.scope.modes, { [weapon]: [mode] });
    assert.equal(flow.scope.boards.length, 1);
    const [board] = flow.scope.boards;
    assert.equal(board.id, target.ruler);
    assert.equal(flow.nodes.length, 1);
    const [node] = flow.nodes;
    assert.equal(node.kind, 'measure');
    assert.equal(node.identity, identity);
    assert.equal(node.ruler, target.ruler);
    assert.equal(node.mode, mode);
    assert.equal(node.review, undefined);
    const request = expandRequest(flow, node);
    assert.equal(node.computation, target.computation);
    assert.equal(await computationId('simulate', request), target.computation);
    template ??= request;
    const remove = Object.keys(template).filter(key => !Object.hasOwn(request, key));
    const set = Object.fromEntries(Object.entries(request).filter(([key, value]) =>
      !Object.hasOwn(template, key) || computationCanon(value) !== computationCanon(template[key])));
    const patch = { set, remove };
    const key = computationCanon(patch);
    if (!distinctPatches.has(key)) {
      distinctPatches.set(key, patches.length);
      patches.push(patch);
    }
    nodes.push({ ...node, template: 0, patch: distinctPatches.get(key) });
    boards.push(board);
    targets.push(target);
  }

  return { targets, plan: {
    id: identity, record, at, initiator: 'system', derived: false, deadlines,
    from: targets.map(target => target.source),
    flow: { schema: 1, builds: [{ id: identity, record }], scope: { boards, modes: { [weapon]: [mode] } },
      templates: [template], patches, nodes },
  } };
}

// The existing campaign owns progress. A different cursor cannot use this batch.
export async function prepareBoardCoverageBatch(checkpoint, proposal, sealed, pins) {
  assert.match(pins.native, /^[0-9a-f]{64}$/);
  assert.match(pins.engine, /^[0-9a-f]{12,64}$/);
  assert.equal(sealed.native, pins.native);
  assert.equal(sealed.engine, pins.engine);
  assert.match(checkpoint.manifest, /^[0-9a-f]{64}$/);
  const namespace = 'coverage-' + checkpoint.manifest.slice(0, 16);
  assert.equal(sealed.namespace, namespace);
  assert.equal(sealed.manifest, checkpoint.manifest);
  assert.equal(proposal.after.manifest, checkpoint.manifest);
  assert.equal(proposal.before, sha256(checkpoint));
  const { id, ...proposed } = proposal;
  assert.equal(id, sha256(proposed));
  assert.equal(sealed.batch, id);
  assert(Number.isSafeInteger(checkpoint.next) && checkpoint.next >= 0);
  assert(Number.isSafeInteger(checkpoint.enqueued) && checkpoint.enqueued >= 0);
  assert(Number.isSafeInteger(checkpoint.target) && checkpoint.target > checkpoint.enqueued);
  assert.equal(proposal.after.target, checkpoint.target);
  assert.equal(proposal.after.enqueued, checkpoint.enqueued);
  const first = checkpoint.next + 1;
  const size = proposal.rows.length;
  assert(size >= 1 && size <= Math.min(BOARD_COVERAGE_BATCH_SIZE, checkpoint.target - checkpoint.enqueued));
  assert.equal(sealed.items.length, size);
  assert.equal(proposal.after.next, checkpoint.next + size);

  const groups = [];
  let segment = [];
  for (const [index, item] of sealed.items.entries()) {
    const { computation, engine, ...row } = item.target;
    assert.match(computation, /^[0-9a-f]{32}$/);
    assert.equal(engine, pins.engine);
    assert.deepEqual(row, proposal.rows[index]);
    assert.equal(row.ordinal, first + index);
    if (segment.length && (row.identity !== segment[0].target.identity || row.mode !== segment[0].target.mode)) {
      groups.push(await captureGroup(segment, pins, namespace));
      segment = [];
    }
    segment.push(item);
  }
  groups.push(await captureGroup(segment, pins, namespace));
  return { epoch: pins.epoch, batch: { batch: id, manifest: checkpoint.manifest, namespace,
    native: pins.native, engine: pins.engine, first, last: proposal.after.next, groups } };
}

function checkpointReceipt(receipt, { batch, epoch }) {
  const targets = batch.groups.flatMap(group => group.targets);
  assert.equal(receipt?.batch, batch.batch);
  assert.equal(receipt.engine, batch.engine);
  assert.equal(receipt.epoch, epoch);
  assert.equal(receipt.rows.length, targets.length);
  const taskIds = new Set();
  const rows = receipt.rows.map((row, index) => {
    assert.equal(row.source, targets[index].source);
    assert.equal(row.computation, targets[index].computation);
    if (row.outcome === 'enqueued' && row.batch === batch.batch) {
      assert(Number.isSafeInteger(row.task_id) && row.task_id > 0 && !taskIds.has(row.task_id));
      taskIds.add(row.task_id);
      return row;
    }
    assert(nonAdmittingOutcomes.includes(row.outcome) || row.outcome === 'joined_shared_task' || row.outcome === 'enqueued');
    if (row.outcome === 'joined_shared_task' || row.outcome === 'enqueued') return { ...row, outcome: 'skipped_prior_work' };
    return row;
  });
  return { receipt: { ...receipt, rows }, dispositions: receipt.rows, taskIds: [...taskIds] };
}

export class BoardCoverageReadbackRequired extends Error {
  constructor(batch, cause) {
    super('coverage transaction requires independent readback for ' + batch, { cause });
  }
}

// Callers can check an uncertain batch again without replaying a write.
export async function readBoardCoverageBatch(reader, prepared) {
  const { batch, epoch } = prepared;
  const [{ receipt }] = await reader.begin(async transaction => {
    await transaction.unsafe("SET TRANSACTION READ ONLY; SET LOCAL statement_timeout='2s'; SET LOCAL lock_timeout='300ms'");
    return transaction.unsafe('SELECT board_coverage_readback(($1::text)::jsonb,$2,$3) receipt',
      [JSON.stringify(batch), epoch, batch.engine]);
  });
  return receipt ? checkpointReceipt(receipt, prepared) : null;
}

export async function admitBoardCoverageBatch(writer, reader, prepared) {
  assert.notEqual(writer, reader, 'coverage readback needs its own connection');
  const { batch, epoch } = prepared;
  let written = false;
  let commitError;
  try {
    await writer.begin(async transaction => {
      await transaction.unsafe("SET LOCAL statement_timeout='8s'; SET LOCAL lock_timeout='500ms'; SET LOCAL temp_file_limit='16MB'");
      await transaction.unsafe('SELECT board_coverage_admit(($1::text)::jsonb,$2,$3)',
        [JSON.stringify(batch), epoch, batch.engine]);
      written = true;
    });
  } catch (error) {
    if (!written) throw error;
    commitError = error;
  }
  let verified;
  try {
    verified = await readBoardCoverageBatch(reader, prepared);
  } catch (error) {
    throw new BoardCoverageReadbackRequired(batch.batch, error);
  }
  if (!verified) throw new BoardCoverageReadbackRequired(batch.batch, commitError);
  return verified;
}

export async function coverageSession(input, output, open) {
  const writer = open();
  const reader = open();
  try {
    for await (const line of createInterface({ input, crlfDelay: Infinity })) {
      const { operation, checkpoint, proposal, sealed, pins } = JSON.parse(line);
      assert(operation === 'admit' || operation === 'read');
      const prepared = await prepareBoardCoverageBatch(checkpoint, proposal, sealed, pins);
      try {
        const verified = operation === 'admit'
          ? await admitBoardCoverageBatch(writer, reader, prepared)
          : await readBoardCoverageBatch(reader, prepared);
        output.write(JSON.stringify({ batch: proposal.id, verified }) + '\n');
        if (!verified) break;
      } catch (error) {
        if (!(error instanceof BoardCoverageReadbackRequired)) throw error;
        output.write(JSON.stringify({ batch: proposal.id, state: 'readback_required' }) + '\n');
        break;
      }
    }
  } finally {
    await Promise.all([writer.end(), reader.end()]);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const url = process.env.WFSIM_BOARD_COVERAGE_DATABASE_URL;
  assert(url && process.argv.length === 2, 'set the owner connection for the coverage session');
  const { default: postgres } = await import('./pg/node_modules/postgres/src/index.js');
  const open = () => postgres(url, { max: 1, fetch_types: false, prepare: false,
    connection: { application_name: 'wfsim-board-coverage' } });
  await coverageSession(process.stdin, process.stdout, open);
}
