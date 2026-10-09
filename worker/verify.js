// SPDX-License-Identifier: AGPL-3.0-or-later
// COMPUTE ORDERS — docs/BOARD.md §"Compute orders". Every owed row is an order;
// this hands one at a time to a machine that has the site open, and keeps what
// comes back. The first result makes an order `fresh` (the server ranks it);
// each later one, from a client that has not measured it, is compared bit for
// bit — the engine is deterministic on every target (docs/WASM.md) — and
// `CLIENTS_PER_FACT` equal results make a fact in `scores`. Nothing here
// computes a number.
//
//   POST /api/board/work    { verifier, engine, protocol, consent, lanes }                → { work: { lease, record, ruler, mode } | null, stale? }
//        — or `work: { kind: "riven_gain", lease, code, weapon, ruler, request, context }`, a
//        riven gain someone is waiting on (worker/appraise.js §"Volunteer work").
//   POST /api/board/verify  { lease, verifier, engine, score, metric, work, compute_ms }  → { ok }
//   POST /api/board/release { lease, verifier }                                          → { ok }
//
// EQUAL means the score, the metric AND the work (`Shard::work`): a fact
// credits that work to every client that measured it, and the clients of one
// fact belong to different owners (docs/BOARD.md §"Contribution").
//
// ONLY THE ENGINE THE SITE SERVES WORKS (`site/release.json`): an order exists
// because the code that scores it is the current code, and a tab left open on
// an older one would answer it with the old arithmetic.

import { ownersOf } from "./contribution.js";
import { rivenTask } from "./appraise.js";
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
/// A REFUSAL IS FORGIVEN BY HONEST WORK: a client with k refusals on record has
/// one taken off by k times this many facts it is part of afterwards — results
/// other volunteers confirmed — so four take ten thousand to clear, and each
/// one off shortens its next cool-down a step (scripts/live_orders.mjs).
export const FACTS_PER_REFUSAL_FORGIVEN = 1000;
/// The range an order's `slot` is drawn from (`ship_queue.sh`).
const SLOT_SPAN = 2147483647;
/// WHAT A PAGE THAT CAN FILL AN ORDER SENDS. A tab opened before orders existed
/// asks too, takes an order, and answers in a shape this refuses — holding the
/// order for a lease's length — so a page that does not say this gets nothing.
export const PROTOCOL = 6;

const VERIFIER_ID = /^[a-z0-9]{16,40}$/;
const ENGINE_ID = /^[A-Za-z0-9._-]{1,40}$/;
const LEASE_ID = /^[a-f0-9]{32}$/;
const METRIC_ID = /^[a-z_]{1,24}$/;
const ISO = /^\d{4}-\d\d-\d\dT[\d:.]+Z$/;

const needed = (env) => env.CLIENTS_PER_FACT ?? CLIENTS_PER_FACT;
/// EVERY CLIENT THAT MEASURED AN ORDER, in the order their results came, and
/// beside it what each one's fight cost it, in ms ("" where a client did not say).
const clientsOf = (o) => (o.clients || o.produced_by || "").split(",").filter(Boolean);
const computeOf = (o) => {
  const ms = (o.clients_compute_ms || "").split(",");
  return clientsOf(o).map((_, i) => ms[i] || "");
};


const json = (body, status = 200) => new Response(JSON.stringify(body), {
  status, headers: { "content-type": "application/json", "cache-control": "no-store" } });
const day = () => new Date().toISOString().slice(0, 10);
const hour = () => new Date().toISOString().slice(0, 13);
const done = "lease = NULL, lease_until = NULL, leased_to = NULL";

