// SPDX-License-Identifier: AGPL-3.0-or-later
// THE SITE'S FACT FACTORY (worker/tasks.js): its verbs, the deadlines that ARE
// the dispatch policy, and the producers that ask. A new kind of task is one more
// producer here and in its own file; worker/tasks.js does not change.
import { factory, canon, nearAnswer, inIdOrder } from "./tasks.js";
import { RIVEN_PRODUCERS, rivenWork } from "./appraise.js";
import { BOARD_PRODUCERS, boardWork, boardPackTask, cellsFilling } from "./board_tasks.js";
import { CLIENTS_PER_FACT, SPOT_SHARE, RECHECK_SHARE, REFUSING, servedRelease } from "./verify.js";
import { watchdogText } from "./watchdog_say.js";
import { idleFindings } from "./idle_devices.js";
import { iso } from "./instant.js";
import { fairShadow, settleBatch } from "./settler.js";
import { DEMAND_KEY } from "./contribution.js";

/// HOW LONG EACH ASK MAY WAIT — THE WHOLE DISPATCH POLICY, stated in
/// worker/deadlines.js.
export { DEADLINE_MS } from "./deadlines.js";
/// THE OLDEST A BOARD ROW MAY GROW: its refresh is due when its fact is this old
/// (`.github/workflows/queue.yml` asks from three days on). `DEADLINE_MS.refresh`
/// is only for a row with no fact time.
export const REFRESH_WITHIN_MS = 15 * 86_400_000;
/// A SCENE'S FIRST PASS: a cell row with no fact yet, whichever batch asked it,
/// is due this long after its ask — ahead of every refresh still days from its
/// own bound, so a new scene fills before the base is measured again.
export const SCENE_FIRST_MS = 3 * 86_400_000;

/// HOW A COMPUTER ANSWERS. The canonical answer is all two equal answers share —
/// and so all of it must be deterministic under every split of the work
/// (`a_search_costs_the_same_work_however_the_fleet_splits_it`).
export const VERBS = {
  // A board row: its score, the metric it is in, and the fight's work. Two
  // answers that differ are a dispute; leased half an hour, past the slowest row.
  // Seconds of work each, so handed out in packs (worker/verify.js `PACK_MS`).
  simulate: { cap: 2, lease_ms: 30 * 60_000, pack: true, canon: (r, work) => canon({ score: r.score, metric: r.metric, work }),
    near: nearAnswer(["metric", "work"]) },
  // A riven gain: the quick search's best build, its score and the search's work,
  // leased a quarter of an hour and renewed while it runs. Three answers without
  // two agreeing is a dispute. No spot checks yet: no official machine runs a
  // search, and a check nobody makes is a question left waiting.
  optimize: { cap: 3, lease_ms: 15 * 60_000, spot: 0, recheck: 0, canon: (r, work) => canon({ build: r.build, score: r.score, work }),
    near: nearAnswer(["build", "work"]) },
};

let made = null, refusing = null;
/// THE FACTORY, made on first use: its producers' modules import this one.
export function facts() {
  made ||= factory({ verbs: VERBS, producers: { ...BOARD_PRODUCERS, ...RIVEN_PRODUCERS }, threshold: CLIENTS_PER_FACT, spot: SPOT_SHARE,
    recheck: RECHECK_SHARE, refusing: refusing ?? REFUSING });
  return made;
}
/// A check of the refusal path turns it on whatever `REFUSING` says.
export function refusalsFor(on) { refusing = on; made = null; }

/// A LEASED QUESTION IN THE SHAPE ITS PAGE RUNS, by the producer that asked it.
export function workOf(leased) {
  const p = leased.demands[0] && leased.demands[0].producer;
  return RIVEN_PRODUCERS[p] ? rivenWork(leased) : BOARD_PRODUCERS[p] ? boardWork(leased) : null;
}
/// …AND ONE QUESTION OF A PACK (`nextPack`); only a board row is packed today.
export function packTaskOf(q) {
  const p = q.demands[0] && q.demands[0].producer;
  return BOARD_PRODUCERS[p] ? boardPackTask(q) : null;
}

