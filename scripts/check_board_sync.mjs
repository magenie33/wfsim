// PROTOCOL 8 (worker/sync.js, docs/CLIENTS.md §"The cycle, protocol 8"), with no
// network: one sync hands in results, renews what it names, gives back what it
// does not, and is handed a pack under the same order, eligibility and fair
// share as protocol 7 — or told when to come back, and held to it. A result
// sent twice is one answer; a result whose lease ran out still counts while
// nobody took the row; a row the client's engine refused is not handed to it
// again, and is withdrawn once enough owners refused it.
//   node scripts/check_board_sync.mjs
import { PROTOCOL, PACK_PROTOCOL, PROTOCOLS_SERVED, verifyRoute } from "../worker/verify.js";
import { syncRoute, forgetWitnesses } from "../worker/sync.js";
import { setDial, forgetDials, DIALS, SYNC_CLIENT_DIALS } from "../worker/dials.js";
import { boardTasksRoute } from "../worker/board_tasks.js";
import { facts } from "../worker/factory.js";
import { testDb } from "./pg/test_db.mjs";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";

Math.random = () => 0.5;
let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const LIBRARY = await testDb("library"), ACCOUNTS = await testDb("accounts"), L = LIBRARY;
const env = { LIBRARY, ACCOUNTS, FACTORY_TOKEN: "ft",
  ASSETS: { fetch: async () => new Response(JSON.stringify({ engine: "e1", release: "r1" })) } };
const post = async (route, path, body, ip = "") =>
  (await route(new Request(`https://x${path}`, { method: "POST", headers: { authorization: "Bearer ft", ...(ip ? { "cf-connecting-ip": ip } : {}) },
    body: JSON.stringify(body) }), env, path)).json();
const YES = { v: 1, at: "2026-10-08T08:00:00.000Z" };
const WORK = 7_000_000_000, SCORE = 1.25;
const sync = (v, body = {}, ip = "") => post(syncRoute, "/api/board/sync",
  { verifier: v, engine: "e1", protocol: PROTOCOL, consent: YES, lanes: 4, want_ms: 600_000, ...body }, ip);
const result = (lease, question, extra = {}) => ({ lease, question, score: SCORE, metric: "kpm", work: WORK, compute_ms: 1000, ...extra });
const today = new Date().toISOString().slice(0, 10);
const ask = (batch, ...ids) => post(boardTasksRoute, "/api/board/ask", { batch, rows: ids.map((identity) => ({ identity, ruler: "standard_single_target", mode: "base" })) });
const build = (id) => L.run("INSERT INTO builds (id, at, record) VALUES (?, ?, ?)", id, new Date().toISOString(), JSON.stringify({ weapon: "braton_prime", mods: ["serration", id] }));
const queue = async (batch, ...ids) => {
  await L.run("INSERT INTO batches (id, at, why, total) VALUES (?, ?, 'test', 0) ON CONFLICT DO NOTHING", batch, batch);
  for (const id of ids) await L.run("INSERT INTO queue (batch, build_id, ruler, mode) VALUES (?, ?, 'standard_single_target', 'base') ON CONFLICT DO NOTHING", batch, id);
};
const qOf = async (id) => (await L.get("SELECT q.id FROM questions q JOIN demands d ON d.question = q.id WHERE d.ref = ?", `${id}|standard_single_target|base`)).id;
const answersOf = (q, v) => L.get("SELECT count(*) AS n FROM answers WHERE question = ? AND device = ?", q, v).then((r) => r.n);
const comeBack = (v) => L.run("UPDATE verifiers SET next_at = NULL WHERE id = ?", v);
const free = () => L.run("UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL, lease_prev = NULL, leased_to_prev = NULL, lease_prev_until = NULL");
const A = "a".repeat(24), B = "b".repeat(24), C = "c".repeat(24), D = "d".repeat(24), E = "e".repeat(24);
const ips = { [A]: "203.0.113.1", [B]: "198.51.100.2", [C]: "192.0.2.3", [D]: "203.0.113.4", [E]: "198.51.100.5" };
const as = (v, body) => sync(v, body, ips[v]);
for (const v of [A, B, C, D, E]) await L.run("INSERT INTO verifiers (id, seen) VALUES (?, '2026-10-08')", v);

const batch = `arrivals-${today}`;
const ids = Array.from({ length: 30 }, (_, i) => `b${String(i).padStart(2, "0")}`);
for (const id of ids) await build(id);
await queue(batch, ...ids);
const asked = await ask(batch, ...ids);
check("thirty owed rows are asked for", asked.asked === 30, JSON.stringify(asked));

