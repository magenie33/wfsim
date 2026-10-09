// COMPUTE ORDERS' DOOR (worker/verify.js), with no network: an order is handed
// out as a build and never a number; its first result waits for the server's
// rank; each further one must come from another client of the same engine;
// CLIENTS_PER_FACT equal results make a fact naming every client and settle the
// queue row, anything else is a dispute, and the
// answer never says which; a row nobody owes, a top-ten row, a live lease and a
// banned client get nothing; only the engine `release.json` names works, and
// what each fight cost its client is kept beside it. Equal includes the WORK,
// which a fact credits to every client that measured it; and a further result
// never comes from a device of the same owner (docs/BOARD.md §"Contribution").
// A scorer run's claim takes the old rows no client holds, and no client is
// handed one until its release (scripts/fetch_queue.sh). A claimed order the
// scorer reproduces to the same bits pays its clients (scripts/order_credit.mjs).
//   node scripts/check_board_verify.mjs
import { verifyRoute, LEASE_MS, PROTOCOL } from "../worker/verify.js";
import { creditConfirmed } from "./order_credit.mjs";
import { DatabaseSync } from "node:sqlite";
import { readFileSync, writeFileSync, mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { execFileSync } from "node:child_process";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const db = new DatabaseSync(":memory:");
db.exec(readFileSync(new URL("../worker/schema.sql", import.meta.url), "utf8"));
const accounts = new DatabaseSync(":memory:");
accounts.exec(readFileSync(new URL("../worker/accounts.sql", import.meta.url), "utf8"));
const stmtIn = (base) => function stmt(sql, args = []) {
  return {
    bind: (...a) => stmt(sql, a),
    first: async () => base.prepare(sql).get(...args) ?? null,
    all: async () => ({ results: base.prepare(sql).all(...args) }),
    run: async () => ({ meta: { changes: Number(base.prepare(sql).run(...args).changes) } }),
  };
};
const stmt = (sql, args = []) => ({
  bind: (...a) => stmt(sql, a),
  first: async () => db.prepare(sql).get(...args) ?? null,
  all: async () => ({ results: db.prepare(sql).all(...args) }),
  run: async () => ({ meta: { changes: Number(db.prepare(sql).run(...args).changes) } }),
  each: async () => (/^\s*select/i.test(sql) ? { results: db.prepare(sql).all(...args) } : { meta: { changes: Number(db.prepare(sql).run(...args).changes) } }),
});
/// THE SITE'S `release.json`, naming the engine it serves.
const site = (engine, release = "r1") => ({ fetch: async () => new Response(JSON.stringify({ engine, release })) });
const env = { LIBRARY: { prepare: (sql) => stmt(sql), batch: async (ss) => Promise.all(ss.map((x) => x.each())) }, ASSETS: site("e1"),
  ACCOUNTS: { prepare: (sql) => stmtIn(accounts)(sql) } };
const WORK = 7_000_000_000;
const callIn = async (e, path, body) =>
  (await verifyRoute(new Request(`https://x${path}`, { method: "POST", body: JSON.stringify(body) }), e, path)).json();
const call = (path, body) => callIn(env, path, body);
const order = (identity, state = "todo", extra = {}) => {
  db.prepare(`INSERT INTO orders (identity, ruler, mode, record, state, slot, at) VALUES (?, 'standard_single_target', 'base', ?, ?, ?, 0)`)
    .run(identity, JSON.stringify({ weapon: "braton_prime", mods: ["serration"] }), state, Math.floor(Math.random() * 1e9));
  db.prepare("INSERT INTO queue (batch, build_id, ruler, mode) VALUES ('arrivals', ?, 'standard_single_target', 'base')").run(identity);
  if (extra.score !== undefined && extra.work === undefined) extra = { ...extra, work: WORK };
  const sets = Object.keys(extra);
  if (sets.length) db.prepare(`UPDATE orders SET ${sets.map((k) => `${k} = ?`).join(", ")} WHERE identity = ?`).run(...Object.values(extra), identity);
};
const row = (identity) => db.prepare("SELECT * FROM orders WHERE identity = ?").get(identity);
const fact = (identity) => db.prepare("SELECT * FROM scores WHERE identity = ?").get(identity);
const only = (identity) => db.prepare("UPDATE orders SET slot = CASE WHEN identity = ? THEN 1 ELSE slot END").run(identity);
const YES = { v: 1, at: "2026-10-08T08:00:00.000Z" };
const work = (v, engine = "e1", protocol = PROTOCOL, consent = YES) => call("/api/board/work", { verifier: v, engine, protocol, consent });
const answer = (w, v, score, metric = "kpm", engine = "e1", compute_ms = 1000, work = WORK) =>
  call("/api/board/verify", { lease: w.lease, verifier: v, engine, score, metric, work, compute_ms });
const A = "a".repeat(24), B = "b".repeat(24), C = "c".repeat(24), D = "d".repeat(24);
const SCORE = 1.1070976928071055;
Math.random = () => 0.5;  // no spot check unless a test asks for one

order("one");
check("a page that cannot fill an order is handed none", (await work(A, "e1", null)).work === null && row("one").lease === null);
check("a computer that never said yes is handed nothing", (await work(A, "e1", PROTOCOL, null)).work === null
  && (await work(A, "e1", PROTOCOL, { v: 1, at: "yesterday" })).work === null);
const first = await work(A);
check("...and the yes it sent is kept on its row, the statement and when",
  JSON.stringify(db.prepare("SELECT consent_v AS v, consent_at AS at FROM verifiers WHERE id = ?").get(A)) === JSON.stringify(YES));
check("an order is handed out as its build", first.work && first.work.record.weapon === "braton_prime", JSON.stringify(first));
// A CLIENT THAT ASKS AGAIN IS WORKING ON NOTHING: what it held is taken back and
// handed out anew, and an answer under the lost lease counts for nothing.
const again = await work(A);
check("a client asking again gets its lost order back under a new lease, never a second one",
  again.work && again.work.lease !== first.work.lease && row("one").lease === again.work.lease
  && db.prepare("SELECT COUNT(*) AS n FROM orders WHERE leased_to = ?").get(A).n === 1, JSON.stringify(again));
await answer(first.work, A, SCORE);
check("...and the lost lease's answer is dropped", row("one").state === "todo" && row("one").score === null);
check("every answer names the release the site serves", again.release === "r1", JSON.stringify(again));
await answer(again.work, A, SCORE);
check("the first result makes it fresh, kept with who measured it and on which engine",
  row("one").state === "fresh" && row("one").score === SCORE && row("one").produced_by === A && row("one").engine === "e1");
check("...and no fact yet", !fact("one"));
check("a fresh order waits for the server's rank and is handed to nobody", (await work(B)).work === null);

db.prepare("UPDATE orders SET state = 'open' WHERE identity = 'one'").run();
check("an open order is never handed back to the client that measured it", (await work(A)).work === null);
check("...nor to a client of another engine", (await work(C, "e2")).work === null);
const second = await work(B);
check("...but to another client of its engine", second.work && second.work.record.weapon === "braton_prime" && !second.stale);
check("...and NOT with the number to agree with", second.work && !JSON.stringify(second.work).includes(String(SCORE)));
const agreed = await answer(second.work, B, SCORE);
check("equal bits make a fact", fact("one") && fact("one").score === SCORE && fact("one").measured_by === "verified:e1", JSON.stringify(fact("one")));
check("...the order is verified by the second client", row("one").state === "verified" && row("one").verifier === B);
check("...and the queue no longer owes the row", !db.prepare("SELECT 1 FROM queue WHERE build_id = 'one'").get());

order("two", "open", { score: SCORE, metric: "kpm", engine: "e1", produced_by: A });
only("two");
const w2 = await work(C);
const disagreed = await answer(w2.work, C, SCORE * 2);
check("a different number makes a dispute, and no fact", row("two").state === "dispute" && !fact("two") && row("two").disputed === SCORE * 2);
check("...and the answer is the same one an agreement gets", JSON.stringify(disagreed) === JSON.stringify(agreed));

order("three", "open", { score: SCORE, metric: "kpm", engine: "e1", produced_by: A });
only("three");
const w3 = await work(D);
await answer(w3.work, D, SCORE, "dps");
check("the same number in another metric is a dispute", row("three").state === "dispute");

order("four");
db.prepare("DELETE FROM queue WHERE build_id = 'four'").run();
only("four");
check("an order nobody owes any more is handed to nobody", (await work("e".repeat(24))).work === null);
check("...and is settled where it was found", row("four").state === "settled");

order("five", "arbiter", { score: SCORE, metric: "kpm", engine: "e1", produced_by: A });
only("five");
check("a top-ten order is the server's, never a client's", (await work("f".repeat(24))).work === null);

order("six");
only("six");
const G = "g".repeat(24), H = "h".repeat(24);
const w6 = await work(G);
db.prepare("UPDATE orders SET lease_until = ? WHERE identity = 'six'").run(Date.now() - 1);
await answer(w6.work, G, SCORE);
check("an answer on an expired lease is nobody's", row("six").state === "todo" && row("six").score === null);
const w6b = await work(H);
check("...and the order goes to the next client", w6b.work && row("six").leased_to === H);

order("seven", "open", { score: SCORE, metric: "kpm", engine: "e1", produced_by: A });
only("seven");
const I = "i".repeat(24);
const w7 = await work(I);
Math.random = () => 0;
await answer(w7.work, I, SCORE);
check("an agreement the dice pick is a spot check: a fact now, recomputed by the server",
  row("seven").state === "spot" && fact("seven") && fact("seven").score === SCORE);
Math.random = () => 0.5;

const J = "j".repeat(24);
order("eight");
await work(J);
const coolUntil = Date.now() + 3_600_000;
db.prepare("UPDATE verifiers SET banned = 1, refusals = 1, refused_until = ? WHERE id = ?").run(coolUntil, J);
db.prepare("UPDATE orders SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE identity = 'eight'").run();
const refused = await work(J);
check("a client in its cool-down is handed nothing, and told why and until when",
  refused.work === null && refused.banned === true && refused.until === coolUntil, JSON.stringify(refused));
db.prepare("UPDATE verifiers SET refused_until = ? WHERE id = ?").run(Date.now() - 1, J);
db.prepare("UPDATE orders SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE identity = 'eight'").run();
const lifted = await work(J);
check("...and once it is over, it works again and the refusal is lifted",
  !lifted.banned && db.prepare("SELECT banned FROM verifiers WHERE id = ?").get(J).banned === 0, JSON.stringify(lifted));
const seen = db.prepare("SELECT seen FROM verifiers WHERE id = ?").get(A).seen;
check("a client is written once a day, not once a poll", seen === new Date().toISOString().slice(0, 10)
  && db.prepare("SELECT COUNT(*) AS n FROM verifiers WHERE id = ?").get(A).n === 1);
check("a malformed id is refused", (await verifyRoute(new Request("https://x/api/board/work",
  { method: "POST", body: JSON.stringify({ verifier: "x", engine: "e1" }) }), env, "/api/board/work")).status === 400);
check("a lease is held for longer than the slowest row", LEASE_MS >= 20 * 60_000);

check("a fact names every client that measured it, first to last", row("one").clients === `${A},${B}`, row("one").clients);

// CLIENTS_PER_FACT: how many different clients must send the same bits.
const callWith = async (n, path, body) => (await verifyRoute(new Request(`https://x${path}`,
  { method: "POST", body: JSON.stringify(body) }), { ...env, CLIENTS_PER_FACT: n }, path)).json();
const workWith = (n, v) => callWith(n, "/api/board/work", { verifier: v, engine: "e1", protocol: PROTOCOL, consent: YES });
const answerWith = (n, w, v) => callWith(n, "/api/board/verify", { lease: w.lease, verifier: v, engine: "e1", score: SCORE, metric: "kpm", work: WORK });
const K = "k".repeat(24), L = "l".repeat(24), M = "m".repeat(24), N = "n".repeat(24);

db.prepare("UPDATE orders SET state = 'settled'").run();  // the orders above are not these checks
order("solo");
only("solo");
await answerWith(1, await workWith(1, K).then((r) => r.work), K);
check("with one client a fact, the first result is the fact", fact("solo") && row("solo").clients === K && row("solo").state === "verified");

order("trio", "open", { score: SCORE, metric: "kpm", engine: "e1", produced_by: L, clients: L });
only("trio");
await answerWith(3, (await workWith(3, M)).work, M);
check("with three, a second agreement is not yet a fact", !fact("trio") && row("trio").state === "open" && row("trio").clients === `${L},${M}`);
check("...and the order goes to neither client again", (await workWith(3, L)).work === null && (await workWith(3, M)).work === null);
await answerWith(3, (await workWith(3, N)).work, N);
check("...the third makes it", fact("trio") && row("trio").clients === `${L},${M},${N}` && row("trio").verifier === N);

// THE ENGINE LOCK, and what a fight cost.
db.prepare("UPDATE orders SET state = 'settled'").run();
order("lock");
only("lock");
const P = "p".repeat(24), Q = "q".repeat(24);
check("a tab of an engine the site does not serve is handed nothing", (await work("o".repeat(24), "e0")).work === null);
check("...and is told it is stale, so a machine left computing reloads", (await work("o".repeat(24), "e0")).stale === true
  && (await work("o".repeat(24), "e1", PROTOCOL - 1)).stale === true);
const blind = { ...env, ASSETS: { fetch: async () => new Response("missing", { status: 404 }) } };
check("...and nobody is while the served engine cannot be read",
  (await callIn(blind, "/api/board/work", { verifier: P, engine: "e1", protocol: PROTOCOL, consent: YES })).work === null);
await answer((await work(P)).work, P, SCORE, "kpm", "e0");
check("an answer from another engine is dropped, and the order handed out again",
  row("lock").state === "todo" && row("lock").lease === null && row("lock").score === null);
await answer((await work(P)).work, P, SCORE, "kpm", "e1", 4321);
check("what the fight cost is kept beside the client that fought it",
  row("lock").clients_compute_ms === "4321"
  && db.prepare("SELECT compute_ms FROM verifiers WHERE id = ?").get(P).compute_ms === 4321);
check("...and both agreeing clients' costs are kept on a fact, in order", row("one").clients_compute_ms === "1000,1000",
  row("one").clients_compute_ms);
db.prepare("UPDATE orders SET state = 'open' WHERE identity = 'lock'").run();
// A RELEASE CARRIES A RESULT: the new engine's client confirms it or replaces it.
const released = (e) => {
  const at = { ...env, ASSETS: site(e) };
  return {
    work: (v) => callIn(at, "/api/board/work", { verifier: v, engine: e, protocol: PROTOCOL, consent: YES }),
    answer: (w, v, score, w2 = WORK) => callIn(at, "/api/board/verify", { lease: w.lease, verifier: v, engine: e, score, metric: "kpm", work: w2, compute_ms: 1000 }),
  };
};
const e9 = released("e9");
const carried = await e9.work(Q);
check("once the site serves a new engine, an open result of the old one is carried to it, clients and all",
  row("lock").state === "open" && row("lock").engine === "e9" && row("lock").carried_from === "e1" && row("lock").clients === P
  && row("lock").score === SCORE && carried.work && row("lock").leased_to === Q, JSON.stringify(row("lock")));
await e9.answer(carried.work, Q, SCORE);
check("...and the new engine reproducing its bits makes the fact, both clients on it",
  row("lock").state === "verified" && row("lock").clients === `${P},${Q}` && fact("lock") && fact("lock").measured_by === "verified:e9");
const REPLACER = "1".repeat(24);
db.prepare("UPDATE orders SET state = 'settled'").run();
order("moved", "open", { score: SCORE, metric: "kpm", engine: "e9", produced_by: P, clients: P });
const e8 = released("e8");
const moved = await e8.work(REPLACER);
await e8.answer(moved.work, REPLACER, SCORE * 3);
check("...one it does not reproduce is replaced by the new result as the first, and nobody is refused",
  row("moved").state === "fresh" && row("moved").score === SCORE * 3 && row("moved").clients === REPLACER && row("moved").produced_by === REPLACER
  && row("moved").carried_from === null && !fact("moved")
  && !db.prepare("SELECT banned FROM verifiers WHERE id = ?").get(P).banned, JSON.stringify(row("moved")));

// A NEW BUILD'S ROWS ARE LEASED BEFORE A RESCORE'S.
db.prepare("UPDATE orders SET state = 'settled'").run();
order("rescore");
order("arrival", "todo", { priority: 0 });
db.prepare("UPDATE orders SET slot = CASE identity WHEN 'rescore' THEN 1 ELSE 2000000000 END").run();
const EARLY = "2".repeat(24);
check("a new build's row is leased before a rescore's, wherever their slots fall",
  (await e8.work(EARLY)).work && row("arrival").leased_to === EARLY && row("rescore").leased_to === null);
db.prepare("UPDATE orders SET state = 'settled' WHERE identity = 'arrival'").run();
const LATER = "3".repeat(24);
check("...and a rescore's when no new build's is left", (await e8.work(LATER)).work && row("rescore").leased_to === LATER);

// THE WORK, and the owners.
const credited = (v) => db.prepare("SELECT work FROM verifiers WHERE id = ?").get(v).work;
check("a fact credits its work to every client that measured it, once a fact",
  credited(B) === WORK && credited(I) === WORK && credited(A) === 2 * WORK, `${credited(A)} ${credited(B)} ${credited(I)}`);
const creditedToday = (v) => db.prepare("SELECT SUM(work) AS w FROM verifier_days WHERE verifier = ? AND day = ?")
  .get(v, new Date().toISOString().slice(0, 10)).w;
check("...and the same work under the day it was credited, for the ranking's last thirty days",
  creditedToday(A) === 2 * WORK && creditedToday(B) === WORK && creditedToday(I) === WORK,
  `${creditedToday(A)} ${creditedToday(B)} ${creditedToday(I)}`);
check("...and a disputed one to nobody", credited(C) === 0 && credited(D) === 0);
db.prepare("UPDATE orders SET state = 'settled'").run();
order("worked", "open", { score: SCORE, metric: "kpm", engine: "e1", produced_by: A, clients: A });
only("worked");
const R = "r".repeat(24);
await answer((await work(R)).work, R, SCORE, "kpm", "e1", 1000, WORK + 1);
check("the same score with other work is a dispute", row("worked").state === "dispute" && !fact("worked"));
check("a result that does not say its work is refused", (await verifyRoute(new Request("https://x/api/board/verify",
  { method: "POST", body: JSON.stringify({ lease: "a".repeat(32), verifier: R, engine: "e1", score: SCORE, metric: "kpm" }) }),
env, "/api/board/verify")).status === 400);

const S = "s".repeat(24), T = "t".repeat(24), U = "u".repeat(24), V = "v".repeat(24);
for (const [id, name] of [["acct1", "one"], ["acct2", "two"]]) {
  accounts.prepare("INSERT INTO accounts (id, created_at, username) VALUES (?, '2026-01-01', ?)").run(id, `owner_${name}`);
}
const own = (v, a) => accounts.prepare("INSERT INTO devices (verifier, account, claimed_at) VALUES (?, ?, '2026-01-01')").run(v, a);
own(S, "acct1"); own(T, "acct1"); own(U, "acct2");
db.prepare("UPDATE orders SET state = 'settled'").run();
order("owned", "open", { score: SCORE, metric: "kpm", engine: "e1", produced_by: S, clients: S });
only("owned");
check("a further result never comes from another device of the same owner", (await work(T)).work === null);
const wu = await work(U);
check("...but from another owner's", wu.work && row("owned").leased_to === U);
db.prepare("UPDATE orders SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE identity = 'owned'").run();
check("...or from a device nobody claimed", (await work(V)).work && row("owned").leased_to === V);

// THE CLAIM (scripts/fetch_queue.sh): an old order nobody holds is the scorer
// run's, and while it is no client is handed it; one a client holds stays its.
const body = (...a) => JSON.parse(execFileSync("bash", ["scripts/fetch_queue.sh", "--body", ...a],
  { env: { ...process.env, HOLD_SECONDS: "14400" }, encoding: "utf8" }));
const sql = (b) => db.prepare(b.sql.replaceAll("unixepoch()", String(Math.floor(Date.now() / 1000))));
const W = "w".repeat(24), X = "x".repeat(24), Y = "y".repeat(24);
db.prepare("UPDATE orders SET state = 'settled'").run();
db.prepare("DELETE FROM queue").run();
db.prepare("INSERT INTO batches (id, at, why, total) VALUES ('arrivals', '2026-01-01', 'check', 4)").run();
order("old");
order("oldopen", "open", { score: SCORE, metric: "kpm", engine: "e1", produced_by: R, clients: R });
order("oldheld", "todo", { leased_to: X, lease: "b".repeat(32), lease_until: Date.now() + LEASE_MS });
order("young");
db.prepare("UPDATE orders SET at = ? WHERE identity = 'young'").run(Date.now() - 60_000);
const reserve = sql(body("reserve")).get(...body("reserve").params);
check("the reserve counts the unmeasured orders a client could take now, young or old, and what of them is old",
  reserve.open_to_clients === 2 && reserve.old === 1
  && reserve.facts_last_hour === db.prepare("SELECT COUNT(*) AS n FROM scores WHERE measured_by LIKE 'verified:%'").get().n
  && reserve.facts_last_hour > 0, JSON.stringify(reserve));
sql(body("claim", "open")).run(...body("claim", "open").params);
sql(body("claim", "todo", "100")).run(...body("claim", "todo", "100").params);
check("a run claims every old order no client holds, open or not",
  row("old").state === "scoring:todo" && row("oldopen").state === "scoring:open", `${row("old").state} ${row("oldopen").state}`);
check("...and leaves a held one and a young one to the clients", row("oldheld").state === "todo" && row("young").state === "todo");
const page = body("page", "100", "0");
const read = sql(page).all(...page.params).map((r) => r.build_id).sort();
check("...and reads exactly what it claimed", JSON.stringify(read) === JSON.stringify(["old", "oldopen"]), JSON.stringify(read));
const wy = await work(W);
check("no client is handed a claimed order", wy.work && row("young").leased_to === W && (await work(Y)).work === null);
sql(body("release")).run(...body("release").params);
check("the release hands what the run left back as it was", row("old").state === "todo" && row("oldopen").state === "open");
sql(body("claim", "open")).run(...body("claim", "open").params);
sql(body("claim", "todo", "0")).run(...body("claim", "todo", "0").params);
check("a run with no share of the unmeasured still takes every old open order, which no lone computer can settle",
  row("old").state === "todo" && row("oldopen").state === "scoring:open", `${row("old").state} ${row("oldopen").state}`);
const rowsFor = (old, open, facts) => Number(execFileSync("bash", ["-c", `source <(sed -n "/^claim_rows() {/,/^}/p" scripts/fetch_queue.sh); claim_rows ${old} ${open} ${facts}`],
  { encoding: "utf8" }).trim());
check("...the clients keep two hours of what they made, never fewer than 500, and the run takes only the old beyond it",
  rowsFor(6000, 6500, 300) === 5900 && rowsFor(6000, 6500, 0) === 6000 && rowsFor(100, 300, 50) === 0 && rowsFor(6000, 6500, 3500) === 0,
  JSON.stringify([rowsFor(6000, 6500, 300), rowsFor(6000, 6500, 0), rowsFor(100, 300, 50), rowsFor(6000, 6500, 3500)]));
sql(body("release")).run(...body("release").params);

// A FIRST RESULT THE LIVE LOOP NEVER RANKED is claimed after the hold and,
// reproduced by the scorer, pays its client like any agreement.
db.prepare("UPDATE orders SET state = 'settled'").run();
const UR = "u".repeat(24);
db.prepare("INSERT OR IGNORE INTO verifiers (id, seen) VALUES (?, '2026-01-01')").run(UR);
order("unranked", "fresh", { score: SCORE, metric: "kpm", engine: "e1", produced_by: UR, clients: UR });
sql(body("claim", "fresh")).run(...body("claim", "fresh").params);
const urPaid = await creditConfirmed(async (s2, p2 = []) => (/^\s*select|returning/i.test(s2) ? db.prepare(s2).all(...p2) : (db.prepare(s2).run(...p2), [])),
  [{ identity: "unranked", ruler: "standard_single_target", mode: "base", score: SCORE, metric: "kpm", work: WORK }], "e1");
check("a first result the live loop never ranked is claimed after the hold, and paid once the scorer reproduces it",
  urPaid === 1 && row("unranked").state === "verified" && db.prepare("SELECT work FROM verifiers WHERE id = ?").get(UR).work === WORK,
  `${urPaid} ${row("unranked").state}`);

// A LEASE GIVEN BACK is anyone's at once, and its client may take another.
db.prepare("UPDATE orders SET state = 'settled'").run();
order("given"); order("next");
const GB = "q".repeat(24);
const gbFirst = (await work(GB)).work;
const gbTaken = row("given").leased_to === GB ? "given" : "next";
await call("/api/board/release", { lease: gbFirst.lease, verifier: GB });
check("a lease given back frees its order and its client at once",
  gbFirst && row(gbTaken).lease === null && !!(await work(GB)).work, `${gbTaken} ${row(gbTaken).lease}`);
const gbSpare = (await work(Y)).work;
check("...and a client cannot give back a lease it does not hold",
  (await call("/api/board/release", { lease: (gbSpare || {}).lease || "c".repeat(32), verifier: GB })).ok
  && (!gbSpare || db.prepare("SELECT leased_to FROM orders WHERE lease = ?").get(gbSpare.lease).leased_to === Y));

// THE SCORER'S FACT PAYS THE CLIENT IT REPRODUCES, once, and no other.
const q = async (s, p = []) => (/^\s*select|returning/i.test(s) ? db.prepare(s).all(...p) : (db.prepare(s).run(...p), []));
const Z = "z".repeat(24);
db.prepare("INSERT OR IGNORE INTO verifiers (id, seen) VALUES (?, '2026-01-01')").run(Z);
const workOf = (v) => db.prepare("SELECT work FROM verifiers WHERE id = ?").get(v).work;
for (const id of ["same", "otherscore", "otherwork", "otherengine"]) {
  order(id, "scoring:open", { score: SCORE, metric: "kpm", engine: id === "otherengine" ? "e0" : "e1", produced_by: Z, clients: Z });
}
const scorer = (identity, score = SCORE, w = WORK) => ({ identity, ruler: "standard_single_target", mode: "base", score, metric: "kpm", work: w });
const facts = [scorer("same"), scorer("otherscore", SCORE + 1), scorer("otherwork", SCORE, WORK + 1), scorer("otherengine")];
const paid = [await creditConfirmed(q, facts.slice(0, 3), "e1"), await creditConfirmed(q, facts.slice(3), "e1")];
check("a client result the scorer reproduces is paid to its client",
  paid[0] === 1 && row("same").state === "verified", `${paid} ${row("same").state} ${workOf(Z)}`);
check("...never one that differs in score or work",
  ["otherscore", "otherwork"].every((id) => row(id).state === "scoring:open"));
check("...and one of another engine id with the same bits is paid too: the bits are the witness",
  paid[1] === 1 && row("otherengine").state === "verified" && workOf(Z) === 2 * WORK, `${paid} ${row("otherengine").state} ${workOf(Z)}`);
check("...and only once", (await creditConfirmed(q, facts, "e1")) === 0 && workOf(Z) === 2 * WORK);

// A NEW BUILD'S ROWS ARE ASKED FOR ONLY WHERE NO SCORE IS (scripts/ship_queue.sh);
// a rescore's batch asks for scored rows too.
const rows = join(mkdtempSync(join(tmpdir(), "owed-")), "rows.ndjson");
writeFileSync(rows, ["scored", "unscored"].map((id) => JSON.stringify({ build_id: id, ruler: "standard_single_target", mode: "base" })).join("\n") + "\n");
const shipped = (what, batch) => execFileSync("bash", ["scripts/ship_queue.sh", "--body", what, ...(what === "queue" ? [batch, rows] : [rows, batch])],
  { encoding: "utf8" }).split("\n").filter(Boolean).map((l) => JSON.parse(l));
const run = (what, batch) => { for (const b of shipped(what, batch)) sql(b).run(...b.params); };
db.prepare("DELETE FROM queue").run();
db.prepare("INSERT OR IGNORE INTO batches (id, at, why, total) VALUES ('arrivals-2026-01-01', '2026-01-01', 'check', 2), ('rescore-t', '2026-01-01', 'check', 2)").run();
for (const id of ["scored", "unscored"]) db.prepare("INSERT INTO builds (id, at, record) VALUES (?, '2026-01-01', ?)").run(id, JSON.stringify({ weapon: "braton_prime" }));
order("scored", "verified", { score: SCORE, metric: "kpm", engine: "e1", produced_by: R, clients: R });
db.prepare("DELETE FROM queue").run();
db.prepare(`INSERT INTO scores (identity, ruler, mode, measured_by, score, metric, cost_seconds, started_at, finished_at)
            VALUES ('scored', 'standard_single_target', 'base', 'verified:e1', ?, 'kpm', 0, 'T0', 'T1')`).run(SCORE);
run("queue", "arrivals-2026-01-01"); run("orders", "arrivals-2026-01-01");
const queued = () => db.prepare("SELECT DISTINCT build_id FROM queue ORDER BY build_id").all().map((r) => r.build_id).join(",");
check("a resubmitted build's scored row is not asked for again, its order left as it was",
  queued() === "unscored" && row("scored").state === "verified" && row("unscored").state === "todo" && row("unscored").priority === 0,
  `${queued()} ${row("scored").state}`);
run("queue", "rescore-t"); run("orders", "rescore-t");
check("...while a rescore asks for it", queued() === "scored,unscored" && row("scored").state === "todo"
  && row("scored").priority === 1);

// AN OLD BUILD ASKED FOR A RULER IT LACKS lands in the day's arrivals batch and
// is a sweep; EVERY OWED ROW GETS ITS ORDER, once (scripts/ship_queue.sh).
db.prepare("UPDATE orders SET state = 'settled'").run();
db.prepare("DELETE FROM queue").run();
db.prepare("INSERT OR IGNORE INTO batches (id, at, why, total) VALUES ('arrivals-2026-03-01', '2026-03-01', 'check', 3)").run();
for (const [id, at] of [["old", "2026-01-01"], ["new", "2026-03-01"], ["held", "2026-03-01"]]) {
  db.prepare("INSERT OR IGNORE INTO builds (id, at, record) VALUES (?, ?, ?)").run(`bf-${id}`, at, JSON.stringify({ weapon: "braton_prime" }));
  db.prepare("INSERT INTO queue (batch, build_id, ruler, mode) VALUES ('arrivals-2026-03-01', ?, 'heavy_gunner', 'base')").run(`bf-${id}`);
}
db.prepare(`INSERT INTO orders (identity, ruler, mode, record, state, slot, at) VALUES ('bf-held', 'heavy_gunner', 'base', '{}', 'open', 1, 0)`).run();
const backfill = () => { const b = JSON.parse(execFileSync("bash", ["scripts/ship_queue.sh", "--body", "backfill"], { encoding: "utf8" }));
  return Number(sql(b).run(...b.params).changes); };
const bf = (id) => db.prepare("SELECT state, priority FROM orders WHERE identity = ? AND ruler = 'heavy_gunner'").get(`bf-${id}`);
const opened = backfill();
check("an owed row with no order is given one — a new build's first, an old build's as a sweep — and one that has an order is left",
  opened === 2 && bf("new").state === "todo" && bf("new").priority === 0 && bf("old").priority === 1 && bf("held").state === "open",
  `${opened} ${JSON.stringify([bf("new"), bf("old"), bf("held")])}`);
check("...and asking again opens nothing", backfill() === 0);

// A FURTHER RESULT NEVER COMES FROM THE FIRST ONE'S NETWORK: two browsers on one
// desk agreeing are one witness counted twice. The network is a salted hash,
// cleared when the order becomes a fact.
{
  const from = (ip) => ({
    work: async (v) => (await verifyRoute(new Request("https://x/api/board/work", { method: "POST", headers: { "cf-connecting-ip": ip },
      body: JSON.stringify({ verifier: v, engine: "e1", protocol: PROTOCOL, consent: YES }) }), env, "/api/board/work")).json(),
    answer: async (w, v) => (await verifyRoute(new Request("https://x/api/board/verify", { method: "POST", headers: { "cf-connecting-ip": ip },
      body: JSON.stringify({ lease: w.lease, verifier: v, engine: "e1", score: SCORE, metric: "kpm", work: WORK, compute_ms: 1000 }) }), env, "/api/board/verify")).json(),
  });
  db.prepare("UPDATE orders SET state = 'settled'").run();
  order("desk");
  const N1 = "4".repeat(24), N2 = "5".repeat(24), N3 = "6".repeat(24);
  const home = from("203.0.113.7"), elsewhere = from("198.51.100.9");
  await home.answer((await home.work(N1)).work, N1);
  db.prepare("UPDATE orders SET state = 'open' WHERE identity = 'desk'").run();
  const nets = row("desk").clients_nets;
  check("the first result's network is kept on the order as a salted hash, never the address",
    /^[0-9a-f]{12}$/.test(nets) && !nets.includes("203"), nets);
  check("a further result is never handed to a browser on the same network", (await home.work(N2)).work === null);
  const w3 = await elsewhere.work(N3);
  check("...but to one on another", w3.work && row("desk").leased_to === N3);
  await elsewhere.answer(w3.work, N3);
  check("...and the networks are forgotten once the order is a fact", row("desk").state === "verified" && row("desk").clients_nets === "",
    `${row("desk").state} ${row("desk").clients_nets}`);
}

console.log(failures ? `\n${failures} failed` : "\nan order reaches the board when CLIENTS_PER_FACT clients measured the same bits");
process.exitCode = failures ? 1 : 0;
