// SPDX-License-Identifier: AGPL-3.0-or-later
// THE VOLUNTEERS' DOOR to the fact factory — docs/BOARD.md §"Compute orders",
// §"The fact factory". A machine with the site open asks for a task, is handed
// the factory's next question it may answer (a board row, a riven gain), and
// sends back what it measured; `CLIENTS_PER_FACT` equal results from different
// owners on different networks make a fact (worker/tasks.js). Nothing here
// computes a number, and nothing here decides an order: the factory does.
//
//   POST /api/board/work    { verifier, engine, protocol, consent, lanes, pace_ms?, ahead? }
//        → { pack: { lease, lease_ms, tasks: [{ question, identity, record, ruler, mode }] } | null, work: null }
//        — or, for a question that goes alone, `work: { kind: "riven_gain", lease, code, weapon, ruler, request, context }`
//        (worker/appraise.js §"Volunteer work"), `pack: null`.
//   POST /api/board/answers { lease, verifier, engine, answers: [{ question, score, metric, work, compute_ms, counts?, lanes? }], done? } → { ok }
//   POST /api/board/release { lease, verifier }                                     → { ok }
//   POST /api/board/renew   { lease, verifier, question }                           → { ok, held, renew_ms }
//   …and PROTOCOL 6, the one before (`PROTOCOLS_SERVED`): `/work` hands a pack of one
//   as `work: { lease, identity, record, ruler, mode }`, answered by
//   POST /api/board/verify { lease, verifier, engine, score, metric, work, compute_ms, counts?, lanes? } → { ok }
//   POST /api/board/mine    { verifier, orders: [{ identity, ruler, mode }] }       → { orders: [{ …, state, at? }] }
//
// EQUAL means the score, the metric AND the work (`Shard::work`): a fact
// credits that work to every client that measured it, and the clients of one
// fact belong to different owners (docs/BOARD.md §"Contribution").
//
// ONLY THE ENGINE THE SITE SERVES WORKS (`site/release.json`): a question is
// asked because the code that answers it is the current code, and a tab left
// open on an older one would answer it with the old arithmetic.

import { witnessOf, RENEW_MS } from "./tasks.js";
export { FACTS_PER_REFUSAL_FORGIVEN } from "./tasks.js";
import { facts, workOf, packTaskOf } from "./factory.js";
import { BOARD_KINDS_SQL } from "./board_tasks.js";
import { iso } from "./instant.js";
import { fairTable, throttled, accountOf } from "./fair.js";

/// A browser fights a crowd row in minutes; a lease outlives the slowest.
export const LEASE_MS = 30 * 60_000;
/// HOW MANY DIFFERENT CLIENTS MUST MEASURE THE SAME BITS before a result is a
/// fact. 1 takes the first result unchecked; each one more is one more
/// independent client the result waits for. A check sets `env.CLIENTS_PER_FACT`.
export const CLIENTS_PER_FACT = 2;
/// THE SHARE OF FACTS ONE MORE VOLUNTEER MEASURES before they count, which is
/// what makes two colluding clients a gamble rather than a method: a third who
/// differs makes a dispute, and the official machines refuse the liar.
export const RECHECK_SHARE = 0.02;
/// …AND THE SHARE THE OFFICIAL MACHINES RECOMPUTE after the fact, for a ring
/// large enough to be its own third: the board's authority is that an official
/// answer can land on any fact. A spot is credited at once, so a backlog of
/// them is a check late.
export const SPOT_SHARE = 0.01;
/// WHAT A PAGE THAT CAN FILL AN ORDER SENDS. A tab opened before orders existed
/// asks too, takes an order, and answers in a shape this refuses — holding the
/// order for a lease's length — so a page that does not say this gets nothing.
export const PROTOCOL = 7;
/// THE PROTOCOL BEFORE IS SERVED TOO, so a release stops no client: a page
/// reloads into the new one, but a program somebody else wrote (simfleet) runs
/// on until its author ships. Dropped once nobody has asked with it for a week
/// (`verifiers.asked_engine` marks it).
export const PROTOCOLS_SERVED = new Set([PROTOCOL, 6]);

