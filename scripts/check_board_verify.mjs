// A BOARD ROW AS A QUESTION OF THE FACT FACTORY (worker/board_tasks.js,
// worker/verify.js, worker/factory.js), with no network: an owed row is asked
// for by a new build's producer or a sweep's, and handed out as its build, never
// a number; a further result comes only from another owner on another network
// of the served engine; CLIENTS_PER_FACT equal results — score, metric AND work
// — make a fact naming its witnesses, written to `scores`, the queue row settled
// and the work credited; anything else is a dispute, and the answer never says
// which. The official machines take what is past its deadline and what only
// they may settle; their fact credits whom it agrees with and refuses whom a
// dispute or a spot check disproves, withdrawing what that client agreed to. The
// scorer's claim, read and release (scripts/fetch_queue.sh), its answers
// (scripts/order_credit.mjs), the queue's ask (scripts/ship_queue.sh) and the
// orders open before the factory (`/api/board/adopt`) are held here too.
//   node scripts/check_board_verify.mjs
import { verifyRoute, LEASE_MS, PROTOCOL, PACKS_HELD, PACK_FIRST, PACK_MAX, packSize, PACK_LEASE_MS } from "../worker/verify.js";
import { RENEW_MS } from "../worker/tasks.js";
import { boardTasksRoute } from "../worker/board_tasks.js";
import { officialRoute, refusalsFor } from "../worker/factory.js";
refusalsFor(true);
import { coolDownMs } from "../worker/tasks.js";
import { tell } from "./order_credit.mjs";
import { testDb } from "./pg/test_db.mjs";
import { execFileSync } from "node:child_process";

// THE DICE PICK NO RECHECK AND NO SPOT CHECK unless a check below says so: an
// agreement the 2% recheck picked waits for a third, and a check expecting a
// fact would fail one run in fifty.
Math.random = () => 0.5;

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const LIBRARY = await testDb("library"), ACCOUNTS = await testDb("accounts"), L = LIBRARY;
/// THE SITE'S `release.json`, naming the engine it serves.
const site = (engine, release = "r1") => ({ fetch: async () => new Response(JSON.stringify({ engine, release })) });
const env = { LIBRARY, ACCOUNTS, ASSETS: site("e1"), FACTORY_TOKEN: "ft" };
const TOKEN = { authorization: "Bearer ft" };
const post = async (route, path, body, headers = {}) =>
  (await route(new Request(`https://x${path}`, { method: "POST", headers, body: JSON.stringify(body) }), env, path)).json();
const WORK = 7_000_000_000, SCORE = 1.1070976928071055;
const YES = { v: 1, at: "2026-10-08T08:00:00.000Z" };
const work = (v, { ip = "", engine = "e1", protocol = PROTOCOL, consent = YES, ahead = false, pace = null } = {}) =>
  post(verifyRoute, "/api/board/work", { verifier: v, engine, protocol, consent, lanes: 4, ...(ahead ? { ahead } : {}), ...(pace ? { pace_ms: pace } : {}) },
    ip ? { "cf-connecting-ip": ip } : {});
/// THE FIRST ROW OF A PACK with the pack's lease — what its page computes first.
const task = (r) => (r.pack ? { ...r.pack.tasks[0], lease: r.pack.lease } : r.work);
const none = (r) => !r.work && !r.pack;
const answerAll = (lease, v, rows, { engine = "e1", ip = "", done = true, keep, since } = {}) =>
  post(verifyRoute, "/api/board/answers", { lease, verifier: v, engine, done, answers: rows, ...(keep ? { keep } : {}),
    ...(since ? { points_since: since } : {}) }, ip ? { "cf-connecting-ip": ip } : {});
const row = (t, { score = SCORE, metric = "kpm", work: wk = WORK, extra = {} } = {}) =>
  ({ question: t.question, score, metric, work: wk, compute_ms: 1000, ...extra });
const answer = (t, v, { engine = "e1", ip = "", ...o } = {}) => answerAll(t.lease, v, [row(t, o)], { engine, ip });
const today = new Date().toISOString().slice(0, 10);
const build = (id, at = new Date().toISOString(), weapon = "braton_prime") =>
  L.run("INSERT INTO builds (id, at, record) VALUES (?, ?, ?)", id, at, JSON.stringify({ weapon, mods: ["serration", id] }));
const queue = async (batch, ...ids) => {
  await L.run("INSERT INTO batches (id, at, why, total) VALUES (?, ?, 'test', 0) ON CONFLICT DO NOTHING", batch, batch);
  for (const id of ids) await L.run("INSERT INTO queue (batch, build_id, ruler, mode) VALUES (?, ?, 'standard_single_target', 'base') ON CONFLICT DO NOTHING", batch, id);
};
const ask = (batch, ...ids) => post(boardTasksRoute, "/api/board/ask",
  { batch, rows: ids.map((identity) => ({ identity, ruler: "standard_single_target", mode: "base" })) }, TOKEN);
