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
import { verifyRoute, LEASE_MS, PROTOCOL, TASKS_AHEAD } from "../worker/verify.js";
import { boardTasksRoute } from "../worker/board_tasks.js";
import { officialRoute } from "../worker/factory.js";
import { coolDownMs } from "../worker/tasks.js";
import { tell } from "./order_credit.mjs";
import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const d1 = (file) => {
  const db = new DatabaseSync(":memory:");
  db.exec(readFileSync(new URL(`../worker/${file}`, import.meta.url), "utf8"));
  const stmt = (sql, args = []) => ({
    sql, args,
    bind: (...a) => stmt(sql, a),
    first: async () => db.prepare(sql).get(...args) ?? null,
    all: async () => ({ results: db.prepare(sql).all(...args) }),
    run: async () => ({ meta: { changes: Number(db.prepare(sql).run(...args).changes) } }),
  });
  return { raw: db, prepare: (sql) => stmt(sql),
    batch: async (ss) => ss.map((s) => (/^\s*select/i.test(s.sql) ? { results: db.prepare(s.sql).all(...s.args) }
      : { meta: { changes: Number(db.prepare(s.sql).run(...s.args).changes) } })) };
};
const LIBRARY = d1("schema.sql"), ACCOUNTS = d1("accounts.sql"), L = LIBRARY.raw;
/// THE SITE'S `release.json`, naming the engine it serves.
const site = (engine, release = "r1") => ({ fetch: async () => new Response(JSON.stringify({ engine, release })) });
const env = { LIBRARY, ACCOUNTS, ASSETS: site("e1"), FACTORY_TOKEN: "ft" };
const TOKEN = { authorization: "Bearer ft" };
const post = async (route, path, body, headers = {}) =>
  (await route(new Request(`https://x${path}`, { method: "POST", headers, body: JSON.stringify(body) }), env, path)).json();
const WORK = 7_000_000_000, SCORE = 1.1070976928071055;
const YES = { v: 1, at: "2026-10-08T08:00:00.000Z" };
const work = (v, { ip = "", engine = "e1", protocol = PROTOCOL, consent = YES, ahead = false } = {}) =>
  post(verifyRoute, "/api/board/work", { verifier: v, engine, protocol, consent, lanes: 4, ...(ahead ? { ahead } : {}) }, ip ? { "cf-connecting-ip": ip } : {});
const answer = (w, v, { score = SCORE, metric = "kpm", engine = "e1", work: wk = WORK, ip = "", extra = {} } = {}) =>
  post(verifyRoute, "/api/board/verify", { lease: w.lease, verifier: v, engine, score, metric, work: wk, compute_ms: 1000, ...extra },
    ip ? { "cf-connecting-ip": ip } : {});
const today = new Date().toISOString().slice(0, 10);
const build = (id, at = new Date().toISOString(), weapon = "braton_prime") =>
  L.prepare("INSERT INTO builds (id, at, record) VALUES (?, ?, ?)").run(id, at, JSON.stringify({ weapon, mods: ["serration", id] }));
const queue = (batch, ...ids) => {
  L.prepare("INSERT OR IGNORE INTO batches (id, at, why, total) VALUES (?, ?, 'test', 0)").run(batch, batch);
  for (const id of ids) L.prepare("INSERT OR IGNORE INTO queue (batch, build_id, ruler, mode) VALUES (?, ?, 'standard_single_target', 'base')").run(batch, id);
};
const ask = (batch, ...ids) => post(boardTasksRoute, "/api/board/ask",
  { batch, rows: ids.map((identity) => ({ identity, ruler: "standard_single_target", mode: "base" })) }, TOKEN);
