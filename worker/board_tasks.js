// SPDX-License-Identifier: AGPL-3.0-or-later
// THE BOARD AS A PRODUCER of the fact factory (worker/tasks.js; docs/BOARD.md
// §"Compute orders"). Every owed row — `queue` holds what is owed — is a
// question: verb `simulate`, request `{ record, ruler, mode }`, asked by
// `new_build` (a new build's row, someone waiting to see it ranked), `refresh`
// (a row measured three days ago or more, due when it is fifteen) or `sweep` (a
// rescore's, or an older build's first pass on a ruler), whose deadlines are
// worker/factory.js `DEADLINE_MS` and `REFRESH_WITHIN_MS`. What a row is FOR stays here: an agreed number written to
// `scores` and the row no longer owed.
//
//   POST /api/board/ask    { batch, rows: [{ identity, ruler, mode }] }  → { asked, skipped }   (factory token)
//                          { backfill: true }                           → { asked, left }: owed rows nobody asked for
//   POST /api/board/adopt  {}                                                      → { adopted, left }   (factory token)
import { iso } from "./instant.js";
import { witnessOf } from "./tasks.js";
import { facts, DEADLINE_MS, REFRESH_WITHIN_MS, SCENE_FIRST_MS, VERBS, factoryAuthed } from "./factory.js";
import { holdPriority } from "./merit.js";
import { servedRelease } from "./verify.js";

