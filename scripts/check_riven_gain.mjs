// A RIVEN GAIN AS VOLUNTEER WORK (worker/appraise.js §"Volunteer work"), a
// question of the fact factory (worker/tasks.js, whose own properties
// check_tasks.mjs holds), with no network: only the bot freezes its question,
// once, and freezing it is the ask; a computer asking for work is handed it on
// the engine the site serves, in deadline order beside the board's orders; a
// second goes only to another owner's computer on another network; an answer
// counts only under its own lease and is kept as a build the bot may judge; two
// owners' equal answers credit both, once, and send the build to the board; one
// someone waits on goes to a computer offering many cores, until it has waited
// long enough for any; and one asked again once agreed is answered unfought.
//   node scripts/check_riven_gain.mjs
import { appraiseRoute } from "../worker/appraise.js";
import { verifyRoute, PROTOCOL } from "../worker/verify.js";
import { canon } from "../worker/tasks.js";
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
const LIBRARY = d1("schema.sql"), ACCOUNTS = d1("accounts.sql");
const site = (engine) => ({ fetch: async () => new Response(JSON.stringify({ engine })) });
const env = { BOT_RELAY_TOKEN: "relay", LIBRARY, ACCOUNTS, ASSETS: site("e1") };
const L = LIBRARY.raw;

const appraise = async (method, path, body, headers = {}) => {
  const r = await appraiseRoute(new Request(`https://x${path}`, { method, headers,
    ...(body === undefined ? {} : { body: JSON.stringify(body) }) }), env, path);
  return { status: r.status, body: await r.json() };
};
const work = async (verifier, engine = "e1", lanes = 8, ip = "") => {
  const r = await verifyRoute(new Request("https://x/api/board/work", { method: "POST", headers: ip ? { "cf-connecting-ip": ip } : {},
    body: JSON.stringify({ verifier, engine, protocol: PROTOCOL, consent: { v: 1, at: "2026-10-08T08:00:00.000Z" }, lanes }) }), env, "/api/board/work");
  return (await r.json()).work;
};
const bot = { authorization: "Bearer relay" };
const riven = { bonuses: [{ id: "critical_damage", roll: 1.1 }, { id: "multishot", roll: 1.1 }], malus: null, rank: 8 };
const open = async (asker, channel = "qq") => (await appraise("POST", "/api/appraise/new", { channel, chat: {}, asker, room: "",
  weapon: "torid", ruler: "standard_single_target", riven }, bot)).body.code;
// EACH APPRAISAL ITS OWN QUESTION: one frozen request asked twice is ONE
// question, which is the dedup this check holds at its end.
const frozen = (tag, engine = "e1") => ({ engine, request: { weapon: "torid", strategy: "quick", tag }, context: { weapon: "torid", rivens: {} } });
const credited = (v) => (L.prepare("SELECT work FROM verifiers WHERE id = ?").get(v) || {}).work || 0;
const today = (v) => (L.prepare("SELECT SUM(work) AS w FROM verifier_hours WHERE verifier = ?").get(v) || {}).w || 0;
const questionOf = (code) => L.prepare("SELECT question FROM demands WHERE ref = ?").get(code).question;
const freeLeases = () => {
  L.prepare("UPDATE orders SET lease = NULL, lease_until = NULL, leased_to = NULL").run();
  L.prepare("UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL").run();
};
/// A question asked `ms` ago: its age (the cores it needs) and its deadline move back.
const age = (code, ms) => {
  const t = (x) => new Date(Date.parse(x) - ms).toISOString();
  const q = L.prepare("SELECT opened_at, due_at FROM questions WHERE id = ?").get(questionOf(code));
  L.prepare("UPDATE questions SET opened_at = ?, due_at = ? WHERE id = ?").run(t(q.opened_at), t(q.due_at), questionOf(code));
  L.prepare("UPDATE demands SET due_at = ? WHERE ref = ?").run(t(q.due_at), code);
};

