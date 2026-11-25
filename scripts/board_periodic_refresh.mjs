// SPDX-License-Identifier: AGPL-3.0-or-later
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { captureBoardBuilds } from './board_coverage_gaps.mjs';
import { requestedPlan } from './board_requested_intake.mjs';
import { prepareIntakeBuild } from './build_batches.mjs';
import { close, query } from './db.mjs';
import { DEADLINE_MS, REFRESH_WITHIN_MS } from '../worker/deadlines.js';

const DAY_MS = 86_400_000;
export const REFRESH_ZONE = 4;
const PAGE = 64;
const REASON = 'periodic board refresh';

export function refreshCutoff(now, days = REFRESH_WITHIN_MS / DAY_MS) {
  if (!Number.isSafeInteger(days) || days <= 0 || !Number.isFinite(now))
    throw Error('the board refresh age must be a positive number of whole days');
  return new Date(now - days * DAY_MS).toISOString();
}

export async function refreshPage({ read, capture, admit, cutoff, after = ['', '', '', ''], now = Date.now() }) {
  const rows = await read('SELECT * FROM board_periodic_refresh_candidates(?::text,?::text,?::text,?::text,?::text,?::bigint)', [cutoff, ...after, PAGE]);
  if (!rows.length) return { after: null, scanned: 0, asked: 0, refused: 0 };

  const builds = new Map(rows.map(row => [row.identity, { id: row.identity, at: row.at, record: row.record }]));
  const captured = new Map((await capture([...builds.values()])).map(build => [build.id, build]));
  const plans = [];
  let refused = 0;
  for (const row of rows) {
    const build = captured.get(row.identity);
    if (!build || !build.flow?.nodes?.some(node => node.kind === 'measure'
        && node.identity === row.identity && node.ruler === row.ruler && node.mode === row.mode)) {
      refused++;
      continue;
    }
    const plan = requestedPlan(build, { kind: 'refresh', ruler: row.ruler, mode: row.mode });
    const source = row.source_id;
    plan.initiator = 'system';
    plan.zone = REFRESH_ZONE;
    plan.from = [source + '|' + row.identity];
    plan.observation = null;
    plan.system_source = { id: source, reason: REASON, measured_at: row.finished_at };
    plan.deadlines = { ...plan.deadlines, system: new Date(now + DEADLINE_MS.refresh).toISOString() };
    plans.push(await prepareIntakeBuild(plan));
  }
  // Recheck the captured baseline before durable admission. Expansion also
  // rechecks it if another calculation lands while the workflow is waiting.
  const admitted = plans.length ? await admit('SELECT board_periodic_refresh_admit(?::text) AS asked', [JSON.stringify(plans)]) : [{ asked: 0 }];
  const last = rows.at(-1);
  return { after: rows.length === PAGE ? [last.finished_at,last.identity,last.ruler,last.mode] : null,
    scanned: rows.length, asked: Number(admitted[0].asked), refused };
}

export async function refreshBoard({ binary, days, now = Date.now(), read = query, admit = query }) {
  const cutoff = refreshCutoff(now, days);
  let after = ['', '', '', ''];
  const total = { cutoff, scanned: 0, asked: 0, refused: 0 };
  do {
    const result = await refreshPage({ cutoff, now, after, read, admit, capture: rows => captureBoardBuilds(binary, rows) });
    for (const key of ['scanned','asked','refused']) total[key] += result[key];
    after = result.after;
  } while (after);
  return total;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  try {
    const binary = process.argv[2] || 'target/release/wfsim-intake';
    const days = process.env.BOARD_REFRESH_DAYS ? Number(process.env.BOARD_REFRESH_DAYS) : undefined;
    console.log(JSON.stringify(await refreshBoard({ binary, days })));
  } finally {
    await close();
  }
}
