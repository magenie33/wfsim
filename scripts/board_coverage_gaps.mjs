// SPDX-License-Identifier: AGPL-3.0-or-later
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { once } from 'node:events';
import { parseArgs } from 'node:util';
import { pathToFileURL } from 'node:url';
import { computationCanon } from '../worker/computation_input.js';
import { prepareIntakeBuild } from './build_batches.mjs';

const shape = node => [node.identity, node.ruler, node.mode];
const identity = /^[0-9a-f]{32}$/;

async function capture(binary, rows) {
  if (!rows.length) return [];
  const stdout = await new Promise((resolve, reject) => {
    const child = execFile(binary, [], { timeout: 35_000, maxBuffer: 32 * 1024 * 1024 },
      (error, output, stderr) => error ? reject(Error('Native coverage capture failed: ' + stderr.slice(-2000))) : resolve(output));
    child.stdin.on('error', reject);
    child.stdin.end(rows.map(({ id, at, record }) => JSON.stringify({ id, at, record, initiator: 'system', observation: {} })).join('\n') + '\n');
  });
  return stdout.split('\n').filter(Boolean).map(JSON.parse);
}

async function ordersForCapture(captured, candidate, native) {
  const { id, at, record, witness } = candidate;
  if (captured.id !== id || computationCanon(captured.record) !== computationCanon(JSON.parse(record)) || !captured.flow?.nodes)
    return [];
  const { flow, deadlines } = await prepareIntakeBuild(captured);
  const boards = new Map(flow.scope.boards.map(board => [board.id, board]));
  const { weapon } = captured.record;
  const orders = [];
  for (const node of flow.nodes) {
    if (node.kind !== 'measure' || node.identity !== id || node.review || !boards.has(node.ruler)) continue;
    const { ruler, mode, computation } = node;
    const order = { kind: 'board_coverage', native, target: { identity: id, weapon, ruler, mode, computation, witness },
      plan: { id, at, record: captured.record, initiator: 'system', derived: false, deadlines,
        flow: { schema: flow.schema, builds: [{ id, record: captured.record }],
          scope: { boards: [boards.get(ruler)], modes: { [weapon]: [mode] } },
          templates: [flow.templates[node.template]], patches: [flow.patches[node.patch]],
          nodes: [{ ...node, template: 0, patch: 0 }] } } };
    orders.push(order);
  }
  return orders.sort((a, b) => a.target.mode.localeCompare(b.target.mode) || a.target.ruler.localeCompare(b.target.ruler));
}