/// WHAT A LEASED QUESTION IS, for a device list: its kind and its public facts.
export function kindOf(producer, request, payload) {
  if (RIVEN_PRODUCERS[producer]) return { kind: "riven_gain", weapon: payload.weapon || null, ruler: payload.ruler || null };
  if (BOARD_PRODUCERS[producer]) return { kind: "board", weapon: (request.record || {}).weapon || null, ruler: request.ruler, mode: request.mode };
  return null;
}

/// THE OFFICIAL MACHINES AND THE PRODUCERS THAT RUN OFF THE SITE (the scorer,
/// the live loop, `ship_queue.sh`) carry the factory's own secret.
export const factoryAuthed = (request, env) => {
  const auth = request.headers.get("authorization") || "";
  return !!env.FACTORY_TOKEN && auth === `Bearer ${env.FACTORY_TOKEN}`;
};

const json = (body, status = 200) => new Response(JSON.stringify(body), {
  status, headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" } });
const OFFICIAL_ID = /^official:[a-z0-9._-]{1,60}$/;
const ENGINE_ID = /^[A-Za-z0-9._-]{1,40}$/;

/// THE OFFICIAL WITNESS'S DOOR — worker/tasks.js `claim`, `answerOfficial`:
///   POST /api/tasks/official/claim    { witness, engine, states?, producers?, max, lease_ms? } → { tasks }
///   POST /api/tasks/official/answer   { witness, engine, producers, answers: [{ ref, result, work | null }] } → { states }
///   POST /api/tasks/official/release  { witness } → {}
/// A witness is `official:<name>`, one per run; its answers are facts alone.
export async function officialRoute(request, env, path) {
  if (!path.startsWith("/api/tasks/official/")) return null;
  if (request.method !== "POST") return json({ ok: false, error: "POST only" }, 405);
  if (!factoryAuthed(request, env)) return json({ ok: false, error: "unauthorized" }, 401);
  let b;
  try { b = await request.json(); } catch (_) { return json({ ok: false, error: "not json" }, 400); }
  if (!OFFICIAL_ID.test(b.witness || "")) return json({ ok: false, error: "needs witness official:<name>" }, 400);
  const w = { device: b.witness, owner: b.witness, official: true };
  const db = env.LIBRARY;
  if (path === "/api/tasks/official/release") {
    await db.prepare(`UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE ${inIdOrder("leased_to = ?")}`).bind(b.witness).run();
    return json({ ok: true });
  }
  if (path === "/api/tasks/official/report") return report(db, b);
  if (!ENGINE_ID.test(b.engine || "")) return json({ ok: false, error: "needs engine" }, 400);
  if (path === "/api/tasks/official/claim") {
    const max = Math.min(5000, Math.max(0, Number(b.max) || 0));
    const tasks = await facts().claim(env, w, { states: Array.isArray(b.states) ? b.states : undefined,
      producers: Array.isArray(b.producers) ? b.producers : null, max,
      lease_ms: Number.isFinite(b.lease_ms) ? Math.min(b.lease_ms, 12 * 3_600_000) : undefined }, b.engine);
    return json({ ok: true, tasks });
  }
  if (path === "/api/tasks/official/answer") {
    const answers = (Array.isArray(b.answers) ? b.answers : []).slice(0, 200);
    const producers = JSON.stringify(Array.isArray(b.producers) ? b.producers : []);
    const states = [];
    for (const a of answers) {
      // THE DEMAND IT ANSWERS, by its ref among the named producers — a live one first.
      const d = a && typeof a.ref === "string" && await db.prepare(`SELECT question FROM demands WHERE ref = ?::text AND producer IN (SELECT value FROM jsonb_array_elements_text((?::text)::jsonb))
          ORDER BY state = 'live' DESC, asked_at DESC LIMIT 1`).bind(a.ref, producers).first();
      // `work: null` is a witness of the score alone (worker/tasks.js `settleOfficial`).
      if (!d || !(a.work === null || Number.isSafeInteger(a.work)) || !a.result || typeof a.result !== "object") { states.push(null); continue; }
      states.push(await facts().answerOfficial(env, w, { question: d.question, engine: b.engine, result: a.result, work: a.work }));
    }
    return json({ ok: true, states });
  }
  return json({ ok: false, error: "not found" }, 404);
}

/// WHAT THE SCRIPTS READ OF THE FACTORY, named, from whichever database holds
/// it — never a query of their own on its tables:
///   claimed     { rows: [{ batch, build_id, ruler, mode }] } → the rows no board
///               demand asks for, or one live and leased to `witness` (scripts/fetch_queue.sh)
///   refresh_live                                            → { n } (queue.yml)
///   calibration { since, offset }                           → paid questions, 2000 a page (scripts/work_calibrate.mjs)

async function report(db, b) {
  const kinds = Object.keys(BOARD_PRODUCERS).map((k) => `'${k}'`).join(", ");
  if (b.report === "claimed") {
    const rows = (Array.isArray(b.rows) ? b.rows : []).filter((r) => r && typeof r.build_id === "string").slice(0, 10_000);
    const refs = rows.map((r) => `${r.build_id}|${r.ruler}|${r.mode}`);
    const { results } = await db.prepare(`SELECT d.ref, d.state, q.leased_to FROM demands d JOIN questions q ON q.id = d.question
        WHERE d.ref IN (SELECT value FROM jsonb_array_elements_text((?::text)::jsonb)) AND d.producer IN (${kinds})`)
      .bind(JSON.stringify(refs)).all();
    const asked = new Set(results.map((r) => r.ref));
    const mine = new Set(results.filter((r) => r.state === "live" && r.leased_to === b.witness).map((r) => r.ref));
    return json({ ok: true, rows: rows.filter((r, i) => !asked.has(refs[i]) || mine.has(refs[i])) });
  }
  if (b.report === "refresh_live") {
    return json({ ok: true, n: (await db.prepare("SELECT count(*) AS n FROM demands WHERE producer = 'refresh' AND state = 'live'").first()).n });
  }
  if (b.report === "calibration") {
    const { results } = await db.prepare(`SELECT q.id AS id, q.request AS request, max(a.work) AS work, max(a.counts) AS counts, '[' || string_agg(to_json(a.device)::text, ',' ORDER BY a.id) || ']' AS devices
         FROM answers a JOIN questions q ON q.id = a.question
        WHERE a.credited = 1 AND a.official = 0 AND a.void = 0 AND q.verb = 'simulate' AND a.at > ?1::text
        GROUP BY q.id ORDER BY q.id LIMIT 2000 OFFSET ?2::bigint`)
      .bind(String(b.since || ""), Number(b.offset) || 0).all();
    return json({ ok: true, rows: results });
  }
  return json({ ok: false, error: "no such report" }, 400);
}

/// AN EVENT, from its first sight to its last (`factory_events`, docs/BOARD.md
/// §"Outages"): seen again within `gap_ms` it is the same one, lasting longer.
/// Never throws — what it records must not be what breaks.
export async function noteEvent(db, kind, now, gap_ms) {
  try {
    const r = await db.prepare("UPDATE factory_events SET last_at = ? WHERE kind = ? AND last_at >= ?")
      .bind(iso(now), kind, iso(now - gap_ms)).run();
    if (!r.meta.changes) {
      await db.prepare("INSERT INTO factory_events (kind, started_at, last_at) VALUES (?, ?, ?) ON CONFLICT DO NOTHING")
        .bind(kind, iso(now), iso(now)).run();
    }
  } catch (_) { /* the outage it records may be the database's own */ }
}
/// A PAUSE IS NOTED by the asks it turns away, once a minute an isolate.
const PAUSE_NOTE_MS = 60_000, PAUSE_GAP_MS = 10 * 60_000;
let pauseNotedAt = 0;
export async function notePause(env, now) {
  if (now - pauseNotedAt < PAUSE_NOTE_MS) return;
  pauseNotedAt = now;
  await noteEvent(env.LIBRARY, "pause", now, PAUSE_GAP_MS);
}

/// A FLEET ASKING AND HANDED NOTHING, read off the daemon's own `demand.json`
/// (`now`, worker/contribution.js `starvedOf`): the one reading the page and
/// the scheduled check (scripts/check_dispatch.py) are given too.
async function starvedFinding(env) {
  const obj = env.PUBLIC && await env.PUBLIC.get(DEMAND_KEY).catch(() => null);
  const d = obj && await new Response(obj.body).json().catch(() => null);
  if (!d?.now?.starved) return null;
  const waiting = d.orders.filter((o) => o.kind !== "riven_gain").reduce((n, o) => n + o.total - o.dispatched, 0);
  return { kind: "starved", ...d.now, waiting, at: d.at };
}

/// THE WATCHDOG'S SCHEDULE (wrangler.jsonc `triggers`), how often what is wrong
/// is written down, and how long a round of it is kept.
export const WATCHDOG_CRON = "*/10 * * * *";
const SAY_EVERY_MS = 3_600_000, SAID_KEPT_MS = 30 * 24 * 3_600_000;
/// THE SETTLER IS GONE once it has not beaten this long, and the cron settles its queue.
export const SETTLER_SILENT_MS = 5 * 60_000;

/// WHAT IS WRONG IS WRITTEN DOWN in `watchdog_said` as a ROUND: every finding at
/// once, one row each under one `at`, at most every `SAY_EVERY_MS` — so the
/// latest round is the whole picture and the rounds before it are its history.
/// A trouble after a quiet spell is written within ten minutes. Nothing is sent
/// to the owner — the bot's messages are his clan's. A scene still filling and
/// idle computers are read only for a round that is written.
export async function watchdog(env, now = Date.now()) {
  if (!env.LIBRARY) return [];
  const db = env.LIBRARY;
  // THE SETTLER'S QUEUE, emptied here when the daemon beside the database has
  // not beaten in `SETTLER_SILENT_MS` (dial `settle_async`): its rows are handed
  // to anyone again after the grace, but nobody else settles them.
  const beat = await db.prepare("SELECT daemon_at FROM factory_snapshot WHERE id = 1").first().catch(() => null);
  if (!beat || Date.parse(beat.daemon_at) <= now - SETTLER_SILENT_MS) await settleBatch(env, now, Date.now() + 20_000).catch(() => 0);
  await facts().redeliver(env, now);
  await facts().repay(env, now);
  await facts().liftIdle(env);
  const { engine } = await servedRelease(env);
  const found = await facts().audit(env, now, engine);
  const starved = await starvedFinding(env);
  if (starved) found.push(starved);
  // THE SETTLER'S FAIR TABLE AGAINST THIS WORKER'S, while it runs in shadow
  // (docs/BOARD.md §"The settler"): a comparison that fails changes nothing.
  if (engine) await fairShadow(env, engine, now).catch(() => null);
  // EVERY ROUND IT STANDS, not only when it is said: the event's span is the outage's.
  for (const f of found) await noteEvent(db, `alert:${f.kind}`, now, 3 * 10 * 60_000);
  const last = await db.prepare("SELECT max(at) AS at FROM watchdog_said").first();
  if (last?.at && Date.parse(last.at) > now - SAY_EVERY_MS) return found;
  const cells = await cellsFilling(env, now);
  if (cells.length) found.push({ kind: "cells_filling", cells });
  if (engine) found.push(...await idleFindings(env, engine, now).catch(() => []));
  for (const f of found) {
    const said = watchdogText(f, now);
    await db.prepare("INSERT INTO watchdog_said (at, cause, said) VALUES (?, ?, ?) ON CONFLICT DO NOTHING")
      .bind(iso(now), f.kind, said).run();
  }
  await db.prepare("DELETE FROM watchdog_said WHERE at < ?").bind(iso(now - SAID_KEPT_MS)).run();
  return found;
}
