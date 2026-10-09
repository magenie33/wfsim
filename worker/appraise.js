// SPDX-License-Identifier: AGPL-3.0-or-later
// RIVEN APPRAISAL — docs/AGENT.md §"Riven appraisal". Someone in a chat asks
// Nona about their own riven; the chat's bot opens an appraisal here, and whoever
// opens its link runs the search in their own browser and hands back the build.
// The worker keeps the appraisal and every build handed back; it computes
// nothing and trusts no number — the bot replays the build before anyone sees one.
//
// CHANNEL-BLIND: `chat` is where the answer goes, written and read only by that
// channel's bot (QQ today, Discord later); this file never looks inside it.

const CODE_CHARS = "23456789ABCDEFGHJKMNPQRSTUVWXYZ"; // no 0/O, 1/I/L
const CODE_LEN = 5;
const KEEP_MS = 86_400_000;
/// A build handed back and not judged within this is handed to the bot again.
const RECLAIM_MS = 120_000;
/// How many appraisals one asker / one room may open in an hour.
const PER_ASKER = 5, PER_ROOM = 20, HOUR = 3_600_000;
const MAX_BUILD = 8_000, MAX_THANKS = 24;
/// A frozen search: the optimize request the page builds, which carries the fight.
const MAX_REQUEST = 65_536;
/// How long a volunteer computer holds a riven gain before another may take it.
export const RIVEN_LEASE_MS = 15 * 60_000;
/// A RIVEN GAIN SOMEONE IS WAITING ON GOES TO THE COMPUTER THAT RUNS IT FASTEST:
/// unanswered, it needs the idle cores of the first tier its age has reached —
/// eight at once, four after one round of asks (a page asks every ten seconds),
/// any after two minutes. A search is many short steps one after another, so it
/// is cores on one computer, not computers, that make it quick
/// (private/plans/fx-volunteer-compute.md, the CTFSU timing).
export const RIVEN_LANE_TIERS = [{ lanes: 8, after_ms: 0 }, { lanes: 4, after_ms: 10_000 }, { lanes: 1, after_ms: 2 * 60_000 }];
const rivenLanesNeeded = (waited_ms) => Math.min(...RIVEN_LANE_TIERS.filter((t) => waited_ms >= t.after_ms).map((t) => t.lanes));
/// Answers after which a riven gain stops being handed out to agree on.
const RIVEN_ANSWERS = 3;
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

import { ownersOf } from "./contribution.js";
import { nameBlocked } from "./names.js";

export async function appraiseRoute(request, env, path) {
  if (!env.LIBRARY) return json({ ok: false, error: "not offered here" }, 501);
  if (path === "/api/appraise/new") return open(request, env);
  if (path === "/api/appraise/claim") return claim(request, env);
  const m = path.match(/^\/api\/appraise\/([A-Za-z0-9]{3,12})(\/result|\/request|\/renew)?$/);
  if (!m) return json({ ok: false, error: "not found" }, 404);
  const code = m[1].toUpperCase();
  if (m[2] === "/request") return freeze(request, env, code);
  if (m[2] === "/renew") return renew(request, env, code);
  return m[2] ? handBack(request, env, code, (ids) => ownersOf(env, ids)) : read(request, env, code);
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
  await db.prepare("DELETE FROM appraisals WHERE at < ?").bind(now - KEEP_MS).run();
  await db.prepare("DELETE FROM appraisal_results WHERE at < ?").bind(now - KEEP_MS).run();
  const [byAsker, byRoom] = await db.batch([
    db.prepare("SELECT count(*) AS n FROM appraisals WHERE channel = ? AND asker = ? AND at > ?").bind(b.channel, b.asker, now - HOUR),
    db.prepare("SELECT count(*) AS n FROM appraisals WHERE channel = ? AND room = ? AND room != '' AND at > ?").bind(b.channel, room, now - HOUR),
  ]);
  if (byAsker.results[0].n >= PER_ASKER) return json({ ok: false, error: "asker_limit", per_hour: PER_ASKER }, 429);
  if (room && byRoom.results[0].n >= PER_ROOM) return json({ ok: false, error: "room_limit", per_hour: PER_ROOM }, 429);
  for (let i = 0; i < 5; i++) {
    const code = newCode();
    const r = await db.prepare(`INSERT OR IGNORE INTO appraisals (code, channel, chat, asker, room, weapon, ruler, riven, at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`).bind(code, b.channel, JSON.stringify(b.chat || {}), b.asker, room,
      b.weapon, b.ruler, JSON.stringify(b.riven), now).run();
    if (r.meta.changes) return json({ ok: true, code });
  }
  return json({ ok: false, error: "no code free" }, 503);
}