const demand = (id) => L.get("SELECT * FROM demands WHERE ref = ?", `${id}|standard_single_target|base`);
const question = (id) => L.get("SELECT q.* FROM questions q JOIN demands d ON d.question = q.id WHERE d.ref = ?", `${id}|standard_single_target|base`);
const fact = (id) => L.get("SELECT * FROM scores WHERE identity = ?", id);
const owed = async (id) => !!(await L.get("SELECT 1 FROM queue WHERE build_id = ?", id));
const credited = async (v) => ((await L.get("SELECT work FROM verifiers WHERE id = ?", v)) || {}).work || 0;
const free = () => L.run("UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL");
/// EVERY ONE OF `xs` passes the async test `f`.
const every = async (xs, f) => { for (const x of xs) if (!(await f(x))) return false; return true; };
/// ONLY ONE ROW IN PLAY: every other question is withdrawn for the test at hand.
const only = async (...ids) => {
  const keep = [];
  for (const id of ids) keep.push((await question(id)).id);
  await L.run(`UPDATE questions SET state = 'withdrawn' WHERE state = 'open' AND id NOT IN (${keep.map(() => "?").join(", ")})`, ...keep);
};
const A = "a".repeat(24), B = "b".repeat(24), C = "c".repeat(24), D = "d".repeat(24), E = "e".repeat(24), F = "f".repeat(24);
for (const v of [A, B, C, D, E, F]) await L.run("INSERT INTO verifiers (id, seen) VALUES (?, '2026-10-08')", v);
// A and D are one person's computers.
await ACCOUNTS.exec(`INSERT INTO accounts (id, created_at, username) VALUES ('ann', 'x', 'ann');
  INSERT INTO devices (verifier, account, claimed_at) VALUES ('${A}', 'ann', 'x'), ('${D}', 'ann', 'x');`);

console.log("asked for");
await build("one"); await build("old", "2020-01-01T00:00:00.000Z"); await build("scored");
const arrivals = `arrivals-${today}`;
await queue(arrivals, "one", "old", "scored");
await L.run("INSERT INTO scores (identity, ruler, mode, measured_by, score, metric, cost_seconds, started_at, finished_at) VALUES ('scored', 'standard_single_target', 'base', 'v', 1, 'kpm', 0, 'x', 'x')");
// A CLIENT WHOSE ID THE SITE DOES NOT MAKE is handed nothing, and told whom to ask.
const stranger = "a7i8qmmcffo718dya4q5";
const strangerAsk = await work(stranger);
check("a client of an id the site does not make gets no work, and is told why",
  strangerAsk.error === "unrecognised_client" && /discord\.gg/.test(strangerAsk.message) && !strangerAsk.pack, JSON.stringify(strangerAsk));
check("...nor are its answers taken", (await answerAll("a".repeat(32), stranger, [])).error === "unrecognised_client");
check("only the factory's secret asks", (await post(boardTasksRoute, "/api/board/ask", { batch: arrivals, rows: [] })).ok === false);
const asked = await ask(arrivals, "one", "old", "scored");
check("an owed row is asked for as a question, an arrival's scored row not again", asked.asked === 2 && asked.skipped === 1, JSON.stringify(asked));
check("...a new build's by `new_build`, an old build's in the same batch by `sweep`",
  (await demand("one")).producer === "new_build" && (await demand("old")).producer === "sweep");
check("...and its question is the build's record on the ruler, simulated",
  (await question("one")).verb === "simulate" && JSON.parse((await question("one")).request).record.weapon === "braton_prime");
await build("cell");
await L.run("INSERT INTO queue (batch, build_id, ruler, mode) VALUES (?, 'cell', 'standard_single_target__sp200', 'base')", arrivals);
await post(boardTasksRoute, "/api/board/ask", { batch: arrivals, rows: [{ identity: "cell", ruler: "standard_single_target__sp200", mode: "base" }] }, TOKEN);
const cellAsk = await L.get("SELECT producer, asked_at, due_at FROM demands WHERE ref = 'cell|standard_single_target__sp200|base'") || {};
check("...but a new build's row under another scene of a ruler by `sweep`", cellAsk.producer === "sweep");
check("...due three days after it is asked, ahead of a refresh days from its bound",
  Date.parse(cellAsk.due_at) - Date.parse(cellAsk.asked_at) === 3 * 86_400_000, `${cellAsk.asked_at} -> ${cellAsk.due_at}`);
