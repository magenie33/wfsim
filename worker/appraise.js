// SPDX-License-Identifier: AGPL-3.0-or-later
// RIVEN APPRAISAL — docs/AGENT.md §"Riven appraisal". Someone in a chat asks
// Nona about their own riven; the chat's bot opens an appraisal here, and whoever
// opens its link runs the search in their own browser and hands back the build.
// The worker keeps the appraisal and every build handed back; it computes
// nothing and trusts no number — the bot replays the build before anyone sees one.
//
// CHANNEL-BLIND: `chat` is where the answer goes, written and read only by that
// channel's bot (QQ today, Discord later); this file never looks inside it.
import { iso, msOf } from "./instant.js";
import { submitRecord } from "./index.js";
import { netOf, servedRelease } from "./verify.js";
import { canon, questionId, witnessOf } from "./tasks.js";
import { facts, DEADLINE_MS, VERBS } from "./factory.js";

const CODE_CHARS = "23456789ABCDEFGHJKMNPQRSTUVWXYZ"; // no 0/O, 1/I/L
const CODE_LEN = 5;
const KEEP_MS = 86_400_000;
/// A SURVEY: every riven shape of a weapon, opened in bulk by the owner (no chat
/// to tell), kept a week, due within a day (worker/factory.js `DEADLINE_MS`), and
/// its agreed builds sent to the board.
export const SURVEY_CHANNEL = "survey";
export const SURVEY_KEEP_MS = 7 * 86_400_000;
const keepFor = (a) => (a.channel === SURVEY_CHANNEL ? SURVEY_KEEP_MS : KEEP_MS);
/// A build handed back and not judged within this is handed to the bot again.
const RECLAIM_MS = 120_000;
/// How many appraisals one asker / one room may open in an hour.
const PER_ASKER = 5, PER_ROOM = 20, HOUR = 3_600_000;
const MAX_BUILD = 8_000, MAX_THANKS = 24;
/// A frozen search: the optimize request the page builds, which carries the fight.
const MAX_REQUEST = 65_536;
/// A RIVEN GAIN SOMEONE IS WAITING ON GOES TO THE COMPUTER THAT RUNS IT FASTEST:
/// unanswered, it needs the idle cores of the first tier its age has reached —
/// eight at once, four after one round of asks (a page asks every ten seconds),
/// any after two minutes. A search is many short steps one after another, so it
/// is cores on one computer, not computers, that make it quick
/// (private/plans/fx-volunteer-compute.md, the CTFSU timing).
export const RIVEN_LANE_TIERS = [{ lanes: 8, after_ms: 0 }, { lanes: 4, after_ms: 10_000 }, { lanes: 1, after_ms: 2 * 60_000 }];
const VERIFIER_ID = /^[a-z0-9]{16,40}$/;
const LEASE_ID = /^[a-f0-9]{32}$/;
const ENGINE_ID = /^[A-Za-z0-9._-]{1,40}$/;

