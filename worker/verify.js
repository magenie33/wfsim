// SPDX-License-Identifier: AGPL-3.0-or-later
// THE VOLUNTEERS' DOOR to the fact factory — docs/BOARD.md §"Compute orders",
// §"The fact factory". A machine with the site open asks for a task, is handed
// the factory's next question it may answer (a board row, a riven gain), and
// sends back what it measured; `CLIENTS_PER_FACT` equal results from different
// owners on different networks make a fact (worker/tasks.js). Nothing here
// computes a number, and nothing here decides an order: the factory does.
//
//   POST /api/board/work    { verifier, engine, protocol, consent, lanes, ahead? }   → { work: { lease, identity, record, ruler, mode } | null, stale? }
//        — or `work: { kind: "riven_gain", lease, code, weapon, ruler, request, context }`
//        (worker/appraise.js §"Volunteer work").
//   POST /api/board/verify  { lease, verifier, engine, score, metric, work, compute_ms, counts?, lanes? } → { ok }
//   POST /api/board/release { lease, verifier }                                     → { ok }
//   POST /api/board/mine    { verifier, orders: [{ identity, ruler, mode }] }       → { orders: [{ …, state, at? }] }
//
// EQUAL means the score, the metric AND the work (`Shard::work`): a fact
// credits that work to every client that measured it, and the clients of one
// fact belong to different owners (docs/BOARD.md §"Contribution").
//
// ONLY THE ENGINE THE SITE SERVES WORKS (`site/release.json`): a question is
// asked because the code that answers it is the current code, and a tab left
// open on an older one would answer it with the old arithmetic.

import { witnessOf } from "./tasks.js";
export { FACTS_PER_REFUSAL_FORGIVEN } from "./tasks.js";
import { facts, workOf } from "./factory.js";
import { BOARD_KINDS_SQL } from "./board_tasks.js";
import { iso } from "./instant.js";

/// A browser fights a crowd row in minutes; a lease outlives the slowest.
export const LEASE_MS = 30 * 60_000;
/// HOW MANY DIFFERENT CLIENTS MUST MEASURE THE SAME BITS before a result is a
/// fact. 1 takes the first result unchecked; each one more is one more
/// independent client the result waits for. A check sets `env.CLIENTS_PER_FACT`.
export const CLIENTS_PER_FACT = 2;
/// The share of facts the server recomputes anyway, which is what makes
/// two colluding clients a gamble rather than a method.
export const SPOT_SHARE = 0.05;
/// WHAT A PAGE THAT CAN FILL AN ORDER SENDS. A tab opened before orders existed
/// asks too, takes an order, and answers in a shape this refuses — holding the
/// order for a lease's length — so a page that does not say this gets nothing.
export const PROTOCOL = 6;

const VERIFIER_ID = /^[a-z0-9]{16,40}$/;
const ENGINE_ID = /^[A-Za-z0-9._-]{1,40}$/;
const LEASE_ID = /^[a-f0-9]{32}$/;
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