export async function boardCoverageGaps({ reader, binary, native, after = '', build = null, rows = 16 }) {
  if (!/^[0-9a-f]{64}$/.test(native || '') || !(after === '' || identity.test(after)) ||
      (build !== null && !identity.test(build)) || !Number.isInteger(rows) || rows < 1 || rows > 64)
    throw Error('invalid board coverage scan');
  const candidates = await reader.begin(async tx => {
    await tx.unsafe("SET TRANSACTION READ ONLY; SET LOCAL statement_timeout='5s'; SET LOCAL jit=off");
    return tx.unsafe(`WITH builds AS MATERIALIZED (
        SELECT id,at,record FROM entity_weapon_builds
        WHERE id>$1 AND ($2::text IS NULL OR id=$2) ORDER BY id LIMIT $3
      ) SELECT b.*,anchor.witness FROM builds b LEFT JOIN LATERAL (
        SELECT jsonb_build_array(s.ruler,s.mode) witness FROM state_board_measurement_selection s
        LEFT JOIN entity_measurement_reviews r ON r.id=s.review_id
        WHERE s.identity=b.id AND s.score>=0 AND s.score<'Infinity'::float8
          AND (s.review_id IS NULL OR (r.id IS NOT NULL AND (r.invalidated_at IS NULL OR r.state='succeeded')))
        ORDER BY s.ruler,s.mode LIMIT 1
      ) anchor ON true ORDER BY b.id`, [after, build, rows]);
  });
  const measured = candidates.filter(row => row.witness);
  const bySource = new Map(measured.map(row => [row.id, row]));
  const normalized = await capture(binary, measured);
  const orders = [];
  for (const row of normalized) {
    const candidate = row.from?.length === 1 && bySource.get(row.from[0]);
    if (!candidate) throw Error('Native coverage capture lost its source');
    orders.push(...await ordersForCapture(row, candidate, native));
    bySource.delete(candidate.id);
  }
  const missing = [];
  for (let first = 0; first < orders.length; first += 256) {
    const page = orders.slice(first, first + 256);
    const existing = await reader.begin(async tx => {
      await tx.unsafe("SET TRANSACTION READ ONLY; SET LOCAL statement_timeout='5s'; SET LOCAL jit=off");
      return tx.unsafe(`SELECT x.identity,x.ruler,x.mode
        FROM jsonb_to_recordset(($1::text)::jsonb) x(identity text,ruler text,mode text,computation text)
        WHERE EXISTS (SELECT 1 FROM state_board_measurement_selection s
          WHERE (s.identity,s.ruler,s.mode)=(x.identity,x.ruler,x.mode) OFFSET 0)
        OR EXISTS (SELECT 1 FROM state_board_pending_work p
          WHERE (p.build_id,p.ruler,p.mode)=(x.identity,x.ruler,x.mode) OFFSET 0)
        OR EXISTS (SELECT 1 FROM state_build_workflow_nodes n
          JOIN state_compute_effective_work q ON q.id=n.computation AND q.task_id=n.task_id
          WHERE n.computation=x.computation AND n.state='queued'
            AND n.definition->>'identity'=x.identity AND n.definition->>'ruler'=x.ruler
            AND n.definition->>'mode'=x.mode OFFSET 0)`, [JSON.stringify(page.map(order => order.target))]);
    });
    const occupied = new Set(existing.map(row => JSON.stringify(shape(row))));
    missing.push(...page.filter(order => !occupied.has(JSON.stringify(shape(order.target)))));
  }
  return { next: candidates.at(-1)?.id ?? after, scanned: candidates.length, measured: measured.length, orders: missing };
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) {
  const { values } = parseArgs({ options: { after: { type: 'string' }, build: { type: 'string' },
    'max-builds': { type: 'string' }, binary: { type: 'string' }, native: { type: 'string' } } });
  const url = process.env.WFSIM_COMPUTE_ORDER_DATABASE_URL, maximum = Number(values['max-builds'] ?? (values.build ? 1 : NaN));
  if (!url || !values.binary || !/^[0-9a-f]{64}$/.test(values.native || '') || !Number.isSafeInteger(maximum) || maximum < 1)
    throw Error('usage: board_coverage_gaps.mjs --binary=PATH --native=SHA (--build=ID | --max-builds=N [--after=ID])');
  const hash = createHash('sha256');
  for await (const bytes of createReadStream(values.binary)) hash.update(bytes);
  if (hash.digest('hex') !== values.native) throw Error('Native intake binary changed');
  const { default: postgres } = await import('./pg/node_modules/postgres/src/index.js');
  const reader = postgres(url, { max: 1, fetch_types: false,
    connection: { application_name: 'wfsim-board-coverage-gaps', default_transaction_read_only: 'on' } });
  let after = values.after ?? '', scanned = 0, measured = 0, missing = 0;
  try {
    while (scanned < maximum) {
      const page = await boardCoverageGaps({ reader, binary: values.binary, native: values.native, after,
        build: values.build ?? null, rows: Math.min(maximum - scanned, 16) });
      if (!page.scanned) break;
      for (const order of page.orders) if (!process.stdout.write(JSON.stringify(order) + '\n')) await once(process.stdout, 'drain');
      after = page.next;
      scanned += page.scanned;
      measured += page.measured;
      missing += page.orders.length;
    }
    console.error(JSON.stringify({ after, scanned, measured, missing }));
  } finally { await reader.end(); }
}
