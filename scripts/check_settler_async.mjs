// THE SETTLER'S QUEUE (dial `settle_async`; worker/tasks.js `recordMany`,
// `settleQueued`; docs/BOARD.md §"The settler"): answers queued and settled by
// the settler's pass leave exactly what settling them in the request leaves; a
// pass that dies part way is finished by the next and counts once; a queued
// question is handed to nobody for the grace, and to anyone after it.
//   node scripts/check_settler_async.mjs
import { factory } from "../worker/tasks.js";
import { VERBS } from "../worker/factory.js";
import { BOARD_PRODUCERS } from "../worker/board_tasks.js";
import { forgetDials, DIAL_DEFAULTS } from "../worker/dials.js";
import { iso } from "../worker/instant.js";
import { testDb } from "./pg/test_db.mjs";

// THIS CHECK SETS THE DIAL ITSELF, on each side: a pass at once (scripts/pg/test_db.mjs) is not what it asks.
delete process.env.WFSIM_SETTLE_ASYNC;

let failures = 0;
const check = (name, ok, detail = "") => {
  console.log(`  ${ok ? "ok  " : "FAIL"}  ${name}${ok ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

// A seeded generator, so a failing trial can be run again by its number.
const rng = (seed) => () => {
  seed = (seed + 0x6D2B79F5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const pick = (r, xs) => xs[Math.floor(r() * xs.length)];
const NOW = Date.parse("2026-10-14T10:00:00.000Z"), WORK = 7_000_000_000, RULER = "standard_single_target";

/// A DATABASE, its factory and whether its answers are queued.
async function side(seed, queued, random = rng(seed)) {
  const db = await testDb("library"), env = { LIBRARY: db };
  await db.run("INSERT INTO factory_dials (name, value, set_by, set_at) VALUES ('settle_async', ?, 'check', ?) ON CONFLICT (name) DO UPDATE SET value = excluded.value",
    queued ? 1 : 0, iso(NOW));
  const f = factory({ verbs: VERBS, producers: BOARD_PRODUCERS, threshold: 2, spot: 0.2, recheck: 0.3, random });
  return { db, env, f };
}

/// THE SAME BOOK on any side: `n` board rows of one batch, each owed and asked.
async function book({ db, env, f }, n) {
  await db.run("INSERT INTO batches (id, at, why, total) VALUES ('arrivals-x', ?, 'check', 0)", iso(NOW));
  const ids = [];
  for (let i = 0; i < n; i++) {
    const identity = `b${String(i).padStart(3, "0")}`;
    await db.run("INSERT INTO builds (id, at, record) VALUES (?, ?, ?)", identity, iso(NOW), JSON.stringify({ weapon: "braton_prime", mods: [identity] }));
    await db.run("INSERT INTO queue (batch, build_id, ruler, mode) VALUES ('arrivals-x', ?, ?, 'base')", identity, RULER);
    forgetDials();
    ids.push(await f.demand(env, { producer: "new_build", ref: `${identity}|${RULER}|base`, verb: "simulate", request: { identity, i },
      due_in_ms: 6 * 3_600_000, payload: { identity, ruler: RULER, mode: "base", batch: "arrivals-x" } }, "e1", NOW));
  }
  return ids;
}

/// THE ANSWERS A BOOK IS GIVEN, as a list both sides replay.
function plan(r, ids, events) {
  const owners = ["o1", "o2", "o3", "o4", "o5"], nets = ["", "n1", "n2", "n3"];
  const people = Array.from({ length: 7 }, (_, i) => ({ device: `d${i}`, owner: pick(r, owners), net: pick(r, nets), liar: r() < 0.2 }));
  const truth = new Map(ids.map((id) => [id, 1 + Math.floor(r() * 50)]));
  return Array.from({ length: events }, () => {
    const w = pick(r, people), q = pick(r, ids);
    return { w, q, score: truth.get(q) + (w.liar ? 1 : 0) };
  });
}

/// ONE ANSWER, under a lease the check hands it, as a page would send it.
async function give({ db, env, f }, e, k, now) {
  const lease = `l${String(k).padStart(5, "0")}`;
  await db.run("UPDATE questions SET lease = ?, leased_to = ?, lease_until = ? WHERE id = ?", lease, e.w.device, iso(now + 600_000), e.q);
  forgetDials();
  await f.answerPack(env, { device: e.w.device, owner: e.w.owner, net: e.w.net },
    { lease, engine: "e1", answers: [{ question: e.q, result: { score: e.score, metric: "kpm" }, work: WORK, compute_ms: 1000 }] }, now);
}

/// WHAT A SETTLEMENT LEAVES, the instants the settlement wrote left out where a
/// side settles later (`times: false`).
async function state(db, times = true) {
  const t = (x) => (times ? x : null);
  const qs = (await db.all("SELECT * FROM questions ORDER BY id")).map((q) => ({ id: q.id, state: q.state, canon: q.canon, needs: q.needs,
    fact_engine: q.fact_engine, fact_at: t(q.fact_at), lease: q.lease, leased_to: q.leased_to }));
  return JSON.stringify([qs,
    await db.all("SELECT question, device, engine, canon, credited, void FROM answers ORDER BY question, device, id"),
    await db.all("SELECT id, work, agreed, clean, refusals FROM verifiers ORDER BY id"),
    await db.all("SELECT verifier, hour, tasks, ms, work FROM verifier_hours ORDER BY verifier, hour"),
    await db.all("SELECT identity, ruler, mode, measured_by, score, metric FROM scores ORDER BY identity, ruler, mode"),
    await db.all("SELECT batch, build_id, ruler, mode FROM queue ORDER BY batch, build_id"),
    (await db.all("SELECT producer, ref, question, state, served_at FROM demands ORDER BY producer, ref")).map((d) => ({ ...d, served_at: t(d.served_at) }))]);
}
const queueLeft = async (db) => (await db.get("SELECT count(*) AS n FROM settle_queue")).n;

console.log("F1: queued and settled by the settler's pass, as settled in the request");
{
  let bad = 0, first = "", queuedAtAll = 0;
  for (let t = 0; t < 12 && bad < 3; t++) {
    const seed = 101 + t, a = await side(seed, false), b = await side(seed, true);
    const ids = await book(a, 6 + (t % 5)); await book(b, 6 + (t % 5));
    const events = plan(rng(seed + 50), ids, 25 + t * 3);
    let now = NOW + 1000;
    for (let k = 0; k < events.length; k++) {
      await give(a, events[k], k, now);
      await give(b, events[k], k, now);
      queuedAtAll += await queueLeft(b.db);
      forgetDials();
      await b.f.settleQueued(b.env, now);
      now += 1000;
    }
    const [x, y] = [await state(a.db), await state(b.db)];
    if (x !== y || await queueLeft(b.db)) { bad++; first ||= `trial ${t}: ${x.slice(0, 160)} … vs … ${y.slice(0, 160)}`; }
    await a.db.close(); await b.db.close();
  }
  check("on 12 random books, every answer queued then settled by a pass leaves the request path's questions, credits, scores, queue and demands",
    bad === 0, first);
  check("…and the queue was used: each answer waited in it for its pass", queuedAtAll > 0, String(queuedAtAll));
}

console.log("\na pass that dies part way is finished by the next, and counts once");
{
  // THE REFERENCE: the same answers, every one queued, one clean pass after them;
  // no draw for spot or recheck, as the dead pass would have spent some of them.
  const never = () => 0.999;
  const seed = 404, events = async (s) => { const ids = await book(s, 8); return plan(rng(seed + 50), ids, 40); };
  const ref = await side(seed, true, never), refEvents = await events(ref);
  for (let k = 0; k < refEvents.length; k++) await give(ref, refEvents[k], k, NOW + 1000 + k * 1000);
  forgetDials();
  await ref.f.settleQueued(ref.env, NOW + 200_000);
  const want = await state(ref.db, false);
  let bad = 0, first = "", died = 0;
  for (let at = 1; at <= 60; at++) {
    const s = await side(seed, true, never), es = await events(s);
    for (let k = 0; k < es.length; k++) await give(s, es[k], k, NOW + 1000 + k * 1000);
    // A HANDLE THAT DIES at its `at`-th call, whatever the call is.
    let n = 0;
    const tick = () => { if (++n === at) throw new Error(`died at call ${at}`); };
    const wrap = (st) => ({ ...st, bind: (...x) => wrap(st.bind(...x)), first: async () => { tick(); return st.first(); },
      all: async () => { tick(); return st.all(); }, run: async () => { tick(); return st.run(); } });
    const dying = { LIBRARY: Object.assign(Object.create(s.db), { prepare: (sql) => wrap(s.db.prepare(sql)), batch: async (l) => { tick(); return s.db.batch(l); } }) };
    forgetDials();
    let finished = true; // the dials read swallows its failure (worker/dials.js), so a pass may outlive its kill
    try { await s.f.settleQueued(dying, NOW + 200_000); } catch (_) { finished = false; died++; }
    // …AND THE NEXT PASS, once the dead one's claim has gone stale, and the watchdog
    // after it, which pays and tells what a settlement committed but never did.
    forgetDials();
    await s.f.settleQueued(s.env, NOW + 200_000 + 61_000);
    await s.f.redeliver(s.env, NOW + 200_000 + 11 * 60_000);
    await s.f.repay(s.env, NOW + 200_000 + 11 * 60_000);
    const got = await state(s.db, false);
    if (got !== want || await queueLeft(s.db)) { bad++; { let i = 0; while (got[i] === want[i]) i++; first ||= `died at call ${at}: …${got.slice(i - 150, i + 150)} … want …${want.slice(i - 150, i + 150)}`; } }
    await s.db.close();
    if (finished && n < at) break;
  }
  check(`a pass dying at each of its ${died} calls in turn leaves, once the next has run, what one clean pass leaves`, bad === 0 && died > 3, first || `died ${died}`);
  // SETTLED TWICE: every question queued again, and a pass over them all.
  const before = await state(ref.db, false);
  await ref.db.run("INSERT INTO settle_queue (question, engine, peer, joined, queued_at) SELECT id, 'e1', '', 0, ? FROM questions ON CONFLICT DO NOTHING",
    iso(NOW + 300_000));
  forgetDials();
  await ref.f.settleQueued(ref.env, NOW + 400_000);
  check("a question settled again changes nothing: no credit is paid twice", (await state(ref.db, false)) === before && !(await queueLeft(ref.db)));
  await ref.db.close();
}

console.log("\nqueued, a question waits for its pass: handed to nobody for the grace, to anyone after it");
{
  const s = await side(9, true), [q] = await book(s, 1);
  await give(s, { w: { device: "d1", owner: "o1", net: "n1" }, q, score: 5 }, 0, NOW + 1000);
  check("an answer to a board row is queued, not settled in the request", (await queueLeft(s.db)) === 1
    && (await s.db.get("SELECT state FROM questions WHERE id = ?", q)).state === "open");
  // ITS HOLDER ASKS AGAIN, which gives back every lease it held (worker/verify.js).
  await s.db.run("UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE id = ?", q);
  const other = { device: "d2", owner: "o2", net: "n2", siblings: ["d2"], lanes: 4 };
  forgetDials();
  check("within the grace, nobody else is handed it", (await s.f.next(s.env, other, "e1", NOW + 2000)) === null);
  forgetDials();
  const after = await s.f.next(s.env, other, "e1", NOW + 1000 + DIAL_DEFAULTS.settle_grace_ms + 1000);
  check("past the grace, with no pass come, it is anyone's again", !!after && after.question === q, JSON.stringify(after));
  // A PASS HOLDING THE ROW when another answer lands: the answer re-arms it.
  await s.db.run("UPDATE settle_queue SET claim = 'x', claimed_at = ? WHERE question = ?", iso(NOW + 3000), q);
  await s.db.run("UPDATE questions SET lease = 'l2', leased_to = 'd2', lease_until = ? WHERE id = ?", iso(NOW + 600_000), q);
  forgetDials();
  await s.f.answerPack(s.env, { device: "d2", owner: "o2", net: "n2" },
    { lease: "l2", engine: "e1", answers: [{ question: q, result: { score: 5, metric: "kpm" }, work: WORK }] }, NOW + 4000);
  check("an answer landing on a row a pass holds clears the claim, so that pass leaves it queued",
    (await s.db.get("SELECT claim FROM settle_queue WHERE question = ?", q)).claim === null);
  forgetDials();
  await s.f.settleQueued(s.env, NOW + 5000);
  check("…and the next pass settles both answers: the fact, its witnesses paid, the holder's lease given back",
    (await s.db.get("SELECT state, leased_to FROM questions WHERE id = ?", q)).leased_to === null
    && ["fact", "spot", "open"].includes((await s.db.get("SELECT state FROM questions WHERE id = ?", q)).state)
    && (await s.db.get("SELECT count(*) AS n FROM answers WHERE question = ? AND credited = 1", q)).n
      === ((await s.db.get("SELECT state FROM questions WHERE id = ?", q)).state === "open" ? 0 : 2) && !(await queueLeft(s.db)));
  await s.db.close();
}

console.log("\nwhat stays in the request");
{
  // A PRODUCER TOLD EACH ANSWER (`on_answer`) is told in the request, so its answers are not queued.
  const db = await testDb("library"), env = { LIBRARY: db }, told = [];
  await db.run("INSERT INTO factory_dials (name, value, set_by, set_at) VALUES ('settle_async', 1, 'check', ?)", iso(NOW));
  const f = factory({ verbs: VERBS, producers: { ...BOARD_PRODUCERS, chat: { on_answer: async (_, d) => { told.push(d.ref); }, on_fact: async () => {} } }, random: () => 1 });
  forgetDials();
  const q = await f.demand(env, { producer: "chat", ref: "c1", verb: "simulate", request: { c: 1 }, due_in_ms: 60_000 }, "e1", NOW);
  await db.run("UPDATE questions SET lease = 'l1', leased_to = 'd1', lease_until = ? WHERE id = ?", iso(NOW + 600_000), q);
  forgetDials();
  await f.answerPack(env, { device: "d1", owner: "o1", net: "n1" }, { lease: "l1", engine: "e1", answers: [{ question: q, result: { score: 1, metric: "kpm" }, work: 1 }] }, NOW + 1000);
  check("an answer whose producer is told each answer is settled in the request, and told", !(await queueLeft(db)) && told.length === 1);
  // THE OFFICIAL MACHINES' ANSWER is the fact at once.
  const o = await f.demand(env, { producer: "new_build", ref: "o1", verb: "simulate", request: { o: 1 }, due_in_ms: 60_000, payload: { identity: "x", ruler: RULER, mode: "base" } }, "e1", NOW);
  forgetDials();
  await f.answerOfficial(env, { device: "official:scorer", owner: "official:scorer" }, { question: o, engine: "e1", result: { score: 1, metric: "kpm" }, work: 1 }, NOW + 2000);
  check("an official answer is settled in the request", !(await queueLeft(db)) && (await db.get("SELECT state FROM questions WHERE id = ?", o)).state === "fact");
  await db.close();
}

console.log(failures ? `\n${failures} failed` : "\nthe settler settles what the request did, once, and nothing waits on it past the grace");
process.exitCode = failures ? 1 : 0;