const VERIFIER_ID = /^[a-z0-9]{16,40}$/;
/// A CLIENT THE SITE KNOWS makes its id as the page does — 24 hex
/// (69-board-work.js `verifierId`); simfleet, a community client, does too. Any
/// other is handed no work and its answers are not taken, and is told why;
/// what it already earned stays.
const RECOGNISED_ID = /^[0-9a-f]{24}$/;
const UNRECOGNISED = "这个客户端不是 WFSim 认可的客户端，已暂停派发任务。使用第三方客户端前请先联系 WFSim 作者说明情况："
  + "QQ 群 https://qm.qq.com/q/uiXrMSTs8S 或 Discord https://discord.gg/H4BMqvYFVg 。已获得的贡献保留。"
  + " / This client is not recognised by WFSim and receives no work. Contact the WFSim author before running a"
  + " third-party client: QQ group https://qm.qq.com/q/uiXrMSTs8S or Discord https://discord.gg/H4BMqvYFVg."
  + " Contribution already earned is kept.";
export const unrecognised = (id) => (RECOGNISED_ID.test(id || "") ? null
  : json({ ok: false, error: "unrecognised_client", message: UNRECOGNISED }, 403));
const ENGINE_ID = /^[A-Za-z0-9._-]{1,40}$/;
const LEASE_ID = /^[a-f0-9]{32}$/;
const QUESTION_ID = /^[a-f0-9]{32}$/;
const METRIC_ID = /^[a-z_]{1,24}$/;
const ISO = /^\d{4}-\d\d-\d\dT[\d:.]+Z$/;

const json = (body, status = 200) => new Response(JSON.stringify(body), {
  status, headers: { "content-type": "application/json", "cache-control": "no-store" } });
const day = () => new Date().toISOString().slice(0, 10);

async function read(request, max = 2048) {
  if (request.method !== "POST") return { err: json({ ok: false, error: "POST only" }, 405) };
  const raw = await request.text();
  if (raw.length > max) return { err: json({ ok: false, error: "payload too large" }, 400) };
  try { return { b: JSON.parse(raw) }; } catch { return { err: json({ ok: false, error: "not json" }, 400) }; }
}

/// THE ENGINE THE SITE SERVES, read once per isolate — a deploy is a new
/// isolate, so it cannot change under one. `null` when it cannot be read, and
/// then nobody works: an order answered by an unknown engine proves nothing.
const served = new WeakMap();
async function servedEngine(env) {
  return (await servedRelease(env)).engine;
}
/// …AND THE RELEASE, told with every answer so a page left computing for weeks
/// reloads into each new one (69-board-work.js `releaseNewer`), not only into a
/// new engine.
export async function servedRelease(env) {
  if (!env.ASSETS) return { engine: null, release: null };
  if (served.has(env.ASSETS)) return served.get(env.ASSETS);
  let out = { engine: null, release: null };
  try {
    const r = await env.ASSETS.fetch(new Request("https://wfsim.app/release.json"));
    const j = r.ok ? await r.json() : {};
    out = { engine: typeof j.engine === "string" && ENGINE_ID.test(j.engine) ? j.engine : null,
      release: typeof j.release === "string" && ENGINE_ID.test(j.release) ? j.release : null };
  } catch { /* unread: nobody works until it is */ }
  if (out.engine) served.set(env.ASSETS, out);
  return out;
}

/// THE NETWORK A CLIENT ASKS FROM, as a salted hash of its public address — an
/// IPv4 address whole, an IPv6 one by its /64 — so a further result never comes
/// from the network of the one it confirms: two browsers on one desk agreeing
/// are one witness counted twice. Kept on an open order and cleared with it;
/// "" when the address is unknown, which excludes nothing.
export async function netOf(request, env) {
  const ip = (request.headers.get("cf-connecting-ip") || "").trim();
  if (!ip) return "";
  const net = ip.includes(":") ? ip.split(":").slice(0, 4).join(":") : ip;
  const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`${env.AUTH_SECRET || "wfsim"}|${net}`));
  return [...new Uint8Array(bytes).slice(0, 6)].map((x) => x.toString(16).padStart(2, "0")).join("");
}
const netsOf = (o) => (o.clients_nets || "").split(",").filter(Boolean);