const demand = (id) => L.prepare("SELECT * FROM demands WHERE ref = ?").get(`${id}|standard_single_target|base`);
const question = (id) => L.prepare("SELECT q.* FROM questions q JOIN demands d ON d.question = q.id WHERE d.ref = ?").get(`${id}|standard_single_target|base`);
const fact = (id) => L.prepare("SELECT * FROM scores WHERE identity = ?").get(id);
const owed = (id) => !!L.prepare("SELECT 1 FROM queue WHERE build_id = ?").get(id);
const credited = (v) => (L.prepare("SELECT work FROM verifiers WHERE id = ?").get(v) || {}).work || 0;
const free = () => L.prepare("UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL").run();
/// ONLY ONE ROW IN PLAY: every other question is withdrawn for the test at hand.
const only = (...ids) => {
  const keep = ids.map((id) => question(id).id);
  L.prepare(`UPDATE questions SET state = 'withdrawn' WHERE state = 'open' AND id NOT IN (${keep.map(() => "?").join(", ")})`).run(...keep);
};
const A = "a".repeat(24), B = "b".repeat(24), C = "c".repeat(24), D = "d".repeat(24), E = "e".repeat(24), F = "f".repeat(24);
for (const v of [A, B, C, D, E, F]) L.prepare("INSERT INTO verifiers (id, seen) VALUES (?, '2026-10-08')").run(v);
// A and D are one person's computers.
ACCOUNTS.raw.exec(`INSERT INTO accounts (id, created_at, username) VALUES ('ann', 'x', 'ann');
  INSERT INTO devices (verifier, account, claimed_at) VALUES ('${A}', 'ann', 'x'), ('${D}', 'ann', 'x');`);

console.log("asked for");
build("one"); build("old", "2020-01-01T00:00:00.000Z"); build("scored");
const arrivals = `arrivals-${today}`;
queue(arrivals, "one", "old", "scored");
L.prepare("INSERT INTO scores (identity, ruler, mode, measured_by, score, metric, cost_seconds, started_at, finished_at) VALUES ('scored', 'standard_single_target', 'base', 'v', 1, 'kpm', 0, 'x', 'x')").run();
check("only the factory's secret asks", (await post(boardTasksRoute, "/api/board/ask", { batch: arrivals, rows: [] })).ok === false);
const asked = await ask(arrivals, "one", "old", "scored");
check("an owed row is asked for as a question, an arrival's scored row not again", asked.asked === 2 && asked.skipped === 1, JSON.stringify(asked));
check("...a new build's by `new_build`, an old build's in the same batch by `sweep`",
  demand("one").producer === "new_build" && demand("old").producer === "sweep");
check("...and its question is the build's record on the ruler, simulated",
  question("one").verb === "simulate" && JSON.parse(question("one").request).record.weapon === "braton_prime");
queue("rescore-x", "scored");
await ask("rescore-x", "scored");
check("a rescore asks for a scored row on purpose", demand("scored") && demand("scored").producer === "sweep");
build("lost");
queue(arrivals, "lost");
const back = await post(boardTasksRoute, "/api/board/ask", { backfill: true }, TOKEN);
check("an owed row nobody asked for is asked by the backfill", back.asked === 1 && back.left === 0 && demand("lost"), JSON.stringify(back));
check("...and asking again asks nothing", (await post(boardTasksRoute, "/api/board/ask", { backfill: true }, TOKEN)).asked === 0);

console.log("\nhanded out, answered");
check("a page that cannot fill a task is handed none", (await work(A, { protocol: null })).work === null);
check("a computer that never said yes is handed nothing", (await work(A, { consent: null })).work === null);
check("a tab of an engine the site does not serve is told it is stale", (await work(A, { engine: "e0" })).stale === true);
const first = await work(A, { ip: "203.0.113.1" });
check("a new build's row goes first, handed out as its build and never a number",
  first.work && first.work.identity === "one" && first.work.record.weapon === "braton_prime" && first.work.ruler === "standard_single_target"
  && first.release === "r1", JSON.stringify(first));
const again = await work(A, { ip: "203.0.113.1" });
check("a client asking again gets a task under a new lease, its lost one given back",
  again.work && again.work.lease !== first.work.lease, JSON.stringify(again));
free();
only("one");
const w1 = await work(A, { ip: "203.0.113.1" });
await answer(w1.work, A, { ip: "203.0.113.1", extra: { counts: { runs: 10, shots: 900, bogus: -1 }, lanes: 4 } });
check("one result is no fact", !fact("one") && question("one").state === "open");
const kept = L.prepare("SELECT counts, lanes, compute_ms FROM answers WHERE device = ?").get(A);
check("...and what it cost is kept beside it: its counts, its cores, its time", kept.counts === '{"runs":10,"shots":900}'
  && kept.lanes === 4 && kept.compute_ms === 1000, JSON.stringify(kept));
check("...its browser's task says it waits", (await post(verifyRoute, "/api/board/mine",
  { verifier: A, orders: [{ identity: "one", ruler: "standard_single_target", mode: "base" }] })).orders[0].state === "waiting");