async function read(request) {
  if (request.method !== "POST") return { err: json({ ok: false, error: "POST only" }, 405) };
  const raw = await request.text();
  if (raw.length > 2048) return { err: json({ ok: false, error: "payload too large" }, 400) };
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
async function servedRelease(env) {
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

/// A RELEASE CARRIES A RESULT, IT DOES NOT ERASE IT: an order an older engine
/// measured is opened for the served one, keeping its clients and noting the
/// engine they used (`carried_from`). Most releases leave most numbers where
/// they were, and wiping them threw a third of the volunteers' work away; a
/// client of the new engine either reproduces the bits — a fact, every client
/// credited — or replaces them (`verify`). A claimed order keeps its claim.
/// Once per engine an isolate sees, since no older engine can answer after that.
const retired = new Set();
async function retire(db, engine) {
  if (retired.has(engine)) return;
  await db.prepare(`UPDATE orders SET carried_from = COALESCE(carried_from, engine), engine = ?,
                    state = CASE WHEN state = 'scoring:open' THEN state ELSE 'open' END, ${done}
                    WHERE state IN ('fresh', 'open', 'scoring:open') AND engine != ? AND engine != ''`).bind(engine, engine).run();
  retired.add(engine);
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
async function netOf(request, env) {
  const ip = (request.headers.get("cf-connecting-ip") || "").trim();
  if (!ip) return "";
  const net = ip.includes(":") ? ip.split(":").slice(0, 4).join(":") : ip;
  const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`${env.AUTH_SECRET || "wfsim"}|${net}`));
  return [...new Uint8Array(bytes).slice(0, 6)].map((x) => x.toString(16).padStart(2, "0")).join("");
}
const netsOf = (o) => (o.clients_nets || "").split(",").filter(Boolean);

/// UP TO `n` LEASABLE ORDERS IN ONE STATE, the rows a new build owes before a
/// rescore's (`priority`), each from a random slot onwards and then from the
/// start: a seek on `orders_pick`, so a lease reads a few rows however long the
/// book is.
async function candidates(db, state, engine, now, n) {
  for (const priority of [0, 1]) {
    const start = Math.floor(Math.random() * SLOT_SPAN);
    for (const from of [start, 0]) {
      const { results } = await db.prepare(
        `SELECT identity, ruler, mode, record, produced_by, clients, clients_nets FROM orders
          WHERE state = ? AND engine = ? AND priority = ? AND slot >= ? AND (lease_until IS NULL OR lease_until < ?)
          ORDER BY slot LIMIT ?`).bind(state, engine, priority, from, iso(now), n).all();
      if (results.length) return results;
    }
  }
  return [];
}

const owed = async (db, o) => !!(await db.prepare(
  "SELECT 1 AS x FROM queue WHERE build_id = ? AND ruler = ? AND mode = ? LIMIT 1").bind(o.identity, o.ruler, o.mode).first());

/// ONE ORDER TO FIGHT: a further result wanted first (never from a client
/// that measured it already), then a first one. Never the number to agree with.
async function work(request, env) {
  const { b, err } = await read(request);
  if (err) return err;
  if (!VERIFIER_ID.test(b.verifier || "") || !ENGINE_ID.test(b.engine || "")) return json({ ok: false, error: "bad request" }, 400);
  // AN OLDER PAGE IS TOLD SO (`stale`), or a machine left computing would ask
  // for ever and be given nothing once a release ships.
  if (b.protocol !== PROTOCOL) return json({ ok: true, work: null, stale: true });
  const { engine, release } = await servedRelease(env);
  if (!engine) return json({ ok: true, work: null });
  if (b.engine !== engine) return json({ ok: true, work: null, stale: true });
  const db = env.LIBRARY, now = Date.now();
  await retire(db, engine);
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
  await db.batch([
    db.prepare(`UPDATE orders SET ${done} WHERE leased_to = ? AND lease_until > ?`).bind(b.verifier, iso(now)),
    db.prepare("UPDATE appraisals SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE leased_to = ? AND lease_until > ?")
      .bind(b.verifier, iso(now)),
  ]);
  // A RIVEN GAIN FIRST: someone is waiting on it in a chat. It holds its own
  // lease, so a client on one gets nothing more here either.
  {
    // HOW MANY CORES IT CAN GIVE NOW (69-board-work.js `communityLanes`), which
    // decides whether a riven gain someone waits on is its to take.
    const lanes = Number.isInteger(b.lanes) && b.lanes > 0 && b.lanes <= 256 ? b.lanes : 1;
    const riven = await rivenTask(env, b.verifier, engine, (ids) => ownersOf(env, ids), lanes);
    if (riven) return json({ ok: true, release, work: riven });
  }
  // A FURTHER RESULT COMES FROM ANOTHER OWNER: one person's machines agreeing
  // with each other would be one witness counted twice.
  // …AND A WINDOW THAT HELD ONLY ITS OWN is not the end: up to three random
  // windows are read, so a client is never told "nothing" while a row it may
  // take waits further along.
  const net = await netOf(request, env);
  let mineOpen = [];
  for (let tries = 0; tries < 3 && !mineOpen.length; tries++) {
    const open = (await candidates(db, "open", engine, now, 8))
      .filter((o) => !clientsOf(o).includes(b.verifier) && !(net && netsOf(o).includes(net)));
    if (!open.length) continue;
    const owners = await ownersOf(env, [b.verifier, ...open.flatMap(clientsOf)]);
    const mine = owners.get(b.verifier);
    mineOpen = open.filter((o) => !mine || !clientsOf(o).some((c) => owners.get(c) === mine));
  }
  const pool = [
    ...mineOpen.map((o) => ({ ...o, state: "open" })),
    ...(await candidates(db, "todo", "", now, 4)).map((o) => ({ ...o, state: "todo" })),
  ];
  for (const o of pool) {
    const key = [o.identity, o.ruler, o.mode];
    // A ROW NOBODY OWES ANY MORE — the scorer measured it — is settled here,
    // where it is found, rather than by a sweep nobody runs.
    if (!(await owed(db, o))) {
      await db.prepare("UPDATE orders SET state = 'settled' WHERE identity = ? AND ruler = ? AND mode = ? AND state = ?")
        .bind(...key, o.state).run();
      continue;
    }
    const lease = [...crypto.getRandomValues(new Uint8Array(16))].map((x) => x.toString(16).padStart(2, "0")).join("");
    // THE RACE IS THE DATABASE'S: two clients reaching one order take it once —
    // and ONE CLIENT TAKES ONE ORDER, in the same statement, so two tabs of one
    // browser asking at once cannot both win (the check above reads first).
    const took = await db.prepare(
      `UPDATE orders SET lease = ?, lease_until = ?, leased_to = ?
        WHERE identity = ? AND ruler = ? AND mode = ? AND state = ? AND (lease_until IS NULL OR lease_until < ?)
          AND NOT EXISTS (SELECT 1 FROM orders h WHERE h.leased_to = ? AND h.lease_until > ?)
          AND NOT EXISTS (SELECT 1 FROM appraisals p WHERE p.leased_to = ? AND p.lease_until > ?)`)
      .bind(lease, iso(now + LEASE_MS), b.verifier, ...key, o.state, iso(now), b.verifier, iso(now), b.verifier, iso(now)).run();
    if (took.meta && took.meta.changes) {
      return json({ ok: true, release, work: { lease, identity: o.identity, record: JSON.parse(o.record), ruler: o.ruler, mode: o.mode } });
    }
  }
  return json({ ok: true, release, work: null });
}

/// WHAT THE CLIENT MEASURED. The answer is always `ok`: a client learns
/// nothing from it about whether it agreed.
async function verify(request, env) {
  const { b, err } = await read(request);
  if (err) return err;
  if (!LEASE_ID.test(b.lease || "") || !VERIFIER_ID.test(b.verifier || "") || !ENGINE_ID.test(b.engine || "")
      || !METRIC_ID.test(b.metric || "")
      || typeof b.score !== "number" || !Number.isFinite(b.score) || b.score < 0
      || !Number.isSafeInteger(b.work) || b.work < 0) {
    return json({ ok: false, error: "bad request" }, 400);
  }
  const db = env.LIBRARY, now = Date.now();
  // WHAT THE FIGHT COST THE CLIENT, as it says — a statistic about the clients'
  // machines and never a number anything is ranked or scheduled by.
  const ms = Number.isInteger(b.compute_ms) && b.compute_ms >= 0 && b.compute_ms <= LEASE_MS ? b.compute_ms : null;
  const engine = await servedEngine(env);
  if (!engine || b.engine !== engine) {
    await db.prepare(`UPDATE orders SET ${done} WHERE lease = ? AND leased_to = ?`).bind(b.lease, b.verifier).run();
    return json({ ok: true });
  }
  const o = await db.prepare(
    `SELECT identity, ruler, mode, state, engine, score, metric, work, produced_by, clients, clients_compute_ms, clients_nets, carried_from FROM orders
      WHERE lease = ? AND leased_to = ? AND lease_until >= ?`).bind(b.lease, b.verifier, iso(now)).first();
  if (!o) return json({ ok: true });
  const net = await netOf(request, env);
  const nets = [...new Set([...netsOf(o), net].filter(Boolean))].join(",");
  const key = [o.identity, o.ruler, o.mode];
  // …AND WHEN IT LAST ANSWERED, for its owner's device list, in the same write.
  const spent = db.prepare("UPDATE verifiers SET compute_ms = compute_ms + ?, last_at = ? WHERE id = ?")
    .bind(ms || 0, iso(now), b.verifier);
  // …AND THE RESULT COUNTED IN ITS HOUR, for the reader's own "today".
  await db.prepare(`INSERT INTO verifier_hours (verifier, hour, tasks, ms) VALUES (?, ?, 1, ?)
    ON CONFLICT (verifier, hour) DO UPDATE SET tasks = tasks + 1, ms = ms + excluded.ms`).bind(b.verifier, hour(), ms || 0).run();
  if (o.state === "todo") {
    if (needed(env) <= 1) {
      await fact(db, key, { ...o, score: b.score, metric: b.metric, work: b.work, engine: b.engine, produced_by: b.verifier },
        [b.verifier], [String(ms ?? "")], b.verifier, spent);
      return json({ ok: true });
    }
    // THE FIRST RESULT, which the server ranks before anyone may agree with it.
    await db.batch([
      db.prepare(`UPDATE orders SET state = 'fresh', score = ?, metric = ?, work = ?, engine = ?, produced_by = ?, clients = ?,
                  clients_compute_ms = ?, clients_nets = ?, carried_from = NULL, ${done} WHERE identity = ? AND ruler = ? AND mode = ? AND state = 'todo'`)
        .bind(b.score, b.metric, b.work, b.engine, b.verifier, b.verifier, String(ms ?? ""), net, ...key),
      spent,
    ]);
    return json({ ok: true });
  }
  if (o.state !== "open") return json({ ok: true });
  const clients = [...clientsOf(o), b.verifier];
  const compute = [...computeOf(o), String(ms ?? "")];
  const differs = b.engine !== o.engine || b.score !== o.score || b.metric !== o.metric || b.work !== o.work;
  // A CARRIED RESULT THE NEW ENGINE DOES NOT REPRODUCE WAS MOVED BY THE RELEASE,
  // not by a lie: the new result replaces it as the first, and nobody is refused.
  if (differs && o.carried_from) {
    await db.batch([
      db.prepare(`UPDATE orders SET state = 'fresh', score = ?, metric = ?, work = ?, engine = ?, produced_by = ?, verifier = NULL,
                  clients = ?, clients_compute_ms = ?, clients_nets = ?, carried_from = NULL, ${done} WHERE identity = ? AND ruler = ? AND mode = ? AND state = 'open'`)
        .bind(b.score, b.metric, b.work, b.engine, b.verifier, b.verifier, String(ms ?? ""), net, ...key),
      spent,
    ]);
    return json({ ok: true });
  }
  if (differs) {
    await db.batch([
      db.prepare(`UPDATE orders SET state = 'dispute', verifier = ?, disputed = ?, clients = ?, clients_compute_ms = ?, clients_nets = ?, ${done}
                  WHERE identity = ? AND ruler = ? AND mode = ?`)
        .bind(b.verifier, b.score, clients.join(","), compute.join(","), nets, ...key),
      spent,
    ]);
    return json({ ok: true });
  }
  if (clients.length < needed(env)) {
    await db.batch([
      db.prepare(`UPDATE orders SET clients = ?, clients_compute_ms = ?, clients_nets = ?, ${done} WHERE identity = ? AND ruler = ? AND mode = ?`)
        .bind(clients.join(","), compute.join(","), nets, ...key),
      db.prepare("UPDATE verifiers SET agreed = agreed + 1 WHERE id = ?").bind(b.verifier),
      spent,
    ]);
    return json({ ok: true });
  }
  await fact(db, key, o, clients, compute, b.verifier, spent);
  return json({ ok: true });
}

/// THE RESULT `clients` MEASURED, made a fact — stamped when the last of them
/// answered, which is when it was verified — and its work credited to each.
/// Its `cost_seconds` stays 0: the scorer plans its shards from that column in
/// ITS runners' seconds, and a browser's are another machine's
/// (`clients_compute_ms` keeps them).
async function fact(db, key, o, clients, compute, last, spent) {
  const state = Math.random() < SPOT_SHARE ? "spot" : "verified";
  const at = iso(Date.now());
  await db.batch([
    db.prepare(`UPDATE orders SET state = ?, score = ?, metric = ?, engine = ?, produced_by = ?, verifier = ?, clients = ?,
                clients_compute_ms = ?, clients_nets = '', ${done} WHERE identity = ? AND ruler = ? AND mode = ?`)
      .bind(state, o.score, o.metric, o.engine, o.produced_by, last, clients.join(","), compute.join(","), ...key),
    // THE CLIENTS' FACT IS THE BOARD'S, as the scorer's is: a row still owed —
    // a rescore asked for it again — takes the new number over the old, and one
    // no longer owed keeps the fact it has. Left alone, a rescore the volunteers
    // computed deleted its queue row and kept the old score.
    db.prepare(
      `INSERT INTO scores (identity, ruler, mode, measured_by, score, metric, cost_seconds, started_at, finished_at)
       VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?)
       ON CONFLICT (identity, ruler, mode) DO UPDATE SET measured_by = excluded.measured_by, score = excluded.score,
         metric = excluded.metric, cost_seconds = 0, started_at = excluded.started_at, finished_at = excluded.finished_at
       WHERE EXISTS (SELECT 1 FROM queue q WHERE q.build_id = scores.identity AND q.ruler = scores.ruler AND q.mode = scores.mode)`)
      .bind(...key, `verified:${o.engine}`, o.score, o.metric, at, at),
    // …AND THE ROW IS NO LONGER OWED, the same delete `ship_facts.sh` makes.
    db.prepare("DELETE FROM queue WHERE build_id = ? AND ruler = ? AND mode = ?").bind(...key),
    db.prepare("UPDATE verifiers SET agreed = agreed + 1 WHERE id = ?").bind(last),
    ...clients.map((c) => db.prepare("UPDATE verifiers SET work = work + ? WHERE id = ?").bind(o.work || 0, c)),
    // Every expression reads the row as it was, so the count and the forgiving
    // agree: the thousandth fact takes a refusal off and starts the count again.
    ...clients.map((c) => db.prepare(`UPDATE verifiers SET
        refusals = CASE WHEN refusals > 0 AND clean + 1 >= refusals * ? THEN refusals - 1 ELSE refusals END,
        clean = CASE WHEN refusals = 0 OR clean + 1 >= refusals * ? THEN 0 ELSE clean + 1 END WHERE id = ?`)
      .bind(FACTS_PER_REFUSAL_FORGIVEN, FACTS_PER_REFUSAL_FORGIVEN, c)),
    ...clients.map((c) => db.prepare(
      `INSERT INTO verifier_hours (verifier, hour, work) VALUES (?, ?, ?)
       ON CONFLICT (verifier, hour) DO UPDATE SET work = work + excluded.work`).bind(c, hour(), o.work || 0)),
    spent,
  ]);
}

/// A LEASE GIVEN BACK by a page that will not answer — closed, reloaded, paused,
/// its fight refused, its lease about to run out. The order is anyone's again at
/// once and this client may take another, instead of both waiting `LEASE_MS`:
/// every release reloads every page that is computing, so that wait was paid
/// by every computer at once.
async function release(request, env) {
  const { b, err } = await read(request);
  if (err) return err;
  if (!LEASE_ID.test(b.lease || "") || !VERIFIER_ID.test(b.verifier || "")) return json({ ok: false, error: "bad request" }, 400);
  await env.LIBRARY.prepare(`UPDATE orders SET ${done} WHERE lease = ? AND leased_to = ?`).bind(b.lease, b.verifier).run();
  return json({ ok: true });
}

/// WHERE EACH OF A BROWSER'S RECENT RESULTS STANDS, asked by its own secret id
/// for the orders it names (73-compute.js, the recent tasks): `waiting` for a
/// client of another owner and network, `checking` while the server settles a
/// difference, `confirmed` once a fact counts it — with when — and `gone` when
/// the row is no longer its to earn. It says nothing about anyone else.
const MINE_MAX = 60;
async function mine(request, env) {
  const { b, err } = await read(request);
  if (err) return err;
  if (!VERIFIER_ID.test(b.verifier || "") || !Array.isArray(b.orders)) return json({ ok: false, error: "bad request" }, 400);
  const keys = b.orders.slice(0, MINE_MAX).filter((k) => k && typeof k.identity === "string" && typeof k.ruler === "string" && typeof k.mode === "string");
  const db = env.LIBRARY, out = [];
  for (let i = 0; i < keys.length; i += 30) {
    const part = keys.slice(i, i + 30);
    const { results } = await db.prepare(
      `SELECT o.identity, o.ruler, o.mode, o.state, o.clients, s.finished_at FROM orders o
        LEFT JOIN scores s ON s.identity = o.identity AND s.ruler = o.ruler AND s.mode = o.mode
        WHERE (o.identity, o.ruler, o.mode) IN (VALUES ${part.map(() => "(?, ?, ?)").join(", ")})`)
      .bind(...part.flatMap((k) => [k.identity, k.ruler, k.mode])).all();
    for (const r of results) {
      const counted = clientsOf(r).includes(b.verifier);
      const state = !counted ? "gone"
        : ["verified", "spot"].includes(r.state) ? "confirmed"
        : r.state === "dispute" ? "checking"
        : ["fresh", "open", "scoring:open"].includes(r.state) ? "waiting" : "gone";
      out.push({ identity: r.identity, ruler: r.ruler, mode: r.mode, state, ...(state === "confirmed" && r.finished_at ? { at: r.finished_at } : {}) });
    }
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
