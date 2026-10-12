// SPDX-License-Identifier: AGPL-3.0-or-later
// THE SITE'S FACT FACTORY (worker/tasks.js): its verbs, the deadlines that ARE
// the dispatch policy, and the producers that ask. A new kind of task is one more
// producer here and in its own file; worker/tasks.js does not change.
import { factory, canon, nearAnswer } from "./tasks.js";
import { RIVEN_PRODUCERS, rivenWork } from "./appraise.js";
import { BOARD_PRODUCERS, boardWork, boardPackTask, cellsFilling } from "./board_tasks.js";
import { CLIENTS_PER_FACT, SPOT_SHARE, RECHECK_SHARE, REFUSING, servedRelease } from "./verify.js";
import { cloudTell } from "./cloud.js";
import { watchdogText } from "./watchdog_say.js";
import { iso } from "./instant.js";
import { factoryDb, pick } from "./factory_db.js";

/// HOW LONG EACH ASK MAY WAIT — THE WHOLE DISPATCH POLICY. Hand-outs go
/// earliest deadline first, and a question past its deadline is the official
/// machines' too, whatever its kind. A kind is only when it is asked and this;
/// a refresh is due when its row turns `REFRESH_WITHIN_MS` old, not a span after
/// its ask, so idle volunteers take it early and a busy fleet lets it wait.
export const DEADLINE_MS = {
  chat: 2 * 60_000,
  new_build: 6 * 3_600_000,
  survey: 86_400_000,
  refresh: 12 * 86_400_000,
  sweep: 7 * 86_400_000,
};
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
  const db = factoryDb(env);
  if (path === "/api/tasks/official/release") {
    await db.prepare("UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE leased_to = ?").bind(b.witness).run();
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
      const d = a && typeof a.ref === "string" && await db.prepare(pick(db,
        `SELECT question FROM demands WHERE ref = ? AND producer IN (SELECT value FROM json_each(?))
          ORDER BY state = 'live' DESC, asked_at DESC LIMIT 1`,
        `SELECT question FROM demands WHERE ref = ?::text AND producer IN (SELECT value FROM jsonb_array_elements_text((?::text)::jsonb))
          ORDER BY state = 'live' DESC, asked_at DESC LIMIT 1`)).bind(a.ref, producers).first();
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
///   unverified                                              → open board rows' first answers (scripts/live_orders.mjs)
///   calibration { since, offset }                           → paid questions, 2000 a page (scripts/work_calibrate.mjs)
const BOARD_KINDS = ["new_build", "refresh", "sweep"];
async function report(db, b) {
  const kinds = BOARD_KINDS.map((k) => `'${k}'`).join(", ");
  if (b.report === "claimed") {
    const rows = (Array.isArray(b.rows) ? b.rows : []).filter((r) => r && typeof r.build_id === "string").slice(0, 10_000);
    const refs = rows.map((r) => `${r.build_id}|${r.ruler}|${r.mode}`);
    const { results } = await db.prepare(pick(db,
      `SELECT d.ref, d.state, q.leased_to FROM demands d JOIN questions q ON q.id = d.question
        WHERE d.ref IN (SELECT value FROM json_each(?)) AND d.producer IN (${kinds})`,
      `SELECT d.ref, d.state, q.leased_to FROM demands d JOIN questions q ON q.id = d.question
        WHERE d.ref IN (SELECT value FROM jsonb_array_elements_text((?::text)::jsonb)) AND d.producer IN (${kinds})`))
      .bind(JSON.stringify(refs)).all();
    const asked = new Set(results.map((r) => r.ref));
    const mine = new Set(results.filter((r) => r.state === "live" && r.leased_to === b.witness).map((r) => r.ref));
    return json({ ok: true, rows: rows.filter((r, i) => !asked.has(refs[i]) || mine.has(refs[i])) });
  }
  if (b.report === "refresh_live") {
    return json({ ok: true, n: (await db.prepare("SELECT count(*) AS n FROM demands WHERE producer = 'refresh' AND state = 'live'").first()).n });
  }
  if (b.report === "unverified") {
    const { results } = await db.prepare(`SELECT d.payload, a.result, a.engine FROM demands d JOIN questions q ON q.id = d.question
        JOIN answers a ON a.id = (SELECT min(x.id) FROM answers x WHERE x.question = q.id AND x.void = 0 AND x.official = 0)
       WHERE d.state = 'live' AND d.producer IN (${kinds}) AND q.state IN ('open', 'disputed', 'nondeterministic')`).all();
    return json({ ok: true, rows: results });
  }
  if (b.report === "calibration") {
    const { results } = await db.prepare(pick(db,
      `SELECT q.id AS id, q.request AS request, max(a.work) AS work, max(a.counts) AS counts, json_group_array(a.device) AS devices
         FROM answers a JOIN questions q ON q.id = a.question
        WHERE a.credited = 1 AND a.official = 0 AND a.void = 0 AND q.verb = 'simulate' AND a.at > ?1
        GROUP BY q.id ORDER BY q.id LIMIT 2000 OFFSET ?2`,
      `SELECT q.id AS id, q.request AS request, max(a.work) AS work, max(a.counts) AS counts, '[' || string_agg(to_json(a.device)::text, ',' ORDER BY a.id) || ']' AS devices
         FROM answers a JOIN questions q ON q.id = a.question
        WHERE a.credited = 1 AND a.official = 0 AND a.void = 0 AND q.verb = 'simulate' AND a.at > ?1::text
        GROUP BY q.id ORDER BY q.id LIMIT 2000 OFFSET ?2::bigint`))
      .bind(String(b.since || ""), Number(b.offset) || 0).all();
    return json({ ok: true, rows: results });
  }
  return json({ ok: false, error: "no such report" }, 400);
}

/// THE WATCHDOG'S SCHEDULE (wrangler.jsonc `triggers`), and how long one cause
/// stays said before it is said again.
export const WATCHDOG_CRON = "*/10 * * * *";
const RETELL_MS = 6 * 3_600_000;

/// WHAT IS WRONG IS SAID TO THE OWNER (worker/tasks.js `audit`), once a cause
/// until it has stood for `RETELL_MS`: a queue that stops moving is found by
/// this, within the hour, and not by a player asking why. A SCENE STILL FILLING
/// is said on the same clock, its progress read only when it is to be said.
export async function watchdog(env, now = Date.now()) {
  if (!env.LIBRARY) return [];
  const db = factoryDb(env);
  const stale = async (kind) => {
    const told = await db.prepare("SELECT told_at FROM watchdog_told WHERE cause = ?").bind(kind).first();
    return !told || Date.parse(told.told_at) <= now - RETELL_MS;
  };
  await facts().redeliver(env, now);
  const found = await facts().audit(env, now, (await servedRelease(env)).engine);
  if (await stale("cells_filling")) {
    const cells = await cellsFilling(env, now);
    if (cells.length) found.push({ kind: "cells_filling", cells });
  }
  for (const f of found) {
    if (!(await stale(f.kind))) continue;
    if (await cloudTell(env, `WFSim compute watchdog
${watchdogText(f, now)}`)) {
      await db.prepare(`INSERT INTO watchdog_told (cause, told_at) VALUES (?, ?)
                        ON CONFLICT (cause) DO UPDATE SET told_at = excluded.told_at`).bind(f.kind, iso(now)).run();
    }
  }
  return found;
}
