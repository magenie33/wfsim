// THE SETTLER IN SHADOW (worker/settler.js, scripts/factoryd.mjs): its fair table
// is the worker's `tableOf` to the last digit through Postgres, its snapshot
// counts what the worker's own answers count, its judgement of a settled
// question agrees with the request path's, and every comparison it makes
// catches a difference put there on purpose.
//   node scripts/check_settler.mjs
import { tableOf } from "../worker/fair.js";
import { factory, canon, nearAnswer } from "../worker/tasks.js";
import { contributionRoute } from "../worker/contribution.js";
import { snapshotTick, fairTick, readFair, compareFair, settleShadow, judgeFact, SETTLE_SHADOW_LAG_MS } from "../worker/settler.js";
import { testDb } from "./pg/test_db.mjs";
import { DIAL_DEFAULTS } from "../worker/dials.js";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};
const iso = (ms) => new Date(ms).toISOString();
const now = Date.now();

const LIBRARY = await testDb("library"), ACCOUNTS = await testDb("accounts"), L = LIBRARY;
const env = { LIBRARY, ACCOUNTS };

console.log("the fair table");
{
  await ACCOUNTS.run("INSERT INTO accounts (id, created_at, username) VALUES ('acc-big', ?, 'big')", iso(now - 3 * 86_400_000));
  const BIG = "b".repeat(24), BIG2 = "e".repeat(24), WALKER = "c".repeat(24);
  for (const v of [BIG, BIG2]) await ACCOUNTS.run("INSERT INTO devices (verifier, account, claimed_at) VALUES (?, 'acc-big', 'x')", v);
  for (const v of [BIG, BIG2, WALKER]) await L.run("INSERT INTO verifiers (id, seen, asked_at, asked_engine, asked_lanes, work) VALUES (?, 'x', ?, 'e1', 4, 1)", v, iso(now - 30_000));
  await L.run(`INSERT INTO answers (question, engine, device, owner, canon, result, work, compute_ms, at, credited) VALUES
      ('a1', 'e1', ?, 'acc-big', 'k', '{}', 3000, 1000, ?, 1), ('a2', 'e1', ?, ?, 'k', '{}', 1000, 1000, ?, 0)`,
    BIG, iso(now - 60_000), WALKER, WALKER, iso(now - 90_000));
  const table = await fairTick(env, "e1", now);
  const worker = await tableOf(env, "e1", now);
  const stored = await readFair(L, "e1");
  check("the daemon's table is the worker's tableOf, account by account", !compareFair(table, worker, 0).differ, JSON.stringify(compareFair(table, worker, 0)));
  check("…and reads back from fair_shares to the last digit", !compareFair(worker, stored.table, 0).differ && Object.keys(worker).length === 2,
    JSON.stringify({ worker, stored: stored.table }));
  await L.run("UPDATE fair_shares SET target = target + 0.1 WHERE account = 'acc-big'");
  const moved = compareFair(worker, (await readFair(L, "e1")).table, 0.05);
  check("a target moved past the tolerance is a difference, on the account it moved", moved.differ && moved.worst.account === "acc-big", JSON.stringify(moved));
  await L.run("DELETE FROM fair_shares WHERE account = 'walk-ins'");
  check("an account missing from one table is a difference", compareFair(worker, (await readFair(L, "e1")).table, 1).missing.includes("walk-ins"));
}

