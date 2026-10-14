// PROTOCOL 8'S SYNC AS ONE CALL (dial `sync_proc`, worker/pg/sync.sql
// `board_sync`) answers and writes what the worker's statements do: on random
// books, with answers settled in the request and queued for the settler alike,
// two databases are handed the same syncs in the same order — one through the
// procedure, one through the statements — and every reply and every table must
// match, leases named by the order they were first handed out. Then the ratchet:
// through the procedure a sync is at most two waits on a database — the call,
// and the accounts' read of the device's span — where the statements are many.
//   node scripts/check_sync_proc.mjs
import { syncRoute } from "../worker/sync.js";
import { boardTasksRoute } from "../worker/board_tasks.js";
import { facts } from "../worker/factory.js";
import { forgetDials } from "../worker/dials.js";
import { testDb } from "./pg/test_db.mjs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

// ONE CLOCK, moved by the check, and dice that never change, so both paths see one world.
let T = Date.parse("2026-10-14T06:00:00.000Z");
Date.now = () => T;
Math.random = () => 0.5;
const iso = (t) => new Date(t).toISOString();
const rng = (seed) => () => {
  seed = (seed + 0x6d2b79f5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};

// THE FAIR TABLE, the same for both: one account over its share, so a short
// sync of its devices is handed nothing (worker/fair.js `throttled`, dice 0.5).
const TABLE = { acc1: { target: 0.2, share: 0.6, ratio: 3 }, acc2: { target: 0.5, share: 0.5, ratio: 1 }, "walk-ins": { target: 0.3, share: 0.3, ratio: 1 } };
globalThis.caches = { default: { match: async () => new Response(JSON.stringify(TABLE)), put: async () => {} } };
// THE ENGINE SERVED, an older one still accepted on all but one weapon, and one joining only.
const ASSETS = { fetch: async () => new Response(JSON.stringify({ engine: "e1", release: "r1", accepted: [
  { engine: "eg", until: "2027-01-01T00:00:00.000Z", changed: ["lex"] }, { engine: "ej", until: "2027-01-01T00:00:00.000Z", join: true }] })) };
const CALLS = { made: 0, js: 0 };
const DEVICES = ["a", "b", "c", "d", "e", "f"].map((x) => x.repeat(24));
const IPS = ["203.0.113.1", "203.0.113.1", "198.51.100.2", "192.0.2.3", "203.0.113.4", "198.51.100.5"];
const WEAPONS = ["braton_prime", "lex", "torid", "soma"];

async function world(proc, settle) {
  const L = await testDb("library"), A = await testDb("accounts");
  const env = { LIBRARY: L, ACCOUNTS: A, FACTORY_TOKEN: "ft", ASSETS };
  // EACH CALL COUNTED, and each one the procedure handed back to the worker (WS001).
  const prepare = L.prepare, counted = (s) => ({ ...s, bind: (...a) => counted(s.bind(...a)),
    first: () => { CALLS.made++; return s.first().catch((e) => { if (e && e.code === "WS001") CALLS.js++; throw e; }); } });
  L.prepare = (sql) => (/^CALL board_sync/.test(sql) ? counted(prepare(sql)) : prepare(sql));
  for (const [name, v] of [["sync_proc", proc], ["settle_async", settle]]) {
    await L.run("INSERT INTO factory_dials (name, value, set_by, set_at) VALUES (?, ?, 'check', '2026-01-01T00:00:00.000Z')", name, v);
  }
  await A.run("INSERT INTO accounts (id, created_at) VALUES ('acc1', '2026-01-01T00:00:00.000Z'), ('acc2', '2026-01-01T00:00:00.000Z')");
  await A.run(`INSERT INTO devices (verifier, account, claimed_at) VALUES (?, 'acc1', '2026-01-01'), (?, 'acc1', '2026-01-01'), (?, 'acc2', '2026-01-01')`,
    DEVICES[0], DEVICES[1], DEVICES[2]);
  // A DEVICE AN ACCOUNT HOLDS NOW, and one an account held once.
  await A.run(`INSERT INTO device_spans (verifier, account, from_hour, until_hour) VALUES (?, 'acc1', '2026-10-01T00', NULL),
    (?, 'acc2', '2026-10-01T00', '2026-10-14T03')`, DEVICES[0], DEVICES[2]);
  return { env, L };
}
const post = (env, route, path, body, ip = "") => route(new Request(`https://x${path}`, { method: "POST",
  headers: { authorization: "Bearer ft", ...(ip ? { "cf-connecting-ip": ip } : {}) }, body: JSON.stringify(body) }), env, path).then((r) => r.json());

/// THE SAME BOOKS IN EVERY WORLD: rows owed and asked, some waited on, one asked by a riven chat too.
async function books(w, rand, n) {
  const batch = "arrivals-2026-10-14";
  await w.L.run("INSERT INTO batches (id, at, why, total) VALUES (?, ?, 'test', 0)", batch, batch);
  const ids = [];
  for (let i = 0; i < n; i++) {
    const id = `b${String(i).padStart(3, "0")}`;
    ids.push(id);
    await w.L.run("INSERT INTO builds (id, at, record) VALUES (?, '2026-10-14T00:00:00.000Z', ?)", id, JSON.stringify({ weapon: WEAPONS[i % 4], mods: [id] }));
    for (const ruler of ["standard_single_target", "aoe"]) {
      if (ruler === "aoe" && rand() < 0.5) continue;
      await w.L.run("INSERT INTO queue (batch, build_id, ruler, mode) VALUES (?, ?, ?, 'base')", batch, id, ruler);
    }
  }
  const rows = (await w.L.all("SELECT build_id, ruler, mode FROM queue ORDER BY build_id, ruler")).map((r) => ({ identity: r.build_id, ruler: r.ruler, mode: r.mode }));
  await post(w.env, boardTasksRoute, "/api/board/ask", { batch, rows });
  // …AND A THIRD OF THEM ASKED AGAIN BY A REFRESH: one question, two producers' demands.
  const again = "refresh-2026-10-14";
  await w.L.run("INSERT INTO batches (id, at, why, total) VALUES (?, ?, 'test', 0)", again, again);
  const some = rows.filter((_, i) => i % 3 === 0);
  for (const r of some) await w.L.run("INSERT INTO queue (batch, build_id, ruler, mode) VALUES (?, ?, ?, 'base')", again, r.identity, r.ruler);
  await post(w.env, boardTasksRoute, "/api/board/ask", { batch: again, rows: some });
}

const SORT = {
  questions: "id", demands: "producer, ref", answers: "id", settle_queue: "question, engine, peer", verifiers: "id",
  verifier_asks: "verifier, hour", verifier_hours: "verifier, hour", producer_use: "producer", engine_refusals: "question, device, engine",
  refusal_withdrawals: "question", scores: "identity, ruler, mode", queue: "batch, build_id, ruler, mode",
};
/// EVERY LEASE NAMED by the order this world first handed it out; the client
/// keeps the names, and each world is sent its own lease under each.
function namer() {
  const names = new Map(), raws = new Map();
  const name = (l) => {
    if (l == null) return l;
    if (!names.has(l)) { names.set(l, `L${names.size}`); raws.set(names.get(l), l); }
    return names.get(l);
  };
  const walk = (v, k) => (Array.isArray(v) ? v.map((x) => walk(x, k)) : v && typeof v === "object"
    ? Object.fromEntries(Object.entries(v).map(([key, x]) => [key, walk(x, key)])) : /^(lease|lease_prev|claim)$/.test(k || "") ? name(v) : v);
  const rawOf = (v, k) => (Array.isArray(v) ? v.map((x) => rawOf(x, k)) : v && typeof v === "object"
    ? Object.fromEntries(Object.entries(v).map(([key, x]) => [key, rawOf(x, key)])) : k === "lease" ? raws.get(v) ?? v : v);
  return Object.assign((v) => walk(v), { rawOf: (v) => rawOf(v) });
}
// AN ANSWER'S ID IS ITS PLACE, not its number: a call handed back to the worker
// used up the numbers it drew, as every rolled-back insert does.
const tables = async (w, name) => Object.fromEntries(await Promise.all(Object.entries(SORT).map(async ([t, by]) =>
  [t, name((await w.L.all(`SELECT * FROM ${t} ORDER BY ${by}`)).map(({ id, ...r }) => (t === "answers" ? r : { id, ...r })))])));
const firstDiff = (a, b, path = "") => {
  if (JSON.stringify(a) === JSON.stringify(b)) return null;
  if (a && b && typeof a === "object" && typeof b === "object") {
    for (const k of new Set([...Object.keys(a), ...Object.keys(b)])) {
      const d = firstDiff(a[k], b[k], `${path}.${k}`);
      if (d) return d;
    }
  }
  return `${path}: ${JSON.stringify(a)?.slice(0, 300)} ≠ ${JSON.stringify(b)?.slice(0, 300)}`;
};

/// ONE RUN: the same syncs to both worlds, compared reply by reply and table by table.
async function run(seed, settle, steps) {
  const rand = rng(seed), pick = (xs) => xs[Math.floor(rand() * xs.length)];
  T = Date.parse("2026-10-14T06:00:00.000Z");
  const worlds = [await world(0, settle), await world(1, settle)], names = [namer(), namer()];
  for (const w of worlds) await books(w, rng(seed), 24);
  // A ROW SOMEONE WAITS ON goes alone; a row a riven chat asks too is judged in JS.
  for (const w of worlds) {
    await w.L.run("UPDATE questions SET waits = 1 WHERE id = (SELECT id FROM questions ORDER BY id OFFSET 3 LIMIT 1)");
    const q = (await w.L.get("SELECT id, due_at FROM questions ORDER BY id OFFSET 7 LIMIT 1"));
    await w.L.run("INSERT INTO demands (producer, ref, question, state, asked_at, due_at, waits, payload) VALUES ('chat', 'riv1', ?, 'live', ?, ?, 0, '{}')",
      q.id, iso(T), q.due_at);
    await w.L.run("UPDATE questions SET producer = 'chat' WHERE id = ?", q.id);
  }
  const clients = new Map(DEVICES.map((v) => [v, { packs: new Map(), sent: [], given: [] }]));
  let syncs = 0, packs = 0, results = 0, refused = 0, dropped = 0, stale = 0, early = 0, idle = 0, alone = 0;
  for (let step = 0; step < steps; step++) {
    // THE WORLD MOVES: time, a device let back early, a row the scorer measured, a settler pass.
    T += Math.floor(rand() * 240_000);
    const k = Math.floor(rand() * DEVICES.length), v = DEVICES[k], me = clients.get(v);
    if (rand() < 0.5) for (const w of worlds) await w.L.run("UPDATE verifiers SET next_at = NULL WHERE id = ?", v);
    if (rand() < 0.12) {
      const owed = await worlds[0].L.get("SELECT build_id, ruler FROM queue ORDER BY build_id, ruler OFFSET ? LIMIT 1", Math.floor(rand() * 10));
      if (owed) for (const w of worlds) await w.L.run("DELETE FROM queue WHERE build_id = ? AND ruler = ?", owed.build_id, owed.ruler);
    }
    if (settle && rand() < 0.3) for (const w of worlds) await facts().settleQueued(w.env, T);
    // WHAT THE CLIENT SAYS: results for some rows it holds, a refusal, the rest held, some forgotten.
    const body = { verifier: v, engine: rand() < 0.08 ? pick(["e0", "eg", "ej"]) : "e1", protocol: 8,
      consent: rand() < 0.95 ? { v: 1, at: "2026-10-08T08:00:00.000Z" } : null, lanes: 1 + Math.floor(rand() * 8),
      want_ms: rand() < 0.05 ? 0 : 60_000 + Math.floor(rand() * 600_000), ...(rand() < 0.7 ? { pace_ms: 500 + Math.floor(rand() * 4000) } : {}),
      results: [], held: [], refused: [] };
    for (const [lease, rows] of me.packs) {
      const keep = [];
      for (const q of rows) {
        const r = rand();
        if (r < 0.45) body.results.push({ lease, question: q, score: rand() < 0.1 ? k + 0.5 : (parseInt(q.slice(0, 6), 16) % 1000) / 8, metric: "kpm",
          work: 1_000_000 + (parseInt(q.slice(6, 12), 16) % 1000), compute_ms: Math.floor(rand() * 3000), ...(rand() < 0.3 ? { counts: { fights: 3 } } : {}),
          ...(rand() < 0.3 ? { lanes: 2 } : {}) });
        else if (r < 0.48) body.refused.push({ lease, question: q, step: "order" });
        else if (r < 0.9) keep.push(q);
        // A ROW SENT AS DONE AND NAMED AS HELD TOO is renewed by neither path.
        if (r < 0.48 && rand() < 0.2) keep.push(q);
      }
      if (keep.length) body.held.push({ lease, rows: keep });
    }
    if (me.sent.length && rand() < 0.3) body.results.push(...me.sent.slice(-2));
    if (rand() < 0.1 && me.given.length) body.keep = { lease: me.given.at(-1).lease, question: me.given.at(-1).question };
    if (me.given.length) body.states = me.given.slice(-5).map((x) => x.question);
    if (rand() < 0.6) body.since = rand() < 0.5 ? "2026-10-14T00" : "yesterday";
    if (rand() < 0.2) body.idle = [{ hour: iso(T).slice(0, 13), asks: 2, failed: 1, lanes: 3 }];
    me.sent = body.results.slice();

    const replies = [];
    for (const [i, w] of worlds.entries()) {
      forgetDials();
      const got = await post(w.env, syncRoute, "/api/board/sync", names[i].rawOf(body), IPS[k]);
      replies.push(names[i](got));
    }
    syncs++;
    const diff = firstDiff(replies[0], replies[1]);
    if (diff) { check(`seed ${seed}, settle_async ${settle}: sync ${step} answers alike`, false, diff); return; }
    const r = replies[0];
    if (!r.ok) { check(`seed ${seed}: sync ${step} is answered`, false, JSON.stringify(r).slice(0, 300)); return; }
    // THE CLIENT LEARNS from the reply — held rows it lost, a new pack.
    for (const h of r.held || []) if (!h.held) for (const [l, rows] of me.packs) me.packs.set(l, rows.filter((q) => q !== h.question));
    const named = new Set(body.held.flatMap((h) => h.rows));
    for (const [l, rows] of me.packs) {
      const left = rows.filter((q) => named.has(q) && !(r.held || []).some((h) => h.question === q && !h.held));
      if (left.length) me.packs.set(l, left); else me.packs.delete(l);
    }
    if (r.pack) {
      me.packs.set(r.pack.lease, r.pack.tasks.map((t) => t.question));
      for (const t of r.pack.tasks) me.given.push({ lease: r.pack.lease, question: t.question });
      packs++;
    }
    results += (r.taken || []).length; dropped += (r.dropped || []).length; refused += body.refused.length;
    stale += r.stale ? 1 : 0; early += r.early ? 1 : 0; idle += !r.pack && !r.work ? 1 : 0; alone += r.work ? 1 : 0;
  }
  const [a, b] = [await tables(worlds[0], names[0]), await tables(worlds[1], names[1])];
  const diff = firstDiff(a, b);
  check(`seed ${seed}, settle_async ${settle}: ${syncs} syncs (${packs} packs, ${results} results taken, ${dropped} dropped, ${refused} refused, `
    + `${stale} stale, ${early} early, ${idle} without work) leave every table alike`, !diff, diff || "");
  return a;
}

console.log("the procedure and the statements, on random books");
for (const settle of [0, 1]) for (const seed of [11, 29, 47]) await run(seed, settle, 160);
check(`the runs went through the call (${CALLS.made}) and, where it needs JS, through the worker (${CALLS.js})`, CALLS.made > 500 && CALLS.js > 0);

console.log("\nround trips");
async function measured(proc) {
  T = Date.parse("2026-10-15T06:00:00.000Z");
  forgetDials();
  const w = await world(proc, 1);
  await books(w, rng(5), 12);
  const v = DEVICES[3], body = { verifier: v, engine: "e1", protocol: 8, consent: { v: 1, at: "2026-10-08T08:00:00.000Z" }, lanes: 4, want_ms: 600_000, pace_ms: 60_000 };
  const first = await post(w.env, syncRoute, "/api/board/sync", body, IPS[3]);
  const rows = first.pack.tasks.map((t) => t.question);
  const lease = (await w.L.get("SELECT lease FROM questions WHERE id = ?", rows[0])).lease;
  await w.L.run("UPDATE verifiers SET next_at = NULL WHERE id = ?", v);
  T += 30_000;
  const before = w.env.LIBRARY.cost.calls + w.env.ACCOUNTS.cost.calls;
  const r = await post(w.env, syncRoute, "/api/board/sync", { ...body, since: "2026-10-15T00", states: rows.slice(0, 2),
    results: rows.slice(0, 3).map((q) => ({ lease, question: q, score: 1.5, metric: "kpm", work: 1_000_000, compute_ms: 900 })),
    held: [{ lease, rows: rows.slice(3) }] }, IPS[3]);
  return { calls: w.env.LIBRARY.cost.calls + w.env.ACCOUNTS.cost.calls - before, r };
}
const viaProc = await measured(1), viaStatements = await measured(0);
check(`through the procedure a sync with results, renewals, states and points is at most 2 waits on a database (${viaProc.calls})`,
  viaProc.calls <= 2 && viaProc.r.taken.length === 3 && !!viaProc.r.points && !!viaProc.r.pack,
  JSON.stringify({ ...viaProc.r, held: undefined, dials: undefined }).slice(0, 600));
check(`…where the statements one by one are many (${viaStatements.calls}): the count counts`, viaStatements.calls > 2);

console.log(failures ? `\n${failures} failed` : "\nthe procedure answers and writes what the statements do, in one call");
process.exit(failures ? 1 : 0);