/// A PACK IS ABOUT A MINUTE OF A COMPUTER'S WORK: the page reports its pace
/// (`pace_ms`, its last rows' wall time each) and is handed that many rows, so
/// the database is asked once a minute a computer, not once a row.
export const PACK_MS = 60_000;
export const PACK_MAX = 20;
/// …A COMPUTER WHOSE PACE IS NOT KNOWN YET is handed this many.
export const PACK_FIRST = 3;
/// A PACK IS LEASED TWICE ITS EXPECTED LENGTH, within these bounds, so a page
/// that vanished without a word holds its rows minutes, not half an hour; a
/// row that runs longer is renewed while it runs (`renew`), never cut off.
export const PACK_LEASE_MIN_MS = 5 * 60_000;
/// HOW MANY PACKS A CLIENT HOLDS AT MOST: the one it computes and the next,
/// asked as this one ends (`ahead`), so no core waits on a round trip.
export const PACKS_HELD = 2;
/// FAIR SHARE: while the open questions are few, a pack is at most their number
/// over the computers asking — the last rows of a batch go to many computers at
/// once, never to one fast one while the rest wait. Counted up to this many.
const FAIR_SCAN = 4000;
export function packSize(pace_ms, open, asking) {
  const byPace = pace_ms ? Math.round(PACK_MS / pace_ms) : PACK_FIRST;
  return Math.max(1, Math.min(PACK_MAX, byPace, Math.floor(open / Math.max(1, asking))));
}
export const packLeaseMs = (n, pace_ms) => (pace_ms ? Math.min(LEASE_MS, Math.max(PACK_LEASE_MIN_MS, 2 * n * pace_ms)) : LEASE_MS);

