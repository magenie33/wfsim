// SPDX-License-Identifier: AGPL-3.0-or-later
// THE SETTLER — docs/BOARD.md §"The settler". What the factory works out away
// from a request: the snapshot of who is on and what is open, the fair-share
// table, and the judging of settled questions. Every function takes an `env`
// whose `LIBRARY` (and `ACCOUNTS`) are statement handles (worker/db.js), so the
// daemon beside the database (scripts/factoryd.mjs) and the worker's cron run
// the same code. IN SHADOW nothing here changes what the worker decides: it
// writes only its own tables and compares its judgement with the request path's.
import { iso } from "./instant.js";
import { tableOf, FAIR_KEEP_MS } from "./fair.js";
import { independent, sameAs, askingIds } from "./tasks.js";
import { dialsOf } from "./dials.js";
import { COMPUTING_MS } from "./contribution.js";
import { facts } from "./factory.js";
import { STATISTICS_OPEN } from "./statistics_open.js";
import {placeholders} from './placeholders.js';

/// THE FACTORY NOW, written as the one `factory_snapshot` row. `asking` and
/// `computing` are the worker's own definitions (worker/verify.js `work`,
/// worker/contribution.js `computingNow`). OPEN IS COUNTED AS FAR AS ANY PACK
/// COULD NEED: the asking times the largest pack either protocol hands out, all
/// that `factory_open` (which every hand-out reads this row through) compares
/// it to — a fixed ceiling read work as short once enough computers asked.
export async function snapshotTick(env, { engine = null, release = null } = {}, now = Date.now(), options = {}) {
  const db = env.LIBRARY;
  if(options.allowed&&!await options.allowed())return false;
  const counts=await(typeof options.counts==='function'?options.counts():options.counts),row=counts?.snapshot;
  if(!row||!options.write)return false;
  if(row.at!==counts.countsAt||row.engine!==engine)throw Error('statistics sample identity changed');
  const {at}=row;
  if(options.allowed&&!await options.allowed())return false;
  const statement=db.prepare(`INSERT INTO state_factory_snapshot (id, at, engine, release, open, urgent, asking, computing, queue_rows, settle_queue_rows, daemon_at, daemon_commit)
      VALUES (1, ?1::text, ?2::text, ?3::text, ?4::bigint, ?5::bigint, ?6::bigint, ?7::bigint, ?8::bigint, ?9::bigint, '1970-01-01T00:00:00.000Z', '')
      ON CONFLICT (id) DO UPDATE SET at = excluded.at, engine = excluded.engine, release = excluded.release, open = excluded.open,
        urgent = excluded.urgent, asking = excluded.asking, computing = excluded.computing, queue_rows = excluded.queue_rows,
        settle_queue_rows = excluded.settle_queue_rows WHERE state_factory_snapshot.at IS NULL OR state_factory_snapshot.at<=excluded.at`)
    .bind(at, row.engine, row.release, row.open, row.urgent, row.asking, row.computing, row.queue_rows, row.settle_queue_rows);
  if(!await options.write(statement))return false;
  return row;
}

export async function statisticsSnapshot(query,counts,{engine=null,release=null},d,now){
  const at=counts.at;
  const [n]=await query(placeholders(`SELECT
    (SELECT count(*) FROM ${askingIds(1,2)} x)::text asking,
    (SELECT count(*) FROM entity_compute_devices WHERE banned=0 AND last_at>=?3::text)::text computing,
    (SELECT count(*) FROM state_pending_verifications)::text settle_queue_rows`),[iso(now-2*d.asked_every_ms),at,iso(now-COMPUTING_MS)]);
  const asking=Number(n.asking);
  return{...counts,engine,release,open:Math.min(counts.open,Math.max(1,asking)*Math.max(d.pack8_max,d.pack_max)),asking,
    computing:Number(n.computing),settle_queue_rows:Number(n.settle_queue_rows)};
}