await queue("rescore-x", "scored");
await ask("rescore-x", "scored");
check("a rescore asks for a scored row on purpose", await demand("scored") && (await demand("scored")).producer === "sweep");
await build("lost");
await queue(arrivals, "lost");
const back = await post(boardTasksRoute, "/api/board/ask", { backfill: true }, TOKEN);
check("an owed row nobody asked for is asked by the backfill", back.asked === 1 && back.left === 0 && await demand("lost"), JSON.stringify(back));
check("...and asking again asks nothing", (await post(boardTasksRoute, "/api/board/ask", { backfill: true }, TOKEN)).asked === 0);

console.log("\nhanded out, answered");
check("a page that cannot fill a task is handed none", none(await work(A, { protocol: null })));
check("a computer that never said yes is handed nothing", none(await work(A, { consent: null })));
check("a tab of an engine the site does not serve is told it is stale", (await work(A, { engine: "e0" })).stale === true);
const first = await work(A, { ip: "203.0.113.1" });
check("a new build's row goes first, in a pack, handed out as its build and never a number",
  first.pack && task(first).identity === "one" && task(first).record.weapon === "braton_prime" && task(first).ruler === "standard_single_target"
  && first.release === "r1", JSON.stringify(first));
const again = await work(A, { ip: "203.0.113.1" });
check("a client asking again gets a pack under a new lease, its lost one given back",
  again.pack && again.pack.lease !== first.pack.lease && task(again).identity === "one", JSON.stringify(again));
await free();
await only("one");
const w1 = await work(A, { ip: "203.0.113.1" });
await answer(task(w1), A, { ip: "203.0.113.1", extra: { counts: { runs: 10, shots: 900, bogus: -1 }, lanes: 4 } });
check("one result is no fact", !await fact("one") && (await question("one")).state === "open");
const kept = await L.get("SELECT counts, lanes, compute_ms FROM answers WHERE device = ?", A);
check("...and what it cost is kept beside it: its counts, its cores, its time", kept.counts === '{"runs":10,"shots":900}'
  && kept.lanes === 4 && kept.compute_ms === 1000, JSON.stringify(kept));
check("...its browser's task says it waits", (await post(verifyRoute, "/api/board/mine",
  { verifier: A, orders: [{ identity: "one", ruler: "standard_single_target", mode: "base" }] })).orders[0].state === "waiting");
check("a further result never comes from the same client", none(await work(A, { ip: "203.0.113.9" })));
check("...nor from another device of its owner", none(await work(D, { ip: "203.0.113.8" })));
check("...nor from its network", none(await work(B, { ip: "203.0.113.1" })));
const w2 = await work(B, { ip: "198.51.100.2" });
check("...but from another owner on another network, without the number to agree with",
  w2.pack && task(w2).identity === "one" && !JSON.stringify(w2).includes(String(SCORE)), JSON.stringify(w2));
const agreed = await answer(task(w2), B, { ip: "198.51.100.2" });
check("equal bits make a fact, written to the board as the clients'", await fact("one") && (await fact("one")).score === SCORE
  && (await fact("one")).measured_by === "verified:e1", JSON.stringify(await fact("one")));
check("...the queue no longer owes the row", !await owed("one"));
check("...and both clients are credited its work, once", await credited(A) === WORK && await credited(B) === WORK);
check("...a send that does not ask for its points is told nothing more", JSON.stringify(agreed) === '{"ok":true}', JSON.stringify(agreed));
const pointsTold = await answerAll(task(w2).lease, B, [row(task(w2))], { ip: "198.51.100.2", since: "2000-01-01T00" });
check("...and one that asks is told them in the reply, the fact's work in them",
  pointsTold.ok && pointsTold.device_points && pointsTold.device_points.points > 0 && pointsTold.device_points.today
  && pointsTold.device_points.today.points === pointsTold.device_points.points, JSON.stringify(pointsTold));
check("...and their tasks say confirmed", (await post(verifyRoute, "/api/board/mine",
  { verifier: B, orders: [{ identity: "one", ruler: "standard_single_target", mode: "base" }] })).orders[0].state === "confirmed");

await build("two");
await queue(arrivals, "two");
await ask(arrivals, "two");
await free(); await only("two");
const w3 = await work(C, { ip: "192.0.2.3" });
await answer(task(w3), C, { ip: "192.0.2.3" });
const w4 = await work(E, { ip: "192.0.2.4" });
const disagreed = await answer(task(w4), E, { ip: "192.0.2.4", score: SCORE + 1 });
check("another score is a dispute, and no fact", (await question("two")).state === "disputed" && !await fact("two"));
check("...and the answer is the one an agreement gets", JSON.stringify(disagreed) === JSON.stringify(agreed));
check("...its clients' tasks say it is being checked", (await post(verifyRoute, "/api/board/mine",
  { verifier: C, orders: [{ identity: "two", ruler: "standard_single_target", mode: "base" }] })).orders[0].state === "checking");
