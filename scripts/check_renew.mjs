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
import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const d1 = (file) => {
  const db = new DatabaseSync(":memory:");
  db.exec(readFileSync(new URL(`../worker/${file}`, import.meta.url), "utf8"));
  const stmt = (sql, args = []) => ({
    sql, args, bind: (...a) => stmt(sql, a),
    first: async () => db.prepare(sql).get(...args) ?? null,
    all: async () => ({ results: db.prepare(sql).all(...args) }),
    run: async () => ({ meta: { changes: Number(db.prepare(sql).run(...args).changes) } }),
  });
  return { raw: db, prepare: (sql) => stmt(sql),
    batch: async (ss) => ss.map((s) => (/^\s*select|\breturning\b/i.test(s.sql) ? { results: db.prepare(s.sql).all(...s.args) }
      : { meta: { changes: Number(db.prepare(s.sql).run(...s.args).changes) } })) };
};
const LIBRARY = d1("schema.sql"), ACCOUNTS = d1("accounts.sql"), L = LIBRARY.raw;
const env = { LIBRARY, ACCOUNTS, ASSETS: { fetch: async () => new Response(JSON.stringify({ engine: "e1", release: "r1" })) }, FACTORY_TOKEN: "ft" };
const TOKEN = { authorization: "Bearer ft" };
const post = async (route, path, body, headers = {}) =>
  (await route(new Request(`https://x${path}`, { method: "POST", headers, body: JSON.stringify(body) }), env, path)).json();
const YES = { v: 1, at: "2026-10-08T08:00:00.000Z" };
const work = (v) => post(verifyRoute, "/api/board/work", { verifier: v, engine: "e1", protocol: PROTOCOL, consent: YES, lanes: 4, pace_ms: 3000 });
const renew = (v, lease, question) => post(verifyRoute, "/api/board/renew", { verifier: v, lease, question });
const iso = (ms) => new Date(ms).toISOString();
const lapse = (id, ms) => L.prepare("UPDATE questions SET lease_until = ? WHERE id = ?").run(iso(Date.now() - ms), id);
const q = (id) => L.prepare("SELECT * FROM questions WHERE id = ?").get(id);

const A = "a".repeat(24), B = "b".repeat(24);
L.prepare("INSERT INTO batches (id, at, why, total) VALUES ('b1', 'b1', 'test', 0)").run();
const ids = ["r1", "r2", "r3"];
for (const id of ids) {
  L.prepare("INSERT INTO builds (id, at, record) VALUES (?, ?, ?)").run(id, iso(Date.now()), JSON.stringify({ weapon: "braton_prime", mods: ["serration", id] }));
  L.prepare("INSERT INTO queue (batch, build_id, ruler, mode) VALUES ('b1', ?, 'standard_single_target', 'base')").run(id);
}
await post(boardTasksRoute, "/api/board/ask", { batch: "b1", rows: ids.map((identity) => ({ identity, ruler: "standard_single_target", mode: "base" })) }, TOKEN);

const pack = (await work(A)).pack;
const [t1, t2] = pack.tasks;
check("the three rows go in one pack", pack.tasks.length === 3, JSON.stringify(pack.tasks.length));
check("a pack's rows carry when they were leased", !!q(t1.question).leased_at, JSON.stringify(q(t1.question)));

// THE ROW RUNS LONG: its lease ran out a minute ago, and nobody took it.
lapse(t1.question, 60_000);
const kept = await renew(A, pack.lease, t1.question);
check("a row whose lease ran out while its page computed it is kept, when nobody took it",
  kept.ok && kept.held === true && kept.renew_ms === RENEW_MS && Date.parse(q(t1.question).lease_until) > Date.now() + RENEW_MS - 5_000, JSON.stringify(kept));
await post(verifyRoute, "/api/board/answers", { lease: pack.lease, verifier: A, engine: "e1", done: false,
  answers: [{ question: t1.question, score: 1.5, metric: "kpm", work: 1000, compute_ms: 40 * 60_000 }] });