export async function signalTick(env,{engine=null,release=null}={},now=Date.now()) {
  const at=iso(now),d=await dialsOf(env,now);
  const n=await env.LIBRARY.prepare(`WITH a AS (SELECT count(*) AS n FROM ${askingIds(3, 1)} x),${STATISTICS_OPEN} SELECT
      (SELECT count(*) FROM (SELECT 1 FROM stats_open WHERE waits = 0
         AND (lease_until IS NULL OR lease_until < $1::text) LIMIT GREATEST(1, (SELECT n FROM a)) * $2::bigint) o) AS open,
      (SELECT count(*) FROM stats_open WHERE waits > 0) AS urgent,
      (SELECT n FROM a) AS asking,
      (SELECT count(*) FROM entity_compute_devices WHERE banned = 0 AND last_at >= $4::text) AS computing,
      compute_order_cut_epoch() AS epoch`)
    .bind(at,Math.max(d.pack8_max,d.pack_max),iso(now-2*d.asked_every_ms),iso(now-COMPUTING_MS)).first();
  return {at,engine,release,...n,pack_max:Math.max(d.pack8_max,d.pack_max)};
}

// ---- The work signal ---------------------------------------------------------------

/// THE WORK SIGNAL, `cdn.wfsim.app/work.json`, from a sample and the dials in
/// force: what an idle computer reads instead of asking the worker. `v` changes
/// only when a field changes meaning.
export function signalBody(snap, d, { protocol, protocols }) {
  return { v: 1, at: snap.at, order_epoch: snap.epoch??null, open: Number(snap.open), urgent: Number(snap.urgent), engine: snap.engine, release: snap.release,
    protocol, protocols: [...protocols].sort((a, b) => b - a), computing: Number(snap.computing),
    poll_ms: d.poll_ms, jitter_ms: d.jitter_ms, urgent_jitter_ms: d.urgent_jitter_ms, mode: d.signal_mode === 1 ? "ask" : "signal" };
}

/// IS THERE WORK AT ALL, open or waited on — two indexed probes, cheap enough to
/// ask every couple of seconds so a wave from nothing is signalled at once.
export async function workWaiting(db, now = Date.now()) {
  const r = await db.prepare(`WITH ${STATISTICS_OPEN} SELECT
      EXISTS (SELECT 1 FROM stats_open WHERE state = 'open' AND waits = 0 AND (lease_until IS NULL OR lease_until < ?1::text)) AS open,
      EXISTS (SELECT 1 FROM stats_open WHERE state = 'open' AND waits > 0) AS urgent`).bind(iso(now)).first();
  return { open: !!r.open, urgent: !!r.urgent };
}

// ---- Fair share ------------------------------------------------------------------

/// A RATIO MAY BE INFINITE (an account that received work with no target); JSON
/// cannot carry that, Postgres's double can, so it travels as its own word.
const asFloat = (x) => (Number.isFinite(x) ? x : String(x));

/// THE FAIR-SHARE TABLE FOR `engine`, worked out by the worker's own `tableOf`
/// and written as `fair_shares` in one transaction.
export async function fairTick(env, engine, now = Date.now()) {
  const table = await tableOf(env, engine, now), db = env.LIBRARY;
  const rows = Object.entries(table).map(([account, t]) => ({ account, target: t.target, share: t.share, ratio: asFloat(t.ratio) }));
  await db.batch([
    db.prepare("DELETE FROM state_compute_fair_shares WHERE engine = ?").bind(engine),
    db.prepare(`INSERT INTO state_compute_fair_shares (engine, account, target, share, ratio, at)
        SELECT ?1::text, r.account, r.target, r.share, r.ratio, ?2::text
        FROM jsonb_to_recordset((?3::text)::jsonb) AS r(account text, target double precision, share double precision, ratio double precision)`)
      .bind(engine, iso(now), JSON.stringify(rows)),
  ]);
  return table;
}

/// `fair_shares` FOR `engine` as `tableOf`'s shape, and when it was written.
export async function readFair(db, engine) {
  const { results } = await db.prepare("SELECT account, target, share, ratio, at FROM state_compute_fair_shares WHERE engine = ?").bind(engine).all();
  const table = {};
  for (const r of results) table[r.account] = { target: Number(r.target), share: Number(r.share), ratio: Number(r.ratio) };
  return { table, at: results.length ? results[0].at : null };
}