check("...and nobody is credited", await credited(C) === 0 && await credited(E) === 0);

console.log("\nthe official machines");
check("only the factory's secret speaks for them", (await post(officialRoute, "/api/tasks/official/claim", { witness: "official:live", engine: "e1" })).ok === false);
const settle = await post(officialRoute, "/api/tasks/official/claim",
  { witness: "official:live", engine: "e1", states: ["disputed", "spot", "nondeterministic"], producers: ["new_build", "sweep"], max: 3 }, TOKEN);
check("the live loop claims what only it may settle", settle.tasks.length === 1 && settle.tasks[0].demands[0].payload.identity === "two",
  JSON.stringify(settle));
const told = await tell([{ identity: "two", ruler: "standard_single_target", mode: "base", score: SCORE, metric: "kpm", work: WORK }], "e1",
  (what, body) => post(officialRoute, `/api/tasks/official/${what}`, { ...body, witness: "official:live" }, TOKEN));
check("its fact settles the dispute", told === 1 && (await question("two")).state === "fact");
check("...crediting the client it agrees with", await credited(C) === WORK);
const cooled = await L.get("SELECT banned, refusals, refused_until FROM verifiers WHERE id = ?", E);
check("...and refusing the one it disproves, for a day", cooled.banned === 1 && cooled.refusals === 1
  && Math.abs(Date.parse(cooled.refused_until) - Date.now() - coolDownMs(0)) < 60_000, JSON.stringify(cooled));
const banned = await work(E, { ip: "192.0.2.4" });
check("a refused client is handed nothing, and told why and until when", none(banned) && banned.banned === true && banned.until);
check("a cool-down is a day, then three times as long each time", coolDownMs(0) === 86_400_000 && coolDownMs(2) === 9 * 86_400_000);

// A REFUSED CLIENT'S AGREEMENTS ARE WITHDRAWN: a spot check that disproves the
// two who agreed takes the clients' number off the board, owes the row again,
// and takes back what they were paid.
await build("three");
await queue(arrivals, "three");
await ask(arrivals, "three");
await free(); await only("three");
const G = "7".repeat(24), H = "6".repeat(24);
for (const v of [G, H]) await L.run("INSERT INTO verifiers (id, seen) VALUES (?, '2026-10-08')", v);
const real = Math.random;
await answer(task(await work(G, { ip: "192.0.2.7" })), G, { ip: "192.0.2.7", score: 9 });
const second = task(await work(H, { ip: "192.0.2.8" }));
// THE DICE pass this agreement over for a recheck and pick it for a spot check.
const rolls = [0.5];
Math.random = () => (rolls.length ? rolls.shift() : 0);
await answer(second, H, { ip: "192.0.2.8", score: 9 });
Math.random = real;
check("an agreement the dice pick is a spot check: a fact now, the clients paid", (await question("three")).state === "spot" && await fact("three")
  && await credited(G) === WORK);
const spot = await post(officialRoute, "/api/tasks/official/claim",
  { witness: "official:live", engine: "e1", states: ["spot"], producers: ["new_build", "sweep"], max: 3 }, TOKEN);
await tell([{ identity: "three", ruler: "standard_single_target", mode: "base", score: SCORE, metric: "kpm", work: WORK }], "e1",
  (what, body) => post(officialRoute, `/api/tasks/official/${what}`, { ...body, witness: "official:live" }, TOKEN));
check("a spot check that disproves them refuses both and takes back what they were paid",
  spot.tasks.length === 1 && await credited(G) === 0 && await credited(H) === 0
  && (await L.get("SELECT banned FROM verifiers WHERE id = ?", G)).banned === 1);
check("...and the official fact stands", (await question("three")).state === "fact"
  && (await L.get("SELECT count(*) AS n FROM answers WHERE void = 1 AND question = ?", (await question("three")).id)).n === 2);

// THE SCORER: past their deadline, a row is its to claim; it reads exactly what
// it claimed and what nobody asked the factory for, and its answer pays a client
// whose bits it reproduces.
await build("late"); await build("young"); await build("orphan");
await queue(arrivals, "late", "young", "orphan");
await ask(arrivals, "late", "young");
await free(); await only("late", "young");
await L.run("UPDATE questions SET due_at = '2000-01-01T00:00:00.000Z' WHERE id = ?", (await question("late")).id);
await answer(task(await work(F, { ip: "192.0.2.9" })), F, { ip: "192.0.2.9", score: 3 });
check("a late row's single result is open", (await question("late")).state === "open" && (await L.get("SELECT count(*) AS n FROM answers WHERE question = ?", (await question("late")).id)).n === 1);
const claim = await post(officialRoute, "/api/tasks/official/claim",
  { witness: "official:scorer", engine: "e1", states: ["open"], producers: ["new_build", "sweep"], max: 100 }, TOKEN);
