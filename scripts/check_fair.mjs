// FAIR SHARE WHILE WORK IS SHORT (worker/fair.js, worker/verify.js `work`): the
// targets are 0.7 by capacity asking (work delivered) and 0.3 equal among established accounts, every
// unclaimed browser counts as one account, a new account shares only by its
// cores, and an account past its target is handed nothing on an ask — only
// while the open rows are fewer than the asking would take.
//   node scripts/check_fair.mjs
import { shares, throttled, accountOf, tableOf, WALK_INS } from "../worker/fair.js";
import { verifyRoute, PACK_PROTOCOL as PROTOCOL } from "../worker/verify.js";
import { boardTasksRoute } from "../worker/board_tasks.js";
import { testDb } from "./pg/test_db.mjs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};
const close = (x, y) => Math.abs(x - y) < 1e-9;

// ---- the targets ----
const big = { account: "big", capacity: 80, received: 0, established: true };
const small = (i) => ({ account: `s${i}`, capacity: 5, received: 0, established: true });
let t = shares([big, small(1), small(2), small(3), small(4)]);
check("one account with 80% of the capacity and four with 5%: 62% and 9.5% each",
  close(t.big.target, 0.62) && close(t.s1.target, 0.095), JSON.stringify(t));
t = shares([big, { ...small(1), established: false }]);
check("a new account shares only by its capacity, and the equal part goes to the established",
  close(t.s1.target, 0.7 * 5 / 85) && close(t.big.target, 0.7 * 80 / 85 + 0.3), JSON.stringify(t));
t = shares([{ ...big, established: false }, { ...small(1), established: false }]);
check("with nobody established the targets are the capacity alone", close(t.big.target, 80 / 85), JSON.stringify(t));
t = shares([{ ...big, received: 90 }, { ...small(1), received: 10 }, small(2), small(3), small(4)]);
check("an account past its target is held back on about 1 − 1/ratio of its asks, one under it never",
  t.big.ratio > 1 && throttled(t, "big", () => 0.99) && !throttled(t, "big", () => 0.5 / t.big.ratio) && !throttled(t, "s2", () => 0.99),
  JSON.stringify(t));
check("every unclaimed browser is one account, a claimed one its owner's", accountOf("d1", "d1") === WALK_INS && accountOf("acc", "d1") === "acc");

// ---- the ask ----
const LIBRARY = await testDb("library"), ACCOUNTS = await testDb("accounts"), L = LIBRARY;
const env = { LIBRARY, ACCOUNTS, ASSETS: { fetch: async () => new Response(JSON.stringify({ engine: "e1", release: "r1" })) }, FACTORY_TOKEN: "ft" };
const post = async (route, path, body, headers = {}) =>
  (await route(new Request(`https://x${path}`, { method: "POST", headers, body: JSON.stringify(body) }), env, path)).json();
const YES = { v: 1, at: "2026-10-08T08:00:00.000Z" };
const work = (v) => post(verifyRoute, "/api/board/work", { verifier: v, engine: "e1", protocol: PROTOCOL, consent: YES, lanes: 4, pace_ms: 3000 });
const now = Date.now(), iso = (ms) => new Date(ms).toISOString();

const BIG = "b".repeat(24), WALKER = "c".repeat(24);
await ACCOUNTS.run("INSERT INTO accounts (id, created_at, username) VALUES ('acc-big', ?, 'big')", iso(now - 3 * 86_400_000));
await ACCOUNTS.run("INSERT INTO devices (verifier, account, claimed_at) VALUES (?, 'acc-big', 'x')", BIG);
for (const v of [BIG, WALKER]) await L.run("INSERT INTO verifiers (id, seen, asked_at, asked_engine, asked_lanes, work) VALUES (?, 'x', ?, 'e1', 4, 1)", v, iso(now - 30_000));
// THE BIG ACCOUNT TOOK EVERYTHING IN THE WINDOW: 100% received against a 50% target.
await L.run(`INSERT INTO answers (question, engine, device, owner, canon, result, work, compute_ms, at) VALUES ('old', 'e1', ?, 'acc-big', 'k', '{}', 1000, 1000, ?)`, BIG, iso(now - 60_000));
const ask = async (n) => {
  const ids = Array.from({ length: n }, (_, i) => `build${i}`);
  await L.run("INSERT INTO batches (id, at, why, total) VALUES ('b1', 'b1', 'test', 0) ON CONFLICT DO NOTHING");
  for (const id of ids) {
    await L.run("INSERT INTO builds (id, at, record) VALUES (?, ?, ?) ON CONFLICT DO NOTHING", id, iso(now), JSON.stringify({ weapon: "braton_prime", mods: ["serration", id] }));
    await L.run("INSERT INTO queue (batch, build_id, ruler, mode) VALUES ('b1', ?, 'standard_single_target', 'base') ON CONFLICT DO NOTHING", id);
  }
  for (let i = 0; i < ids.length; i += 50) {
    await post(boardTasksRoute, "/api/board/ask", { batch: "b1", rows: ids.slice(i, i + 50).map((identity) => ({ identity, ruler: "standard_single_target", mode: "base" })) },
      { authorization: "Bearer ft" });
  }
};
const random = Math.random;
Math.random = () => 0.99;
await ask(2);
const held = await work(BIG), walked = await work(WALKER);
check("while work is short, the account past its share is handed nothing, as an ordinary nothing",
  held.ok && held.pack === null && held.work === null && !held.stale && !held.banned, JSON.stringify(held));