/// THE FACTORY'S NEXT WORK FOR THIS CLIENT — a pack of rows, or one question that
/// goes alone — never the number to agree with. Everything the ask writes and
/// reads before choosing is ONE batch: one round trip to the database, which
/// every computer asking shares.
async function work(request, env) {
  const { b, err } = await read(request);
  if (err) return err;
  if (!VERIFIER_ID.test(b.verifier || "") || !ENGINE_ID.test(b.engine || "")) return json({ ok: false, error: "bad request" }, 400);
  const refused = unrecognised(b.verifier);
  if (refused) return refused;
  const db = env.LIBRARY, now = Date.now(), at = iso(now), id = b.verifier;
  const asked = askedOf(db, b, now);
  // AN OLDER PAGE IS TOLD SO (`stale`), or a machine left computing would ask
  // for ever and be given nothing once a release ships.
  if (!PROTOCOLS_SERVED.has(b.protocol)) { await asked.run(); return json({ ok: true, work: null, pack: null, stale: true }); }
  const single = b.protocol !== PROTOCOL;
  const { engine, release } = await servedRelease(env);
  if (!engine) { await asked.run(); return json({ ok: true, work: null, pack: null }); }
  if (b.engine !== engine) { await asked.run(); return json({ ok: true, work: null, pack: null, stale: true }); }
  // NO WORK WITHOUT THE READER'S YES — `consent: { v, at }`, the statement they
  // agreed to and when (69-board-work.js `computeConsent`) — kept on the
  // client's row the first time it is seen and whenever it changes.
  const c = b.consent;
  if (!c || !Number.isInteger(c.v) || c.v < 1 || typeof c.at !== "string" || !ISO.test(c.at)) {
    await asked.run();
    return json({ ok: true, work: null, pack: null });
  }
  // A CLIENT THAT ASKS IS WORKING ON NOTHING: one browser computes in one tab
  // (69-board-work.js, a Web Lock) and asks only between packs, so a lease it
  // still holds is one its page lost — a crash, a reload, an old page — and is
  // handed back here rather than waited out. …UNLESS IT ASKS AHEAD, as its pack
  // ends: it may then hold `PACKS_HELD` and no more, and never a question
  // someone waits on, which goes to a computer free now (no cores: `lanes` 0).
  const ahead = b.ahead === true;
  const lanes = ahead ? 0 : Number.isInteger(b.lanes) && b.lanes > 0 && b.lanes <= 256 ? b.lanes : 1;
  const pace = Number.isInteger(b.pace_ms) && b.pace_ms >= 100 && b.pace_ms <= LEASE_MS ? b.pace_ms : null;
  const who = await witnessOf(env, id);
  const w = { device: id, owner: who.owner, siblings: who.siblings, net: await netOf(request, env), lanes };
  const f = facts();
  const [, , , , , held, me, fair, head] = await db.batch([
    // THE CLIENT, remembered by the id it made and nothing else; its day kept,
    // and a refusal — a cool-down (scripts/live_orders.mjs `ban`) — lifted once over.
    db.prepare("INSERT OR IGNORE INTO verifiers (id, seen) VALUES (?, ?)").bind(id, day()),
    asked,
    db.prepare("UPDATE verifiers SET consent_v = ?, consent_at = ? WHERE id = ? AND (consent_v IS NOT ? OR consent_at IS NOT ?)")
      .bind(c.v, c.at, id, c.v, c.at),
    db.prepare("UPDATE verifiers SET banned = 0 WHERE id = ? AND banned = 1 AND (refused_until IS NULL OR refused_until <= ?)").bind(id, at),
    db.prepare("UPDATE verifiers SET seen = ? WHERE id = ? AND seen IS NOT ?").bind(day(), id, day()),
    ahead ? db.prepare("SELECT count(*) AS questions, count(DISTINCT lease) AS packs FROM questions WHERE leased_to = ? AND lease_until > ?").bind(id, at)
      : f.releaseAllOf(env, id, now),
    db.prepare("SELECT banned, refused_until FROM verifiers WHERE id = ?").bind(id),
    db.prepare(`SELECT (SELECT count(*) FROM (SELECT 1 FROM questions WHERE state = 'open' AND waits = 0
                  AND (lease_until IS NULL OR lease_until < ?1) LIMIT ?2)) AS open,
                (SELECT count(*) FROM verifiers WHERE asked_at > ?3) AS asking`).bind(at, FAIR_SCAN, iso(now - 2 * ASKED_EVERY_MS)),
    f.headOf(env, w, engine, now),
  ]);
  // A REFUSED CLIENT IS TOLD SO (`banned`), or its page would wait for a next
  // task for ever and its reader would never learn why.
  const v = me.results[0] || {};
  if (v.banned) return json({ ok: true, work: null, pack: null, banned: true, until: v.refused_until });
  const holds = ahead ? held.results[0] : { questions: 0, packs: 0 };
  const none = json({ ok: true, release, work: null, pack: null });
  if (ahead && (holds.packs < 1 || holds.packs >= PACKS_HELD)) return none;
  const first = head.results[0];
  if (!first) return none;
  if (!f.packable(first)) {
    const leased = await f.next(env, { ...w, most: holds.questions + 1 }, engine, now);
    return json({ ok: true, release, work: leased && workOf(leased), pack: null });
  }
  const { open, asking } = fair.results[0];
  // WHILE THE OPEN ROWS ARE FEWER THAN THE ASKING WOULD TAKE, an account past its
  // fair share is handed nothing this ask (worker/fair.js) — and only then, so
  // the table is never read while work is plentiful.
  const short = Math.floor(open / Math.max(1, asking)) < (pace ? Math.round(PACK_MS / pace) : PACK_FIRST);
  if (short && throttled(await fairTable(request, env, engine, now), accountOf(who.owner, id))) return none;
  const n = single ? 1 : packSize(pace, open, asking), lease_ms = single ? LEASE_MS : packLeaseMs(n, pace);
  const p = await f.nextPack(env, w, engine, { n, lease_ms, most: ahead ? PACKS_HELD : 1 }, now);
  const tasks = p ? p.questions.map(packTaskOf).filter(Boolean) : [];
  if (single) {
    const t = tasks[0];
    return json({ ok: true, release, work: t ? { lease: p.lease, identity: t.identity, record: t.record, ruler: t.ruler, mode: t.mode } : null });
  }
  return json({ ok: true, release, work: null, pack: tasks.length ? { lease: p.lease, lease_ms, tasks } : null });
}

/// A CLIENT THAT ASKS IS ON, its engine (with an older protocol marked) and
/// free cores kept, which tells an idle page from a closed one — written at
/// most once a minute, or a page asking every ten seconds is a write each time.
export const ASKED_EVERY_MS = 60_000;
function askedOf(db, b, now) {
  const engine = b.protocol === PROTOCOL ? b.engine : `${b.engine}/p${Number(b.protocol) || 0}`;
  const lanes = b.ahead === true ? null : Number.isInteger(b.lanes) && b.lanes > 0 && b.lanes <= 256 ? b.lanes : 1;
  return db.prepare(`UPDATE verifiers SET asked_at = ?1, asked_engine = ?2, asked_lanes = coalesce(?3, asked_lanes)
      WHERE id = ?4 AND (asked_at IS NULL OR asked_at < ?5 OR asked_engine IS NOT ?2 OR (?3 IS NOT NULL AND asked_lanes IS NOT ?3))`)
    .bind(iso(now), engine, lanes, b.verifier, iso(now - ASKED_EVERY_MS));
}