check("the scorer claims what is past its deadline and leaves a young row to the clients",
  claim.tasks.map((t) => t.demands[0].payload.identity).join() === "late" && (await question("young")).leased_to === null, JSON.stringify(claim.tasks.length));
check("no client is handed a claimed row", task(await work(C, { ip: "192.0.2.30" })).identity === "young");
await free();
await L.run("UPDATE questions SET lease = 'x', lease_until = '2999-01-01T00:00:00.000Z', leased_to = 'official:scorer' WHERE id = ?", (await question("late")).id);
const page = JSON.parse(execFileSync("bash", ["scripts/fetch_queue.sh", "--body", "page", "100", "0"]).toString());
const claimedRows = await post(officialRoute, "/api/tasks/official/report",
  { witness: "official:scorer", report: "claimed", rows: await L.all(page.sql, ...page.params) }, TOKEN);
const read = claimedRows.rows.map((r) => r.build_id).sort();
check("...it reads exactly what it claimed and what nobody asked for", JSON.stringify(read) === JSON.stringify(["late", "orphan"]), JSON.stringify(read));
await tell([{ identity: "late", ruler: "standard_single_target", mode: "base", score: 3, metric: "kpm", work: WORK }], "e1",
  (what, body) => post(officialRoute, `/api/tasks/official/${what}`, { ...body, witness: "official:scorer" }, TOKEN));
check("...and its answer is the fact, paying the client whose bits it reproduced", (await question("late")).state === "fact" && await credited(F) === WORK);
check("...whose number the scorer itself writes to the board", !await fact("late"));
await L.run(`INSERT INTO scores (identity, ruler, mode, measured_by, score, metric, cost_seconds, started_at, finished_at)
  VALUES ('late', 'standard_single_target', 'base', 'abc123', 3, 'kpm', 1, 'x', '2026-01-01T00:00:00.000Z')`);
await queue(`refresh-${today}`, "late");
await ask(`refresh-${today}`, "late");
const refreshed = await L.get("SELECT * FROM demands WHERE producer = 'refresh' AND ref = ?", "late|standard_single_target|base");
check("a `refresh-` batch's row is asked by `refresh`, due fifteen days after its fact finished",
  refreshed && refreshed.due_at === "2026-01-16T00:00:00.000Z", JSON.stringify(refreshed));
check("...and one whose official fact is on the served engine costs nothing: the row is measured now, its number kept",
  !await owed("late") && (await fact("late")).finished_at > "2026-01-01T00:00:00.000Z" && (await fact("late")).measured_by === "abc123" && (await fact("late")).score === 3,
  JSON.stringify(await fact("late")));
await post(officialRoute, "/api/tasks/official/release", { witness: "official:scorer" }, TOKEN);
check("the release hands back what the run held", !await L.get("SELECT 1 FROM questions WHERE leased_to = 'official:scorer'"));

console.log("\nleases, engines");
await free(); await only("young");
const wy = await work(C, { ip: "192.0.2.30" });
await post(verifyRoute, "/api/board/release", { lease: task(wy).lease, verifier: C });
check("a lease given back frees its row at once", (await question("young")).leased_to === null);
check("a lease is held for longer than the slowest row", LEASE_MS >= 20 * 60_000);
const stale = await work(B, { ip: "198.51.100.40" });
const dropped = await answer(task(stale), B, { engine: "e2", ip: "198.51.100.40" });
check("an answer from another engine is dropped, and the row handed out again", dropped.ok
  && (await L.get("SELECT count(*) AS n FROM answers WHERE question = ?", (await question("young")).id)).n === 0 && (await question("young")).leased_to === null);