console.log("the snapshot");
{
  await L.run(`INSERT INTO questions (id, verb, request, state, due_at, opened_at, waits) VALUES
      ('o1', 'simulate', '{}', 'open', ?, ?, 0), ('o2', 'simulate', '{}', 'open', ?, ?, 0), ('u1', 'simulate', '{}', 'open', ?, ?, 1)`,
    iso(now + 1), iso(now), iso(now + 1), iso(now), iso(now + 1), iso(now));
  await L.run("UPDATE questions SET lease = 'l', lease_until = ?, leased_to = 'x' WHERE id = 'o2'", iso(now + 60_000));
  await L.run("UPDATE verifiers SET last_at = ? WHERE id = ?", iso(now - 60_000), "b".repeat(24));
  const s = await snapshotTick(env, { engine: "e1", release: "r1", commit: "abc" }, now);
  const served = await (await contributionRoute(new Request("https://x/api/board/computing"), env, "/api/board/computing")).json();
  check("computing is the count /api/board/computing serves", s.computing === served.computing && s.computing === 1, JSON.stringify({ s, served }));
  // ASKING IS WHO ASKED LATELY OR HOLDS A LIVE LEASE (worker/tasks.js `askingIds`):
  // three asked, and 'x' holds o2 without having asked.
  check("open counts what may be taken now, urgent what someone waits on, asking who asked lately or holds a lease",
    s.open === 1 && s.urgent === 1 && s.asking === 4, JSON.stringify(s));
  const row = await L.get("SELECT * FROM factory_snapshot WHERE id = 1");
  check("the heartbeat names the commit the daemon runs", row && row.daemon_commit === "abc" && row.daemon_at === iso(now), JSON.stringify(row));
}

console.log("settling, in shadow");
{
  const VERBS = { simulate: { cap: 2, lease_ms: 30 * 60_000, pack: true, canon: (r, work) => canon({ score: r.score, work }), near: nearAnswer(["work"]) } };
  const f = factory({ verbs: VERBS, producers: { p: { on_fact: async () => {} } }, random: () => 0.999 });
  const t0 = now - 10 * 60_000;
  const ask = async (id) => {
    await L.run("INSERT INTO questions (id, verb, request, state, due_at, opened_at) VALUES (?, 'simulate', '{}', 'open', ?, ?)", id, iso(t0 + 3_600_000), iso(t0));
    await L.run("INSERT INTO demands (producer, ref, question, state, asked_at, due_at) VALUES ('p', ?, ?, 'live', ?, ?)", id, id, iso(t0), iso(t0 + 3_600_000));
  };
  const give = async (id, device, owner, score) => {
    await L.run("INSERT INTO verifiers (id, seen) VALUES (?, 'x') ON CONFLICT DO NOTHING", device);
    await L.run("UPDATE questions SET lease = ?, lease_until = ?, leased_to = ? WHERE id = ?", `L-${device}`, iso(t0 + 600_000), device, id);
    await f.answer(env, { device, owner }, { lease: `L-${device}`, engine: "e1", result: { score }, work: 7, compute_ms: 10 }, t0 + 1_000);
  };
  for (const id of ["s1", "s2", "s3"]) await ask(id);
  for (const id of ["s1", "s2", "s3"]) { await give(id, `${id}a`, "o1", 5); await give(id, `${id}b`, "o2", 5); }
  const facts = await L.all("SELECT id, state, canon FROM questions WHERE id IN ('s1', 's2', 's3') ORDER BY id");
  check("the request path made three facts", facts.every((q) => q.state === "fact"), JSON.stringify(facts));
  const from = { at: iso(t0 - 1), id: "" };
  let r = await settleShadow(env, { threshold: 2, from }, t0 + 1_000 + SETTLE_SHADOW_LAG_MS + 1);
  check("the shadow judges each settled question as the request path did", r.checked === 3 && r.ok === 3 && !Object.keys(r.differ).length, JSON.stringify(r));
  check("…and goes on from its cursor, judging none twice",
    (await settleShadow(env, { threshold: 2, from: r.cursor }, t0 + 1_000 + SETTLE_SHADOW_LAG_MS + 1)).checked === 0);
  check("a question settled less than the lag ago waits", (await settleShadow(env, { threshold: 2, from }, t0 + 1_000)).checked === 0);
  await L.run("UPDATE questions SET canon = 'x' WHERE id = 's1'");
  await L.run("UPDATE answers SET credited = 0 WHERE question = 's2' AND device = 's2b'");
  r = await settleShadow(env, { threshold: 2, from }, t0 + 1_000 + SETTLE_SHADOW_LAG_MS + 1);
  check("a fact whose canon no group gave, and a witness left unpaid, are each a difference",
    r.differ.canon === 1 && r.differ.uncredited === 1 && r.ok === 1, JSON.stringify(r));
  const finding = JSON.parse((await L.get("SELECT body FROM factory_findings WHERE kind = 'settle_shadow'")).body);
  check("the finding keeps the running count and the questions that differed", finding.checked === 6 && finding.samples.length === 2, JSON.stringify(finding));
  check("an official answer is the official path's to judge, an older engine's a peer's",
    judgeFact({ fact_engine: "e1", fact_at: "z", canon: "k" }, [{ engine: "e1", official: 1, canon: "k", at: "a" }], 2).skip === "official"
    && judgeFact({ fact_engine: "e1", fact_at: "z", canon: "k" }, [{ engine: "e1", owner: "a", canon: "k", at: "a" }, { engine: "e0", owner: "b", canon: "j", at: "a" }], 2).skip === "peer");
  check("an older engine's answer of the same bits joins the group", judgeFact({ fact_engine: "e1", fact_at: "z", canon: "k" },
    [{ engine: "e1", owner: "a", canon: "k", at: "a", credited: 1 }, { engine: "e0", owner: "b", canon: "k", at: "a", credited: 1 }], 2).ok === true);
  check("two answers of one owner are one witness", judgeFact({ fact_engine: "e1", fact_at: "z", canon: "k" },
    [{ engine: "e1", owner: "a", canon: "k", at: "a", credited: 1 }, { engine: "e1", owner: "a", canon: "k", at: "a", credited: 1 }], 2).differ === "no_group");
}

