// A PACK'S LEASE IS A HEARTBEAT (worker/tasks.js `renewRow`, worker/verify.js
// `renew`): a row its page is still computing is kept for it — even once its
// lease ran out, as long as nobody took it — and its answer then counts; a row
// taken since is not. Past the ceiling nobody is cut off: another computer may
// take the row too, the holder computing longest kept as the one before, whose
// answer still counts; one owner's two holders answering are one witness.
//   node scripts/check_renew.mjs
import { verifyRoute, PROTOCOL } from "../worker/verify.js";
import { boardTasksRoute } from "../worker/board_tasks.js";
import { RENEW_MS, ROW_CEILING_MS } from "../worker/tasks.js";
import { testDb } from "./pg/test_db.mjs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const LIBRARY = await testDb("library"), ACCOUNTS = await testDb("accounts"), L = LIBRARY;
const env = { LIBRARY, ACCOUNTS, ASSETS: { fetch: async () => new Response(JSON.stringify({ engine: "e1", release: "r1" })) }, FACTORY_TOKEN: "ft" };
const TOKEN = { authorization: "Bearer ft" };
const post = async (route, path, body, headers = {}) =>
  (await route(new Request(`https://x${path}`, { method: "POST", headers, body: JSON.stringify(body) }), env, path)).json();
const YES = { v: 1, at: "2026-10-08T08:00:00.000Z" };
const work = (v) => post(verifyRoute, "/api/board/work", { verifier: v, engine: "e1", protocol: PROTOCOL, consent: YES, lanes: 4, pace_ms: 3000 });
const renew = (v, lease, question) => post(verifyRoute, "/api/board/renew", { verifier: v, lease, question });
const iso = (ms) => new Date(ms).toISOString();
const lapse = (id, ms) => L.run("UPDATE questions SET lease_until = ? WHERE id = ?", iso(Date.now() - ms), id);
const q = (id) => L.get("SELECT * FROM questions WHERE id = ?", id);

const A = "a".repeat(24), B = "b".repeat(24);
await L.run("INSERT INTO batches (id, at, why, total) VALUES ('b1', 'b1', 'test', 0)");
const ids = ["r1", "r2", "r3"];
for (const id of ids) {
  await L.run("INSERT INTO builds (id, at, record) VALUES (?, ?, ?)", id, iso(Date.now()), JSON.stringify({ weapon: "braton_prime", mods: ["serration", id] }));
  await L.run("INSERT INTO queue (batch, build_id, ruler, mode) VALUES ('b1', ?, 'standard_single_target', 'base')", id);
}
await post(boardTasksRoute, "/api/board/ask", { batch: "b1", rows: ids.map((identity) => ({ identity, ruler: "standard_single_target", mode: "base" })) }, TOKEN);

const pack = (await work(A)).pack;
const [t1, t2] = pack.tasks;
check("the three rows go in one pack", pack.tasks.length === 3, JSON.stringify(pack.tasks.length));
check("a pack's rows carry when they were leased", !!(await q(t1.question)).leased_at, JSON.stringify(await q(t1.question)));

// THE ROW RUNS LONG: its lease ran out a minute ago, and nobody took it.
await lapse(t1.question, 60_000);
const kept = await renew(A, pack.lease, t1.question);
check("a row whose lease ran out while its page computed it is kept, when nobody took it",
  kept.ok && kept.held === true && kept.renew_ms === RENEW_MS && Date.parse((await q(t1.question)).lease_until) > Date.now() + RENEW_MS - 5_000, JSON.stringify(kept));
await post(verifyRoute, "/api/board/answers", { lease: pack.lease, verifier: A, engine: "e1", done: false,
  answers: [{ question: t1.question, score: 1.5, metric: "kpm", work: 1000, compute_ms: 40 * 60_000 }] });
check("...and its answer, forty minutes in, is taken",
  !!await L.get("SELECT 1 FROM answers WHERE question = ? AND device = ?", t1.question, A));

