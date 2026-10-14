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
const first = await as(A, { pace_ms: 1000 });
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
await comeBack(C);
const busy = await as(C, { held: [{ lease: shortPack.pack.lease, rows: shortPack.pack.tasks.map((t) => t.question) }] });
check("a computer holding a pack is never handed a question someone waits on",
  !(busy.work && busy.work.question === waiting) && !(busy.pack && busy.pack.tasks.some((t) => t.question === waiting)), JSON.stringify(busy).slice(0, 200));
await comeBack(D);
const alone = await as(D);
check("…a computer holding nothing is, alone", alone.work && !alone.pack, JSON.stringify(alone).slice(0, 300));

console.log("\nthe owner's switches");
await setDial(env, "sync_off", 1, "check");
check("sync_off answers every sync with fallback 7", (await as(E)).fallback === 7);
await setDial(env, "sync_off", 0, "check");
forgetDials(); forgetWitnesses();

console.log(failures ? `\n${failures} failed` : "\nprotocol 8 hands out by protocol 7's rules and holds a client to its time");
process.exit(failures ? 1 : 0);