/// WHAT THE PAGE READS: the weapon, the ruler and the riven, whether it is done
/// and whom it thanked — never where the chat is or who asked.
async function read(request, env, code) {
  if (request.method !== "GET") return json({ ok: false, error: "GET only" }, 405);
  const a = await env.LIBRARY.prepare("SELECT * FROM appraisals WHERE code = ?").bind(code).first();
  if (!a || a.at < Date.now() - KEEP_MS) return json({ ok: false, error: "no such appraisal" }, 404);
  const won = a.winner && await env.LIBRARY.prepare("SELECT thanks FROM appraisal_results WHERE id = ?").bind(a.winner).first();
  // ONE BUILD HANDED BACK, by its id — what the bot's replay reads. A build is
  // board material and public; whose browser found it is not said.
  const rid = Number(new URL(request.url).searchParams.get("result"));
  const res = Number.isInteger(rid) && rid > 0
    && await env.LIBRARY.prepare("SELECT id, build, thanks FROM appraisal_results WHERE id = ? AND code = ?").bind(rid, code).first();
  return json({ ok: true, code, weapon: a.weapon, ruler: a.ruler, riven: JSON.parse(a.riven), at: a.at,
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
  const r = await env.LIBRARY.prepare("UPDATE appraisals SET request = ?, engine = ? WHERE code = ? AND request IS NULL")
    .bind(JSON.stringify({ request: b.request, context: b.context }), b.engine, code).run();
  return json({ ok: true, stored: !!r.meta.changes });
}

// ---- Volunteer work ------------------------------------------------------------
//
// A RIVEN GAIN IS ALSO A TASK the community's computers take, as a board order is
// (worker/verify.js): one nobody has answered goes first — someone is waiting in
// a chat — then one answered once, to a computer of another owner, until two
// owners' answers agree on the build, its score and the search's work, which
// credits both. The search is deterministic for a frozen request, so honest
// computers agree to the bit; the bot replays the first answer before the chat
// hears it, so a made-up number is never said.

/// THE CANONICAL TEXT OF A BUILD: keys sorted, so two computers' answers compare.
const canon = (v) => (Array.isArray(v) ? `[${v.map(canon).join(",")}]`
  : v && typeof v === "object" ? `{${Object.keys(v).sort().map((k) => `${JSON.stringify(k)}:${canon(v[k])}`).join(",")}}`
  : JSON.stringify(v));

/// THE TASK FOR `verifier` on the served `engine`, leased, or null. `owners(ids)`
/// answers who owns each device (worker/verify.js `ownersOf`).
export async function rivenTask(env, verifier, engine, owners, lanes = 1) {
  const db = env.LIBRARY, now = Date.now();
  const held = await db.prepare("SELECT 1 FROM appraisals WHERE leased_to = ? AND lease_until > ?").bind(verifier, now).first();
  if (held) return null;
  const { results } = await db.prepare(
    `SELECT a.code, a.weapon, a.ruler, a.request, a.at,
            (SELECT count(*) FROM appraisal_results r WHERE r.code = a.code AND r.verifier IS NOT NULL) AS answered
       FROM appraisals a
      WHERE a.request IS NOT NULL AND a.engine = ? AND a.at > ? AND a.agreed_at IS NULL
        AND (a.lease_until IS NULL OR a.lease_until < ?)
      ORDER BY (answered = 0) DESC, a.at LIMIT 8`).bind(engine, now - KEEP_MS, now).all();
  for (const a of results) {
    if (a.answered >= RIVEN_ANSWERS) continue;
    if (a.answered === 0 && lanes < rivenLanesNeeded(now - a.at)) continue;
    const by = (await db.prepare("SELECT verifier FROM appraisal_results WHERE code = ? AND verifier IS NOT NULL")
      .bind(a.code).all()).results.map((r) => r.verifier);
    if (by.includes(verifier)) continue;
    if (by.length) {
      const own = await owners([verifier, ...by]);
      const mine = own.get(verifier);
      if (mine && by.some((v) => own.get(v) === mine)) continue;
    }
    const lease = [...crypto.getRandomValues(new Uint8Array(16))].map((x) => x.toString(16).padStart(2, "0")).join("");
    const took = await db.prepare(`UPDATE appraisals SET lease = ?, lease_until = ?, leased_to = ?,
        started_at = COALESCE(started_at, ?)
        WHERE code = ? AND (lease_until IS NULL OR lease_until < ?)`)
      .bind(lease, now + RIVEN_LEASE_MS, verifier, now, a.code, now).run();
    if (took.meta.changes) {
      const { request, context } = JSON.parse(a.request);
      return { kind: "riven_gain", lease, code: a.code, weapon: a.weapon, ruler: a.ruler, request, context };
    }
  }
  return null;
}

/// STILL AT IT: the computer holding a riven gain says so while its search runs,
/// and its lease runs on from now — a slow computer is never overtaken by its own
/// lease. `held: false` tells it the task went elsewhere, so it stops.
async function renew(request, env, code) {
  if (request.method !== "POST") return json({ ok: false, error: "POST only" }, 405);
  let b = {};
  try { b = await request.json(); } catch (_) { return json({ ok: false, error: "not json" }, 400); }
  if (!VERIFIER_ID.test(b.verifier || "") || !LEASE_ID.test(b.lease || "")) return json({ ok: false, error: "bad renew" }, 400);
  const now = Date.now();
  // `release: true` GIVES IT BACK: a computer that stopped says so, and the
  // person waiting in the chat is not left behind a lease nobody is working.
  if (b.release === true) {
    await env.LIBRARY.prepare(`UPDATE appraisals SET lease = NULL, lease_until = NULL, leased_to = NULL
        WHERE code = ? AND lease = ? AND leased_to = ? AND agreed_at IS NULL`).bind(code, b.lease, b.verifier).run();
    return json({ ok: true, held: false });
  }
  const r = await env.LIBRARY.prepare(`UPDATE appraisals SET lease_until = ?
      WHERE code = ? AND lease = ? AND leased_to = ? AND lease_until >= ? AND agreed_at IS NULL`)
    .bind(now + RIVEN_LEASE_MS, code, b.lease, b.verifier, now).run();
  return json({ ok: true, held: !!r.meta.changes });
}

/// A VOLUNTEER'S ANSWER, under its lease: kept like any build handed back, and
/// once two owners' answers agree, the work credited to both.
async function volunteerAnswer(env, a, b, now, owners) {
  if (!VERIFIER_ID.test(b.verifier || "") || !LEASE_ID.test(b.lease || "")
      || !Number.isSafeInteger(b.work) || b.work < 0 || typeof b.score !== "number" || !Number.isFinite(b.score)) {
    return json({ ok: false, error: "bad answer" }, 400);
  }
  const db = env.LIBRARY;
  if (a.lease !== b.lease || a.leased_to !== b.verifier || !(a.lease_until >= now)) return json({ ok: true, first: false });
  const key = canon(b.build);
  await db.batch([
    db.prepare(`INSERT INTO appraisal_results (code, build, thanks, at, verifier, score, work, key) VALUES (?, ?, '', ?, ?, ?, ?, ?)`)
      .bind(a.code, JSON.stringify(b.build), now, b.verifier, b.score, b.work, key),
    db.prepare("UPDATE appraisals SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE code = ?").bind(a.code),
  ]);
  const { results } = await db.prepare(
    "SELECT verifier, score, work, key FROM appraisal_results WHERE code = ? AND verifier IS NOT NULL ORDER BY id").bind(a.code).all();
  const own = await owners(results.map((r) => r.verifier));
  const ownerOf = (v) => own.get(v) || v;
  for (let i = 0; i < results.length; i++) {
    for (let j = i + 1; j < results.length; j++) {
      const x = results[i], y = results[j];
      if (x.key !== y.key || x.score !== y.score || x.work !== y.work || ownerOf(x.verifier) === ownerOf(y.verifier)) continue;
      const won = await db.prepare("UPDATE appraisals SET agreed_at = ? WHERE code = ? AND agreed_at IS NULL").bind(now, a.code).run();
      if (!won.meta.changes) return json({ ok: true, first: !a.done_at });
      const today = new Date(now).toISOString().slice(0, 10);
      await db.batch([x, y].flatMap((r) => [
        db.prepare("UPDATE verifiers SET work = work + ? WHERE id = ?").bind(r.work, r.verifier),
        db.prepare(`INSERT INTO verifier_days (verifier, day, work) VALUES (?, ?, ?)
          ON CONFLICT (verifier, day) DO UPDATE SET work = work + excluded.work`).bind(r.verifier, today, r.work),
      ]));
      return json({ ok: true, first: !a.done_at });
    }
  }
  return json({ ok: true, first: !a.done_at });
}

/// A BUILD HANDED BACK: `{ build, thanks }`, a board record and an optional name.
/// Every one is kept until the bot judges it; the first it accepts wins. One sent
/// under a volunteer lease carries `{ lease, verifier, score, work }` instead.
async function handBack(request, env, code, owners = async () => new Map()) {
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
  const a = await db.prepare("SELECT code, done_at, at, lease, lease_until, leased_to FROM appraisals WHERE code = ?").bind(code).first();
  if (!a || a.at < now - KEEP_MS) return json({ ok: false, error: "no such appraisal" }, 404);
  if (b.lease !== undefined) return volunteerAnswer(env, a, b, now, owners);
  await db.prepare("INSERT INTO appraisal_results (code, build, thanks, at) VALUES (?, ?, ?, ?)")
    .bind(code, JSON.stringify(b.build), cleanThanks(b.thanks), now).run();
  return json({ ok: true, first: !a.done_at });
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
      .bind(JSON.stringify({ ok: !!j.ok, ...(j.verdict || {}) }), now, id).run();
    // THE FIRST ACCEPTED BUILD WINS, once: a later one changes nothing.
    if (j.ok) {
      await db.prepare(`UPDATE appraisals SET winner = ?, done_at = ?
        WHERE done_at IS NULL AND code = (SELECT code FROM appraisal_results WHERE id = ?)`).bind(id, now, id).run();
    }
  }
  for (const code of (Array.isArray(b.told) ? b.told : []).slice(0, 50)) {
    await db.prepare("UPDATE appraisals SET told_at = ? WHERE code = ?").bind(now, String(code)).run();
  }
  for (const code of (Array.isArray(b.started_told) ? b.started_told : []).slice(0, 50)) {
    await db.prepare("UPDATE appraisals SET started_told = ? WHERE code = ?").bind(now, String(code)).run();
  }
  const { results: toJudge } = await db.prepare(`SELECT r.id, r.code, r.build, r.thanks, r.at,
      a.weapon, a.ruler, a.riven, a.chat, a.asker, a.at AS asked_at
    FROM appraisal_results r JOIN appraisals a ON a.code = r.code
    WHERE a.channel = ? AND a.done_at IS NULL AND r.checked_at IS NULL AND (r.claimed_at IS NULL OR r.claimed_at < ?)
    ORDER BY r.at LIMIT 5`).bind(channel, now - RECLAIM_MS).all();
  if (toJudge.length) {
    await db.batch(toJudge.map((r) => db.prepare("UPDATE appraisal_results SET claimed_at = ? WHERE id = ?").bind(now, r.id)));
  }
  const { results: toTell } = await db.prepare(`SELECT a.code, a.weapon, a.ruler, a.riven, a.chat, a.asker, a.at AS asked_at,
      a.done_at, r.id AS result_id, r.thanks, r.verdict, r.verifier IS NOT NULL AS volunteer
    FROM appraisals a JOIN appraisal_results r ON r.id = a.winner
    WHERE a.channel = ? AND a.done_at IS NOT NULL AND a.told_at IS NULL ORDER BY a.done_at LIMIT 20`).bind(channel).all();
  // A COMPUTER TOOK IT: the chat may be told so, once, while it waits.
  const { results: toStart } = await db.prepare(`SELECT code, weapon, ruler, riven, chat, asker, at AS asked_at, started_at
    FROM appraisals WHERE channel = ? AND started_at IS NOT NULL AND started_told IS NULL AND told_at IS NULL AND done_at IS NULL
    ORDER BY started_at LIMIT 20`).bind(channel).all();
  const parse = (r) => ({ ...r, riven: JSON.parse(r.riven), chat: JSON.parse(r.chat),
    ...(r.build ? { build: JSON.parse(r.build) } : {}), ...(r.verdict ? { verdict: JSON.parse(r.verdict) } : {}) });
  return json({ ok: true, judge: toJudge.map(parse), tell: toTell.map(parse), started: toStart.map(parse) });
}