check("a further result never comes from the same client", (await work(A, { ip: "203.0.113.9" })).work === null);
check("...nor from another device of its owner", (await work(D, { ip: "203.0.113.8" })).work === null);
check("...nor from its network", (await work(B, { ip: "203.0.113.1" })).work === null);
const w2 = await work(B, { ip: "198.51.100.2" });
check("...but from another owner on another network, without the number to agree with",
  w2.work && w2.work.identity === "one" && !JSON.stringify(w2).includes(String(SCORE)), JSON.stringify(w2));
const agreed = await answer(w2.work, B, { ip: "198.51.100.2" });
check("equal bits make a fact, written to the board as the clients'", fact("one") && fact("one").score === SCORE
  && fact("one").measured_by === "verified:e1", JSON.stringify(fact("one")));
check("...the queue no longer owes the row", !owed("one"));
check("...and both clients are credited its work, once", credited(A) === WORK && credited(B) === WORK);
check("...and their tasks say confirmed", (await post(verifyRoute, "/api/board/mine",
  { verifier: B, orders: [{ identity: "one", ruler: "standard_single_target", mode: "base" }] })).orders[0].state === "confirmed");

build("two");
queue(arrivals, "two");
await ask(arrivals, "two");
free(); only("two");
const w3 = await work(C, { ip: "192.0.2.3" });
await answer(w3.work, C, { ip: "192.0.2.3" });
const w4 = await work(E, { ip: "192.0.2.4" });
const disagreed = await answer(w4.work, E, { ip: "192.0.2.4", work: WORK + 1 });
check("the same score with other work is a dispute, and no fact", question("two").state === "disputed" && !fact("two"));
check("...and the answer is the one an agreement gets", JSON.stringify(disagreed) === JSON.stringify(agreed));
check("...its clients' tasks say it is being checked", (await post(verifyRoute, "/api/board/mine",
  { verifier: C, orders: [{ identity: "two", ruler: "standard_single_target", mode: "base" }] })).orders[0].state === "checking");
check("...and nobody is credited", credited(C) === 0 && credited(E) === 0);

console.log("\nthe official machines");
check("only the factory's secret speaks for them", (await post(officialRoute, "/api/tasks/official/claim", { witness: "official:live", engine: "e1" })).ok === false);
const settle = await post(officialRoute, "/api/tasks/official/claim",
  { witness: "official:live", engine: "e1", states: ["disputed", "spot", "nondeterministic"], producers: ["new_build", "sweep"], max: 3 }, TOKEN);
check("the live loop claims what only it may settle", settle.tasks.length === 1 && settle.tasks[0].demands[0].payload.identity === "two",
  JSON.stringify(settle));
const told = await tell([{ identity: "two", ruler: "standard_single_target", mode: "base", score: SCORE, metric: "kpm", work: WORK }], "e1",
  (what, body) => post(officialRoute, `/api/tasks/official/${what}`, { ...body, witness: "official:live" }, TOKEN));
check("its fact settles the dispute", told === 1 && question("two").state === "fact");
check("...crediting the client it agrees with", credited(C) === WORK);
const cooled = L.prepare("SELECT banned, refusals, refused_until FROM verifiers WHERE id = ?").get(E);
check("...and refusing the one it disproves, for a day", cooled.banned === 1 && cooled.refusals === 1
  && Math.abs(Date.parse(cooled.refused_until) - Date.now() - coolDownMs(0)) < 60_000, JSON.stringify(cooled));
const banned = await work(E, { ip: "192.0.2.4" });
check("a refused client is handed nothing, and told why and until when", banned.work === null && banned.banned === true && banned.until);
check("a cool-down is a day, then three times as long each time", coolDownMs(0) === 86_400_000 && coolDownMs(2) === 9 * 86_400_000);

// A REFUSED CLIENT'S AGREEMENTS ARE WITHDRAWN: a spot check that disproves the
// two who agreed takes the clients' number off the board, owes the row again,
// and takes back what they were paid.
build("three");
queue(arrivals, "three");
await ask(arrivals, "three");
free(); only("three");
const G = "g".repeat(24), H = "h".repeat(24);
for (const v of [G, H]) L.prepare("INSERT INTO verifiers (id, seen) VALUES (?, '2026-10-08')").run(v);
const real = Math.random;
Math.random = () => 0;  // the dice pick this agreement for a spot check
await answer((await work(G, { ip: "192.0.2.7" })).work, G, { ip: "192.0.2.7", score: 9 });
await answer((await work(H, { ip: "192.0.2.8" })).work, H, { ip: "192.0.2.8", score: 9 });
Math.random = real;
check("an agreement the dice pick is a spot check: a fact now, the clients paid", question("three").state === "spot" && fact("three")
  && credited(G) === WORK);