/// WHAT THE CLIENT MEASURED, a pack's rows together, under the pack's lease —
/// sent every half minute and as the pack ends (`done`, which gives back the
/// rows it did not answer). The answer is always `ok`: a client learns nothing
/// from it about whether it agreed. `counts` and `lanes` are what each cost and
/// on how many cores (worker/tasks.js `answerPack`).
async function answers(request, env) {
  const { b, err } = await read(request, ANSWERS_BODY);
  if (err) return err;
  if (!LEASE_ID.test(b.lease || "") || !VERIFIER_ID.test(b.verifier || "") || !ENGINE_ID.test(b.engine || "")
      || !Array.isArray(b.answers) || b.answers.length > PACK_MAX) {
    return json({ ok: false, error: "bad request" }, 400);
  }
  const refused = unrecognised(b.verifier);
  if (refused) return refused;
  const items = new Map();
  for (const a of b.answers) {
    if (!a || !QUESTION_ID.test(a.question || "") || !METRIC_ID.test(a.metric || "")
        || typeof a.score !== "number" || !Number.isFinite(a.score) || a.score < 0 || !Number.isSafeInteger(a.work) || a.work < 0) {
      return json({ ok: false, error: "bad request" }, 400);
    }
    // WHAT THE FIGHT COST THE CLIENT, as it says — a statistic about the clients'
    // machines and never a number anything is ranked or scheduled by.
    items.set(a.question, { question: a.question, result: { score: a.score, metric: a.metric }, work: a.work,
      compute_ms: Number.isInteger(a.compute_ms) && a.compute_ms >= 0 && a.compute_ms <= LEASE_MS ? a.compute_ms : null,
      counts: countsOf(a.counts), lanes: Number.isInteger(a.lanes) && a.lanes > 0 && a.lanes <= 256 ? a.lanes : null });
  }
  const engine = await servedEngine(env);
  if (!engine || b.engine !== engine) {
    await facts().release(env, b.verifier, b.lease);
    return json({ ok: true });
  }
  const who = await witnessOf(env, b.verifier);
  await facts().answerPack(env, { device: b.verifier, owner: who.owner, net: await netOf(request, env) },
    { lease: b.lease, engine, answers: [...items.values()], done: b.done === true });
  return json({ ok: true });
}
/// A full pack's answers, each about three hundred bytes.
const ANSWERS_BODY = 32_768;

/// PROTOCOL 6'S ANSWER: one row under the lease of its pack of one, whose
/// question the lease names.
async function verify(request, env) {
  const { b, err } = await read(request, 4096);
  if (err) return err;
  if (!LEASE_ID.test(b.lease || "") || !VERIFIER_ID.test(b.verifier || "")) return json({ ok: false, error: "bad request" }, 400);
  const refused = unrecognised(b.verifier);
  if (refused) return refused;
  const q = await env.LIBRARY.prepare("SELECT id FROM questions WHERE lease = ? AND leased_to = ? LIMIT 1").bind(b.lease, b.verifier).first();
  if (!q) return json({ ok: true });
  const { lease, verifier, engine, ...a } = b;
  return answers(new Request(request.url, { method: "POST", headers: request.headers,
    body: JSON.stringify({ lease, verifier, engine, done: true, answers: [{ ...a, question: q.id }] }) }), env);
}

/// WHAT A TASK COST, as a page reports it: named whole counts, kept beside the
/// answer to fit what a point is worth — never credited, so only their shape is checked.
export const countsOf = (c) => {
  if (!c || typeof c !== "object" || Array.isArray(c)) return null;
  const out = {};
  for (const [k, v] of Object.entries(c).slice(0, 32)) if (/^[a-z_]{1,32}$/.test(k) && Number.isSafeInteger(v) && v >= 0) out[k] = v;
  return Object.keys(out).length ? out : null;
};