console.log("\npacks");
{
  // FORTY ROWS OPEN: a pack is sized to a minute of the computer's pace, never
  // past its fair share of what is open, and every row in it is still judged
  // alone — another owner on another network makes each one a fact.
  const ids = Array.from({ length: 40 }, (_, i) => `p${String(i).padStart(2, "0")}`);
  for (const id of ids) await build(id);
  await queue(arrivals, ...ids);
  await ask(arrivals, ...ids);
  await free(); await only(...ids);
  const P = "5".repeat(24), Q = "4".repeat(24);
  for (const v of [P, Q]) await L.run("INSERT INTO verifiers (id, seen) VALUES (?, '2026-10-08')", v);
  check("a pack is a minute of the computer's pace, a fair share of what is open, PACK_FIRST before its pace is known",
    packSize(20_000, 4000, 10) === 3 && packSize(1000, 4000, 10) === PACK_MAX && packSize(null, 4000, 10) === PACK_FIRST
    && packSize(1000, 6, 7) === 1 && packSize(1000, 40, 8) === 5);
  check("...leased for one heartbeat, whatever its length", PACK_LEASE_MS === RENEW_MS && PACK_LEASE_MS < LEASE_MS);
  const asking = (await L.get("SELECT count(*) AS n FROM verifiers WHERE asked_at > ?", new Date(Date.now() - 120_000).toISOString())).n + 1;
  const p1 = await work(P, { ip: "192.0.2.60", pace: 20_000 });
  const rows = p1.pack ? p1.pack.tasks : [];
  check("a computer is handed a pack: its rows under one lease, in the factory's order",
    rows.length === packSize(20_000, 40, asking) && rows.length > 1 && new Set(rows.map((t) => t.question)).size === rows.length
    && p1.pack.lease_ms === PACK_LEASE_MS
    && await every(rows, async (t) => (await question(t.identity)).lease === p1.pack.lease && (await question(t.identity)).leased_to === P)
    // THE FIRST IN ORDER: these forty share a deadline, so no row left open ranks before the pack's last.
    && !await L.get("SELECT 1 FROM questions WHERE state = 'open' AND lease IS NULL AND id < ?", rows.map((t) => t.question).sort().at(-1)),
    JSON.stringify(p1));
  const ahead1 = await work(P, { ip: "192.0.2.60", pace: 20_000, ahead: true });
  const ahead2 = await work(P, { ip: "192.0.2.60", pace: 20_000, ahead: true });
  check("as a pack ends its next is asked ahead, and no more than PACKS_HELD are held",
    PACKS_HELD === 2 && ahead1.pack && ahead1.pack.lease !== p1.pack.lease && none(ahead2), JSON.stringify(ahead2));
  const [kept, ...rest] = ahead1.pack.tasks;
  await answerAll(ahead1.pack.lease, P, [], { ip: "192.0.2.60", keep: kept.question });
  check("...and one given back frees its rows at once, but the row its page will resume (`keep`)",
    rest.length > 0 && await every(rest, async (t) => (await question(t.identity)).leased_to === null) && (await question(kept.identity)).lease === ahead1.pack.lease);
  await post(verifyRoute, "/api/board/release", { lease: ahead1.pack.lease, verifier: P, keep: kept.question });
  const keptStill = (await question(kept.identity)).lease === ahead1.pack.lease;
  await post(verifyRoute, "/api/board/release", { lease: ahead1.pack.lease, verifier: P });
  check("...as a release keeps it, and one without `keep` gives it back too", keptStill && (await question(kept.identity)).leased_to === null);
  await answerAll(p1.pack.lease, P, rows.slice(0, 2).map((t) => row(t)), { ip: "192.0.2.60", done: false });
  check("answers sent before the pack ends are taken, and its other rows stay its own",
    await every(rows.slice(0, 2), async (t) => (await question(t.identity)).leased_to === null && await L.get("SELECT 1 FROM answers WHERE question = ? AND device = ?", t.question, P))
    && await every(rows.slice(2), async (t) => (await question(t.identity)).lease === p1.pack.lease));
  await answerAll(p1.pack.lease, P, [row(rows[2]), row(rows[0])], { ip: "192.0.2.60" });
  check("...the rest with `done`, a row answered twice in one pack counted once, and the unanswered rows given back",
    (await L.get("SELECT count(*) AS n FROM answers WHERE question = ? AND device = ?", rows[0].question, P)).n === 1
    && await L.get("SELECT 1 FROM answers WHERE question = ? AND device = ?", rows[2].question, P)
    && await every(rows.slice(3), async (t) => (await question(t.identity)).leased_to === null));
  check("one owner's pack makes no fact alone", await every(rows.slice(0, 3), async (t) => (await question(t.identity)).state === "open" && !await fact(t.identity)));
  const p2 = await work(Q, { ip: "198.51.100.61", pace: 20_000 });
  check("another owner's pack starts with the rows still waiting for a second witness",
    p2.pack && rows.slice(0, 3).every((t) => p2.pack.tasks.some((u) => u.question === t.question)), JSON.stringify(p2));
  const before = await credited(P);
  await answerAll(p2.pack.lease, Q, p2.pack.tasks.map((t) => row(t, t.question === rows[1].question ? { score: 2 } : {})), { ip: "198.51.100.61" });
  check("each row is judged alone: equal bits a fact, a difference a dispute", await fact(rows[0].identity) && await fact(rows[2].identity)
    && (await question(rows[1].identity)).state === "disputed" && !await fact(rows[1].identity));
  check("...and each fact credited once, to each witness", await credited(P) === before + 2 * WORK && await credited(Q) === 2 * WORK);
  // A ROW THE SCORER MEASURED MEANWHILE is not owed: left out of the pack and withdrawn.
  await free();
  const next = (await L.get(`SELECT d.ref FROM questions q JOIN demands d ON d.question = q.id WHERE q.state = 'open'
    AND NOT EXISTS (SELECT 1 FROM answers a WHERE a.question = q.id) ORDER BY q.due_at, q.id LIMIT 1`)).ref.split("|")[0];
  await L.run("DELETE FROM queue WHERE build_id = ?", next);
  const p3 = await work(B, { ip: "198.51.100.62", pace: 20_000 });
  check("a row no longer owed is left out of a pack and withdrawn where it is found",
    p3.pack && !p3.pack.tasks.some((t) => t.identity === next) && (await question(next)).state === "withdrawn", JSON.stringify(p3.pack && p3.pack.tasks.map((t) => t.identity)));
  await free();
  await L.run("UPDATE questions SET state = 'withdrawn' WHERE state = 'open'");
}