const spot = await post(officialRoute, "/api/tasks/official/claim",
  { witness: "official:live", engine: "e1", states: ["spot"], producers: ["new_build", "sweep"], max: 3 }, TOKEN);
await tell([{ identity: "three", ruler: "standard_single_target", mode: "base", score: SCORE, metric: "kpm", work: WORK }], "e1",
  (what, body) => post(officialRoute, `/api/tasks/official/${what}`, { ...body, witness: "official:live" }, TOKEN));
check("a spot check that disproves them refuses both and takes back what they were paid",
  spot.tasks.length === 1 && credited(G) === 0 && credited(H) === 0
  && L.prepare("SELECT banned FROM verifiers WHERE id = ?").get(G).banned === 1);
check("...and the official fact stands", question("three").state === "fact"
  && L.prepare("SELECT count(*) AS n FROM answers WHERE void = 1 AND question = ?").get(question("three").id).n === 2);

// THE SCORER: past their deadline, a row is its to claim; it reads exactly what
// it claimed and what nobody asked the factory for, and its answer pays a client
// whose bits it reproduces.
build("late"); build("young"); build("orphan");
queue(arrivals, "late", "young", "orphan");
await ask(arrivals, "late", "young");
free(); only("late", "young");
L.prepare("UPDATE questions SET due_at = '2000-01-01T00:00:00.000Z' WHERE id = ?").run(question("late").id);
await answer((await work(F, { ip: "192.0.2.9" })).work, F, { ip: "192.0.2.9", score: 3 });
check("a late row's single result is open", question("late").state === "open" && L.prepare("SELECT count(*) AS n FROM answers WHERE question = ?").get(question("late").id).n === 1);
const claim = await post(officialRoute, "/api/tasks/official/claim",
  { witness: "official:scorer", engine: "e1", states: ["open"], producers: ["new_build", "sweep"], max: 100 }, TOKEN);
check("the scorer claims what is past its deadline and leaves a young row to the clients",
  claim.tasks.map((t) => t.demands[0].payload.identity).join() === "late" && question("young").leased_to === null, JSON.stringify(claim.tasks.length));
check("no client is handed a claimed row", (await work(C, { ip: "192.0.2.30" })).work.identity === "young");
free();
L.prepare("UPDATE questions SET lease = 'x', lease_until = '2999-01-01T00:00:00.000Z', leased_to = 'official:scorer' WHERE id = ?").run(question("late").id);
const page = JSON.parse(execFileSync("bash", ["scripts/fetch_queue.sh", "--body", "page", "100", "0"],
  { env: { ...process.env, CLAIMED: "1", WITNESS: "official:scorer" } }).toString());
const read = L.prepare(page.sql).all(...page.params).map((r) => r.build_id).sort();
check("...it reads exactly what it claimed and what nobody asked for", JSON.stringify(read) === JSON.stringify(["late", "orphan"]), JSON.stringify(read));
await tell([{ identity: "late", ruler: "standard_single_target", mode: "base", score: 3, metric: "kpm", work: WORK }], "e1",
  (what, body) => post(officialRoute, `/api/tasks/official/${what}`, { ...body, witness: "official:scorer" }, TOKEN));
check("...and its answer is the fact, paying the client whose bits it reproduced", question("late").state === "fact" && credited(F) === WORK);
check("...whose number the scorer itself writes to the board", !fact("late"));
L.prepare(`INSERT INTO scores (identity, ruler, mode, measured_by, score, metric, cost_seconds, started_at, finished_at)
  VALUES ('late', 'standard_single_target', 'base', 'abc123', 3, 'kpm', 1, 'x', '2026-01-01T00:00:00.000Z')`).run();
queue(`refresh-${today}`, "late");
await ask(`refresh-${today}`, "late");
const refreshed = L.prepare("SELECT * FROM demands WHERE producer = 'refresh' AND ref = ?").get("late|standard_single_target|base");
check("a `refresh-` batch's row is asked by `refresh`, due four days later",
  refreshed && Math.abs(Date.parse(refreshed.due_at) - Date.parse(refreshed.asked_at) - 4 * 86_400_000) < 1000, JSON.stringify(refreshed));
check("...and one whose official fact is on the served engine costs nothing: the row is measured now, its number kept",
  !owed("late") && fact("late").finished_at > "2026-01-01T00:00:00.000Z" && fact("late").measured_by === "abc123" && fact("late").score === 3,
  JSON.stringify(fact("late")));