const A = "a".repeat(24), B = "b".repeat(24), C = "c".repeat(24), D = "d".repeat(24);
for (const v of [A, B, C, D]) L.prepare("INSERT INTO verifiers (id, seen) VALUES (?, '2026-10-08')").run(v);
// A and D are one person's computers; B and C are two others'.
ACCOUNTS.raw.exec(`INSERT INTO accounts (id, created_at, username) VALUES ('ann', 'x', 'ann'), ('bob', 'x', 'bob');
  INSERT INTO devices (verifier, account, claimed_at) VALUES ('${A}', 'ann', 'x'), ('${D}', 'ann', 'x'), ('${B}', 'bob', 'x');`);

console.log("asked, handed out, kept");
const code = await open("asker1");
check("nobody runs a riven gain whose question is not frozen", (await work(A) || {}).kind !== "riven_gain");
check("only the bot freezes the question", (await appraise("POST", `/api/appraise/${code}/request`, frozen(1))).status === 401);
const stored = await appraise("POST", `/api/appraise/${code}/request`, frozen(1), bot);
const again = await appraise("POST", `/api/appraise/${code}/request`, frozen(9), bot);
check("...once: a second freeze changes nothing", stored.body.stored === true && again.body.stored === false);
check("...and freezing it is the ask: one question, one live demand",
  L.prepare("SELECT count(*) AS n FROM demands WHERE ref = ? AND state = 'live' AND producer = 'chat'").get(code).n === 1);

check("a computer on another engine is handed nothing", (await work(A, "e0")) === null);
check("a computer offering few cores is not handed a fresh riven gain", (await work(A, "e1", 2) || {}).kind !== "riven_gain");
freeLeases();
const t1 = await work(A);
check("a computer offering many is handed it, frozen question and all",
  t1 && t1.kind === "riven_gain" && t1.code === code && canon(t1.request) === canon(frozen(1).request)
  && canon(t1.context) === canon(frozen(1).context), JSON.stringify(t1));
check("...and nobody else holds it meanwhile", (await work(B) || {}).kind !== "riven_gain");
// STILL AT IT: the holder's word runs its lease on; anyone else's runs nothing.
L.prepare("UPDATE questions SET lease_until = ? WHERE lease = ?").run(new Date(Date.now() + 1000).toISOString(), t1.lease);
const kept = await appraise("POST", `/api/appraise/${code}/renew`, { lease: t1.lease, verifier: A });
const until = L.prepare("SELECT lease_until FROM questions WHERE lease = ?").get(t1.lease).lease_until;
check("a computer still searching keeps its lease running on", kept.body.held === true && Date.parse(until) > Date.now() + 60_000);
check("...and one that does not hold it is told so, and changes nothing",
  (await appraise("POST", `/api/appraise/${code}/renew`, { lease: t1.lease, verifier: B })).body.held === false
  && L.prepare("SELECT lease_until FROM questions WHERE lease = ?").get(t1.lease).lease_until === until);
const told1 = await appraise("POST", "/api/appraise/claim", { channel: "qq" }, bot);
check("the chat may be told a computer took it, once", told1.body.started.some((x) => x.code === code));
await appraise("POST", "/api/appraise/claim", { channel: "qq", started_told: [code] }, bot);
check("...and is not told twice", !(await appraise("POST", "/api/appraise/claim", { channel: "qq" }, bot)).body.started.some((x) => x.code === code));

console.log("\nanswered, agreed, credited");
const BUILD = { weapon: "torid", mods: ["serration", "riven"], riven_pos: ["critical_damage", "multishot"], riven_neg: "" };
const answer = (t, v, extra = {}, ip = "") => appraise("POST", `/api/appraise/${t.code}/result`,
  { build: BUILD, lease: t.lease, verifier: v, engine: "e1", score: 12.5, work: 4e9, ...extra }, ip ? { "cf-connecting-ip": ip } : {});
check("an answer under another computer's lease is not kept", (await answer(t1, B)).body.first === false
  && !L.prepare("SELECT 1 FROM appraisal_results WHERE verifier = ?").get(B));
