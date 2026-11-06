// SPDX-License-Identifier: AGPL-3.0-or-later
// COMPUTERS ON BUT IDLE (worker/idle_devices.js), found and said by the watchdog.
// Plain node on PGlite: ids that asked in the last half hour are sorted into an
// old engine, an asking loop and not computing; one that holds a row, answered,
// only just opened or went away is none of them; the watchdog writes each kind
// down in Chinese, once a round, listing all of its devices.
import { testDb } from "./pg/test_db.mjs";
import { idleFindings, loopAsksPerHour } from "../worker/idle_devices.js";
import { watchdog } from "../worker/factory.js";
import { watchdogText } from "../worker/watchdog_say.js";
import { DIALS } from "../worker/dials.js";
import { iso } from "../worker/instant.js";
import { factory } from "../worker/tasks.js";

let failures = 0;
const check = (name, ok, detail) => {
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || detail === undefined ? "" : `  — ${detail}`}`);
  if (!ok) failures++;
};

const db = await testDb("library"), sent = [];
const env = { LIBRARY: db,
  ASSETS: { fetch: async () => new Response(JSON.stringify({ engine: "e1", release: "r1", accepted: [] })) },
  CLOUD: { fetch: async (r) => { sent.push((await r.json()).text); return new Response('{"ok":true}'); } } };
const NOW = Date.now(), hour = iso(NOW).slice(0, 13);
const id = (c) => c.repeat(24);
const on = async (c, { engine = "e1", ago = 60_000, asks = 3 } = {}) => {
  await db.run("INSERT INTO verifiers (id, seen, asked_at, asked_engine, asked_lanes) VALUES (?, ?, ?, ?, 8)", id(c), hour.slice(0, 10), iso(NOW - ago), engine);
  if (asks) await db.run("INSERT INTO verifier_asks (verifier, hour, asks) VALUES (?, ?, ?)", id(c), hour, asks);
};
await on("a", { engine: "e0/p7", asks: 0 });
await on("b", { asks: loopAsksPerHour({ sync_floor_ms: DIALS.sync_floor_ms.default }) + 1 });
await on("c");
await on("d");
await db.run(`INSERT INTO answers (question, engine, device, owner, canon, result, work, at) VALUES ('q1', 'e1', ?, ?, 'x', '{}', 1, ?)`, id("d"), id("d"), iso(NOW - 60_000));
await on("e", { asks: 1 });
await on("f", { ago: 2 * 3_600_000 });

console.log("who is idle");
const found = await idleFindings(env, "e1", NOW);
const of = (kind) => (found.find((f) => f.kind === kind) || { devices: [] }).devices.map((x) => x.device);
check("an id asking with another engine is on an old engine", JSON.stringify(of("idle:old_engine")) === JSON.stringify([id("a")]), JSON.stringify(found));
check("one asking more than twice what a page makes with nothing is a loop", JSON.stringify(of("idle:asking_loop")) === JSON.stringify([id("b")]));
check("one on the served engine, asking, holding and answering nothing is not computing", JSON.stringify(of("idle:not_computing")) === JSON.stringify([id("c")]));
check("one that answered, only just asked once, or went away is none of them",
  ![id("d"), id("e"), id("f")].some((x) => found.some((f) => f.devices.some((y) => y.device === x))));

console.log("\nwhat the watchdog says");
// THE DAEMON'S PICTURE SAYS THE FLEET IS STARVED: the watchdog finds it there.
const picture = (starved) => ({ get: async () => ({ body: JSON.stringify({ at: iso(NOW), orders: [{ kind: "sweep", total: 84_000, dispatched: 32 }],
  now: { window_seconds: 600, asking: 28, held: 48, answered: 110, starved } }) }) });
env.PUBLIC = picture(true);
await watchdog(env, NOW);
const starvedSaid = (await db.all("SELECT said FROM watchdog_said WHERE cause = 'starved'")).map((x) => x.said);
check("a starved fleet in the daemon's picture is written down, with the rows waiting", starvedSaid.length === 1
  && starvedSaid[0].includes("领不到活") && starvedSaid[0].includes("83968"), JSON.stringify(starvedSaid));
env.PUBLIC = picture(false);
check("…and one the picture calls fed is not found", !(await watchdog(env, NOW + 30_000)).some((f) => f.kind === "starved"));
delete env.PUBLIC;
const saidIdle = async () => (await db.all("SELECT said FROM watchdog_said ORDER BY at, cause")).map((x) => x.said).filter((t) => /idle:/.test(t));
let idleSaid = await saidIdle();
check("each kind is written down once, in Chinese, in one round", idleSaid.length === 3 && (await db.all("SELECT DISTINCT at FROM watchdog_said")).length === 1 && idleSaid.some((t) => t.includes("停在旧引擎"))
  && idleSaid.some((t) => t.includes("频繁请求")) && idleSaid.some((t) => t.includes("在线但没在算")), JSON.stringify(idleSaid));
check("…and sent to nobody", sent.length === 0, JSON.stringify(sent));
await watchdog(env, NOW + 60_000);
idleSaid = await saidIdle();
check("…and not again within the hour", idleSaid.length === 3, String(idleSaid.length));
await on("g");
await db.run("UPDATE watchdog_said SET at = ?", iso(NOW - 3_600_000));
await watchdog(env, NOW + 120_000);
const again = (await saidIdle()).slice(3).filter((t) => t.includes("在线但没在算"));
check("…and the next round lists every idle device of a kind, a new one with the rest", again.length === 1
  && again[0].includes(id("c").slice(0, 8)) && again[0].includes(id("g").slice(0, 8)), JSON.stringify(again));
check("every idle kind has its own words, none the fallback", ["old_engine", "asking_loop", "not_computing"].every((why) =>
  !watchdogText({ kind: `idle:${why}`, devices: [{ device: id("z"), engine: "e1", protocol: 8, lanes: 4, asks: 3, last_answer: null, label: null, why }],
    online: 10, served: "e1", loop_per_hour: 120 }, NOW).includes("没有这一类的说明")));

// A REPAIR THAT THROWS — the repay outran its timeout — keeps no round from being written.
await db.run("DELETE FROM watchdog_said");
const real = db.prepare.bind(db);
const timingOut = { ...env, PUBLIC: picture(true), LIBRARY: new Proxy(db, { get: (o, k) => k === "prepare"
  ? (sql) => (/name = 'repay'/.test(sql) ? { bind: () => ({ first: async () => { throw new Error("statement timeout"); } }), first: async () => { throw new Error("statement timeout"); } } : real(sql))
  : typeof o[k] === "function" ? o[k].bind(o) : o[k] }) };
await watchdog(timingOut, NOW + 4 * 3_600_000);
check("a repair that throws keeps no round from being written", (await db.all("SELECT cause FROM watchdog_said")).some((r) => r.cause === "starved"));

console.log("\nfailed checks remain unknown while other findings survive");
const failAt = NOW + 5 * 3_600_000;
await db.run("UPDATE verifiers SET last_at = ?", iso(failAt));
await db.run("INSERT INTO questions (id, verb, request, state, opened_at, due_at) VALUES ('odd', 'simulate', '{}', 'nondeterministic', ?, ?)", iso(failAt), iso(failAt));
const fails = (pattern) => new Proxy(db, { get: (o, k) => {
  if (k !== "prepare") return typeof o[k] === "function" ? o[k].bind(o) : o[k];
  return (sql) => {
    if (!pattern.test(sql)) return real(sql);
    const no = async () => { throw Object.assign(new Error("SQL private-value"), { code: "57014" }); };
    const statement = { bind: () => statement, first: no, all: no, run: no };
    return statement;
  };
} });
const brokenChecks = [
  ["stalled", /WITH RECURSIVE producers\(producer\) AS|SELECT d\.producer, count\(DISTINCT d\.question\)/],
  ["nondeterministic", /SELECT SUM\(n\) AS n FROM/],
  ["unsettled", /SELECT count\(\*\) AS n FROM questions q WHERE q\.state IN/],
  ["overdue", /SELECT d\.producer, count\(\*\) AS questions/],
  ["disagreeing", /SELECT a\.owner, count\(\*\) AS answers/],
  ["refused_rows", /SELECT question, engine, owners, at FROM refusal_withdrawals/],
  ["redeliver", /SELECT through FROM factory_cursors WHERE name = 'redeliver'/],
  ["cells_filling", /SELECT ruler, count\(DISTINCT build_id/],
  ["idle", /WITH online AS/],
];
for (const [step, sql] of brokenChecks) {
  await db.run("DELETE FROM watchdog_said");
  const round = await watchdog({ ...env, PUBLIC: picture(true), LIBRARY: fails(sql) }, failAt);
  const rows = await db.all("SELECT cause, said FROM watchdog_said");
  const failure = round.find(f => f.kind === `watchdog_failed:${step}`);
  check(`${step} failure is explicit and preserves another finding`, failure?.code === "timeout"
    && rows.some(r => r.cause === failure.kind && r.said.includes("状态未知") && r.said.includes("🔴"))
    && rows.some(r => r.cause === "starved"), JSON.stringify(round));
  check(`${step} failure stores no raw exception or SQL`, !!failure && !JSON.stringify(rows).includes("private-value"));
}
await db.run("DELETE FROM watchdog_said");
const noRelease = await watchdog({ ...env, PUBLIC: picture(true),
  ASSETS: { fetch: async () => { throw new Error("private-release-failure"); } } }, failAt);
const noReleaseRows = await db.all("SELECT cause, said FROM watchdog_said");
check("an unread release is explicit while independent findings survive", noRelease.some(f => f.kind === "watchdog_failed:release")
  && noReleaseRows.some(r => r.cause === "watchdog_failed:release" && r.said.includes("状态未知"))
  && noReleaseRows.some(r => r.cause === "starved"));
check("an unread release stores no raw exception", !JSON.stringify(noReleaseRows).includes("private-release-failure"));
const unbound = await watchdog({ ...env, ASSETS: undefined, PUBLIC: picture(true) }, failAt);
check("an intentionally absent release binding keeps the optional checks unchanged", !unbound.some(f => f.kind === "watchdog_failed:release")
  && unbound.some(f => f.kind === "starved"));
await db.run("DELETE FROM watchdog_said");
await watchdog({ ...env, PUBLIC: picture(true) }, failAt);
const early = failAt + 60_000, broken = { ...env, PUBLIC: picture(true), LIBRARY: fails(brokenChecks[0][1]) };
await watchdog(broken, early);
const earlyRows = await db.all("SELECT at, cause FROM watchdog_said WHERE at = ?", iso(early));
check("a newly failed check writes a whole round before the hour", earlyRows.some(r => r.cause === "watchdog_failed:stalled")
  && earlyRows.some(r => r.cause === "starved") && earlyRows.some(r => r.cause === "nondeterministic"), JSON.stringify(earlyRows));
const beforeRepeat = (await db.get("SELECT count(*) AS n FROM watchdog_said")).n;
await watchdog(broken, early + 60_000);
check("the same failed check retains the hourly cadence", (await db.get("SELECT count(*) AS n FROM watchdog_said")).n === beforeRepeat);

console.log("\nstalls respect each producer's complete demand history");
{
  const db = await testDb("library"), env = { LIBRARY: db }, old = iso(NOW - 3_600_000), at = iso(NOW);
  const f = factory({ verbs: {}, producers: {} });
  await db.run("INSERT INTO verifiers (id, seen, last_at) VALUES ('busy', 'today', ?)", at);
  await db.exec(`INSERT INTO questions (id,verb,request,state,opened_at,due_at) VALUES
    ('quiet','simulate','{}','open','${old}','${old}'),('speaking','simulate','{}','open','${old}','${old}'),
    ('held','simulate','{}','open','${old}','${old}');
    INSERT INTO demands (producer,ref,question,state,asked_at,due_at) VALUES
    ('quiet','a','quiet','live','${old}','${old}'),('quiet','b','quiet','live','${old}','${old}'),
    ('speaking','current','speaking','live','${old}','${old}'),('speaking','past','gone','withdrawn','${old}','${old}'),
    ('held','held','held','live','${old}','${old}');`);
  await db.run("UPDATE questions SET leased_at=?,lease_until=? WHERE id='held'", at, iso(NOW + 3_600_000));
  await db.run("INSERT INTO answers (question,engine,device,owner,canon,result,work,at) VALUES ('gone','e1','busy','busy','{}','{}',1,?)", iso(NOW - 60_000));
  const found = await f.audit(env, NOW);
  check("recent answers through a withdrawn demand still suppress its producer's stall", found.length === 1
    && found[0].kind === "stalled:quiet", JSON.stringify(found));
  check("duplicate demands count their question once and a renewed row is held", found[0]?.questions === 1
    && found[0]?.oldest_due === old && found[0]?.witnesses_active === 1, JSON.stringify(found));
}

console.log("\nlegacy redelivery advances without an unbounded history scan");
{
  const db = await testDb("library"), env = { LIBRARY: db }, delivered = [];
  const start = NOW - 3 * 86_400_000;
  const f = factory({ verbs: { simulate: { cap: 2, canon: (result) => JSON.stringify(result) } },
    producers: { p: { on_fact: async (_, demand) => { delivered.push(demand.ref); } } } });
  const fact = async (ref, at) => {
    await f.demand(env, { producer: "p", ref, verb: "simulate", request: { ref }, due_in_ms: 1000 }, "e1", start);
    await db.run("UPDATE questions SET state='fact',canon='{}',fact_engine='e1',fact_at=? WHERE id=(SELECT question FROM demands WHERE ref=?)", iso(at), ref);
  };
  await db.run("INSERT INTO factory_cursors(name,through) VALUES ('redeliver',?)", iso(start));
  await fact("later", start + 2 * 3_600_000);
  await f.redeliver(env, NOW, 2);
  check("an empty legacy window advances without scanning through later facts", delivered.length === 0
    && (await db.get("SELECT through FROM factory_cursors WHERE name='redeliver'")).through === iso(start + 3_600_000));
  const tied = start + 3_600_000 + 60_000;
  for (let i = 0; i < 5; i++) await fact(`tie${i}`, tied);
  await f.redeliver(env, NOW, 2);
  await f.redeliver(env, NOW, 2);
  await f.redeliver(env, NOW, 2);
  check("more than a batch of facts at the same timestamp are each delivered", delivered.filter(ref => ref.startsWith("tie")).length === 5
    && new Set(delivered).size === delivered.length, JSON.stringify(delivered));
  await f.redeliver(env, NOW, 2);
  check("the next bounded window reaches its pending fact", delivered.includes("later"), JSON.stringify(delivered));
  await fact("retry", start + 3 * 3_600_000 + 60_000);
  const failed = factory({ verbs: { simulate: { cap: 2, canon: (result) => JSON.stringify(result) } },
    producers: { p: { on_fact: async () => { throw new Error("consumer unavailable"); } } } });
  await failed.redeliver(env, NOW, 2);
  const held = await db.get("SELECT through FROM factory_cursors WHERE name='redeliver'");
  check("a failed legacy consumer holds the cursor at its fact", held.through === iso(start + 3 * 3_600_000 + 60_000)
    && (await db.get("SELECT state FROM demands WHERE ref='retry'")).state === "live", JSON.stringify(held));
  await f.redeliver(env, NOW + 60_000, 2);
  check("the ready retry path delivers a held legacy fact once", delivered.filter(ref => ref === "retry").length === 1);
}
{
  const db = await testDb("library"), env = { LIBRARY: db }, delivered = [];
  const f = factory({ verbs: { simulate: { cap: 2, canon: (result) => JSON.stringify(result) } },
    producers: { p: { on_fact: async (_, demand) => { delivered.push(demand.ref); } } } });
  await f.demand(env, { producer: "p", ref: "cold", verb: "simulate", request: { cold: true }, due_in_ms: 1000 }, "e1", NOW - 3_600_000);
  await db.run("UPDATE questions SET state='fact',canon='{}',fact_engine='e1',fact_at=?", iso(NOW - 30 * 60_000));
  let through = NOW - 7 * 86_400_000, bounded = true, skipped = false, passes = 0;
  for (; passes < 168 && !delivered.length; passes++) {
    await f.redeliver(env, NOW, 2);
    const next = Date.parse((await db.get("SELECT through FROM factory_cursors WHERE name='redeliver'")).through);
    bounded &&= next >= through && next <= through + 3_600_000;
    skipped ||= next > NOW - 30 * 60_000 && !delivered.length;
    through = next;
  }
  check("a missing cursor recovers a recent legacy fact through bounded windows", bounded && !skipped
    && delivered.length === 1 && delivered[0] === "cold" && passes > 1, JSON.stringify({ delivered, passes, through: iso(through) }));
}

console.log(failures ? `\n${failures} failed` : "\nan idle computer is found by what the server keeps, and said once");
process.exit(failures ? 1 : 0);