await post(officialRoute, "/api/tasks/official/release", { witness: "official:scorer" }, TOKEN);
check("the release hands back what the run held", !L.prepare("SELECT 1 FROM questions WHERE leased_to = 'official:scorer'").get());

console.log("\nleases, ahead, engines");
free(); only("young");
const wy = await work(C, { ip: "192.0.2.30" });
const ahead = await work(C, { ip: "192.0.2.30", ahead: true });
check("a client near the end of a task may ask ahead, and holds no more than TASKS_AHEAD more", TASKS_AHEAD >= 1 && ahead.ahead === TASKS_AHEAD);
await post(verifyRoute, "/api/board/release", { lease: wy.work.lease, verifier: C });
check("a lease given back frees its row at once", question("young").leased_to === null);
check("a lease is held for longer than the slowest row", LEASE_MS >= 20 * 60_000);
const stale = await work(B, { ip: "198.51.100.40" });
const dropped = await answer(stale.work, B, { engine: "e2", ip: "198.51.100.40" });
check("an answer from another engine is dropped, and the row handed out again", dropped.ok
  && L.prepare("SELECT count(*) AS n FROM answers WHERE question = ?").get(question("young").id).n === 0 && question("young").leased_to === null);

console.log("\norders open before the factory");
{
  const at = "2026-10-09T00:00:00.000Z";
  build("legacy");
  queue("rescore-y", "legacy");
  L.prepare(`INSERT INTO orders (identity, ruler, mode, record, state, engine, slot, at, priority, score, metric, work, produced_by, clients, lease, lease_until, leased_to)
    VALUES ('legacy', 'standard_single_target', 'base', ?, 'open', 'e1', 1, ?, 1, ?, 'kpm', ?, ?, ?, NULL, NULL, NULL)`)
    .run(L.prepare("SELECT record FROM builds WHERE id = 'legacy'").get().record, at, SCORE, WORK, B, B);
  L.prepare(`INSERT INTO orders (identity, ruler, mode, record, state, engine, slot, at, priority, lease, lease_until, leased_to)
    VALUES ('gone', 'standard_single_target', 'base', '{}', 'todo', '', 2, ?, 0, NULL, NULL, NULL)`).run(at);
  check("only the factory's secret adopts", (await post(boardTasksRoute, "/api/board/adopt", {})).ok === false);
  const r = await post(boardTasksRoute, "/api/board/adopt", {}, TOKEN);
  check("an open order is asked of the factory, as of when it was opened, and nothing is left", r.left === 0 && demand("legacy")
    && demand("legacy").asked_at === at, JSON.stringify(r));
  check("...its client's result carried", L.prepare("SELECT count(*) AS n FROM answers WHERE question = ? AND device = ?").get(question("legacy").id, B).n === 1);
  check("...and an order nobody owes any more is settled where it is found", L.prepare("SELECT state FROM orders WHERE identity = 'gone'").get().state === "settled");
  free(); only("legacy");
  check("its client is not asked again", (await work(B, { ip: "198.51.100.50" })).work === null);
  const before = credited(B);
  const wl = await work(C, { ip: "192.0.2.50" });
  await answer(wl.work, C, { ip: "192.0.2.50" });
  check("...another agreeing with the carried result makes the fact and credits both", fact("legacy") && credited(B) === before + WORK);
}

{
  // AN ASK IS RECORDED: when, on which engine (an older protocol marked) and
  // with how many cores, which is what tells an idle page from a closed one.
  const O = "0".repeat(24);
  L.prepare("INSERT INTO verifiers (id, seen) VALUES (?, '2026-10-08')").run(O);
  await work(O, { engine: "old" });
  const v = L.prepare("SELECT asked_at, asked_engine, asked_lanes FROM verifiers WHERE id = ?").get(O);
  check("a page's ask for work is recorded, with its engine and its cores", v.asked_at && v.asked_engine === "old" && v.asked_lanes === 4, JSON.stringify(v));
  await work(O, { engine: "old", protocol: PROTOCOL - 1 });
  check("...and an older protocol is marked on it", L.prepare("SELECT asked_engine FROM verifiers WHERE id = ?").get(O).asked_engine === `old/p${PROTOCOL - 1}`);
  free();
}

console.log(failures ? `\n${failures} failed` : "\na board row reaches the board when CLIENTS_PER_FACT clients measured the same bits");
process.exitCode = failures ? 1 : 0;