// THE NEXT ROWS' leases ran out meanwhile, and another computer took one.
await lapse(t2.question, 60_000);
await lapse(pack.tasks[2].question, 60_000);
const taken = (await work(B)).pack.tasks[0].question;
check("a row another computer took since is not kept: its page passes it by",
  (await q(taken)).leased_to === B && (await renew(A, pack.lease, taken)).held === false, JSON.stringify(await q(taken)));
check("a renewal of a row not in the lease holds nothing", (await renew(A, "f".repeat(32), t1.question)).held === false);

// A ROW PAST THE CEILING: nobody is cut off, and another may take it too.
const heavy = pack.tasks.map((t) => t.question).find((x) => x !== t1.question && x !== taken);
await L.run("UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL");
await L.run("DELETE FROM answers");
await L.run("UPDATE questions SET state = 'fact' WHERE id != ?", heavy);
const LA = "e".repeat(32), C = "c".repeat(24), D = "d".repeat(24), E = "e".repeat(24);
const holdPast = (lease, who) => L.run("UPDATE questions SET lease = ?, leased_to = ?, lease_until = ?, leased_at = ? WHERE id = ?", lease, who, iso(Date.now() + 60_000), iso(Date.now() - ROW_CEILING_MS - 1000), heavy);
await L.run("UPDATE questions SET lease = ?, leased_to = ?, lease_until = ?, leased_at = ? WHERE id = ?", LA, A, iso(Date.now() + 60_000), iso(Date.now() - ROW_CEILING_MS + 60_000), heavy);
check("a row held under the ceiling is handed to nobody else", !(await work(C)).pack, JSON.stringify(await q(heavy)));
await holdPast(LA, A);
check("past the ceiling its page is never cut off: the renewal holds", (await renew(A, LA, heavy)).held === true);
const second = (await work(C)).pack;
check("...and another computer may take it too, the first kept as the holder before",
  second && second.tasks[0].question === heavy && (await q(heavy)).leased_to === C && (await q(heavy)).leased_to_prev === A
    && (await renew(A, LA, heavy)).held && (await renew(C, second.lease, heavy)).held, JSON.stringify(await q(heavy)));
// A THIRD TAKE keeps the holder computing longest; the one between is let go.
await holdPast(second.lease, C);
await L.run("UPDATE questions SET lease_prev_until = ? WHERE id = ?", iso(Date.now() + 60_000), heavy);
const third = (await work(E)).pack;
check("a third take keeps the first holder, still beating, and lets the second go",
  third && (await q(heavy)).leased_to === E && (await q(heavy)).leased_to_prev === A
    && (await renew(A, LA, heavy)).held && !(await renew(C, second.lease, heavy)).held, JSON.stringify(await q(heavy)));
// ONE OWNER'S TWO HOLDERS both answering are one witness: nothing credited twice.
await ACCOUNTS.run("INSERT INTO accounts (id, created_at, username) VALUES ('acc-ae', ?, 'ae')", iso(Date.now()));
for (const v of [A, E]) await ACCOUNTS.run("INSERT INTO devices (verifier, account, claimed_at) VALUES (?, 'acc-ae', 'x')", v);
const say = (v, lease) => post(verifyRoute, "/api/board/answers", { lease, verifier: v, engine: "e1", done: true,
  answers: [{ question: heavy, score: 2.5, metric: "kpm", work: 1000, compute_ms: 1000 }] });
await say(A, LA);
await say(E, third.lease);
const got = await L.all("SELECT device, credited FROM answers WHERE question = ?", heavy);
check("the holder before answers and its answer counts, and one owner's two answers are one witness, credited to nobody yet",
  got.length === 2 && got.every((a) => !a.credited) && (await q(heavy)).state === "open", JSON.stringify(got));

console.log(failures ? `\n${failures} failed` : "\na row its page still computes is kept for it, and only a silent one loses it");
process.exitCode = failures ? 1 : 0;