/// TWO FAIR-SHARE TABLES COMPARED: accounts in one and not the other, and the
/// largest difference in a target or a share. `tolerance` 0 asks for the same
/// numbers; the cross-runtime shadow allows what moves between two readings.
export function compareFair(a, b, tolerance = 0) {
  const missing = Object.keys(a).filter((k) => !(k in b)), extra = Object.keys(b).filter((k) => !(k in a));
  let worst = { account: null, field: null, delta: 0 };
  for (const k of Object.keys(a)) {
    if (!(k in b)) continue;
    for (const f of ["target", "share", "ratio"]) {
      const x = a[k][f], y = b[k][f];
      const delta = x === y ? 0 : Number.isFinite(x) && Number.isFinite(y) ? Math.abs(x - y) : Infinity;
      if (f !== "ratio" && delta > worst.delta) worst = { account: k, field: f, delta };
      if (f === "ratio" && delta === Infinity && worst.delta !== Infinity) worst = { account: k, field: f, delta };
    }
  }
  return { accounts: Object.keys(a).length, missing, extra, worst, differ: missing.length > 0 || extra.length > 0 || worst.delta > tolerance };
}

/// HOW FAR TWO READINGS A FEW MINUTES APART MAY DIFFER and still be one table:
/// asks and answers land between them.
export const FAIR_SHADOW_TOLERANCE = 0.05;

/// THE CROSS-RUNTIME SHADOW, run by the worker's cron: the daemon's table
/// against the worker's own `tableOf` now, folded into the `fair_shadow`
/// finding. Nothing when the daemon has written nothing lately.
export async function fairShadow(env, engine, now = Date.now()) {
  const { table, at } = await readFair(env.LIBRARY, engine);
  if (!at || now - Date.parse(at) > 2 * FAIR_KEEP_MS) return null;
  const cmp = compareFair(table, await tableOf(env, engine, now), FAIR_SHADOW_TOLERANCE);
  await findingsTick(env.LIBRARY, "fair_shadow", (prev) => ({
    since: prev.since || iso(now), runs: (prev.runs || 0) + 1, differ: (prev.differ || 0) + (cmp.differ ? 1 : 0),
    last: { at: iso(now), daemon_at: at, ...cmp },
    worst: cmp.differ ? { at: iso(now), ...cmp } : prev.worst || null,
  }), now);
  return cmp;
}

// ---- Settling, in shadow ----------------------------------------------------------

/// A SETTLED QUESTION IS JUDGED ONLY THIS LONG AFTER ITS FACT: its credits are
/// paid after the settlement commits (worker/tasks.js `credit`).
export const SETTLE_SHADOW_LAG_MS = 2 * 60_000;

/// WHAT THE REQUEST PATH SHOULD HAVE WRITTEN for one settled question, judged
/// again from its answers as they stood at its fact: `{ skip }` where the
/// shadow does not judge (an official answer settled it, or an older engine was
/// a peer), `{ ok }` where it agrees, `{ differ, ids? }` where it does not.
export function judgeFact(q, answers, threshold) {
  if (!q.fact_engine) return { skip: "no_engine" };
  const before = answers.filter((a) => a.at <= q.fact_at);
  if (before.some((a) => a.official)) return { skip: "official" };
  const served = before.filter((a) => a.engine === q.fact_engine), needs = q.needs || threshold;
  const others = before.some((a) => a.engine !== q.fact_engine);
  for (const lead of served) {
    const equal = before.filter(sameAs(lead));
    const group = independent([...equal.filter((a) => a.engine === q.fact_engine), ...equal.filter((a) => a.engine !== q.fact_engine)]);
    if (group.length < needs) continue;
    if (lead.canon !== q.canon) return others ? { skip: "peer" } : { differ: "canon" };
      const unpaid = group.filter((a) => !a.credited);
      if(unpaid.length && unpaid.every(a=>a.credit_work!=null))return {skip:"pending_credit"};
      return unpaid.length ? { differ: "uncredited", ids: unpaid.map((a) => a.id) } : { ok: true };
  }
  return others ? { skip: "peer" } : { differ: "no_group" };
}

const SHADOW_PAGE_MAX = 200;