/// THE CLIENT, remembered by the id it made and nothing else — written once,
/// and again only when the day changes. `{ until }` while it is refused: a
/// refusal is a cool-down (scripts/live_orders.mjs `ban`), lifted here once over.
async function admit(db, id, now) {
  const v = await db.prepare("SELECT banned, seen, refused_until FROM verifiers WHERE id = ?").bind(id).first();
  if (!v) {
    await db.prepare("INSERT OR IGNORE INTO verifiers (id, seen) VALUES (?, ?)").bind(id, day()).run();
    return { ok: true };
  }
  if (v.banned && Date.parse(v.refused_until) > now) return { ok: false, until: v.refused_until };
  if (v.banned) await db.prepare("UPDATE verifiers SET banned = 0 WHERE id = ?").bind(id).run();
  if (v.seen !== day()) await db.prepare("UPDATE verifiers SET seen = ? WHERE id = ?").bind(day(), id).run();
  return { ok: true };
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

/// ONE TASK TO RUN, the factory's next for this client — never the number to
/// agree with.
async function work(request, env) {
  const { b, err } = await read(request);
  if (err) return err;
  if (!VERIFIER_ID.test(b.verifier || "") || !ENGINE_ID.test(b.engine || "")) return json({ ok: false, error: "bad request" }, 400);
  await askedNow(env.LIBRARY, b, Date.now());
  // AN OLDER PAGE IS TOLD SO (`stale`), or a machine left computing would ask
  // for ever and be given nothing once a release ships.
  if (b.protocol !== PROTOCOL) return json({ ok: true, work: null, stale: true });
  const { engine, release } = await servedRelease(env);
  if (!engine) return json({ ok: true, work: null });
  if (b.engine !== engine) return json({ ok: true, work: null, stale: true });
  const db = env.LIBRARY, now = Date.now();
  // NO WORK WITHOUT THE READER'S YES — `consent: { v, at }`, the statement they
  // agreed to and when (69-board-work.js `computeConsent`) — kept on the
  // client's row the first time it is seen and whenever it changes.
  const c = b.consent;
  if (!c || !Number.isInteger(c.v) || c.v < 1 || typeof c.at !== "string" || !ISO.test(c.at)) return json({ ok: true, work: null });
  // A REFUSED CLIENT IS TOLD SO (`banned`), or its page would wait for a next
  // task for ever and its reader would never learn why.
  const admitted = await admit(db, b.verifier, now);
  if (!admitted.ok) return json({ ok: true, work: null, banned: true, until: admitted.until });
  await db.prepare(`UPDATE verifiers SET consent_v = ?, consent_at = ? WHERE id = ? AND (consent_v IS NOT ? OR consent_at IS NOT ?)`)
    .bind(c.v, c.at, b.verifier, c.v, c.at).run();
  // A CLIENT THAT ASKS IS WORKING ON NOTHING: one browser computes in one tab
  // (69-board-work.js, a Web Lock) and asks only between tasks, so a lease it
  // still holds is one its page lost — a crash, a reload, an old page — and is
  // handed back here rather than waited out for half an hour.
  // …UNLESS IT ASKS AHEAD: near the end of a task it asks for the next, so no
  // time passes between the two. It may then hold `TASKS_AHEAD` more and no
  // further, and never a riven gain someone waits on, which goes to a computer
  // free now. Every answer says `ahead`, so the page asks for no more than that.
  const ahead = b.ahead === true;
  if (ahead) {
    const held = (await db.prepare("SELECT count(*) AS n FROM questions WHERE leased_to = ? AND lease_until > ?")
      .bind(b.verifier, iso(now)).first()).n;
    if (held < 1 || held > TASKS_AHEAD) return json({ ok: true, release, ahead: TASKS_AHEAD, work: null });
  } else {
    await facts().releaseAll(env, b.verifier, now);
  }
  // HOW MANY CORES IT CAN GIVE NOW (69-board-work.js `communityLanes`), which
  // decides whether a question someone waits on is its to take — none, when it
  // asks ahead: a riven gain someone waits on goes to a computer free now.
  const lanes = ahead ? 0 : Number.isInteger(b.lanes) && b.lanes > 0 && b.lanes <= 256 ? b.lanes : 1;
  const who = await witnessOf(env, b.verifier);
  const leased = await facts().next(env, { device: b.verifier, owner: who.owner, siblings: who.siblings,
    net: await netOf(request, env), lanes, most: ahead ? TASKS_AHEAD + 1 : 1 }, engine, now);
  return json({ ok: true, release, ahead: TASKS_AHEAD, work: leased && workOf(leased) });
}

/// A CLIENT THAT ASKS IS ON, its engine (with an older protocol marked) and
/// free cores kept, which tells an idle page from a closed one — written at
/// most once a minute, or a page asking every ten seconds is a write each time.
export const ASKED_EVERY_MS = 60_000;
async function askedNow(db, b, now) {
  const engine = b.protocol === PROTOCOL ? b.engine : `${b.engine}/p${Number(b.protocol) || 0}`;
  const lanes = b.ahead === true ? null : Number.isInteger(b.lanes) && b.lanes > 0 && b.lanes <= 256 ? b.lanes : 1;
  await db.prepare(`UPDATE verifiers SET asked_at = ?1, asked_engine = ?2, asked_lanes = coalesce(?3, asked_lanes)
      WHERE id = ?4 AND (asked_at IS NULL OR asked_at < ?5 OR asked_engine IS NOT ?2 OR (?3 IS NOT NULL AND asked_lanes IS NOT ?3))`)
    .bind(iso(now), engine, lanes, b.verifier, iso(now - ASKED_EVERY_MS)).run();
}

/// HOW MANY TASKS A CLIENT MAY HOLD BESIDE THE ONE IT IS COMPUTING, asked near
/// its end (`work`, `ahead`). More hides more of a slow round trip; each is a
/// lease no other computer can take while it waits.
export const TASKS_AHEAD = 1;

/// WHAT THE CLIENT MEASURED, under its lease. The answer is always `ok`: a
/// client learns nothing from it about whether it agreed. `counts` and `lanes`
/// are what it cost and on how many cores (worker/tasks.js `answer`).
async function verify(request, env) {
  const { b, err } = await read(request, VERIFY_BODY);
  if (err) return err;
  if (!LEASE_ID.test(b.lease || "") || !VERIFIER_ID.test(b.verifier || "") || !ENGINE_ID.test(b.engine || "")
      || !METRIC_ID.test(b.metric || "")
      || typeof b.score !== "number" || !Number.isFinite(b.score) || b.score < 0
      || !Number.isSafeInteger(b.work) || b.work < 0) {
    return json({ ok: false, error: "bad request" }, 400);
  }
  // WHAT THE FIGHT COST THE CLIENT, as it says — a statistic about the clients'
  // machines and never a number anything is ranked or scheduled by.
  const ms = Number.isInteger(b.compute_ms) && b.compute_ms >= 0 && b.compute_ms <= LEASE_MS ? b.compute_ms : null;
  const engine = await servedEngine(env);
  if (!engine || b.engine !== engine) {
    await facts().release(env, b.verifier, b.lease);
    return json({ ok: true });
  }
  const who = await witnessOf(env, b.verifier);
  await facts().answer(env, { device: b.verifier, owner: who.owner, net: await netOf(request, env) },
    { lease: b.lease, engine, result: { score: b.score, metric: b.metric }, work: b.work, compute_ms: ms,
      counts: countsOf(b.counts), lanes: Number.isInteger(b.lanes) && b.lanes > 0 && b.lanes <= 256 ? b.lanes : null });
  return json({ ok: true });
}

/// WHAT A TASK COST, as a page reports it: named whole counts, kept beside the
/// answer to fit what a point is worth — never credited, so only their shape is checked.
export const countsOf = (c) => {
  if (!c || typeof c !== "object" || Array.isArray(c)) return null;
  const out = {};
  for (const [k, v] of Object.entries(c).slice(0, 32)) if (/^[a-z_]{1,32}$/.test(k) && Number.isSafeInteger(v) && v >= 0) out[k] = v;
  return Object.keys(out).length ? out : null;
};
const VERIFY_BODY = 4096;

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
  const db = env.LIBRARY, out = [];
  for (const k of keys) {
    // WHAT THE FACTORY SAYS of this client's answer to that row's question.
    const r = await db.prepare(
      `SELECT q.state, q.fact_at,
              (SELECT max(credited) FROM answers a WHERE a.question = q.id AND a.device = ?1 AND a.void = 0) AS credited,
              (SELECT count(*) FROM answers a WHERE a.question = q.id AND a.device = ?1 AND a.void = 0) AS mine
         FROM demands d JOIN questions q ON q.id = d.question
        WHERE d.ref = ?2 AND d.producer IN ${BOARD_KINDS_SQL} ORDER BY d.asked_at DESC LIMIT 1`)
      .bind(b.verifier, `${k.identity}|${k.ruler}|${k.mode}`).first();
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
  if (path === "/api/board/verify") return verify(request, env);
  if (path === "/api/board/release") return release(request, env);
  if (path === "/api/board/mine") return mine(request, env);
  return json({ ok: false, error: "not found" }, 404);
}