const a1 = await answer(t1, A);
check("an answer under its lease is kept, as a build the bot may judge", a1.body.ok && a1.body.first === true
  && !!L.prepare("SELECT 1 FROM appraisal_results WHERE verifier = ? AND code = ?").get(A, code));
check("...and one answer credits nobody", credited(A) === 0);
const mine = async (v, tasks) => (await appraise("POST", "/api/appraise/mine", { verifier: v, tasks })).body.tasks || [];
const stateOf = async (v, task) => ((await mine(v, [task]))[0] || {}).state;
check("...and its task says it waits for another computer", await stateOf(A, { code, at: 1 }) === "waiting");

check("the second run never goes to a computer of the same owner", (await work(D) || {}).kind !== "riven_gain");
freeLeases();
const t2 = await work(B);
check("...but to another owner's", t2 && t2.kind === "riven_gain" && t2.code === code, JSON.stringify(t2));
await answer(t2, B, { score: 12.6 });
check("unequal answers credit nobody", credited(A) === 0 && credited(B) === 0);
freeLeases();
const t3 = await work(C);
check("...and a third computer is asked", t3 && t3.kind === "riven_gain", JSON.stringify(t3));
const inboxBefore = L.prepare("SELECT COUNT(*) AS n FROM inbox").get().n;
await answer(t3, C);
check("two owners' equal answers credit both — the work, and the hour — and not the one that differed",
  credited(A) === 4e9 && credited(C) === 4e9 && credited(B) === 0 && today(A) === 4e9 && today(C) === 4e9,
  `${credited(A)} ${credited(B)} ${credited(C)}`);
check("...the appraisal is agreed and its build goes to the board's door, as a reader's submission",
  !!L.prepare("SELECT agreed_at FROM appraisals WHERE code = ?").get(code).agreed_at
  && L.prepare("SELECT COUNT(*) AS n FROM inbox").get().n === inboxBefore + 1);
check("...after which it is handed out no more", (await work(D) || {}).kind !== "riven_gain");
check("a browser's own riven gain says confirmed once its answer is credited, and gone when it was not",
  await stateOf(A, { code, at: 1 }) === "confirmed" && await stateOf(C, { code, at: 1 }) === "confirmed" && await stateOf(B, { code, at: 1 }) === "gone");
const answeredAt = Date.parse(L.prepare("SELECT at FROM appraisal_results WHERE verifier = ?").get(C).at);
const legacy = await mine(C, [{ weapon: "torid", at: answeredAt + 3000 }]);
check("...a task stored without its code is matched to its answer by weapon and time",
  legacy.length === 1 && legacy[0].code === code && legacy[0].state === "confirmed", JSON.stringify(legacy));
check("...and it says nothing about another browser's", !(await mine(D, [{ code, at: 1 }])).length);

console.log("\ncores, engines, deadlines");
const late = await open("asker2");
await appraise("POST", `/api/appraise/${late}/request`, frozen(2), bot);
freeLeases();
check("a fresh one waits for eight cores", (await work(D, "e1", 4) || {}).code !== late);
freeLeases();
age(late, 20_000);
check("...four once a round of asks has passed", (await work(D, "e1", 4) || {}).code === late);
freeLeases();
check("...still not one core in its first two minutes", (await work(D, "e1", 1) || {}).code !== late);
freeLeases();
age(late, 3 * 60_000);
check("...but past them, any computer takes it", (await work(D, "e1", 1) || {}).code === late);
freeLeases();
L.prepare("UPDATE questions SET state = 'withdrawn'").run();

// A QUESTION FROZEN BEFORE A RELEASE is still served after it: three answers of
// the old engine that never agreed do not shut it.
const older = await open("asker3");
await appraise("POST", `/api/appraise/${older}/request`, frozen(3, "e0"), bot);
age(older, 3 * 60_000);
for (const [v, k] of [["x1", "a"], ["x2", "b"], ["x3", "c"]]) {
  L.prepare(`INSERT INTO answers (question, engine, device, owner, canon, result, work, at) VALUES (?, 'e0', ?, ?, ?, '{}', 1, ?)`)
    .run(questionOf(older), v.repeat(8), v, k, new Date().toISOString());
}
check("a question frozen on an older engine is still handed out after a release, its old answers not counted against it",
  (await work(D, "e1", 8) || {}).code === older);