/// ONE BATCH OF SETTLED QUESTIONS judged in shadow, after `from` (a `{ at, id }`
/// keyset cursor), and folded into the `settle_shadow` finding. Read-only on
/// every table the request path writes. Returns the next cursor and the counts.
export async function settleShadow(env, { threshold, from, max = SHADOW_PAGE_MAX }, now = Date.now()) {
  if (!Number.isInteger(max) || max < 1 || max > SHADOW_PAGE_MAX) throw new Error("invalid settlement shadow page size");
  const db = env.LIBRARY, until = iso(now - SETTLE_SHADOW_LAG_MS);
  const { results: page } = await db.prepare(`WITH page AS MATERIALIZED (
      SELECT id, fact_at FROM state_compute_scheduling
      WHERE state IN ('fact', 'spot') AND fact_at IS NOT NULL AND fact_at <= ?1::text
        AND (fact_at, id) > (?2::text, ?3::text)
      ORDER BY fact_at, id LIMIT ?4::bigint
    )
    SELECT p.id, p.fact_at, q.id IS NOT NULL AS effective, q.state, q.canon, q.needs, q.fact_engine
    FROM page p LEFT JOIN LATERAL (
      SELECT id, state, canon, needs, fact_engine FROM state_compute_work WHERE id = p.id OFFSET 0
    ) q ON true
    ORDER BY p.fact_at, p.id`).bind(until, from.at, from.id, max).all();
  const out = { cursor: from, checked: 0, ok: 0, skipped: {}, differ: {}, samples: [] };
  if (!page.length) return out;
  const qs = page.filter(q => q.effective);
  const { results: as } = qs.length ? await db.prepare(`SELECT a.* FROM jsonb_array_elements_text((?::text)::jsonb) q(id)
      CROSS JOIN LATERAL (
        SELECT id, question, engine, owner, net, official, canon, credited, at
        FROM record_candidate_answers WHERE question = q.id AND void = 0 OFFSET 0
      ) a ORDER BY a.id`).bind(JSON.stringify(qs.map(q => q.id))).all() : { results: [] };
  const byQ = new Map(qs.map((q) => [q.id, []]));
  for (const a of as) byQ.get(a.question).push(a);
  for (const q of qs) {
    const v = judgeFact(q, byQ.get(q.id), threshold);
    out.checked += 1;
    if (v.ok) out.ok += 1;
    else if (v.skip) out.skipped[v.skip] = (out.skipped[v.skip] || 0) + 1;
    else { out.differ[v.differ] = (out.differ[v.differ] || 0) + 1; out.samples.push({ question: q.id, why: v.differ }); }
  }
  const last = page[page.length - 1];
  out.cursor = { at: last.fact_at, id: last.id };
  await findingsTick(db, "settle_shadow", (prev) => {
    const add = (a = {}, b) => Object.fromEntries([...new Set([...Object.keys(a), ...Object.keys(b)])].map((k) => [k, (a[k] || 0) + (b[k] || 0)]));
    return { since: prev.since || iso(now), checked: (prev.checked || 0) + out.checked, ok: (prev.ok || 0) + out.ok,
      skipped: add(prev.skipped, out.skipped), differ: add(prev.differ, out.differ),
      samples: [...(prev.samples || []), ...out.samples].slice(-20), cursor: out.cursor };
  }, now);
  return out;
}

// ---- Settling, for real (dial `settle_async`) ----------------------------------

/// A PASS IS GIVEN THIS LONG TO EMPTY THE QUEUE, so one tick never outlives the next.
export const SETTLE_PASS_MS = 1_500;
/// THE SETTLER'S PASSES (worker/tasks.js `settleQueued`): the factory's own
/// settlement over the queued questions, a batch at a time until none is left or
/// the pass has had its time. Returns how many it settled.
export async function settleBatch(env, now = Date.now(), until = Date.now() + SETTLE_PASS_MS) {
  const f = facts();
  let total = 0;
  for (;;) {
    const {processed:n} = await f.runStage(env,"verification",now);
    total += n;
    if (!n || Date.now() >= until) return total;
    now = Date.now();
  }
}

// ---- Findings -------------------------------------------------------------------

/// ONE FINDING, its body folded from the one before (`fold(previous) → next`).
export async function findingsTick(db, kind, fold, now = Date.now()) {
  const had = await db.prepare("SELECT body FROM state_factory_findings WHERE kind = ?").bind(kind).first();
  let prev = {};
  try { prev = had ? JSON.parse(had.body) : {}; } catch { /* a body unread starts again */ }
  const body = JSON.stringify(fold(prev));
  await db.prepare(`INSERT INTO state_factory_findings (kind, body, at) VALUES (?1::text, ?2::text, ?3::text)
      ON CONFLICT (kind) DO UPDATE SET body = excluded.body, at = excluded.at`).bind(kind, body, iso(now)).run();
  return body;
}