console.log("the daemon");
{
  const { run } = await import("./factoryd.mjs");
  process.env.DAEMON_COMMIT = "c0ffee";
  const code = await run({ once: true, opened: { env, close: async () => {} }, servedNow: async () => ({ engine: "e1", release: "r1" }), target: null });
  const row = await L.get("SELECT daemon_commit, engine FROM factory_snapshot WHERE id = 1");
  check("one pass of every tick runs clean and the heartbeat names its commit", code === 0 && row.daemon_commit === "c0ffee" && row.engine === "e1",
    JSON.stringify({ code, row }));
  check("without an R2 token the signal is skipped and said so, the other ticks unharmed",
    code === 0 && !!(await L.get("SELECT 1 AS x FROM factory_findings WHERE kind = 'signal_unconfigured'")));
}

console.log("the work signal");
{
  const { run, SIGNAL_KEY, SIGNAL_CACHE } = await import("./factoryd.mjs");
  const puts = [];
  const target = { endpoint: "https://acct.r2.cloudflarestorage.com", bucket: "wfsim-public", accessKey: "k", secretKey: "s" };
  const go = () => run({ once: true, opened: { env, close: async () => {} }, servedNow: async () => ({ engine: "e1", release: "r1" }),
    target, put: async (t, o) => { puts.push({ t, o }); } });
  const signals = () => puts.filter((p) => p.o.key === SIGNAL_KEY);
  check("one pass writes the signal once", (await go()) === 0 && signals().length === 1, JSON.stringify(puts.map((p) => p.o.key)));
  const { o } = signals()[0], body = JSON.parse(o.body);
  const { DEMAND_KEY, DEMAND_CACHE } = await import("../worker/contribution.js");
  const demands = puts.filter((p) => p.o.key === DEMAND_KEY);
  check("…and the compute page's picture once, as demand.json, kept half a minute at the edge",
    demands.length === 1 && DEMAND_KEY === "demand.json" && demands[0].o.cacheControl === DEMAND_CACHE
      && JSON.parse(demands[0].o.body).v === 1, JSON.stringify(demands.map((p) => p.o)));
  const { TALLY_KEY, TALLY_CACHE } = await import("../worker/contribution.js");
  const tallies = puts.filter((p) => p.o.key === TALLY_KEY), told = tallies[0] && JSON.parse(tallies[0].o.body);
  check("…and the home page's count once, as stats/tally.json, kept half a minute at the edge, nobody named in it",
    tallies.length === 1 && TALLY_KEY === "stats/tally.json" && tallies[0].o.cacheControl === TALLY_CACHE && TALLY_CACHE === "public, max-age=30"
      && JSON.stringify(Object.keys(told)) === JSON.stringify(["ok", "v", "at", "totals", "per_hour", "computing"]),
    JSON.stringify(tallies.map((p) => p.o)));
  check("…as work.json, kept five seconds at the edge, as JSON",
    o.key === SIGNAL_KEY && SIGNAL_KEY === "work.json" && o.cacheControl === "public, max-age=5" && SIGNAL_CACHE === o.cacheControl && o.contentType === "application/json", JSON.stringify(o));
  const { PROTOCOL, PROTOCOLS_SERVED } = await import("../worker/verify.js");
  const snap = await L.get("SELECT * FROM factory_snapshot WHERE id = 1");
  check("its fields are the snapshot's counts, the served engine and protocols, and the dials in force",
    body.v === 1 && body.at === snap.at && body.open === 1 && body.urgent === 1 && body.computing === Number(snap.computing)
    && body.engine === "e1" && body.release === "r1" && body.protocol === PROTOCOL && body.protocols.length === PROTOCOLS_SERVED.size
    && body.poll_ms === DIAL_DEFAULTS.poll_ms && body.jitter_ms === DIAL_DEFAULTS.jitter_ms
    && body.urgent_jitter_ms === DIAL_DEFAULTS.urgent_jitter_ms && body.mode === "signal"
    && Object.keys(body).join() === "v,at,open,urgent,engine,release,protocol,protocols,computing,poll_ms,jitter_ms,urgent_jitter_ms,mode", JSON.stringify(body));
  const { setDial, forgetDials } = await import("../worker/dials.js");
  await setDial(env, "signal_mode", 1, "check");
  await setDial(env, "poll_ms", 30_000, "check");
  puts.length = 0; await go(); forgetDials();
  const turned = JSON.parse(signals()[0].o.body);
  check("the owner's dials reach the signal: ask mode turns every computer back to asking", turned.mode === "ask" && turned.poll_ms === 30_000, JSON.stringify(turned));
  await setDial(env, "signal_mode", 0, "check"); await setDial(env, "poll_ms", 10_000, "check"); forgetDials();
  const { workWaiting } = await import("../worker/settler.js");
  const w = await workWaiting(L);
  check("the wave probe sees open work and waited-on work", w.open === true && w.urgent === true, JSON.stringify(w));
  await L.run("UPDATE questions SET state = 'withdrawn' WHERE id IN ('o1', 'u1')");
  const none = await workWaiting(L);
  check("…and none when nothing may be taken", none.open === false && none.urgent === false, JSON.stringify(none));
}

console.log("signing for R2");
{
  // AWS's published example (S3 API reference, "Signature Calculations for the
  // Authorization Header", GET Object): the signature must come out to the hex they print.
  const { signV4 } = await import("./sigv4.mjs");
  const s = signV4({ method: "GET", url: "https://examplebucket.s3.amazonaws.com/test.txt", headers: { range: "bytes=0-9" },
    payloadHash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    accessKey: "AKIAIOSFODNN7EXAMPLE", secretKey: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
    region: "us-east-1", service: "s3", amzDate: "20130524T000000Z" });
  check("the signature is AWS's own example to the last digit",
    s.signature === "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41", s.signature);
  check("…and the header names what was signed",
    s.authorization === "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41",
    s.authorization);
}

console.log(failures ? `\n${failures} failure(s)` : "\nthe settler in shadow agrees with the request path, and says where it does not");
process.exit(failures ? 1 : 0);