console.log("\norders open before the factory");
{
  const at = "2026-10-09T00:00:00.000Z";
  await build("legacy");
  await queue("rescore-y", "legacy");
  await L.run(`INSERT INTO orders (identity, ruler, mode, record, state, engine, slot, at, priority, score, metric, work, produced_by, clients, lease, lease_until, leased_to)
    VALUES ('legacy', 'standard_single_target', 'base', ?, 'open', 'e1', 1, ?, 1, ?, 'kpm', ?, ?, ?, NULL, NULL, NULL)`, (await L.get("SELECT record FROM builds WHERE id = 'legacy'")).record, at, SCORE, WORK, B, B);
  await L.run(`INSERT INTO orders (identity, ruler, mode, record, state, engine, slot, at, priority, lease, lease_until, leased_to)
    VALUES ('gone', 'standard_single_target', 'base', '{}', 'todo', '', 2, ?, 0, NULL, NULL, NULL)`, at);
  check("only the factory's secret adopts", (await post(boardTasksRoute, "/api/board/adopt", {})).ok === false);
  const r = await post(boardTasksRoute, "/api/board/adopt", {}, TOKEN);
  check("an open order is asked of the factory, as of when it was opened, and nothing is left", r.left === 0 && await demand("legacy")
    && (await demand("legacy")).asked_at === at, JSON.stringify(r));
  check("...its client's result carried", (await L.get("SELECT count(*) AS n FROM answers WHERE question = ? AND device = ?", (await question("legacy")).id, B)).n === 1);
  check("...and an order nobody owes any more is settled where it is found", (await L.get("SELECT state FROM orders WHERE identity = 'gone'")).state === "settled");
  await free(); await only("legacy");
  check("its client is not asked again", none(await work(B, { ip: "198.51.100.50" })));
  const before = await credited(B);
  const wl = await work(C, { ip: "192.0.2.50" });
  await answer(task(wl), C, { ip: "192.0.2.50" });
  check("...another agreeing with the carried result makes the fact and credits both", await fact("legacy") && await credited(B) === before + WORK);
}

{
  // AN ASK IS RECORDED: when, on which engine (an older protocol marked) and
  // with how many cores, which is what tells an idle page from a closed one.
  const O = "0".repeat(24);
  await L.run("INSERT INTO verifiers (id, seen) VALUES (?, '2026-10-08')", O);
  await work(O, { engine: "old" });
  const v = await L.get("SELECT asked_at, asked_engine, asked_lanes FROM verifiers WHERE id = ?", O);
  check("a page's ask for work is recorded, with its engine and its cores", v.asked_at && v.asked_engine === "old" && v.asked_lanes === 4, JSON.stringify(v));
  await work(O, { engine: "old", protocol: PROTOCOL - 1 });
  check("...and an older protocol is marked on it", (await L.get("SELECT asked_engine FROM verifiers WHERE id = ?", O)).asked_engine === `old/p${PROTOCOL - 1}`);
  await free();
}

{
  // THE PROTOCOL BEFORE IS STILL SERVED: a client a release did not reach — a
  // program somebody else wrote — is handed a pack of one as one order, and its
  // answer counts as any page's.
  await build("p6"); await queue(arrivals, "p6"); await ask(arrivals, "p6");
  await free(); await only("p6");
  const w6 = await work(B, { ip: "198.51.100.66", protocol: 6 });
  check("a client of the protocol before gets one order, as it did", w6.work && w6.work.identity === "p6" && w6.work.lease && !w6.pack, JSON.stringify(w6));
  const before = await credited(B);
  await post(verifyRoute, "/api/board/verify", { lease: w6.work.lease, verifier: B, engine: "e1", score: SCORE, metric: "kpm", work: WORK, compute_ms: 1000 },
    { "cf-connecting-ip": "198.51.100.66" });
  const w7 = await work(C, { ip: "192.0.2.66" });
  await answer(task(w7), C, { ip: "192.0.2.66" });
  check("...and its answer, agreed with by a page of this one, makes the fact and is credited", await fact("p6") && await credited(B) === before + WORK);
  check("...while a protocol older still is told it is stale", (await work(B, { protocol: 5 })).stale === true);
  await free();
}