console.log("the door");
check("protocol 8 is the current one, 7 and 6 served beside it", PROTOCOL === 8 && PACK_PROTOCOL === 7 && [...PROTOCOLS_SERVED].join() === "8,7,6");
check("an id the site does not make is told so", (await sync("a7i8qmmcffo718dya4q5")).error === "unrecognised_client");
check("a malformed result refuses the whole sync", (await sync(A, { results: [{ lease: "x", question: "y" }] })).error === "bad request");
check("another protocol at /sync is told it is stale", (await sync(A, { protocol: PACK_PROTOCOL })).stale === true);
check("no yes, no work", !(await as(A, { consent: null })).pack);
await comeBack(A);

console.log("\na pack, and the time to come back");
// A PACE IS KNOWN, so the pack is sized by time: many rows, enough to test with.
const first = await as(A, { pace_ms: 30_000 });
check("a sync is handed a pack of board rows, and when to come back", first.pack && first.pack.tasks.length >= 6 && first.pack.tasks[0].record.weapon === "braton_prime"
  && first.next_ms >= DIALS.sync_floor_ms.default && first.protocol === 8, JSON.stringify(first).slice(0, 400));
check("…with every dial a protocol 8 client reads", SYNC_CLIENT_DIALS.every((k) => first.dials && first.dials[k] === DIALS[k].default), JSON.stringify(first.dials));
check("…and the protocols served, as release.json says them", JSON.stringify(first.protocols_served) === "[8,7,6]");
const lease = first.pack.lease, rows = first.pack.tasks.map((t) => t.question);

// F4 — NO HEAD START FROM ASKING OFTEN: a sync before its time gets no work,
// but its results are taken and its rows renewed.
const early = await as(A, { results: [result(lease, rows[0])], held: [{ lease, rows: rows.slice(1) }] });
check("a sync before its time is handed nothing, told how long to wait", early.early === true && !early.pack && early.next_ms > 0, JSON.stringify(early).slice(0, 300));
check("…yet its result is taken", early.taken.includes(rows[0]) && await answersOf(rows[0], A) === 1, JSON.stringify(early.taken));
check("…and the rows it names are still its own", early.held.length === rows.length - 1 && early.held.every((h) => h.held));