freeLeases();
L.prepare("UPDATE questions SET state = 'withdrawn'").run();

// EARLIEST DEADLINE FIRST, beside the board's orders: a chat's two minutes, a new
// build's hour, a survey's day, a rescore's week.
{
  const ago = (ms) => new Date(Date.now() - ms).toISOString();
  const shape = async (tag, agoMs) => {
    const c = await open(`survey-${tag}`, "survey");
    await appraise("POST", `/api/appraise/${c}/request`, frozen(`s${tag}`), bot);
    if (agoMs) age(c, agoMs);
    return c;
  };
  L.prepare(`INSERT INTO orders (identity, ruler, mode, record, state, slot, at, priority) VALUES
    ('new1', 'standard_single_target', 'base', '{"weapon":"furis"}', 'todo', 1, ?, 0),
    ('old1', 'standard_single_target', 'base', '{"weapon":"furis"}', 'todo', 2, ?, 1)`).run(ago(0), ago(0));
  L.prepare(`INSERT INTO queue (batch, build_id, ruler, mode) VALUES ('arrivals-x', 'new1', 'standard_single_target', 'base'),
    ('rescore-x', 'old1', 'standard_single_target', 'base')`).run();
  const E = "e".repeat(24), F = "f".repeat(24), G = "g".repeat(24), H = "h".repeat(24);
  for (const v of [E, F, G, H]) L.prepare("INSERT OR IGNORE INTO verifiers (id, seen) VALUES (?, '2026-10-08')").run(v);
  const fresh = await shape("fresh", 0);
  const first = await work(E, "e1", 8, "203.0.113.7");
  check("a new build's board row goes before a survey shape due in a day", first && first.kind !== "riven_gain" && first.identity === "new1",
    JSON.stringify(first));
  freeLeases();
  const overdue = await shape("overdue", 2 * 86_400_000);
  const second = await work(E, "e1", 8, "203.0.113.7");
  check("...but a survey shape a day overdue goes before it: nothing waits for ever behind a kind that keeps arriving",
    second && second.code === overdue, JSON.stringify(second));
  freeLeases();
  const qq = await open("someone");
  await appraise("POST", `/api/appraise/${qq}/request`, frozen("chat"), bot);
  const third = await work(H, "e1", 8, "192.0.2.4");
  check("a riven gain someone waits on in a chat goes before them all", third && third.code === qq, JSON.stringify(third));
  freeLeases();
  L.prepare("UPDATE questions SET state = 'withdrawn' WHERE id = ?").run(questionOf(qq));
  L.prepare("UPDATE orders SET state = 'settled' WHERE identity = 'new1'").run();
  L.prepare("UPDATE questions SET state = 'withdrawn' WHERE id = ?").run(questionOf(overdue));
  const fourth = await work(E, "e1", 8, "203.0.113.7");
  check("a survey shape due in a day goes before a rescore due in a week", fourth && fourth.code === fresh, JSON.stringify(fourth));
  await answer(fourth, E, {}, "203.0.113.7");
  freeLeases();
  check("a further answer never goes to the network of the first", ((await work(F, "e1", 8, "203.0.113.7")) || {}).code !== fresh);
  freeLeases();
  const s2 = await work(G, "e1", 8, "198.51.100.9");
  check("...but to another", s2 && s2.code === fresh, JSON.stringify(s2));
  await answer(s2, G, {}, "198.51.100.9");
  check("two owners on two networks agree", !!L.prepare("SELECT agreed_at FROM appraisals WHERE code = ?").get(fresh).agreed_at);

  // ONE QUESTION, ASKED AGAIN: a chat asking what a survey already agreed on is
  // answered at once, and nobody computes it a second time.
  const asked = await open("asks-the-same");
  await appraise("POST", `/api/appraise/${asked}/request`, frozen(`sfresh`), bot);
  check("a chat asking what a survey already agreed on is answered at once, unfought, with a build for the bot to tell",
    !!L.prepare("SELECT agreed_at FROM appraisals WHERE code = ?").get(asked).agreed_at
    && L.prepare("SELECT count(*) AS n FROM questions WHERE id = ?").get(questionOf(asked)).n === 1
    && questionOf(asked) === questionOf(fresh)
    && !!L.prepare("SELECT 1 FROM appraisal_results WHERE code = ?").get(asked));
}