{
  // A REFRESH IS DUE BY ITS ROW'S AGE: fifteen days after its fact finished,
  // however late it was asked — by the batch's ask and by the backfill alike.
  const DAY = 86_400_000, now = Date.now();
  const measured = async (id, daysAgo) => {
    await build(id);
    await L.run(`INSERT INTO scores (identity, ruler, mode, measured_by, score, metric, cost_seconds, started_at, finished_at)
      VALUES (?, 'standard_single_target', 'base', 'v', 1, 'kpm', 0, 'x', ?)`, id, new Date(now - daysAgo * DAY).toISOString());
  };
  const dueIn = async (id) => (Date.parse((await demand(id)).due_at) - now) / DAY;
  const refresh = `refresh-${today}`;
  await measured("aged5", 5); await measured("aged14", 14);
  await queue(refresh, "aged5", "aged14");
  await ask(refresh, "aged5", "aged14");
  check("a refresh asked at five days old is due at fifteen, ten days on",
    (await demand("aged5")).producer === "refresh" && Math.abs(await dueIn("aged5") - 10) < 0.01, await dueIn("aged5"));
  check("...and one asked at fourteen, the next day", Math.abs(await dueIn("aged14") - 1) < 0.01, await dueIn("aged14"));
  await measured("aged8", 8);
  await queue(refresh, "aged8");
  await post(boardTasksRoute, "/api/board/ask", { backfill: true }, TOKEN);
  check("...and one the backfill asks, by its own age too", await demand("aged8") && Math.abs(await dueIn("aged8") - 7) < 0.01, await demand("aged8") && await dueIn("aged8"));
}

console.log("\nthe database's safety valve");
{
  env.COMPUTE_CAP = "1";
  await L.run("UPDATE verifiers SET asked_at = ?", new Date().toISOString());
  const fresh = await work("9".repeat(24), { ip: "203.0.113.90" });
  check("past COMPUTE_CAP a computer that answered nothing lately is handed nothing", fresh.busy === true && none(fresh), JSON.stringify(fresh));
  delete env.COMPUTE_CAP;
  check("...and with no cap it is handed work as before", !(await work("8".repeat(24), { ip: "203.0.113.91" })).busy);
}

console.log("\nan older engine still accepted computes the weapons its data matches");
{
  await L.run("UPDATE questions SET state = 'withdrawn' WHERE state = 'open'");
  const until = new Date(Date.now() + 86_400_000).toISOString();
  env.ASSETS = { fetch: async () => new Response(JSON.stringify({ engine: "e1", release: "r9",
    accepted: [{ engine: "e0", until, changed: ["braton_prime"] }, { engine: "eold", until: "2026-01-01T00:00:00.000Z", changed: [] }] })) };
  await build("moved", undefined, "braton_prime");
  await build("same", undefined, "torid");
  await queue("grace-1", "moved", "same");
  await ask("grace-1", "moved", "same");
  const O1 = "1".repeat(24), O2 = "2".repeat(24);
  const p1 = await work(O1, { engine: "e0", ip: "203.0.113.71" });
  const weapons = (p1.pack ? p1.pack.tasks : []).map((t) => t.record.weapon);
  check("an accepted older engine is handed the rows of a weapon its data matches, and only those",
    weapons.length === 1 && weapons[0] === "torid" && p1.engine === "e1" && p1.accepted_until === until && !p1.stale, JSON.stringify(p1));
  check("...and never a row of a weapon whose data moved", none(await work("3".repeat(24), { engine: "e0", ip: "203.0.113.73" })));
  check("...and one past its days is told it is stale", (await work(O2, { engine: "eold", ip: "203.0.113.72" })).stale === true);
  await answer(task(p1), O1, { engine: "e0", ip: "203.0.113.71" });
  const p2 = await work(O2, { engine: "e0", ip: "203.0.113.72" });
  await answer(task(p2), O2, { engine: "e0", ip: "203.0.113.72" });
  const q = await L.get("SELECT state, fact_engine FROM questions WHERE id = ?", task(p1).question);
  check("...two of them agreeing make a fact, the served engine's", q && ["fact", "spot"].includes(q.state) && q.fact_engine === "e1", JSON.stringify(q));
  env.ASSETS = site("e1");
}

console.log(failures ? `\n${failures} failed` : "\na board row reaches the board when CLIENTS_PER_FACT clients measured the same bits");
process.exitCode = failures ? 1 : 0;