check("...and the walk-ins under theirs are handed a pack", !!(walked.pack && walked.pack.tasks.length), JSON.stringify(walked));

await L.run("UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL");
await ask(400);
const plenty = await work(BIG);
check("while work is plentiful the same account is handed a full pack", !!(plenty.pack && plenty.pack.tasks.length >= 10), JSON.stringify(plenty).slice(0, 200));
Math.random = random;

// ---- capacity is work delivered ----
// Four machines asking. One delivered 1000 in its best credited hour, one 2000.
// One delivered 3000 in the last ten minutes, an hourly rate of 18000. One has
// nothing credited, and it counts at the median, 2000.
{
  const LIB = await testDb("library"), ACC = await testDb("accounts");
  const hour = iso(now - 3 * 3_600_000).slice(0, 13);
  for (const [name, best, recent] of [["slow", 1000, 0], ["mid", 2000, 0], ["busy", 0, 3000], ["fresh", 0, 0]]) {
    const dev = name.padEnd(24, "0");
    await ACC.run("INSERT INTO accounts (id, created_at, username) VALUES (?, ?, ?)", `acc-${name}`, iso(now), name);
    await ACC.run("INSERT INTO devices (verifier, account, claimed_at) VALUES (?, ?, 'x')", dev, `acc-${name}`);
    await LIB.run("INSERT INTO verifiers (id, seen, asked_at, asked_engine, asked_lanes) VALUES (?, 'x', ?, 'e1', 4)", dev, iso(now - 30_000));
    if (best) await LIB.run("INSERT INTO verifier_hours (verifier, hour, work) VALUES (?, ?, ?)", dev, hour, best);
    // UNCREDITED WORK NEVER COUNTS AS CAPACITY: anyone can claim it.
    await LIB.run(`INSERT INTO answers (question, engine, device, owner, canon, result, work, compute_ms, at, credited)
                     VALUES ('q', 'e1', ?, ?, 'k', '{}', ?, 1, ?, ?)`, dev, `acc-${name}`, recent || 1e9, iso(now - 60_000), recent ? 1 : 0);
  }
  // THE ENGINE HAS BEEN SERVED SINCE TEN HOURS AGO, so the three-hour-old best hours count.
  await LIB.run(`INSERT INTO answers (question, engine, device, owner, canon, result, work, at) VALUES ('q0', 'e1', 'x', 'x', 'k', '{}', 1, ?)`, iso(now - 10 * 3_600_000));
  const tb = await tableOf({ LIBRARY: LIB, ACCOUNTS: ACC }, "e1", now);
  const r = (a) => tb[`acc-${a}`].target / tb["acc-slow"].target;
  check("capacity is the best credited hour, so a machine that delivered twice as much has twice the target", close(r("mid"), 2), JSON.stringify(tb));
  check("...or the credited rate now if that is more, and a machine with nothing credited counts at the median",
    close(r("busy"), 18) && close(r("fresh"), 2), JSON.stringify(tb));
  // A NEW ENGINE first answered a minute ago: no whole hour of it yet, so the
  // old best hours are another unit and count for nothing.
  await LIB.run("UPDATE answers SET engine = 'e2' WHERE question = 'q'");
  const t2 = await tableOf({ LIBRARY: LIB, ACCOUNTS: ACC }, "e2", now);
  check("a best hour from before the served engine's first answer counts for nothing",
    close(t2["acc-mid"].target, t2["acc-slow"].target) && close(t2["acc-busy"].target / t2["acc-slow"].target, 1), JSON.stringify(t2));
}

console.log(failures ? `\n${failures} failed` : "\nan account past its share waits while work is short, and only then");
process.exitCode = failures ? 1 : 0;
