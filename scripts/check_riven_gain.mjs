// A RIVEN GAIN AS VOLUNTEER WORK (worker/appraise.js §"Volunteer work"), with no
// network: only the bot freezes its question, once; a computer asking for work
// is handed one nobody answered before any board order, only on the engine the
// site serves; a second goes only to another owner's computer; an answer counts
// only under its own lease; two owners' equal answers credit both, once, and
// unequal ones credit nobody; and one someone waits on goes to a computer
// offering many cores, until it has waited long enough for any.
//   node scripts/check_riven_gain.mjs
import { appraiseRoute } from "../worker/appraise.js";
import { verifyRoute, PROTOCOL } from "../worker/verify.js";
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

const appraise = async (method, path, body, headers = {}) => {
  const r = await appraiseRoute(new Request(`https://x${path}`, { method, headers,
    ...(body === undefined ? {} : { body: JSON.stringify(body) }) }), env, path);
  return { status: r.status, body: await r.json() };
};
const work = async (verifier, engine = "e1", lanes = 8) => {
  const r = await verifyRoute(new Request("https://x/api/board/work", { method: "POST",
    body: JSON.stringify({ verifier, engine, protocol: PROTOCOL, consent: { v: 1, at: "2026-10-08T08:00:00.000Z" }, lanes }) }), env, "/api/board/work");
  return (await r.json()).work;
};
const bot = { authorization: "Bearer relay" };
const riven = { bonuses: [{ id: "critical_damage", roll: 1.1 }, { id: "multishot", roll: 1.1 }], malus: null, rank: 8 };
const open = async (asker) => (await appraise("POST", "/api/appraise/new", { channel: "qq", chat: {}, asker, room: "",
  weapon: "torid", ruler: "standard_single_target", riven }, bot)).body.code;
const FROZEN = { engine: "e1", request: { weapon: "torid", strategy: "quick" }, context: { weapon: "torid", rivens: {} } };
const credited = (v) => (LIBRARY.raw.prepare("SELECT work FROM verifiers WHERE id = ?").get(v) || {}).work || 0;
const today = (v) => (LIBRARY.raw.prepare("SELECT SUM(work) AS w FROM verifier_hours WHERE verifier = ?").get(v) || {}).w || 0;

const A = "a".repeat(24), B = "b".repeat(24), C = "c".repeat(24), D = "d".repeat(24);
for (const v of [A, B, C, D]) LIBRARY.raw.prepare("INSERT INTO verifiers (id, seen) VALUES (?, '2026-10-08')").run(v);
// A and D are one person's computers; B and C are two others'.
ACCOUNTS.raw.exec(`INSERT INTO accounts (id, created_at, username) VALUES ('ann', 'x', 'ann'), ('bob', 'x', 'bob');
  INSERT INTO devices (verifier, account, claimed_at) VALUES ('${A}', 'ann', 'x'), ('${D}', 'ann', 'x'), ('${B}', 'bob', 'x');`);
// A board order is waiting too, so "first" means first.
LIBRARY.raw.prepare(`INSERT INTO orders (identity, ruler, mode, record, state, slot, at) VALUES
  ('b1', 'standard_single_target', 'base', '{"weapon":"furis"}', 'todo', 1, '1970-01-01T00:00:00.000Z')`).run();
LIBRARY.raw.prepare("INSERT INTO queue (batch, build_id, ruler, mode) VALUES ('q', 'b1', 'standard_single_target', 'base')").run();

const code = await open("asker1");
check("nobody runs a riven gain whose question is not frozen", (await work(A) || {}).kind !== "riven_gain");
// The lease A was just handed on the board order would keep it from more work.
LIBRARY.raw.prepare("UPDATE orders SET lease = NULL, lease_until = NULL, leased_to = NULL").run();
check("only the bot freezes the question", (await appraise("POST", `/api/appraise/${code}/request`, FROZEN)).status === 401);
const stored = await appraise("POST", `/api/appraise/${code}/request`, FROZEN, bot);
const again = await appraise("POST", `/api/appraise/${code}/request`, { ...FROZEN, request: { other: 1 } }, bot);
check("...once: a second freeze changes nothing", stored.body.stored === true && again.body.stored === false);