const json = (body, status = 200) => new Response(JSON.stringify(body), {
  status, headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" } });

/// A ROW'S KEY, the demand's `ref`: one per (build, ruler, mode), whichever batch asked.
export const rowRef = (identity, ruler, mode) => `${identity}|${ruler}|${mode}`;
const boardProducer = {
  // A ROW NOBODY OWES ANY MORE — the scorer measured it — is withdrawn where it
  // is found: worker/pg/sync.sql `board_owed`, which the sync's procedure asks
  // too (`wanted_in_sql`).
  wanted_in_sql: true,
  still_wanted: async (env, d) => !!(await env.LIBRARY.prepare("SELECT board_owed(?::text, ?::text, ?::text) AS x")
    .bind(d.payload.identity, d.payload.ruler, d.payload.mode).first()).x,
  // …A WHOLE PACK'S ROWS IN ONE READ: the refs still owed.
  wanted_many: async (env, ds) => {
    const { results } = await env.LIBRARY.prepare(`SELECT k ->> 3 AS ref FROM jsonb_array_elements((?::text)::jsonb) k
        WHERE board_owed(k ->> 0, k ->> 1, k ->> 2)`)
      .bind(JSON.stringify(ds.map((d) => [d.payload.identity, d.payload.ruler, d.payload.mode, d.ref]))).all();
    return new Set(results.map((r) => r.ref));
  },
  // THE CLIENTS' FACT IS THE BOARD'S, as the scorer's is: a row still owed — a
  // rescore asked for it again — takes the new number over the old, and one no
  // longer owed keeps the fact it has. The official machines' own number is
  // written by the scorer (`ship_facts.sh`), which owns `scores` for its rows;
  // one delivered again — a refresh on the engine that made it — only says the
  // row was measured now, or it is asked again every hour.
  // Stated as writes (worker/tasks.js `deliverPlan`), so a pack's facts land in one round trip.
  on_fact_writes: (env, d, fact) => {
    if (!fact.result) return [];
    const { identity, ruler, mode } = d.payload, at = iso(Date.now()), db = env.LIBRARY;
    if (fact.official) {
      return [
        db.prepare("UPDATE scores SET finished_at = ? WHERE identity = ? AND ruler = ? AND mode = ? AND score = ? AND finished_at < ?")
          .bind(at, identity, ruler, mode, fact.result.score, at),
        db.prepare("DELETE FROM queue WHERE build_id = ? AND ruler = ? AND mode = ?").bind(identity, ruler, mode),
      ];
    }
    return [
      db.prepare(
        `INSERT INTO scores (identity, ruler, mode, measured_by, score, metric, cost_seconds, started_at, finished_at)
         VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?)
         ON CONFLICT (identity, ruler, mode) DO UPDATE SET measured_by = excluded.measured_by, score = excluded.score,
           metric = excluded.metric, cost_seconds = 0, started_at = excluded.started_at, finished_at = excluded.finished_at
         WHERE EXISTS (SELECT 1 FROM queue q WHERE q.build_id = scores.identity AND q.ruler = scores.ruler AND q.mode = scores.mode)`)
        .bind(identity, ruler, mode, `verified:${fact.engine}`, fact.result.score, fact.result.metric, at, at),
      db.prepare("DELETE FROM queue WHERE build_id = ? AND ruler = ? AND mode = ?").bind(identity, ruler, mode),
    ];
  },
  // A FACT WITHDRAWN — a witness to it refused — is the clients' number taken
  // off the board and the row owed again, in the batch that asked for it.
  on_retract: async (env, d) => {
    const { identity, ruler, mode, batch } = d.payload, db = env.LIBRARY;
    await db.batch([
      db.prepare("DELETE FROM scores WHERE identity = ? AND ruler = ? AND mode = ? AND measured_by LIKE 'verified:%'").bind(identity, ruler, mode),
      db.prepare(`INSERT INTO queue (batch, build_id, ruler, mode) SELECT ?, ?, ?, ? WHERE EXISTS (SELECT 1 FROM batches WHERE id = ?)
        ON CONFLICT DO NOTHING`)
        .bind(batch, identity, ruler, mode, batch),
    ]);
  },
};
export const BOARD_PRODUCERS = { new_build: boardProducer, refresh: boardProducer, sweep: boardProducer };
/// THE BOARD'S KINDS as an SQL list, for a query over its demands.
export const BOARD_KINDS_SQL = `(${Object.keys(BOARD_PRODUCERS).map((k) => `'${k}'`).join(", ")})`;

/// A SCENE'S RULERS STILL FILLING, for the watchdog: per cell ruler (an id with
/// `__`, docs/BOARD.md §"A ruler's ladder"), the rows measured, those measured
/// in the last `since_ms`, and the rows still owed (a measured row leaves the
/// queue). Scans of the tables, so read only when it is to be said.
export async function cellsFilling(env, now = Date.now(), since_ms = 6 * 3_600_000) {
  const db = env.LIBRARY;
  const { results: owed } = await db.prepare(
    `SELECT ruler, count(DISTINCT build_id || '|' || mode) AS owed FROM queue WHERE ruler LIKE '%!_!_%' ESCAPE '!' GROUP BY ruler`).all();
  if (!owed.length) return [];
  const { results: measured } = await db.prepare(
    `SELECT ruler, count(*) AS measured, sum(CASE WHEN finished_at > ? THEN 1 ELSE 0 END) AS recent FROM scores
       WHERE ruler LIKE '%!_!_%' ESCAPE '!' GROUP BY ruler`)
    .bind(iso(now - since_ms)).all();
  const m = new Map(measured.map((r) => [r.ruler, r]));
  return owed.map((r) => ({ ruler: r.ruler, measured: m.get(r.ruler)?.measured || 0, recent: m.get(r.ruler)?.recent || 0,
    recent_hours: since_ms / 3_600_000, owed: r.owed }));
}

/// THE TASK THE FACTORY LEASED, in the shape the page fights a board row in.
export function boardWork(leased) {
  const d = leased.demands[0];
  return { lease: leased.lease, identity: d.payload.identity, record: leased.request.record, ruler: leased.request.ruler, mode: leased.request.mode };
}
/// …ONE ROW OF A PACK, named by its question, the pack's lease being the pack's.
export const boardPackTask = (q) => ({ question: q.question, identity: q.demands[0].payload.identity,
  record: q.request.record, ruler: q.request.ruler, mode: q.request.mode });

/// WHICH PRODUCER ASKS FOR A ROW: a new build's — an `arrivals-` batch's row
/// whose build came that day or the one before — a `refresh-` batch's, or a
/// sweep's. An older build asked for a ruler it lacks lands in the same daily
/// batch, and is a sweep. A `backfill-` batch is the reconciliation's — rows
/// nobody ordered — and always a sweep: classed by build age, a day's library
/// growth queued inside `new_build` ahead of a submission somebody waits on.
///
/// A LADDER CELL'S ROW IS NEVER `new_build`'s: a new build waits on its base
/// rulers' rows, and every other scene is owed it at a sweep's pace — or each
/// scene added would multiply what must land inside six hours. A cell's id is
/// its base's plus `__<rung>` (`engine::board::benchmarks::expand_ladder`).
export const isLadderCell = (ruler) => ruler.includes("__");
const producerOf = (batch, builtAt, ruler) => {
  if (batch.startsWith("refresh-")) return "refresh";
  if (isLadderCell(ruler)) return "sweep";
  const day = batch.startsWith("arrivals-") && batch.slice(9, 19);
  return day && builtAt && builtAt.slice(0, 10) >= iso(Date.parse(day) - 86_400_000).slice(0, 10) ? "new_build" : "sweep";
};

/// ONE ROW'S ASK: the question its record and ruler make, by its producer. A
/// REFRESH IS DUE BY ITS ROW'S AGE, `REFRESH_WITHIN_MS` after its fact finished,
/// however late it was asked: the bound is on how stale a row gets, not on how
/// long an ask waits.
/// WHO ASKED FOR A BOARD ROW (docs/BOARD.md §"Who asked"): a player's arrival is
/// whoever its queue row names (`queue.initiator`, NULL before it was kept),
/// every other batch WFSim's own — a backfill's too, though it is asked as an
/// arrival is.
export const initiatorOf = (batch, initiator = null) => (batch.startsWith("arrivals-") ? initiator ?? null : "system");
const isPlayer = (initiator) => typeof initiator === "string" && (initiator.startsWith("user:") || initiator === "anonymous");
/// THE FIGHT A PLAYER'S ARRIVAL NAMED IS WAITED ON: they just ran it and are the
/// one looking for the row, so it goes ahead of the build's other rows and of
/// every row nobody waits on. Only the runner knows which row that is (`named`,
/// from `wfsim-intake --asked`), and only a player's (`user:`, `anonymous`)
/// is waited: a survey's or a chat's riven gain, or an arrival nobody recorded,
/// is an ordinary row, as is a backfill's re-ask.
export function rowAsk(batch, r, record, builtAt, measuredAt, now = Date.now()) {
  const producer = producerOf(batch, builtAt, r.ruler);
  const measured = producer === "refresh" && measuredAt ? Date.parse(measuredAt) : NaN;
  const due_in_ms = Number.isFinite(measured) ? measured + REFRESH_WITHIN_MS - now
    : isLadderCell(r.ruler) && !measuredAt ? SCENE_FIRST_MS : DEADLINE_MS[producer];
  return { producer, ref: rowRef(r.identity, r.ruler, r.mode), verb: "simulate",
    request: { record: JSON.parse(record), ruler: r.ruler, mode: r.mode }, due_in_ms,
    payload: { identity: r.identity, ruler: r.ruler, mode: r.mode, batch }, initiator: initiatorOf(batch, r.initiator),
    waits: batch.startsWith("arrivals-") && r.named === true && isPlayer(r.initiator) };
}

/// THE ROWS A BATCH OWES, asked of the factory (`ship_queue.sh`). An arrival's
/// row the board already scored is not asked: a resubmission the page did not
/// recognise is the same build, and asking again would fight it again.
/// A THOUSAND A CALL: a call is a round trip from the runner and a few from the
/// worker whatever its size (`demandAll`), and `ship_queue.sh` sends this many.
const ASK_MAX = 1000;
async function ask(request, env) {
  if (request.method !== "POST") return json({ ok: false, error: "POST only" }, 405);
  if (!factoryAuthed(request, env)) return json({ ok: false, error: "unauthorized" }, 401);
  let b;
  try { b = await request.json(); } catch (_) { return json({ ok: false, error: "not json" }, 400); }
  const db = env.LIBRARY, now = Date.now();
  const { engine } = await servedRelease(env);
  // EVERY OWED ROW IS ASKED FOR, or the clients are never handed it and only the
  // scorer measures it: a run stopped between the queue's write and the ask
  // leaves exactly that. `ASK_MAX` a call, until none is left.
  if (b.backfill === true) {
    const { results } = await db.prepare(
      `SELECT q.build_id AS identity, q.ruler, q.mode, min(q.batch) AS batch, (array_agg(q.initiator ORDER BY q.batch))[1] AS initiator, bu.record, bu.at,
              (SELECT s.finished_at FROM scores s WHERE s.identity = q.build_id AND s.ruler = q.ruler AND s.mode = q.mode) AS measured
         FROM queue q JOIN batches t ON t.id = q.batch JOIN builds bu ON bu.id = q.build_id
        WHERE NOT EXISTS (SELECT 1 FROM demands d WHERE d.ref = q.build_id || '|' || q.ruler || '|' || q.mode
                            AND d.producer IN ${BOARD_KINDS_SQL} AND d.state = 'live')
        GROUP BY q.build_id, q.ruler, q.mode, bu.record, bu.at LIMIT ?`).bind(ASK_MAX).all();
    await facts().demandAll(env, results.map((r) => rowAsk(r.batch, r, r.record, r.at, r.measured, now)), engine, now);
    return json({ ok: true, asked: results.length, left: results.length === ASK_MAX ? null : 0 });
  }
  const rows = Array.isArray(b.rows) ? b.rows.filter((r) => r && typeof r.identity === "string" && typeof r.ruler === "string" && typeof r.mode === "string") : [];
  if (typeof b.batch !== "string" || !b.batch || rows.length > ASK_MAX) return json({ ok: false, error: `needs batch and at most ${ASK_MAX} rows` }, 400);
  const { results: builds } = await db.prepare("SELECT id, record, at FROM builds WHERE id IN (SELECT value FROM jsonb_array_elements_text((?::text)::jsonb))")
    .bind(JSON.stringify([...new Set(rows.map((r) => r.identity))])).all();
  const buildOf = new Map(builds.map((x) => [x.id, x]));
  // WHO ASKED, as the queue row states it: the runner names the rows, never who asked them.
  const { results: named } = await db.prepare(`SELECT q.build_id, q.ruler, q.mode, q.initiator FROM queue q JOIN jsonb_array_elements((?2::text)::jsonb) j
        ON q.build_id = j ->> 'identity' AND q.ruler = j ->> 'ruler' AND q.mode = j ->> 'mode' WHERE q.batch = ?1::text AND q.initiator IS NOT NULL`)
    .bind(b.batch, JSON.stringify(rows.map(({ identity, ruler, mode }) => ({ identity, ruler, mode })))).all();
  const whoOf = new Map(named.map((x) => [rowRef(x.build_id, x.ruler, x.mode), x.initiator]));
  // AN ARRIVAL'S ROW THE BOARD ALREADY SCORED is not asked: a resubmission the
  // page did not recognise is the same build, and asking again fought it again.
  // A REFRESH'S ROW is due by when its fact finished.
  const scored = new Map();
  const arrivals = b.batch.startsWith("arrivals-") || b.batch.startsWith("backfill-");
  if (arrivals || b.batch.startsWith("refresh-")) {
    const { results: had } = await db.prepare(`SELECT s.identity, s.ruler, s.mode, s.finished_at FROM scores s JOIN jsonb_array_elements((?::text)::jsonb) j
          ON s.identity = j ->> 'identity' AND s.ruler = j ->> 'ruler' AND s.mode = j ->> 'mode'`)
      .bind(JSON.stringify(rows.map(({ identity, ruler, mode }) => ({ identity, ruler, mode })))).all();
    for (const x of had) scored.set(rowRef(x.identity, x.ruler, x.mode), x.finished_at);
  }
  const asks = rows.filter((r) => buildOf.has(r.identity) && !(arrivals && scored.has(rowRef(r.identity, r.ruler, r.mode))))
    .map((r) => rowAsk(b.batch, { ...r, initiator: whoOf.get(rowRef(r.identity, r.ruler, r.mode)) }, buildOf.get(r.identity).record, buildOf.get(r.identity).at, scored.get(rowRef(r.identity, r.ruler, r.mode)), now));
  // A SIGNED-IN PLAYER'S MERIT may move their rows sooner (docs/BOARD.md §"Merit priority").
  await facts().demandAll(env, await holdPriority(env, b.batch, asks, BOARD_KINDS_SQL, now), engine, now);
  return json({ ok: true, asked: asks.length, skipped: rows.length - asks.length });
}

/// THE ORDERS OPEN BEFORE THE FACTORY, asked of it once each with the results
/// clients already sent, carried; a lease held now is kept, so its answer lands.
/// Called until `left` is 0. Each is asked as of when it was opened.
const ADOPT_BATCH = 100;
const LIVE = "('todo', 'fresh', 'open', 'dispute', 'spot', 'arbiter', 'scoring:todo', 'scoring:open', 'scoring:fresh')";
async function adopt(request, env) {
  if (request.method !== "POST") return json({ ok: false, error: "POST only" }, 405);
  if (!factoryAuthed(request, env)) return json({ ok: false, error: "unauthorized" }, 401);
  const db = env.LIBRARY, now = Date.now();
  const { engine } = await servedRelease(env);
  const { results } = await db.prepare(
    `SELECT o.*, (SELECT min(q.batch) FROM queue q WHERE q.build_id = o.identity AND q.ruler = o.ruler AND q.mode = o.mode) AS batch
       FROM orders o WHERE o.state IN ${LIVE} AND NOT EXISTS (SELECT 1 FROM demands d WHERE d.ref = o.identity || '|' || o.ruler || '|' || o.mode
         AND d.producer IN ${BOARD_KINDS_SQL}) LIMIT ?`).bind(ADOPT_BATCH).all();
  for (const o of results) {
    const ref = rowRef(o.identity, o.ruler, o.mode);
    // A ROW NOBODY OWES is settled where it is found, as a lease once settled it.
    if (!o.batch) {
      await db.prepare("UPDATE orders SET state = 'settled' WHERE identity = ? AND ruler = ? AND mode = ?").bind(o.identity, o.ruler, o.mode).run();
      continue;
    }
    const producer = o.priority === 0 ? "new_build" : "sweep";
    const request = { record: JSON.parse(o.record), ruler: o.ruler, mode: o.mode };
    await facts().demand(env, { producer, ref, verb: "simulate", request, due_in_ms: DEADLINE_MS[producer],
      payload: { identity: o.identity, ruler: o.ruler, mode: o.mode, batch: o.batch }, initiator: initiatorOf(o.batch) }, engine, Date.parse(o.at));
    const id = (await db.prepare("SELECT question FROM demands WHERE producer = ? AND ref = ?").bind(producer, ref).first()).question;
    // ITS CLIENTS' RESULT, carried: the first and those that agreed with it hold
    // its number; a disputer's own number is not kept.
    if (o.score !== null && o.work !== null) {
      const clients = (o.clients || o.produced_by || "").split(",").filter(Boolean);
      const keep = o.state === "dispute" ? clients.slice(0, -1) : clients;
      for (const c of keep) {
        const who = await witnessOf(env, c);
        await db.prepare(`INSERT INTO answers (question, engine, device, owner, net, canon, result, work, at)
            VALUES (?, ?, ?, ?, '', ?, ?, ?, ?)`)
          .bind(id, o.carried_from || o.engine || "", c, who.owner, VERBS.simulate.canon({ score: o.score, metric: o.metric }, o.work),
            JSON.stringify({ score: o.score, metric: o.metric }), o.work, o.at).run();
      }
      if (keep.length) await facts().settle(env, id, engine, now);
    }
    if (o.lease && o.leased_to && Date.parse(o.lease_until) > now) {
      await db.prepare("UPDATE questions SET lease = ?, lease_until = ?, leased_to = ? WHERE id = ? AND lease IS NULL")
        .bind(o.lease, o.lease_until, o.leased_to, id).run();
    }
  }
  const left = await db.prepare(
    `SELECT count(*) AS n FROM orders o WHERE o.state IN ${LIVE} AND NOT EXISTS (SELECT 1 FROM demands d
       WHERE d.ref = o.identity || '|' || o.ruler || '|' || o.mode AND d.producer IN ${BOARD_KINDS_SQL})
       AND EXISTS (SELECT 1 FROM queue q WHERE q.build_id = o.identity AND q.ruler = o.ruler AND q.mode = o.mode)`).first();
  return json({ ok: true, adopted: results.length, left: left.n });
}

export async function boardTasksRoute(request, env, path) {
  if (path === "/api/board/ask") return ask(request, env);
  if (path === "/api/board/adopt") return adopt(request, env);
  return null;
}