const json = (body, status = 200) => new Response(JSON.stringify(body), {
  status, headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" } });

const newCode = () => {
  const b = crypto.getRandomValues(new Uint8Array(CODE_LEN));
  return [...b].map((x) => CODE_CHARS[x % CODE_CHARS.length]).join("");
};

/// A name to thank, as a person typed it: short, one line, and never a link —
/// it is posted into a chat on their behalf.
const cleanThanks = (s) => {
  const t = String(s || "").replace(/[\u0000-\u001f\u007f<>]/g, " ")
    .replace(/(https?:\/\/|www\.)\S*/gi, "").replace(/\S+\.(com|net|org|cn|app|io|gg)\b\S*/gi, "")
    .replace(/\s+/g, " ").trim().slice(0, MAX_THANKS).trim();
  // …AND NEVER A WORD A CHAT MUST NOT CARRY (worker/names.js): dropped, not
  // refused — the build still counts, it is only not thanked by that name.
  return nameBlocked(t) ? "" : t;
};

const botAuthed = (request, env) => {
  const auth = request.headers.get("authorization") || "";
  return !!env.BOT_RELAY_TOKEN && auth === `Bearer ${env.BOT_RELAY_TOKEN}`;
};

import { nameBlocked } from "./names.js";

export async function appraiseRoute(request, env, path) {
  if (!env.LIBRARY) return json({ ok: false, error: "not offered here" }, 501);
  if (path === "/api/appraise/new") return open(request, env);
  if (path === "/api/appraise/claim") return claim(request, env);
  if (path === "/api/appraise/adopt") return adopt(request, env);
  if (path === "/api/appraise/mine") return mine(request, env);
  const m = path.match(/^\/api\/appraise\/([A-Za-z0-9]{3,12})(\/result|\/request|\/renew)?$/);
  if (!m) return json({ ok: false, error: "not found" }, 404);
  const code = m[1].toUpperCase();
  if (m[2] === "/request") return freeze(request, env, code);
  if (m[2] === "/renew") return renew(request, env);
  return m[2] ? handBack(request, env, code) : read(request, env, code);
}

/// THE BOT OPENS ONE: `{ channel, chat, asker, room, weapon, ruler, riven }`.
async function open(request, env) {
  if (request.method !== "POST") return json({ ok: false, error: "POST only" }, 405);
  if (!botAuthed(request, env)) return json({ ok: false, error: "unauthorized" }, 401);
  let b;
  try { b = await request.json(); } catch (_) { return json({ ok: false, error: "not json" }, 400); }
  const need = ["channel", "asker", "weapon", "ruler"];
  if (need.some((k) => typeof b[k] !== "string" || !b[k]) || !b.riven || typeof b.riven !== "object") {
    return json({ ok: false, error: `needs ${need.join(", ")} and riven` }, 400);
  }
  const db = env.LIBRARY, now = Date.now();
  const room = typeof b.room === "string" ? b.room : "";
  await db.prepare("DELETE FROM appraisals WHERE at < ? AND channel != ?").bind(iso(now - KEEP_MS), SURVEY_CHANNEL).run();
  await db.prepare("DELETE FROM appraisals WHERE at < ? AND channel = ?").bind(iso(now - SURVEY_KEEP_MS), SURVEY_CHANNEL).run();
  await db.prepare("DELETE FROM appraisal_results WHERE at < ? AND code NOT IN (SELECT code FROM appraisals)").bind(iso(now - KEEP_MS)).run();
  const [byAsker, byRoom] = await db.batch([
    db.prepare("SELECT count(*) AS n FROM appraisals WHERE channel = ? AND asker = ? AND at > ?").bind(b.channel, b.asker, iso(now - HOUR)),
    db.prepare("SELECT count(*) AS n FROM appraisals WHERE channel = ? AND room = ? AND room != '' AND at > ?").bind(b.channel, room, iso(now - HOUR)),
  ]);
  if (byAsker.results[0].n >= PER_ASKER) return json({ ok: false, error: "asker_limit", per_hour: PER_ASKER }, 429);
  if (room && byRoom.results[0].n >= PER_ROOM) return json({ ok: false, error: "room_limit", per_hour: PER_ROOM }, 429);
  for (let i = 0; i < 5; i++) {
    const code = newCode();
    const r = await db.prepare(`INSERT OR IGNORE INTO appraisals (code, channel, chat, asker, room, weapon, ruler, riven, at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`).bind(code, b.channel, JSON.stringify(b.chat || {}), b.asker, room,
      b.weapon, b.ruler, JSON.stringify(b.riven), iso(now)).run();
    if (r.meta.changes) return json({ ok: true, code });
  }
  return json({ ok: false, error: "no code free" }, 503);
}

/// WHAT THE PAGE READS: the weapon, the ruler and the riven, whether it is done
/// and whom it thanked — never where the chat is or who asked.
async function read(request, env, code) {
  if (request.method !== "GET") return json({ ok: false, error: "GET only" }, 405);
  const a = await env.LIBRARY.prepare("SELECT * FROM appraisals WHERE code = ?").bind(code).first();
  if (!a || Date.parse(a.at) < Date.now() - KEEP_MS) return json({ ok: false, error: "no such appraisal" }, 404);
  const won = a.winner && await env.LIBRARY.prepare("SELECT thanks FROM appraisal_results WHERE id = ?").bind(a.winner).first();
  // ONE BUILD HANDED BACK, by its id — what the bot's replay reads. A build is
  // board material and public; whose browser found it is not said.
  const rid = Number(new URL(request.url).searchParams.get("result"));
  const res = Number.isInteger(rid) && rid > 0
    && await env.LIBRARY.prepare("SELECT id, build, thanks FROM appraisal_results WHERE id = ? AND code = ?").bind(rid, code).first();
  return json({ ok: true, code, weapon: a.weapon, ruler: a.ruler, riven: JSON.parse(a.riven), at: msOf(a.at),
    done: !!a.done_at, ...(won ? { thanked: won.thanks } : {}),
    ...(res ? { result: { id: res.id, build: JSON.parse(res.build), thanks: res.thanks } } : {}) });
}

/// THE BOT FREEZES THE QUESTION: `{ engine, request, context }`, the optimize
/// request its clean page built (81-appraisal.js `?freeze`) and what turns a
/// result into a build, stored once — every computer that runs this riven gain
/// runs exactly it.
async function freeze(request, env, code) {
  if (request.method !== "POST") return json({ ok: false, error: "POST only" }, 405);
  if (!botAuthed(request, env)) return json({ ok: false, error: "unauthorized" }, 401);
  const text = await request.text();
  if (text.length > MAX_REQUEST) return json({ ok: false, error: "too large" }, 413);
  let b;
  try { b = JSON.parse(text); } catch (_) { return json({ ok: false, error: "not json" }, 400); }
  if (!ENGINE_ID.test(b.engine || "") || !b.request || typeof b.request !== "object"
      || !b.context || typeof b.context !== "object") {
    return json({ ok: false, error: "needs engine, request and context" }, 400);
  }
  // THE REQUEST AND WHAT TURNS ITS RESULT INTO A BUILD (68-board-submit.js
  // `boardBuildContext`), kept together: neither means anything alone.
  const frozen = { request: b.request, context: b.context };
  const r = await env.LIBRARY.prepare("UPDATE appraisals SET request = ?, engine = ? WHERE code = ? AND request IS NULL")
    .bind(JSON.stringify(frozen), b.engine, code).run();
  if (r.meta.changes) {
    const a = await env.LIBRARY.prepare("SELECT code, channel, weapon, ruler FROM appraisals WHERE code = ?").bind(code).first();
    await askRiven(env, a, frozen, Date.now());
  }
  return json({ ok: true, stored: !!r.meta.changes });
}

// ---- Volunteer work ------------------------------------------------------------
//
// A RIVEN GAIN IS A QUESTION OF THE FACT FACTORY (worker/tasks.js): frozen, it
// is asked for — by `chat` when someone waits on it, by `survey` when the owner
// opened every shape of a weapon — and the factory hands it out, holds the
// answers to their agreement and credits the work. What is left here is what a
// riven gain is FOR: each answer kept as a build the bot may judge and replay,
// and an agreed one sent to the board's door.

/// THE ASK A FROZEN RIVEN GAIN MAKES of the factory: its deadline is the policy
/// (worker/factory.js `DEADLINE_MS`), and one someone waits on in a chat goes
/// before any other, to a computer offering idle cores by tier (`RIVEN_LANE_TIERS`).
export async function askRiven(env, a, frozen, now) {
  const chat = a.channel !== SURVEY_CHANNEL;
  const { engine } = await servedRelease(env);
  await facts().demand(env, { producer: chat ? "chat" : "survey", ref: a.code, verb: "optimize", request: frozen,
    due_in_ms: DEADLINE_MS[chat ? "chat" : "survey"], payload: { weapon: a.weapon, ruler: a.ruler },
    lanes: chat ? RIVEN_LANE_TIERS : null, waits: chat }, engine, now);
}

/// A RIVEN GAIN'S PRODUCERS (worker/factory.js): one still kept is still wanted;
/// a computer taking it is said to the chat once; every answer is kept as a build
/// the bot may judge; the agreed one is the appraisal's, and goes to the board.
const rivenProducer = {
  still_wanted: async (env, d) => {
    const a = await env.LIBRARY.prepare("SELECT channel, at FROM appraisals WHERE code = ?").bind(d.ref).first();
    return !!a && Date.parse(a.at) >= Date.now() - keepFor(a);
  },
  on_lease: async (env, d, now) => {
    await env.LIBRARY.prepare("UPDATE appraisals SET started_at = COALESCE(started_at, ?) WHERE code = ?").bind(iso(now), d.ref).run();
  },
  on_answer: async (env, d, x) => {
    if (x.official) return;
    await env.LIBRARY.prepare(`INSERT INTO appraisal_results (code, build, thanks, at, verifier, score, work, key, engine, net)
        VALUES (?, ?, '', ?, ?, ?, ?, ?, ?, ?)`)
      .bind(d.ref, JSON.stringify(x.result.build), x.at, x.device, x.result.score, x.work, canon(x.result.build), x.engine, x.net).run();
  },
  on_fact: async (env, d, fact) => {
    const at = iso(Date.now());
    const won = await env.LIBRARY.prepare("UPDATE appraisals SET agreed_at = ? WHERE code = ? AND agreed_at IS NULL")
      .bind(at, d.ref).run();
    // AN ASK ANSWERED BY A FACT IT DID NOT WAIT FOR — the same question, agreed
    // for another — still hands the bot a build to judge and tell.
    if (won.meta.changes && fact.result) {
      await env.LIBRARY.prepare(`INSERT INTO appraisal_results (code, build, thanks, at, score, work, key)
          SELECT ?, ?, '', ?, ?, ?, ? WHERE NOT EXISTS (SELECT 1 FROM appraisal_results WHERE code = ? AND key = ?)`)
        .bind(d.ref, JSON.stringify(fact.result.build), at, fact.result.score, fact.work, canon(fact.result.build),
          d.ref, canon(fact.result.build)).run();
    }
    // …AND THE AGREED BUILD GOES TO THE BOARD, through the reader's own door.
    if (won.meta.changes && fact.result) {
      try { await submitRecord(env, fact.result.build); } catch (_) { /* the answer stands without it */ }
    }
  },
};
export const RIVEN_PRODUCERS = { chat: rivenProducer, survey: rivenProducer };

/// THE TASK THE FACTORY LEASED, in the shape the page runs a riven gain in.
export function rivenWork(leased) {
  const d = leased.demands[0];
  const { request, context } = leased.request;
  return { kind: "riven_gain", lease: leased.lease, code: d.ref, weapon: d.payload.weapon, ruler: d.payload.ruler, request, context };
}

/// STILL AT IT: the computer holding a riven gain says so while its search runs,
/// and its lease runs on from now — a slow computer is never overtaken by its own
/// lease. `held: false` tells it the task went elsewhere, so it stops; `release`
/// gives it back, so the person waiting is not left behind a lease nobody works.
async function renew(request, env) {
  if (request.method !== "POST") return json({ ok: false, error: "POST only" }, 405);
  let b = {};
  try { b = await request.json(); } catch (_) { return json({ ok: false, error: "not json" }, 400); }
  if (!VERIFIER_ID.test(b.verifier || "") || !LEASE_ID.test(b.lease || "")) return json({ ok: false, error: "bad renew" }, 400);
  if (b.release === true) {
    await facts().release(env, b.verifier, b.lease);
    return json({ ok: true, held: false });
  }
  return json({ ok: true, held: await facts().renew(env, b.verifier, b.lease) });
}

/// A VOLUNTEER'S ANSWER, under its lease: the factory keeps it, and credits the
/// work once two owners' answers agree.
async function volunteerAnswer(request, env, b) {
  if (!VERIFIER_ID.test(b.verifier || "") || !LEASE_ID.test(b.lease || "")
      || !Number.isSafeInteger(b.work) || b.work < 0 || typeof b.score !== "number" || !Number.isFinite(b.score)) {
    return json({ ok: false, error: "bad answer" }, 400);
  }
  const who = await witnessOf(env, b.verifier);
  const net = await netOf(request, env);
  const engine = typeof b.engine === "string" && ENGINE_ID.test(b.engine) ? b.engine : "";
  const { taken } = await facts().answer(env, { device: b.verifier, owner: who.owner, net },
    { lease: b.lease, engine, result: { build: b.build, score: b.score }, work: b.work });
  return json({ ok: true, first: taken });
}

/// WHERE EACH OF A BROWSER'S RIVEN GAINS STANDS, asked by its own secret id —
/// the riven-gain twin of worker/verify.js `mine`: `confirmed` (with when) once
/// its answer was credited in the fact, `gone` when the question was agreed
/// without it or is no longer kept, `waiting` otherwise. A task names its `code`;
/// one stored before tasks carried it names its weapon and when it ended, and is
/// matched to this browser's nearest answer on that weapon.
const MINE_MAX = 60, MINE_NEAR_MS = 10 * 60_000;
async function mine(request, env) {
  if (request.method !== "POST") return json({ ok: false, error: "POST only" }, 405);
  let b;
  try { b = await request.json(); } catch (_) { return json({ ok: false, error: "not json" }, 400); }
  if (!VERIFIER_ID.test(b.verifier || "") || !Array.isArray(b.tasks)) return json({ ok: false, error: "bad request" }, 400);
  const tasks = b.tasks.slice(0, MINE_MAX).filter((t) => t && Number.isFinite(t.at) && (typeof t.code === "string" || typeof t.weapon === "string"));
  if (!tasks.length) return json({ ok: true, tasks: [] });
  const db = env.LIBRARY;
  const since = iso(Math.min(...tasks.map((t) => t.at)) - MINE_NEAR_MS);
  const { results: answers } = await db.prepare(
    `SELECT r.code, r.at, a.weapon FROM appraisal_results r LEFT JOIN appraisals a ON a.code = r.code
      WHERE r.verifier = ? AND r.at >= ? ORDER BY r.at LIMIT 400`).bind(b.verifier, since).all();
  const used = new Set(), out = [];
  for (const t of tasks) {
    let r = typeof t.code === "string" ? answers.find((x) => x.code === t.code) : null;
    if (!r && typeof t.code !== "string") {
      let best = MINE_NEAR_MS;
      for (const x of answers) {
        const d = Math.abs(Date.parse(x.at) - t.at);
        if (x.weapon === t.weapon && !used.has(x.code) && d <= best) { best = d; r = x; }
      }
    }
    if (!r) continue;
    used.add(r.code);
    // WHAT THE FACTORY SAYS of this browser's answer to that question.
    const q = r.weapon == null ? null : await db.prepare(
      `SELECT q.state, q.fact_at, (SELECT max(credited) FROM answers x WHERE x.question = q.id AND x.device = ?) AS credited
         FROM demands d JOIN questions q ON q.id = d.question WHERE d.ref = ? AND d.producer IN ('chat', 'survey')`)
      .bind(b.verifier, r.code).first();
    const state = !q ? "gone" : q.credited ? "confirmed" : q.state === "fact" || q.state === "spot" ? "gone" : "waiting";
    out.push({ at: t.at, code: r.code, state, ...(state === "confirmed" ? { agreed_at: q.fact_at } : {}) });
  }
  return json({ ok: true, tasks: out });
}

/// A BUILD HANDED BACK: `{ build, thanks }`, a board record and an optional name.
/// Every one is kept until the bot judges it; the first it accepts wins. One sent
/// under a volunteer lease carries `{ lease, verifier, score, work }` instead.
async function handBack(request, env, code) {
  if (request.method !== "POST") return json({ ok: false, error: "POST only" }, 405);
  if (env.OCR_LIMIT) {
    const { success } = await env.OCR_LIMIT.limit({ key: "appraise" + (request.headers.get("cf-connecting-ip") || "unknown") });
    if (!success) return json({ ok: false, error: "slow down" }, 429);
  }
  const text = await request.text();
  if (text.length > MAX_BUILD) return json({ ok: false, error: "too large" }, 413);
  let b;
  try { b = JSON.parse(text); } catch (_) { return json({ ok: false, error: "not json" }, 400); }
  if (!b.build || typeof b.build !== "object" || !Array.isArray(b.build.mods)) return json({ ok: false, error: "needs build" }, 400);
  const db = env.LIBRARY, now = Date.now();
  const a = await db.prepare("SELECT code, channel, done_at, at FROM appraisals WHERE code = ?").bind(code).first();
  if (!a || Date.parse(a.at) < now - keepFor(a)) return json({ ok: false, error: "no such appraisal" }, 404);
  if (b.lease !== undefined) return volunteerAnswer(request, env, b);
  await db.prepare("INSERT INTO appraisal_results (code, build, thanks, at) VALUES (?, ?, ?, ?)")
    .bind(code, JSON.stringify(b.build), cleanThanks(b.thanks), iso(now)).run();
  return json({ ok: true, first: !a.done_at });
}

/// THE RIVEN GAINS FROZEN BEFORE THE FACTORY, asked of it once each, with the
/// answers they already had: the bot's relay calls this until `left` is 0. Each
/// is asked as of when it was opened, so a survey shape opened two days ago is
/// as overdue as it is; an older engine's answers are carried, for the served
/// one to reproduce or replace (worker/tasks.js), and a search running now keeps
/// its lease.
const ADOPT_BATCH = 40;
const UNADOPTED = `request IS NOT NULL AND agreed_at IS NULL AND done_at IS NULL
  AND at > CASE WHEN channel = ? THEN ? ELSE ? END
  AND NOT EXISTS (SELECT 1 FROM demands d WHERE d.ref = a.code AND d.producer IN ('chat', 'survey'))`;
const kept = (now) => [SURVEY_CHANNEL, iso(now - SURVEY_KEEP_MS), iso(now - KEEP_MS)];
async function adopt(request, env) {
  if (request.method !== "POST") return json({ ok: false, error: "POST only" }, 405);
  if (!botAuthed(request, env)) return json({ ok: false, error: "unauthorized" }, 401);
  const db = env.LIBRARY, now = Date.now(), since = kept(now);
  const { results } = await db.prepare(
    `SELECT code, channel, weapon, ruler, at, request, lease, lease_until, leased_to FROM appraisals a
      WHERE ${UNADOPTED} ORDER BY at LIMIT ?`).bind(...since, ADOPT_BATCH).all();
  const { engine } = await servedRelease(env);
  for (const a of results) {
    await askRiven(env, a, JSON.parse(a.request), Date.parse(a.at));
    const id = await questionId("optimize", JSON.parse(a.request));
    // A SEARCH RUNNING NOW keeps its lease, so its answer lands and is credited.
    if (a.lease && Date.parse(a.lease_until) > now) {
      await db.prepare("UPDATE questions SET lease = ?, lease_until = ?, leased_to = ? WHERE id = ? AND lease IS NULL")
        .bind(a.lease, a.lease_until, a.leased_to, id).run();
    }
    const { results: old } = await db.prepare(
      "SELECT verifier, build, score, work, engine, net, at FROM appraisal_results WHERE code = ? AND verifier IS NOT NULL ORDER BY id").bind(a.code).all();
    for (const r of old) {
      const who = await witnessOf(env, r.verifier);
      const build = JSON.parse(r.build);
      await db.prepare(`INSERT INTO answers (question, engine, device, owner, net, canon, result, work, at)
          VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`)
        .bind(id, r.engine || "", r.verifier, who.owner, r.net || "", VERBS.optimize.canon({ build, score: r.score }, r.work),
          JSON.stringify({ build, score: r.score }), r.work, r.at).run();
    }
    if (old.length) await facts().settle(env, id, engine, now);
  }
  const left = await db.prepare(`SELECT count(*) AS n FROM appraisals a WHERE ${UNADOPTED}`).bind(...since).first();
  return json({ ok: true, adopted: results.length, left: left.n });
}

/// THE BOT'S PULL, per channel. Its body reports what it judged and what it told:
/// `{ channel, judged: [{ id, ok, verdict }], told: [code] }`. It gets back the
/// builds still to judge and the finished appraisals still to tell.
async function claim(request, env) {
  if (request.method !== "POST") return json({ ok: false, error: "POST only" }, 405);
  if (!botAuthed(request, env)) return json({ ok: false, error: "unauthorized" }, 401);
  let b = {};
  try { b = await request.json(); } catch (_) {}
  const channel = String(b.channel || "");
  if (!channel) return json({ ok: false, error: "needs channel" }, 400);
  const db = env.LIBRARY, now = Date.now();
  for (const j of (Array.isArray(b.judged) ? b.judged : []).slice(0, 50)) {
    const id = Number(j.id);
    if (!Number.isInteger(id)) continue;
    await db.prepare("UPDATE appraisal_results SET verdict = ?, checked_at = ? WHERE id = ?")
      .bind(JSON.stringify({ ok: !!j.ok, ...(j.verdict || {}) }), iso(now), id).run();
    // THE FIRST ACCEPTED BUILD WINS, once: a later one changes nothing.
    if (j.ok) {
      await db.prepare(`UPDATE appraisals SET winner = ?, done_at = ?
        WHERE done_at IS NULL AND code = (SELECT code FROM appraisal_results WHERE id = ?)`).bind(id, iso(now), id).run();
    }
  }
  for (const code of (Array.isArray(b.told) ? b.told : []).slice(0, 50)) {
    await db.prepare("UPDATE appraisals SET told_at = ? WHERE code = ?").bind(iso(now), String(code)).run();
  }
  for (const code of (Array.isArray(b.started_told) ? b.started_told : []).slice(0, 50)) {
    await db.prepare("UPDATE appraisals SET started_told = ? WHERE code = ?").bind(iso(now), String(code)).run();
  }
  const { results: toJudge } = await db.prepare(`SELECT r.id, r.code, r.build, r.thanks, r.at,
      a.weapon, a.ruler, a.riven, a.chat, a.asker, a.at AS asked_at
    FROM appraisal_results r JOIN appraisals a ON a.code = r.code
    WHERE a.channel = ? AND a.done_at IS NULL AND r.checked_at IS NULL AND (r.claimed_at IS NULL OR r.claimed_at < ?)
    ORDER BY r.at LIMIT 5`).bind(channel, iso(now - RECLAIM_MS)).all();
  if (toJudge.length) {
    await db.batch(toJudge.map((r) => db.prepare("UPDATE appraisal_results SET claimed_at = ? WHERE id = ?").bind(iso(now), r.id)));
  }
  const { results: toTell } = await db.prepare(`SELECT a.code, a.weapon, a.ruler, a.riven, a.chat, a.asker, a.at AS asked_at,
      a.done_at, r.id AS result_id, r.thanks, r.verdict, r.verifier IS NOT NULL AS volunteer
    FROM appraisals a JOIN appraisal_results r ON r.id = a.winner
    WHERE a.channel = ? AND a.done_at IS NOT NULL AND a.told_at IS NULL ORDER BY a.done_at LIMIT 20`).bind(channel).all();
  // A COMPUTER TOOK IT: the chat may be told so, once, while it waits.
  const { results: toStart } = await db.prepare(`SELECT code, weapon, ruler, riven, chat, asker, at AS asked_at, started_at
    FROM appraisals WHERE channel = ? AND started_at IS NOT NULL AND started_told IS NULL AND told_at IS NULL AND done_at IS NULL
    ORDER BY started_at LIMIT 20`).bind(channel).all();
  const parse = (r) => ({ ...r, at: msOf(r.at), asked_at: msOf(r.asked_at), done_at: msOf(r.done_at), started_at: msOf(r.started_at),
    riven: JSON.parse(r.riven), chat: JSON.parse(r.chat),
    ...(r.build ? { build: JSON.parse(r.build) } : {}), ...(r.verdict ? { verdict: JSON.parse(r.verdict) } : {}) });
  return json({ ok: true, judge: toJudge.map(parse), tell: toTell.map(parse), started: toStart.map(parse) });
}