check("a computer on another engine is handed nothing", (await work(A, "e0")) === null);
// FEW CORES WAIT THEIR TURN: a riven gain someone waits on goes to a computer
// that can run it fast, until it has waited long enough for anyone to take it.
check("a computer offering few cores is not handed a fresh riven gain", (await work(A, "e1", 2) || {}).kind !== "riven_gain");
LIBRARY.raw.prepare("UPDATE orders SET lease = NULL, lease_until = NULL, leased_to = NULL").run();
const t1 = await work(A);
check("a computer asking for work is handed the riven gain before any board order, frozen question and all",
  t1 && t1.kind === "riven_gain" && t1.code === code && JSON.stringify(t1.request) === JSON.stringify(FROZEN.request)
  && JSON.stringify(t1.context) === JSON.stringify(FROZEN.context), JSON.stringify(t1));
check("...and nobody else holds it meanwhile", (await work(B) || {}).kind !== "riven_gain");
// STILL AT IT: the holder's word runs its lease on; anyone else's runs nothing.
LIBRARY.raw.prepare("UPDATE appraisals SET lease_until = ? WHERE code = ?").run(new Date(Date.now() + 1000).toISOString(), code);
const kept = await appraise("POST", `/api/appraise/${code}/renew`, { lease: t1.lease, verifier: A });
const until = LIBRARY.raw.prepare("SELECT lease_until FROM appraisals WHERE code = ?").get(code).lease_until;
check("a computer still searching keeps its lease running on", kept.body.held === true && Date.parse(until) > Date.now() + 60_000, String(Date.parse(until) - Date.now()));
check("...and one that does not hold it is told so, and changes nothing",
  (await appraise("POST", `/api/appraise/${code}/renew`, { lease: t1.lease, verifier: B })).body.held === false
  && LIBRARY.raw.prepare("SELECT lease_until FROM appraisals WHERE code = ?").get(code).lease_until === until);
const told1 = await appraise("POST", "/api/appraise/claim", { channel: "qq" }, bot);
check("the chat may be told a computer took it, once", told1.body.started.some((x) => x.code === code));
await appraise("POST", "/api/appraise/claim", { channel: "qq", started_told: [code] }, bot);
check("...and is not told twice", !(await appraise("POST", "/api/appraise/claim", { channel: "qq" }, bot)).body.started.some((x) => x.code === code));
LIBRARY.raw.prepare("UPDATE orders SET lease = NULL, lease_until = NULL, leased_to = NULL").run();

const BUILD = { weapon: "torid", mods: ["serration", "riven"], riven_pos: ["critical_damage", "multishot"], riven_neg: "" };
const answer = (t, v, extra = {}) => appraise("POST", `/api/appraise/${code}/result`,
  { build: BUILD, lease: t.lease, verifier: v, engine: "e1", score: 12.5, work: 4e9, ...extra });
check("an answer under another computer's lease is not kept", (await answer(t1, B)).body.first === false
  && !LIBRARY.raw.prepare("SELECT 1 FROM appraisal_results WHERE verifier = ?").get(B));
const a1 = await answer(t1, A);
check("an answer under its lease is kept, and is the first", a1.body.ok && a1.body.first === true
  && !!LIBRARY.raw.prepare("SELECT 1 FROM appraisal_results WHERE verifier = ?").get(A));
check("...and one answer credits nobody", credited(A) === 0);

check("the second run never goes to a computer of the same owner", (await work(D) || {}).kind !== "riven_gain");
LIBRARY.raw.prepare("UPDATE orders SET lease = NULL, lease_until = NULL, leased_to = NULL").run();
const t2 = await work(B);
check("...but to another owner's", t2 && t2.kind === "riven_gain" && t2.code === code, JSON.stringify(t2));
await answer(t2, B, { score: 12.6 });
check("unequal answers credit nobody", credited(A) === 0 && credited(B) === 0);
LIBRARY.raw.prepare("UPDATE orders SET lease = NULL, lease_until = NULL, leased_to = NULL").run();
const t3 = await work(C);
check("...and a third computer is asked", t3 && t3.kind === "riven_gain", JSON.stringify(t3));
await answer(t3, C);
check("two owners' equal answers credit both — the work, and the day — and not the one that differed",
  credited(A) === 4e9 && credited(C) === 4e9 && credited(B) === 0 && today(A) === 4e9 && today(C) === 4e9,
  `${credited(A)} ${credited(B)} ${credited(C)}`);
check("...after which it is handed out no more", (await work(D) || {}).kind !== "riven_gain");