/// A LEASE GIVEN BACK by a page that will not answer — closed, reloaded, paused,
/// its fight refused, its lease about to run out. The order is anyone's again at
/// once and this client may take another, instead of both waiting `LEASE_MS`:
/// every release reloads every page that is computing, so that wait was paid
/// by every computer at once.
async function release(request, env) {
  const { b, err } = await read(request);
  if (err) return err;
  if (!LEASE_ID.test(b.lease || "") || !VERIFIER_ID.test(b.verifier || "")) return json({ ok: false, error: "bad request" }, 400);
  await facts().release(env, b.verifier, b.lease);
  return json({ ok: true });
}

/// THE ROW A PAGE IS COMPUTING, kept for it (worker/tasks.js `renewRow`): sent
/// while the row runs and its lease nears its end, and before a row whose lease
/// may have run out. `held` false — taken since, and not as the holder before — stops it.
async function renew(request, env) {
  const { b, err } = await read(request);
  if (err) return err;
  if (!LEASE_ID.test(b.lease || "") || !VERIFIER_ID.test(b.verifier || "") || !QUESTION_ID.test(b.question || "")) {
    return json({ ok: false, error: "bad request" }, 400);
  }
  const refused = unrecognised(b.verifier);
  if (refused) return refused;
  const { held } = await facts().renewRow(env, b.verifier, b.lease, b.question);
  return json({ ok: true, held, renew_ms: RENEW_MS });
}

/// WHERE EACH OF A BROWSER'S RECENT RESULTS STANDS, asked by its own secret id
/// for the orders it names (73-compute.js, the recent tasks): `waiting` for a
/// client of another owner and network, `checking` while the server settles a
/// difference, `confirmed` once a fact counts it — with when — and `gone` when
/// the row is no longer its to earn. It says nothing about anyone else.
const MINE_MAX = 60;
/// A WHOLE LIST OF KEYS fits: fifty tasks are about five kilobytes, and at the
/// common limit every list past twenty was refused and no task left "waiting".
const MINE_BODY = 8192;
async function mine(request, env) {
  const { b, err } = await read(request, MINE_BODY);
  if (err) return err;
  if (!VERIFIER_ID.test(b.verifier || "") || !Array.isArray(b.orders)) return json({ ok: false, error: "bad request" }, 400);
  const keys = b.orders.slice(0, MINE_MAX).filter((k) => k && typeof k.identity === "string" && typeof k.ruler === "string" && typeof k.mode === "string");
  // WHAT THE FACTORY SAYS of this client's answer to each row's question, in one read.
  const refs = keys.map((k) => `${k.identity}|${k.ruler}|${k.mode}`);
  const { results } = await env.LIBRARY.prepare(
    `SELECT k.value AS ref, q.state, q.fact_at,
            (SELECT max(credited) FROM answers a WHERE a.question = q.id AND a.device = ?1 AND a.void = 0) AS credited,
            (SELECT count(*) FROM answers a WHERE a.question = q.id AND a.device = ?1 AND a.void = 0) AS mine
       FROM json_each(?2) k
       JOIN questions q ON q.id = (SELECT d.question FROM demands d WHERE d.ref = k.value AND d.producer IN ${BOARD_KINDS_SQL}
                                     ORDER BY d.asked_at DESC LIMIT 1)`)
    .bind(b.verifier, JSON.stringify(refs)).all();
  const byRef = new Map(results.map((r) => [r.ref, r]));
  const out = [];
  for (const k of keys) {
    const r = byRef.get(`${k.identity}|${k.ruler}|${k.mode}`);
    if (!r) continue;
    const state = !r.mine ? "gone" : r.credited ? "confirmed"
      : ["disputed", "nondeterministic"].includes(r.state) ? "checking"
      : r.state === "open" ? "waiting" : "gone";
    out.push({ identity: k.identity, ruler: k.ruler, mode: k.mode, state, ...(state === "confirmed" && r.fact_at ? { at: r.fact_at } : {}) });
  }
  return json({ ok: true, orders: out });
}

export async function verifyRoute(request, env, path) {
  if (!env.LIBRARY) return json({ ok: false, error: "the library is not configured" }, 503);
  if (path === "/api/board/work") return work(request, env);
  if (path === "/api/board/answers") return answers(request, env);
  if (path === "/api/board/verify") return verify(request, env);
  if (path === "/api/board/release") return release(request, env);
  if (path === "/api/board/renew") return renew(request, env);
  if (path === "/api/board/mine") return mine(request, env);
  return json({ ok: false, error: "not found" }, 404);
}
