// SPDX-License-Identifier: AGPL-3.0-or-later
// THE SITE'S FACT FACTORY (worker/tasks.js): its verbs, the deadlines that ARE
// the dispatch policy, and the producers that ask. A new kind of task is one more
// producer here and in its own file; worker/tasks.js does not change.
import { factory, canon } from "./tasks.js";
import { RIVEN_PRODUCERS, rivenWork } from "./appraise.js";
import { BOARD_PRODUCERS, boardWork } from "./board_tasks.js";
import { CLIENTS_PER_FACT, SPOT_SHARE } from "./verify.js";
import { cloudTell } from "./cloud.js";
import { iso } from "./instant.js";

/// HOW LONG EACH ASK MAY WAIT, which orders every hand-out: earliest deadline
/// first, so a survey shape a day overdue goes before a new build's row due in
/// six hours, and nothing waits for ever behind a kind that keeps arriving. It is
/// also when the official machines may take a question: one due sooner than the
/// volunteers can drain it hands their work to a runner.
export const DEADLINE_MS = {
  chat: 2 * 60_000,
  new_build: 6 * 3_600_000,
  survey: 86_400_000,
  sweep: 7 * 86_400_000,
};

/// HOW A COMPUTER ANSWERS. The canonical answer is all two equal answers share —
/// and so all of it must be deterministic under every split of the work
/// (`a_search_costs_the_same_work_however_the_fleet_splits_it`).
export const VERBS = {
  // A board row: its score, the metric it is in, and the fight's work. Two
  // answers that differ are a dispute; leased half an hour, past the slowest row.
  simulate: { cap: 2, lease_ms: 30 * 60_000, canon: (r, work) => canon({ score: r.score, metric: r.metric, work }) },
  // A riven gain: the quick search's best build, its score and the search's work,
  // leased a quarter of an hour and renewed while it runs. Three answers without
  // two agreeing is a dispute. No spot checks yet: no official machine runs a
  // search, and a check nobody makes is a question left waiting.
  optimize: { cap: 3, lease_ms: 15 * 60_000, spot: 0, canon: (r, work) => canon({ build: r.build, score: r.score, work }) },
};

let made = null;
/// THE FACTORY, made on first use: its producers' modules import this one.
export function facts() {
  made ||= factory({ verbs: VERBS, producers: { ...BOARD_PRODUCERS, ...RIVEN_PRODUCERS }, threshold: CLIENTS_PER_FACT, spot: SPOT_SHARE });
  return made;
}

/// A LEASED QUESTION IN THE SHAPE ITS PAGE RUNS, by the producer that asked it.
export function workOf(leased) {
  const p = leased.demands[0] && leased.demands[0].producer;
  return RIVEN_PRODUCERS[p] ? rivenWork(leased) : BOARD_PRODUCERS[p] ? boardWork(leased) : null;
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
///   POST /api/tasks/official/answer   { witness, engine, producers, answers: [{ ref, result, work }] } → { states }
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
    await db.prepare("UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE leased_to = ?").bind(b.witness).run();
    return json({ ok: true });
  }
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
      const d = a && typeof a.ref === "string" && await db.prepare(
        `SELECT question FROM demands WHERE ref = ? AND producer IN (SELECT value FROM json_each(?))
          ORDER BY state = 'live' DESC, asked_at DESC LIMIT 1`).bind(a.ref, producers).first();
      if (!d || !Number.isSafeInteger(a.work) || !a.result || typeof a.result !== "object") { states.push(null); continue; }
      states.push(await facts().answerOfficial(env, w, { question: d.question, engine: b.engine, result: a.result, work: a.work }));
    }
    return json({ ok: true, states });
  }
  return json({ ok: false, error: "not found" }, 404);
}

/// THE WATCHDOG'S SCHEDULE (wrangler.jsonc `triggers`), and how long one cause
/// stays said before it is said again.
export const WATCHDOG_CRON = "*/10 * * * *";
const RETELL_MS = 6 * 3_600_000;

/// WHAT IS WRONG IS SAID TO THE OWNER (worker/tasks.js `audit`), once a cause
/// until it has stood for `RETELL_MS`: a queue that stops moving is found by
/// this, within the hour, and not by a player asking why.
export async function watchdog(env, now = Date.now()) {
  if (!env.LIBRARY) return [];
  const found = await facts().audit(env, now);
  const db = env.LIBRARY;
  for (const f of found) {
    const told = await db.prepare("SELECT told_at FROM watchdog_told WHERE cause = ?").bind(f.kind).first();
    if (told && Date.parse(told.told_at) > now - RETELL_MS) continue;
    if (await cloudTell(env, `WFSim compute watchdog: ${f.kind} ${JSON.stringify(f)}`)) {
      await db.prepare(`INSERT INTO watchdog_told (cause, told_at) VALUES (?, ?)
                        ON CONFLICT (cause) DO UPDATE SET told_at = excluded.told_at`).bind(f.kind, iso(now)).run();
    }
  }
  return found;
}