console.log("\nfrozen before the factory");
{
  // A SHAPE FROZEN AND ONCE ANSWERED under the old tables is asked of the factory
  // as of when it was opened, its answer carried.
  const at = new Date(Date.now() - 2 * 86_400_000).toISOString();
  L.prepare(`INSERT INTO appraisals (code, channel, chat, asker, room, weapon, ruler, riven, at, request, engine)
    VALUES ('OLD01', 'survey', '{}', 'wfsim', '', 'furis', 'standard_single_target', '{}', ?, ?, 'e1')`).run(at, JSON.stringify(frozen("old")));
  L.prepare(`INSERT INTO appraisal_results (code, build, at, verifier, score, work, key, engine, net)
    VALUES ('OLD01', ?, ?, ?, 12.5, 4000000000, 'k', 'e1', '')`).run(JSON.stringify(BUILD), at, A);
  check("only the bot's relay adopts", (await appraise("POST", "/api/appraise/adopt", {})).status === 401);
  const r = await appraise("POST", "/api/appraise/adopt", {}, bot);
  check("an appraisal frozen before the factory is asked of it, and nothing is left", r.body.adopted === 1 && r.body.left === 0, JSON.stringify(r.body));
  check("...due as of when it was opened",
    L.prepare("SELECT due_at FROM questions WHERE id = ?").get(questionOf("OLD01")).due_at < new Date().toISOString());
  check("...its answer carried", L.prepare("SELECT count(*) AS n FROM answers WHERE question = ? AND device = ?").get(questionOf("OLD01"), A).n === 1);
  check("...and a second adoption adds nothing", (await appraise("POST", "/api/appraise/adopt", {}, bot)).body.adopted === 0);
  freeLeases();
  L.prepare("UPDATE orders SET state = 'settled'").run();
  check("its owner's computers are not asked again", (await work(D, "e1", 8, "198.51.100.77") || {}).code !== "OLD01");
  freeLeases();
  const t = await work(C, "e1", 8, "198.51.100.78");
  check("...another owner's is", t && t.code === "OLD01", JSON.stringify(t));
  const before = credited(A);
  await answer(t, C, {}, "198.51.100.78");
  check("...and agreeing with the carried answer credits both", credited(A) === before + 4e9
    && !!L.prepare("SELECT agreed_at FROM appraisals WHERE code = 'OLD01'").get().agreed_at, `${credited(A) - before}`);
  // A SEARCH RUNNING while it is adopted keeps its lease, and its answer lands.
  const lease = "f".repeat(32), soon = new Date(Date.now() + 600_000).toISOString();
  L.prepare(`INSERT INTO appraisals (code, channel, chat, asker, room, weapon, ruler, riven, at, request, engine, lease, lease_until, leased_to)
    VALUES ('OLD02', 'survey', '{}', 'wfsim', '', 'furis', 'standard_single_target', '{}', ?, ?, 'e1', ?, ?, ?)`)
    .run(at, JSON.stringify(frozen("running")), lease, soon, B);
  await appraise("POST", "/api/appraise/adopt", {}, bot);
  const landed = await answer({ code: "OLD02", lease }, B);
  check("a search running while it is adopted keeps its lease, and its answer lands", landed.body.first === true,
    JSON.stringify(landed.body));
}

console.log(failures ? `\n${failures} failed` : "\na riven gain is run by the community and credited when two owners agree");
process.exitCode = failures ? 1 : 0;