// A SYNC SENT TWICE IS ONE ANSWER.
const again = await as(A, { results: [result(lease, rows[0])], held: [{ lease, rows: rows.slice(1) }] });
check("a result sent again is taken again and recorded once", again.taken.includes(rows[0]) && await answersOf(rows[0], A) === 1);
// …AND SENT TWICE AT ONCE (a beacon on the way out beside a sync in flight):
// both pass the checks before either writes, played here by a database whose
// checks answer each send as they answered the first — the row still held, the
// answer not yet given — and still one answer lands.
const raced = rows[rows.length - 1];
const heldThen = await L.all("SELECT * FROM questions WHERE id = ?", raced);
const blindTo = [];
const blind = { ...env, LIBRARY: { ...env.LIBRARY, prepare: (sql) => (/FROM factory_given\(/.test(sql)
  ? (blindTo.push("given"), { bind: () => ({ all: async () => ({ results: [] }) }) })
  : /FROM factory_held\(/.test(sql)
    ? (blindTo.push("held"), { bind: () => ({ all: async () => ({ results: heldThen }) }) })
  : env.LIBRARY.prepare(sql)) } };
const racedAnswer = { question: raced, result: { score: SCORE, metric: "kpm" }, work: WORK };
for (let k = 0; k < 2; k++) {
  await facts().answerPack(blind, { device: A, owner: A, net: "" }, { lease, engine: "e1", answers: [racedAnswer], late: true });
}
check("a result sent twice at once is recorded once", await answersOf(raced, A) === 1 && blindTo.join() === "held,given,held,given",
  `${await answersOf(raced, A)} ${blindTo}`);

// A LATE RESULT: the lease ran out, nobody took the row since.
await L.run("UPDATE questions SET lease_until = ? WHERE id = ?", new Date(Date.now() - 60_000).toISOString(), rows[1]);
const late = await as(A, { results: [result(lease, rows[1])], held: [{ lease, rows: rows.slice(2) }] });
check("a result whose lease ran out still counts while nobody took the row", late.taken.includes(rows[1]) && await answersOf(rows[1], A) === 1, JSON.stringify(late).slice(0, 300));
// …BUT NOT ONCE ANOTHER TOOK IT.
await L.run("UPDATE questions SET lease = ?, leased_to = ?, lease_until = ? WHERE id = ?", "f".repeat(32), B, new Date(Date.now() + 600_000).toISOString(), rows[2]);
const lost = await as(A, { results: [result(lease, rows[2])], held: [{ lease, rows: rows.slice(3) }] });
check("a result for a row taken since is dropped, and said why", lost.dropped.some((x) => x.question === rows[2] && x.why === "not_held") && await answersOf(rows[2], A) === 0,
  JSON.stringify(lost.dropped));

// A SYNC STATES EVERYTHING ITS ID HOLDS: what it does not name goes back.
const before = await L.get("SELECT count(*) AS n FROM questions WHERE leased_to = ? AND lease_until > ?", A, new Date().toISOString());
const named = rows.slice(3, 5);
await as(A, { held: [{ lease, rows: named }] });
const after = await L.all("SELECT id FROM questions WHERE leased_to = ? AND lease_until > ?", A, new Date().toISOString());
check("a held row it names is kept, every other one it held goes back", before.n > named.length && after.length === named.length
  && after.every((r) => named.includes(r.id)), JSON.stringify({ before: before.n, after }));

// PROTOCOL 7 AND 8 ON ONE ID: one answer however it arrives.
await free(); await comeBack(A);
const p8 = await as(A);
const q8 = p8.pack.tasks[0].question, l8 = p8.pack.lease;
await post(verifyRoute, "/api/board/answers", { lease: l8, verifier: A, engine: "e1", answers: [{ question: q8, score: SCORE, metric: "kpm", work: WORK }] }, ips[A]);
await as(A, { results: [result(l8, q8)] });
check("an answer given by protocol 7 and sent again by 8 is one answer", await answersOf(q8, A) === 1);

console.log("\nrefused by the engine");
await free();
for (const v of [A, B, C, D, E]) await comeBack(v);
const target = await qOf("b29");
// ONLY b29 IN PLAY, so every sync is handed it.
await L.run("UPDATE questions SET state = 'withdrawn' WHERE state = 'open' AND id <> ?", target);
const r1 = await as(B);
check("the row is handed out", r1.pack && r1.pack.tasks[0].question === target, JSON.stringify(r1).slice(0, 200));
await as(B, { refused: [{ lease: r1.pack.lease, question: target, step: "order" }] });
await comeBack(B);
check("a row its engine refused is not handed to it again", !(await as(B)).pack);
for (const v of [C, E]) {
  await comeBack(v);
  const r = await as(v);
  await as(v, { refused: [{ lease: r.pack.lease, question: target, step: "order" }] });
}
const gone = await L.get("SELECT state FROM questions WHERE id = ?", target);
check(`refused by ${DIALS.refusals_to_withdraw.default} owners on the served engine, it is withdrawn and written down`,
  gone.state === "withdrawn" && !!(await L.get("SELECT 1 FROM refusal_withdrawals WHERE question = ?", target)), JSON.stringify(gone));
const found = await facts().audit(env, Date.now(), "e1");
check("…and the watchdog says so", found.some((f) => f.kind === "refused_rows" && f.questions === 1), JSON.stringify(found));
await ask(batch, "b29");
check("asked for again on the same engine, it stays withdrawn", (await L.get("SELECT state FROM questions WHERE id = ?", target)).state === "withdrawn");

console.log("\nfair share and who waits");
// F2 — SHORT WORK: a pack is at most open over asking, leased short.
await L.run("UPDATE questions SET state = 'open' WHERE state = 'withdrawn' AND id <> ?", target);
await free();
for (const v of [A, B, C, D, E]) await comeBack(v);
await L.run("UPDATE verifiers SET asked_at = ?", new Date().toISOString());
const open = (await L.get("SELECT count(*) AS n FROM questions WHERE state = 'open' AND waits = 0")).n;
const shortPack = await as(C, { pace_ms: 1000 });
const cap = Math.floor(open / 5);
check("while work is short a pack is at most open over asking, and leased short",
  shortPack.pack && shortPack.pack.tasks.length <= cap && shortPack.pack.lease_ms === DIALS.pack_lease_ms.default,
  JSON.stringify({ open, cap, got: shortPack.pack && shortPack.pack.tasks.length, lease_ms: shortPack.pack && shortPack.pack.lease_ms }));
// F8 — A QUESTION SOMEONE WAITS ON goes alone, to a computer holding nothing.
await L.run("UPDATE questions SET waits = 1 WHERE id = (SELECT id FROM questions WHERE state = 'open' AND leased_to IS NULL ORDER BY id LIMIT 1)");
const waiting = (await L.get("SELECT id FROM questions WHERE waits = 1")).id;
const fairRefill = await L.get("SELECT id,producer,due_at FROM questions WHERE state='open' AND waits=0 AND leased_to IS NULL ORDER BY id DESC LIMIT 1");
await L.run("UPDATE questions SET producer='survey_build',due_at='2999-01-01' WHERE id=?", fairRefill.id);
await L.run("UPDATE demands SET producer='survey_build' WHERE question=?", fairRefill.id);
await L.run("INSERT INTO producer_use (producer,used,answer_ms) VALUES ('survey_build',-1000000,1)");
await comeBack(C);
const busy = await as(C, { held: [{ lease: shortPack.pack.lease, rows: shortPack.pack.tasks.map((t) => t.question) }] });
check("a computer holding a pack is never handed a question someone waits on",
  !(busy.work && busy.work.question === waiting) && !(busy.pack && busy.pack.tasks.some((t) => t.question === waiting)), JSON.stringify(busy).slice(0, 200));
check("…but is handed a pack of the rest, not idled while it stands open", !!(busy.pack && busy.pack.tasks.length), JSON.stringify(busy).slice(0, 200));
check("a busy refill preserves producer fairness behind a waited-on head", busy.pack?.tasks.some(t => t.question === fairRefill.id), JSON.stringify(busy.pack?.tasks.map(t => t.question)));
await L.run("UPDATE questions SET producer=?,due_at=? WHERE id=?", fairRefill.producer, fairRefill.due_at, fairRefill.id);
await L.run("UPDATE demands SET producer=? WHERE question=?", fairRefill.producer, fairRefill.id);
await L.run("DELETE FROM producer_use WHERE producer='survey_build'");
await comeBack(D);
const alone = await as(D);
check("…a computer holding nothing is, alone, as a pack of one its page runs",
  !alone.work && alone.pack && alone.pack.tasks.length === 1 && alone.pack.tasks[0].question === waiting, JSON.stringify(alone).slice(0, 300));

console.log("\nwhether work is short, and coming back");
// THE SETTLER'S SNAPSHOT DECIDES, while it is fresh: open past any fixed ceiling
// on a count is plenty, and a fast computer is handed a long-leased pack.
await L.run("UPDATE questions SET waits = 0");
const snap = (open, at) => L.run(`INSERT INTO factory_snapshot (id, at, open, urgent, asking, computing, queue_rows, settle_queue_rows, daemon_at, daemon_commit)
  VALUES (1, ?1, ?2, 0, 5, 0, 0, 0, ?1, 'check') ON CONFLICT (id) DO UPDATE SET at = excluded.at, open = excluded.open, asking = excluded.asking`, at, open);
await free(); await comeBack(E);
await snap(1_000_000, new Date().toISOString());
const plenty = await as(E, { pace_ms: 1000 });
check("a fresh snapshot of plenty open is not short: a fast computer's pack is leased long",
  plenty.pack && plenty.pack.lease_ms === DIALS.sync_lease_ms.default, JSON.stringify(plenty.pack && { n: plenty.pack.tasks.length, lease_ms: plenty.pack.lease_ms }));
// A bootstrap pack cannot establish a safe delay before its first measurement.
await free(); await comeBack(E);
const cold = await as(E);
check("a bootstrap pack allows refill as soon as its pace is measured",
  cold.pack && cold.next_ms === 0, JSON.stringify({ n: cold.pack && cold.pack.tasks.length, next_ms: cold.next_ms }));
check("the persisted bootstrap gate agrees with the reply",
  Date.parse((await L.get("SELECT next_at FROM verifiers WHERE id = ?", E)).next_at) <= Date.now());
await free(); await comeBack(E);
const small = await as(E, { pace_ms: 1000 });
check("a small pack does not gate refill beyond its low-work threshold",
  small.pack && small.next_ms === Math.min(DIALS.sync_floor_ms.default,
    Math.max(0, small.pack.tasks.length * 1000 - DIALS.sync_low_ms.default)), JSON.stringify({ n: small.pack?.tasks.length, next_ms: small.next_ms }));
const renewOnly = await as(E, { want_ms: 0, held: [{ lease: small.pack.lease, rows: small.pack.tasks.map((t) => t.question) }] });
check("a renewal without new work does not impose a new refill delay", renewOnly.next_ms === 0, JSON.stringify(renewOnly).slice(0, 200));
await free(); await comeBack(E);
await setDial(env, "sync_scale_pct", 125, "check");
const beforeScaled = Date.now(), scaled = await as(E, { pace_ms: 30_000, want_ms: 150_000 }), afterScaled = Date.now();
const scaledGate = Date.parse((await L.get("SELECT next_at FROM verifiers WHERE id = ?", E)).next_at);
check("a paced partial pack scales its gate and persists the same deadline",
  scaled.pack?.tasks.length === 5 && scaled.next_ms === 37_500
    && scaledGate >= beforeScaled + scaled.next_ms && scaledGate <= afterScaled + scaled.next_ms,
  JSON.stringify({ n: scaled.pack?.tasks.length, next_ms: scaled.next_ms, scaledGate, beforeScaled, afterScaled }));
await setDial(env, "sync_scale_pct", 100, "check");
// …A SNAPSHOT OVER A MINUTE OLD IS NOT READ: open is counted here, and these few rows are short.
await free(); await comeBack(E);
await snap(1_000_000, new Date(Date.now() - 120_000).toISOString());
const counted = await as(E, { pace_ms: 1000 });
check("a stale snapshot is not read: open counted here is short, the pack leased short",
  counted.pack && counted.pack.lease_ms === DIALS.pack_lease_ms.default, JSON.stringify(counted.pack && { n: counted.pack.tasks.length, lease_ms: counted.pack.lease_ms }));
await L.run("DELETE FROM factory_snapshot");

console.log("\nthe owner's switches");
await setDial(env, "sync_off", 1, "check");
check("sync_off answers every sync with fallback 7", (await as(E)).fallback === 7);
await setDial(env, "sync_off", 0, "check");
forgetDials(); forgetWitnesses();

console.log("\nthe official page and desktop refill loop");
const source = readFileSync(new URL("../web/src/static/app/69-board-work.js", import.meta.url), "utf8");
let monotonic = 0;
const page = {
  Date: class extends Date { static now() { return 100_000; } },
  setTimeout, heldPack: null, packQueue: [], lostRows: new Set(), boardPaceMs: 1000,
  syncDials: { packs_held: 2, poll_ms: 10_000, sync_low_ms: 120_000, flush_ms: 600_000, report_ms: 0 },
  boardProtocol: 8, syncNotBefore: 0, syncInFlight: null, syncWork: null, bootSyncAt: 1, lastSyncAt: 100_000, resultsMem: [],
  jittered: (ms) => ms, keptResults: async () => [], loadCut: () => null, maybeReloadForRelease() {}, idleCount() {},
  communityLanes: () => 4,
  communityPower: () => 4, performance: { now: () => monotonic }, PIECE_MS: 250,
  boardVerifyOn: () => true, computeHeld: () => false, computeFrozen: false, returned: 0,
  reads: 0, measured: 0, asked: 0,
};
page.giveBack = () => { page.returned++; page.heldPack = null; page.packQueue = []; page.syncWork = null; };
page.readSignal = async () => { page.reads++; return { open: 0, urgent: 0 }; };
page.packRowOnce = async () => { page.measured++; return true; };
page.boardSync = async () => { page.asked++; return null; };
const client = runInNewContext(source.slice(source.indexOf("const refillTiming ="), source.indexOf("function boardSyncBody(")) +
  source.slice(source.indexOf("const idleWaitMs ="), source.indexOf("function keepRow8(")) +
  "\n({ workOnce8, refill8, idleWaitMs, workLeftMs, refillLeadMs, learnTiming, refillTiming, rowProgress, rowPaces });", page);
const pack = (lease) => ({ proto: 8, lease, next: 0, tasks: [{ question: lease }], done: new Set() });
let finishSync;
page.heldPack = { ...pack("old"), next: 1, done: new Set(["old"]) };
page.syncInFlight = new Promise((ok) => { finishSync = ok; }).then(() => { page.packQueue.push(pack("next")); page.syncInFlight = null; });
let settled = false;
const handoff = client.workOnce8("device").then((worked) => { settled = true; return worked; });
for (let i = 0; i < 10; i++) await Promise.resolve();
const waitedForReply = !settled;
finishSync();
const worked = await handoff;
check("an exhausted pack waits for its refill rather than entering the idle poll",
  waitedForReply && worked && page.measured === 1 && page.reads === 0, JSON.stringify({ waitedForReply, worked, measured: page.measured, reads: page.reads }));
page.heldPack = null; page.packQueue = [];
page.syncInFlight = new Promise((ok) => { finishSync = ok; }).then(() => { page.packQueue.push(pack("stopped")); page.syncInFlight = null; });
const stopped = client.workOnce8("device");
page.computeHeld = () => true;
finishSync();
check("pausing during a slow refill returns the new work without starting it",
  !(await stopped) && page.measured === 1 && page.returned === 1);
page.computeHeld = () => false;
page.heldPack = pack("ready"); page.packQueue = []; page.syncInFlight = new Promise(() => {});
check("a refill in flight never blocks work already buffered", await client.workOnce8("device") && page.measured === 2);
page.syncInFlight = null;
page.syncNotBefore = 101_000;
check("an idle retry wakes at its gate, not a whole poll later", client.idleWaitMs() === 1000, String(client.idleWaitMs()));
page.syncNotBefore = 0; page.heldPack = pack("one"); page.packQueue = [pack("two")]; page.asked = 0;
client.refill8("device");
check("a full local buffer does not ask for a third pack", page.asked === 0);
page.packQueue = []; page.syncDials.packs_held = 1;
client.refill8("device");
check("a one-pack server limit suppresses refill until the held pack is done", page.asked === 0);
page.syncDials.packs_held = 2;
client.refill8("device");
check("a low buffer refills when another slot is available", page.asked === 1);
page.heldPack = { ...pack("long"), row: "long", progress: 0.99 }; page.boardPaceMs = 300_000; page.asked = 0;
client.refill8("device");
check("the running row's completed fraction does not delay a long-row refill", page.asked === 1, String(page.asked));
page.heldPack.done.add("long");
check("a completed row does not make remaining work negative", client.workLeftMs() === 0);
page.heldPack.tasks.push({ question: "queued" });
check("a completed row's progress is not subtracted from the next row", client.workLeftMs() === 300_000);
page.heldPack.done.clear(); page.lostRows.add("long");
check("a lost row's progress is not subtracted from the next row", client.workLeftMs() === 300_000);
page.lostRows.clear();
page.heldPack = null; page.packQueue = [];
page.refusedPending = []; page.syncFailures = 0;
page.boardSyncBody = () => ({ held: [], results: [] });
page.takeSynced = async () => {};
page.noteHandOut = () => {};
page.postBoardWork = async () => ({ ok: true, next_ms: 60_000, work: { kind: "user_calculation" } });
const send = runInNewContext(source.slice(source.indexOf("async function boardSync("), source.indexOf("async function takeSynced(")) + "\nboardSync;", page);
await send("device");
let calculations = 0;
page.userCalculationOnce = async () => { calculations++; return true; };
check("a standalone task handed to a completed refill is run, not given back by another sync",
  await client.workOnce8("device") && calculations === 1 && page.syncWork === null && page.reads === 0);
let networkCalls = 0;
page.syncWork = null; page.syncNotBefore = 101_000; page.syncFailures = 1;
page.postBoardWork = async () => { networkCalls++; return { ok: true, next_ms: 0 }; };
await send("device", { want: 0 });
check("result flushing and renewal cannot bypass a failed-sync backoff", networkCalls === 0, String(networkCalls));
page.syncFailures = 0;
await send("device", { want: 0 });
check("an ordinary server gate still allows results and renewals", networkCalls === 1, String(networkCalls));
page.syncFailures = 1; page.syncNotBefore = 99_000;
await send("device", { want: 0 });
check("a failed request retries once its backoff expires", networkCalls === 2, String(networkCalls));

Object.assign(page, {
  setInterval, clearInterval, RENEW_RETRY_MS: 30_000, RESUME_SAVE_MS: 15_000, AHEAD_AT: 0.8,
  computeBeats: 0, resumeCut: null, resumeSavedAt: 100_000, boardPaceMs: 300_000,
  keepRow8: async () => {}, dropCut() {}, computeStart() {}, computeProgress() {}, computeEnd() {},
  api: async () => ({ ok: true, request: {} }), saveCut() {}, syncInFlight: null, syncNotBefore: 0,
});
page.heldPack = pack("progress");
page.heldPack.tasks[0].record = { weapon: "braton_prime" };
page.ENGINE_ID = "engine";
page.measureRow = async (_request, _ruler, _live, onPiece) => { monotonic += 60_000; onPiece(90, 100, { folded: 90 }); return null; };
const measure = runInNewContext(source.slice(source.indexOf("async function packRowOnce("), source.indexOf("/// DOES THIS BROWSER KEEP COMPUTING")) + "\npackRowOnce;", page);
page.refill8 = client.refill8; page.asked = 0;
page.boardSync = async () => { page.asked++; return null; };
await measure("device", page.heldPack);
check("real row progress triggers refill before the long row finishes",
  page.heldPack.progress === 0.9 && page.asked === 1, JSON.stringify({ progress: page.heldPack.progress, asked: page.asked }));

page.heldPack = pack("accelerated");
page.heldPack.tasks[0].record = { weapon: "braton_prime" };
page.asked = 0;
for (const stat of Object.values(client.refillTiming)) for (const key of Object.keys(stat)) delete stat[key];
page.measureRow = async (_request, _ruler, _live, onPiece) => { monotonic += 3000; onPiece(30, 100, { folded: 30 }); return null; };
await measure("device", page.heldPack);
check("a fast current row can request refill before the old 80-percent trigger", page.asked === 1);
check("current progress replaces a stale previous-row duration", Math.abs(client.workLeftMs() - 7000) < 1e-6);

for (const stat of Object.values(client.refillTiming)) for (const key of Object.keys(stat)) delete stat[key];
const coldLead = client.refillLeadMs();
client.learnTiming(client.refillTiming.sync, 200);
client.learnTiming(client.refillTiming.prepare, 100);
client.learnTiming(client.refillTiming.piece, 250);
const fastLead = client.refillLeadMs();
client.learnTiming(client.refillTiming.sync, 5000);
const slowLead = client.refillLeadMs();
for (let i = 0; i < 40; i++) client.learnTiming(client.refillTiming.sync, 200);
const recoveredLead = client.refillLeadMs();
check("measured fast dispatch shrinks the cold-start lead", fastLead === 550 && fastLead < coldLead);
check("one slow useful dispatch expands the lead immediately", slowLead >= 5350);
check("stable recovery shrinks the lead rather than retaining a historical maximum", recoveredLead < 600);
client.learnTiming(client.refillTiming.sync, 1_000_000);
check("a latency spike cannot exceed the server low-work ceiling", client.refillLeadMs() === page.syncDials.sync_low_ms);

page.heldPack = pack("slow");
page.heldPack.tasks[0].record = { weapon: "braton_prime" };
page.heldPack.row = "slow"; page.heldPack.next = 1;
Object.assign(page.heldPack, { paceKey: JSON.stringify(["braton_prime", undefined, undefined, 4]), sampleProgress: 0, sampleAt: monotonic });
monotonic += 30_000;
client.rowProgress(page.heldPack, 10, 100);
check("a slow row is not refilled merely because its percentage advanced", client.workLeftMs() > client.refillLeadMs());
page.communityPower = () => 2;
monotonic += 250;
client.rowProgress(page.heldPack, 20, 100);
check("a changed compute share discards the old active rate", page.heldPack.rowMs === 0);

page.communityPower = () => 4;
page.heldPack = null; page.packQueue = []; page.syncNotBefore = 0;
page.postBoardWork = async () => { monotonic += 700; return { ok: true, next_ms: 0, pack: { lease: "timed", lease_ms: 60_000, tasks: [{ question: "timed" }] } }; };
page.takeSynced = async () => { monotonic += 200; };
await send("device");
check("actual sync timing includes local reply processing", client.refillTiming.sync.last === 900);
page.postBoardWork = async () => { monotonic += 100_000; return { ok: true, next_ms: 20_000 }; };
await send("device");
check("an empty or fair-limited reply does not inflate learned dispatch latency", client.refillTiming.sync.last === 900);

page.packQueue = []; page.syncNotBefore = 0;
page.packRowOnce = async () => true;
page.syncInFlight = Promise.resolve().then(() => {
  monotonic += 600;
  page.packQueue.push(pack("gap")); page.syncInFlight = null;
  return { pack: { tasks: [{ question: "gap" }] } };
});
await client.workOnce8("device");
check("a late useful refill feeds its actual handoff gap back into the lead", client.refillTiming.gap.last === 600);
page.heldPack = null; page.packQueue = [pack("on-time")];
await client.workOnce8("device");
check("an on-time buffered handoff feeds a zero gap back into the lead", client.refillTiming.gap.last === 0);

let orders = 0;
page.heldPack = pack("working"); page.packQueue = []; page.syncNotBefore = 0;
page.api = async () => { orders++; monotonic += 120; return { ok: true, request: { runs: 100 } }; };
page.postBoardWork = async () => {
  monotonic += 300;
  return { ok: true, next_ms: 0, pack: { lease: "prepared", lease_ms: 60_000,
    tasks: [{ question: "prepared", record: { weapon: "braton_prime" } }] } };
};
await send("device");
const prepared = page.packQueue.shift();
check("background refill prepares the first next row while the current row computes",
  !!prepared.prepared && orders === 1 && client.refillTiming.prepare.last === 120);
page.heldPack = prepared;
page.measureRow = async (_request, _ruler, _live, onPiece) => { monotonic += 100; onPiece(10, 100, { folded: 10 }); return null; };
await measure("device", prepared);
check("the next row reuses its prepared order without another preparation call", orders === 1);
page.heldPack = { ...pack("retry-prepare"), prepared: { question: "retry-prepare", promise: Promise.resolve(null) } };
page.heldPack.tasks[0].record = { weapon: "braton_prime" };
await measure("device", page.heldPack);
check("a transient background preparation failure is retried when the row starts", orders === 2);

Object.assign(page, {
  taskKey: () => "row", idbWrite: async () => {}, computeFinished: 0, renderBoardConsent() {},
});
page.heldPack = pack("completed");
page.heldPack.tasks[0].record = { weapon: "braton_prime" };
client.rowPaces.clear();
page.measureRow = async (_request, _ruler, _live, onPiece) => {
  monotonic += 2500; onPiece(50, 100, { folded: 50 });
  monotonic += 2500; onPiece(100, 100, { folded: 100 });
  return { score: 1 };
};
await measure("device", page.heldPack);
const learnedKey = JSON.stringify(["braton_prime", undefined, undefined, 4]);
check("a completed row teaches the matching task and compute-share pace", client.rowPaces.get(learnedKey) === 5000);
page.heldPack = pack("changed");
page.heldPack.tasks[0].record = { weapon: "braton_prime" };
page.measureRow = async (_request, _ruler, _live, onPiece) => {
  page.communityPower = () => 2;
  monotonic += 2500; onPiece(50, 100, { folded: 50 });
  monotonic += 2500; onPiece(100, 100, { folded: 100 });
  return { score: 1 };
};
await measure("device", page.heldPack);
check("a row that changed share is not learned as a whole row at the new share", client.rowPaces.size === 1);

page.communityPower = () => 4;
page.heldPack = { ...pack("resumed"), from: { folded: 50 } };
page.heldPack.tasks[0].record = { weapon: "lex" };
page.measureRow = async (_request, _ruler, _live, onPiece) => {
  monotonic += 2000; onPiece(100, 100, { folded: 100 }); return { score: 1 };
};
await measure("device", page.heldPack);
check("resuming halfway learns a full-row pace from only the remaining half",
  client.rowPaces.get(JSON.stringify(["lex", undefined, undefined, 4])) === 4000);

function handoffTrace(delay) {
  for (const stat of Object.values(client.refillTiming)) for (const key of Object.keys(stat)) delete stat[key];
  client.learnTiming(client.refillTiming.sync, delay);
  client.learnTiming(client.refillTiming.prepare, 100);
  client.learnTiming(client.refillTiming.piece, 100);
  page.heldPack = { ...pack("trace"), row: "trace", next: 1, paceKey: JSON.stringify([undefined, undefined, undefined, 4]),
    sampleAt: monotonic, sampleProgress: 0 };
  page.packQueue = []; page.syncInFlight = null; page.syncNotBefore = 0; page.resultsMem = [];
  let at = null, calls = 0;
  const began = monotonic;
  page.boardSync = async () => { calls++; at = monotonic - began; page.syncInFlight = new Promise(() => {}); };
  for (let piece = 1; piece <= 100; piece++) {
    monotonic += 100;
    client.rowProgress(page.heldPack, piece, 100);
    client.refill8("device");
  }
  page.syncInFlight = null;
  return { at, calls, gap: Math.max(0, at + delay + 100 - 10_000), held: Math.max(0, 10_000 - at - delay - 100) };
}
const fastTrace = handoffTrace(200), slowTrace = handoffTrace(4000);
check("a trained fast link hands off seamlessly without holding the next pack for seconds",
  fastTrace.calls === 1 && fastTrace.gap === 0 && fastTrace.held <= 200 && fastTrace.at >= 9000, JSON.stringify(fastTrace));
check("a trained slow link starts earlier, not more often, and still hands off seamlessly",
  slowTrace.calls === 1 && slowTrace.gap === 0 && slowTrace.held <= 200 && slowTrace.at < fastTrace.at, JSON.stringify(slowTrace));

console.log(failures ? `\n${failures} failed` : "\nprotocol 8 hands out by protocol 7's rules and holds a client to its time");
process.exit(failures ? 1 : 0);