check("...and its answer, forty minutes in, is taken",
  !!L.prepare("SELECT 1 FROM answers WHERE question = ? AND device = ?").get(t1.question, A));

// THE NEXT ROWS' leases ran out meanwhile, and another computer took one.
lapse(t2.question, 60_000);
lapse(pack.tasks[2].question, 60_000);
const taken = (await work(B)).pack.tasks[0].question;
check("a row another computer took since is not kept: its page passes it by",
  q(taken).leased_to === B && (await renew(A, pack.lease, taken)).held === false, JSON.stringify(q(taken)));
check("a renewal of a row not in the lease holds nothing", (await renew(A, "f".repeat(32), t1.question)).held === false);

// A ROW PAST THE CEILING: nobody is cut off, and another may take it too.
const heavy = pack.tasks.map((t) => t.question).find((x) => x !== t1.question && x !== taken);
L.prepare("UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL").run();
L.prepare("DELETE FROM answers").run();
L.prepare("UPDATE questions SET state = 'fact' WHERE id != ?").run(heavy);
const LA = "e".repeat(32), C = "c".repeat(24), D = "d".repeat(24), E = "e".repeat(24);
const holdPast = (lease, who) => L.prepare("UPDATE questions SET lease = ?, leased_to = ?, lease_until = ?, leased_at = ? WHERE id = ?")
  .run(lease, who, iso(Date.now() + 60_000), iso(Date.now() - ROW_CEILING_MS - 1000), heavy);
L.prepare("UPDATE questions SET lease = ?, leased_to = ?, lease_until = ?, leased_at = ? WHERE id = ?")
  .run(LA, A, iso(Date.now() + 60_000), iso(Date.now() - ROW_CEILING_MS + 60_000), heavy);
check("a row held under the ceiling is handed to nobody else", !(await work(C)).pack, JSON.stringify(q(heavy)));
holdPast(LA, A);
check("past the ceiling its page is never cut off: the renewal holds", (await renew(A, LA, heavy)).held === true);
const second = (await work(C)).pack;
check("...and another computer may take it too, the first kept as the holder before",
  second && second.tasks[0].question === heavy && q(heavy).leased_to === C && q(heavy).leased_to_prev === A
    && (await renew(A, LA, heavy)).held && (await renew(C, second.lease, heavy)).held, JSON.stringify(q(heavy)));
// A THIRD TAKE keeps the holder computing longest; the one between is let go.
holdPast(second.lease, C);
L.prepare("UPDATE questions SET lease_prev_until = ? WHERE id = ?").run(iso(Date.now() + 60_000), heavy);
const third = (await work(E)).pack;
check("a third take keeps the first holder, still beating, and lets the second go",
  third && q(heavy).leased_to === E && q(heavy).leased_to_prev === A
    && (await renew(A, LA, heavy)).held && !(await renew(C, second.lease, heavy)).held, JSON.stringify(q(heavy)));
// ONE OWNER'S TWO HOLDERS both answering are one witness: nothing credited twice.
ACCOUNTS.raw.prepare("INSERT INTO accounts (id, created_at, username) VALUES ('acc-ae', ?, 'ae')").run(iso(Date.now()));
for (const v of [A, E]) ACCOUNTS.raw.prepare("INSERT INTO devices (verifier, account, claimed_at) VALUES (?, 'acc-ae', 'x')").run(v);
const say = (v, lease) => post(verifyRoute, "/api/board/answers", { lease, verifier: v, engine: "e1", done: true,
  answers: [{ question: heavy, score: 2.5, metric: "kpm", work: 1000, compute_ms: 1000 }] });
await say(A, LA);
await say(E, third.lease);
const got = L.prepare("SELECT device, credited FROM answers WHERE question = ?").all(heavy);
check("the holder before answers and its answer counts, and one owner's two answers are one witness, credited to nobody yet",
  got.length === 2 && got.every((a) => !a.credited) && q(heavy).state === "open", JSON.stringify(got));

console.log(failures ? `\n${failures} failed` : "\na row its page still computes is kept for it, and only a silent one loses it");
process.exitCode = failures ? 1 : 0;