const late = await open("asker2");
await appraise("POST", `/api/appraise/${late}/request`, FROZEN, bot);
LIBRARY.raw.prepare("UPDATE orders SET lease = NULL, lease_until = NULL, leased_to = NULL").run();
LIBRARY.raw.prepare("UPDATE appraisals SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE leased_to = ?").run(D);
check("a fresh one waits for eight cores", (await work(D, "e1", 4) || {}).code !== late);
LIBRARY.raw.prepare("UPDATE appraisals SET at = ? WHERE code = ?").run(new Date(Date.now() - 20_000).toISOString(), late);
LIBRARY.raw.prepare("UPDATE orders SET lease = NULL, lease_until = NULL, leased_to = NULL").run();
check("...four once a round of asks has passed", (await work(D, "e1", 4) || {}).code === late);
LIBRARY.raw.prepare("UPDATE appraisals SET lease = NULL, lease_until = NULL, leased_to = NULL WHERE code = ?").run(late);
LIBRARY.raw.prepare("UPDATE orders SET lease = NULL, lease_until = NULL, leased_to = NULL").run();
check("...still not one core in its first two minutes", (await work(D, "e1", 1) || {}).code !== late);
LIBRARY.raw.prepare("UPDATE orders SET lease = NULL, lease_until = NULL, leased_to = NULL").run();
LIBRARY.raw.prepare("UPDATE appraisals SET at = ? WHERE code = ?").run(new Date(Date.now() - 3 * 60_000).toISOString(), late);
check("...but past them, any computer takes it", (await work(D, "e1", 1) || {}).code === late);

// A QUESTION FROZEN BEFORE A RELEASE is still served after it, and its cap on
// answers counts the served engine's alone: three answers of the old engine
// that never agreed do not shut it.
LIBRARY.raw.prepare("UPDATE appraisals SET agreed_at = '1970-01-01T00:00:00.001Z' WHERE code != ?").run("none");
const older = await open("asker3");
await appraise("POST", `/api/appraise/${older}/request`, { ...FROZEN, engine: "e0" }, bot);
LIBRARY.raw.prepare("UPDATE appraisals SET at = ? WHERE code = ?").run(new Date(Date.now() - 3 * 60_000).toISOString(), older);
for (const [v, k] of [["x1", "a"], ["x2", "b"], ["x3", "c"]]) {
  LIBRARY.raw.prepare("INSERT INTO appraisal_results (code, build, at, verifier, score, work, key, engine) VALUES (?, '{}', ?, ?, 1, 1, ?, 'e0')")
    .run(older, new Date().toISOString(), v.repeat(8), k);
}
LIBRARY.raw.prepare("UPDATE appraisals SET lease = NULL, lease_until = NULL, leased_to = NULL").run();
LIBRARY.raw.prepare("UPDATE orders SET lease = NULL, lease_until = NULL, leased_to = NULL").run();
check("a question frozen on an older engine is still handed out after a release, its old answers not counted against it",
  (await work(D, "e1", 8) || {}).code === older);

// A SURVEY: riven gains the owner opens in bulk go after a new build's board rows
// and after a chat's; like every riven gain they need another owner AND another
// network to agree, and the agreed build goes into the board's door.
{
  const L = LIBRARY.raw, now = Date.now(), ago = (ms) => new Date(now - ms).toISOString();
  L.prepare("UPDATE appraisals SET agreed_at = ?").run(ago(0));
  L.prepare("UPDATE orders SET state = 'settled', lease = NULL, lease_until = NULL, leased_to = NULL").run();
  const from = (ip) => ({
    work: async (v) => (await (await verifyRoute(new Request("https://x/api/board/work", { method: "POST", headers: { "cf-connecting-ip": ip },
      body: JSON.stringify({ verifier: v, engine: "e1", protocol: PROTOCOL, consent: { v: 1, at: "2026-10-08T08:00:00.000Z" }, lanes: 8 }) }),
      env, "/api/board/work")).json()).work,
    answer: async (c, t, v, build) => (await appraiseRoute(new Request(`https://x/api/appraise/${c}/result`, { method: "POST",
      headers: { "cf-connecting-ip": ip }, body: JSON.stringify({ build, lease: t.lease, verifier: v, score: 9, work: 5e9, engine: "e1" }) }),
      env, `/api/appraise/${c}/result`)).json(),
  });
  const home = from("203.0.113.7"), there = from("198.51.100.9"), third = from("192.0.2.4");
  L.prepare(`INSERT INTO appraisals (code, channel, chat, asker, room, weapon, ruler, riven, at, request, engine)
    VALUES ('SV1', 'survey', '{}', 'wfsim', '', 'furis', 'standard_single_target', '{}', ?, ?, 'e1')`).run(ago(10 * 86_400_000 / 10), JSON.stringify(FROZEN));
  L.prepare(`INSERT INTO orders (identity, ruler, mode, record, state, slot, at, priority) VALUES
    ('new1', 'standard_single_target', 'base', '{"weapon":"furis"}', 'todo', 1, ?, 0)`).run(ago(0));
  L.prepare("INSERT INTO queue (batch, build_id, ruler, mode) VALUES ('arrivals-x', 'new1', 'standard_single_target', 'base')").run();
  const E = "e".repeat(24), F = "f".repeat(24), G = "g".repeat(24);
  for (const v of [E, F, G]) L.prepare("INSERT OR IGNORE INTO verifiers (id, seen) VALUES (?, '2026-10-08')").run(v);
  const first = await home.work(E);
  check("a new build's board row goes before a survey's riven gain", first && first.kind !== "riven_gain" && first.record && first.record.weapon === "furis",
    JSON.stringify(first));
  // …and a new build's row this client may NOT take (it measured it already)
  // does not hold the survey back from it, nor does a rescore go first.
  L.prepare(`UPDATE orders SET state = 'open', engine = 'e1', produced_by = ?, clients = ?, lease = NULL, lease_until = NULL, leased_to = NULL
    WHERE identity = 'new1'`).run(E, E);
  L.prepare(`INSERT INTO orders (identity, ruler, mode, record, state, slot, at, priority) VALUES
    ('old1', 'standard_single_target', 'base', '{"weapon":"furis"}', 'todo', 2, ?, 1)`).run(ago(0));
  L.prepare("INSERT INTO queue (batch, build_id, ruler, mode) VALUES ('rescore-x', 'old1', 'standard_single_target', 'base')").run();
  const s1 = await home.work(E);
  check("...and with none it may take, the survey's is handed out, before a rescore", s1 && s1.kind === "riven_gain" && s1.code === "SV1", JSON.stringify(s1));
  L.prepare("UPDATE orders SET state = 'settled', lease = NULL, lease_until = NULL, leased_to = NULL").run();
  const build = { weapon: "furis", mods: ["serration", "riven"], riven_pos: ["critical_damage", "multishot"], riven_neg: "zoom" };
  await home.answer("SV1", s1, E, build);
  check("a further answer never goes to the network of the first", ((await home.work(F)) || {}).code !== "SV1");
  const s2 = await there.work(G);
  check("...but to another", s2 && s2.code === "SV1", JSON.stringify(s2));
  const inboxBefore = L.prepare("SELECT COUNT(*) AS n FROM inbox").get().n;
  await there.answer("SV1", s2, G, build);
  const row = L.prepare("SELECT record FROM inbox ORDER BY rowid DESC LIMIT 1").get();
  check("two owners on two networks agreeing send the build to the board's door, as a reader's submission",
    L.prepare("SELECT agreed_at FROM appraisals WHERE code = 'SV1'").get().agreed_at
    && L.prepare("SELECT COUNT(*) AS n FROM inbox").get().n === inboxBefore + 1 && JSON.parse(row.record).weapon === "furis", row && row.record);
  L.prepare(`INSERT INTO appraisals (code, channel, chat, asker, room, weapon, ruler, riven, at, request, engine)
    VALUES ('SV2', 'survey', '{}', 'wfsim', '', 'furis', 'standard_single_target', '{}', ?, ?, 'e1'),
           ('QQ1', 'qq', '{}', 'someone', '', 'furis', 'standard_single_target', '{}', ?, ?, 'e1')`)
    .run(ago(3_600_000), JSON.stringify(FROZEN), ago(600_000), JSON.stringify(FROZEN));
  const chat = await third.work("h".repeat(24));
  check("a riven gain someone waits on in a chat goes before a survey's", chat && chat.code === "QQ1", JSON.stringify(chat));
}

console.log(failures ? `\n${failures} failed` : "\na riven gain is run by the community and credited when two owners agree");
process.exitCode = failures ? 1 : 0;
