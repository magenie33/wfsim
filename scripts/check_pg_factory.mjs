// THE FACT FACTORY ON POSTGRES DOES WHAT IT DOES ON D1 (worker/factory_db.js).
// One seeded workload — rows asked, packs handed out and answered, rows renewed
// and given back, disputes the official machines settle and refuse a client in,
// points, ledgers, the ranking, the fair-share table, the watchdog's audit — run
// twice through the worker's own doors, each in a process of its own: once with
// the factory's tables in D1, once in Postgres (PGlite) with them DROPPED from
// D1, so a query left on the wrong database fails. Every answer and every table
// must be the same, a lease token compared by when it first appeared.
//   npm install --prefix scripts/pg && node scripts/check_pg_factory.mjs
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const ROOT = new URL("../", import.meta.url);
const HOT = ["questions", "demands", "answers", "verifiers", "verifier_hours", "watchdog_told", "points_pool", "verifier_asks"];

if (process.argv[2] === "--run") await run(process.argv[3]);
else main();

function main() {
  const of = (backend) => {
    const r = spawnSync(process.execPath, [fileURLToPath(import.meta.url), "--run", backend], { encoding: "utf8", maxBuffer: 256 << 20 });
    const line = (r.stdout || "").trim().split("\n").pop();
    if (r.status !== 0 || !line.startsWith("{")) {
      console.log(`FAIL  the ${backend} run did not finish\n${(r.stderr || r.stdout || "").slice(-2000)}`);
      process.exit(1);
    }
    return JSON.parse(line);
  };
  // THE SECOND RUN IS PGLITE, or a real Postgres when `PG_URL` names one.
  const lite = of("d1"), pg = of(process.env.PG_URL ? "pgurl" : "pg");
  let failures = 0;
  const steps = Math.max(lite.trace.length, pg.trace.length);
  for (let i = 0; i < steps; i++) {
    const a = lite.trace[i], b = pg.trace[i];
    if (JSON.stringify(a) !== JSON.stringify(b)) {
      failures++;
      console.log(`FAIL  step ${i} (${(a || b).step}) differs\n  d1: ${JSON.stringify(a).slice(0, 900)}\n  pg: ${JSON.stringify(b).slice(0, 900)}`);
      if (failures >= 3) break;
    }
  }
  if (!failures) console.log(`  ok   ${steps} steps answered alike, ${lite.trace.filter((t) => t.tables).length} table dumps equal`);
  const kinds = new Map();
  for (const t of lite.trace) if (t.out) { const k = t.step.replace(/ \d+/, ""); kinds.set(k, (kinds.get(k) || 0) + 1); }
  console.log(`       ${[...kinds].map(([k, n]) => `${k} x${n}`).join(", ")}`);
  const need = ["work 6e1", "work 7e0", "verify", "renew", "release", "work 7e1 ahead", "answers", "claim", "official answers"];
  const missing = need.filter((k) => !kinds.has(k));
  console.log(`  ${missing.length ? "FAIL" : "ok "}  every door is walked through${missing.length ? `: not ${missing.join(", ")}` : ""}`);
  if (missing.length) failures++;
  const facts = lite.trace.at(-1).tables.questions.filter((q) => q.state === "fact").length;
  const refused = lite.trace.at(-1).tables.verifiers.filter((v) => v.refusals > 0).length;
  const ok = facts >= 10 && refused >= 1;
  console.log(`  ${ok ? "ok " : "FAIL"}  the workload reaches what it must: ${facts} facts, ${refused} client(s) refused`);
  if (!ok) failures++;
  const pairs = lite.trace.at(-1).tables.answers.filter((a) => !a.official).map((a) => `${a.question}|${a.device}|${a.engine}`);
  const once = pairs.length === new Set(pairs).size;
  console.log(`  ${once ? "ok " : "FAIL"}  answers sent twice are recorded once`);
  if (!once) failures++;
  const asks = lite.trace.at(-1).tables.verifier_asks;
  const counted = asks.some((a) => a.asks > 0) && asks.some((a) => a.failed > 0 && a.idle >= a.failed)
    && !asks.some((a) => a.hour < "2026-10-01");
  console.log(`  ${counted ? "ok " : "FAIL"}  an ask is counted by its hour, with the page's own idle and failed asks, and none too old`);
  if (!counted) failures++;
  console.log(failures ? `\n${failures} failed` : "\nthe factory answers alike on D1 and on Postgres");
  process.exitCode = failures ? 1 : 0;
}

async function run(backend) {
  // ONE CLOCK AND ONE DIE for both runs, moved only by the workload.
  let clock = Date.parse("2026-10-12T08:00:00.000Z");
  const RealDate = Date;
  globalThis.Date = class extends RealDate {
    constructor(...a) { super(...(a.length ? a : [clock])); }
    static now() { return clock; }
  };
  // THE WORKLOAD'S OWN DIE, apart from `Math.random`, which a database library
  // may draw from too; the factory's (a recheck, a spot check) never picks.
  let seed = 7;
  const rnd = () => { seed = (seed * 1103515245 + 12345) % 2147483648; return seed / 2147483648; };
  Math.random = () => 0.5;

  const { DatabaseSync } = await import("node:sqlite");
  const d1 = (file, drop = []) => {
    const db = new DatabaseSync(":memory:");
    db.exec(readFileSync(new URL(`worker/${file}`, ROOT), "utf8"));
    for (const t of drop) db.exec(`DROP TABLE ${t}`);
    const stmt = (sql, args = []) => ({
      sql, args,
      bind: (...a) => stmt(sql, a),
      first: async () => db.prepare(sql).get(...args) ?? null,
      all: async () => ({ results: db.prepare(sql).all(...args) }),
      run: async () => ({ meta: { changes: Number(db.prepare(sql).run(...args).changes) } }),
    });
    return { raw: db, prepare: (sql) => stmt(sql),
      batch: async (ss) => {
        db.exec("BEGIN");
        try {
          const out = ss.map((s) => (/^\s*select|\breturning\b/i.test(s.sql) ? { results: db.prepare(s.sql).all(...s.args) }
            : { meta: { changes: Number(db.prepare(s.sql).run(...s.args).changes) } }));
          db.exec("COMMIT");
          return out;
        } catch (e) { db.exec("ROLLBACK"); throw e; }
      } };
  };
  const LIBRARY = d1("schema.sql", backend === "d1" ? [] : HOT), ACCOUNTS = d1("accounts.sql");
  const env = { LIBRARY, ACCOUNTS, FACTORY_TOKEN: "ft", AUTH_SECRET: "s",
    ASSETS: { fetch: async () => new Response(JSON.stringify({ engine: "e1", release: "r1", accepted: [{ engine: "e0", until: "2099-01-01T00:00:00.000Z", changed: ["torid"] }] })) } };
  let pgc = null;
  // A REAL POSTGRES (`PG_URL`, a scratch database) through postgres.js, as the
  // worker reaches one — node's build of the same release as worker/vendor's.
  if (backend === "pgurl") {
    const { default: postgres } = await import("./pg/node_modules/postgres/src/index.js");
    const { pgDb, postgresClient, POSTGRES_OPTIONS } = await import("../worker/factory_db.js");
    const sql = postgres(process.env.PG_URL, { ...POSTGRES_OPTIONS, onnotice: () => {} });
    await sql.unsafe(`DROP TABLE IF EXISTS ${HOT.join(", ")}`);
    await sql.unsafe(readFileSync(new URL("worker/pg/schema.sql", ROOT), "utf8"));
    const client = postgresClient(sql);
    // A QUERY THAT FAILS says which, and with what.
    const said = (q) => async (t, p) => {
      try { return await q(t, p); } catch (e) { console.error(`QUERY ${t.slice(0, 600)}\nPARAMS ${JSON.stringify(p).slice(0, 400)}`); throw e; }
    };
    client.query = said(client.query);
    const begin = client.transaction;
    client.transaction = (fn) => begin((tx) => fn({ query: said(tx.query) }));
    env.FACTORY = pgDb(client);
    pgc = { close: () => client.end() };
  }
  if (backend === "pg") {
    const { PGlite } = await import("./pg/node_modules/@electric-sql/pglite/dist/index.js");
    const { pgDb } = await import("../worker/factory_db.js");
    pgc = new PGlite({ parsers: { 20: (x) => Number(x), 1700: (x) => Number(x) } });
    await pgc.exec(readFileSync(new URL("worker/pg/schema.sql", ROOT), "utf8"));
    const wrap = (c) => ({ query: async (t, p) => { const r = await c.query(t, p); return { rows: r.rows, affected: r.affectedRows ?? 0 }; } });
    env.FACTORY = pgDb({ ...wrap(pgc), transaction: (fn) => pgc.transaction((tx) => fn(wrap(tx))) });
  }
  const { verifyRoute, PROTOCOL } = await import("../worker/verify.js");
  const { boardTasksRoute } = await import("../worker/board_tasks.js");
  const { officialRoute, refusalsFor, facts } = await import("../worker/factory.js");
  const { contributionRoute } = await import("../worker/contribution.js");
  const { tableOf } = await import("../worker/fair.js");
  const { sha256 } = await import("../worker/accounts.js");
  refusalsFor(true);

  // A LEASE TOKEN IS NOBODY'S TO COMPARE: each is named by when it first appeared.
  const tokens = new Map();
  const tok = (x) => (x == null ? x : (tokens.has(x) || tokens.set(x, `L${tokens.size}`), tokens.get(x)));
  const norm = (v, key = "") => Array.isArray(v) ? v.map((x) => norm(x))
    : v && typeof v === "object" ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, norm(v[k], k)]))
    : ["lease", "lease_prev"].includes(key) ? tok(v) : v;
  const trace = [];
  const say = (step, out) => trace.push({ step, out: norm(out) });
  const dump = async (step) => {
    const db = env.FACTORY || LIBRARY, tables = {};
    const key = { questions: "id", demands: "producer, ref", answers: "id", verifiers: "id", verifier_hours: "verifier, hour",
      watchdog_told: "cause", points_pool: "at, source, work", verifier_asks: "verifier, hour" };
    for (const t of HOT) tables[t] = norm((await db.prepare(`SELECT * FROM ${t} ORDER BY ${key[t]}`).all()).results);
    trace.push({ step, tables });
  };
  const call = async (route, path, body, headers = {}, method = "POST") => {
    const r = await route(new Request(`https://wfsim.app${path}`, { method, headers: { origin: "https://wfsim.app", ...headers },
      ...(method === "POST" ? { body: JSON.stringify(body) } : {}) }), env, path.split("?")[0]);
    return r ? r.json() : { route: "none" };
  };
  const TOKEN = { authorization: "Bearer ft" };
  const YES = { v: 1, at: "2026-10-01T08:00:00.000Z" };

  // THE LIBRARY: builds on three weapons, owed on two rulers and a scene, some measured.
  const L = LIBRARY.raw, weapons = ["braton_prime", "torid", "boar_prime"], rulers = ["standard_single_target", "standard_multi_target", "standard_single_target__sp200"];
  const today = "2026-10-12";
  L.exec(`INSERT INTO batches (id, at, why, total) VALUES ('arrivals-${today}', '${today}', 't', 0), ('refresh-${today}', '${today}', 't', 0), ('rescore-x', '${today}', 't', 0)`);
  const ids = [];
  for (let i = 0; i < 24; i++) {
    const id = `b${String(i).padStart(2, "0")}`;
    ids.push(id);
    L.prepare("INSERT INTO builds (id, at, record) VALUES (?, ?, ?)").run(id, i % 3 ? `${today}T07:00:00.000Z` : "2026-01-01T00:00:00.000Z",
      JSON.stringify({ weapon: weapons[i % 3], mods: ["serration", id] }));
    for (const r of rulers) {
      const batch = i % 5 === 0 ? `refresh-${today}` : `arrivals-${today}`;
      L.prepare("INSERT INTO queue (batch, build_id, ruler, mode) VALUES (?, ?, ?, 'base')").run(batch, id, r);
      if (i % 5 === 0) L.prepare(`INSERT INTO scores (identity, ruler, mode, measured_by, score, metric, cost_seconds, started_at, finished_at)
        VALUES (?, ?, 'base', 'v', 1, 'kpm', 0, '2026-10-01T00:00:00.000Z', '2026-10-01T00:00:00.000Z')`).run(id, r);
    }
  }
  // DEVICES: two of one person's, one of another's, the rest unclaimed.
  const dev = Array.from({ length: 8 }, (_, i) => (i + 1).toString(16).repeat(24));
  ACCOUNTS.raw.exec(`INSERT INTO accounts (id, created_at, username) VALUES ('ann', '2026-01-01T00:00:00.000Z', 'ann'), ('bob', '2026-01-01T00:00:00.000Z', 'bob');
    INSERT INTO devices (verifier, account, claimed_at) VALUES ('${dev[0]}', 'ann', 'x'), ('${dev[1]}', 'ann', 'x'), ('${dev[2]}', 'bob', 'x');
    INSERT INTO device_spans (verifier, account, from_hour) VALUES ('${dev[0]}', 'ann', ''), ('${dev[1]}', 'ann', ''), ('${dev[2]}', 'bob', '2026-10-12T09');
    INSERT INTO contribution_choice (account, named, chosen_at) VALUES ('ann', 1, 'x');`);
  ACCOUNTS.raw.prepare("INSERT INTO sessions (token_hash, account, expires_at) VALUES (?, 'ann', '2099-01-01T00:00:00.000Z')").run(await sha256("tok-ann"));
  const ANN = { cookie: "wfsim_session=tok-ann" };

  // EVERY ROW ASKED, by batch and by the backfill.
  for (const batch of [`arrivals-${today}`, `refresh-${today}`]) {
    const rows = L.prepare("SELECT build_id AS identity, ruler, mode FROM queue WHERE batch = ?").all(batch).slice(0, 40);
    say(`ask ${batch}`, await call(boardTasksRoute, "/api/board/ask", { batch, rows }, TOKEN));
  }
  for (let i = 0; i < 3; i++) say("backfill", await call(boardTasksRoute, "/api/board/ask", { backfill: true }, TOKEN));
  await dump("asked");

  // A ROW'S TRUE SCORE, and the liar who answers another.
  const truth = (q) => 1 + (parseInt(q.slice(0, 6), 16) % 1000) / 7;
  const liar = dev[1];
  const held = new Map();
  for (let step = 0; step < 70; step++) {
    clock += 37_000 + Math.floor(rnd() * 60_000);
    const v = dev[Math.floor(rnd() * dev.length)], ip = `203.0.113.${dev.indexOf(v) + 1}`;
    const roll = rnd();
    if (roll < 0.55) {
      // A PAGE OF THIS PROTOCOL, one of the last (a row alone, `next`), one asking
      // ahead, one on an older engine still accepted on some weapons.
      const kind = rnd();
      const body = { verifier: v, engine: kind > 0.9 ? "e0" : "e1", protocol: kind < 0.12 ? 6 : PROTOCOL, consent: YES, lanes: 4,
        ...(kind > 0.8 && kind <= 0.9 && held.has(v) ? { ahead: true } : {}), ...(rnd() < 0.3 ? { pace_ms: 20_000 } : {}),
        // THE PAGE'S OWN COUNT of asks that brought nothing, one hour of it too old to keep.
        ...(step % 4 === 0 ? { idle: [{ hour: new Date(clock - 3_600_000).toISOString().slice(0, 13), asks: 3, failed: 2, lanes: 12 },
          { hour: "2026-01-01T00", asks: 5, failed: 0, lanes: 20 }] } : {}) };
      const r = await call(verifyRoute, "/api/board/work", body, { "cf-connecting-ip": ip });
      say(`work ${dev.indexOf(v)} ${body.protocol}${body.engine}${body.ahead ? " ahead" : ""}`, r);
      if (r && r.pack) held.set(v, { ...r.pack, engine: body.engine });
      // A ROW ALONE is answered at once, by protocol 6's door.
      if (r && r.work) {
        say(`verify ${dev.indexOf(v)}`, await call(verifyRoute, "/api/board/verify", { lease: r.work.lease, verifier: v, engine: body.engine,
          score: truth(r.work.identity), metric: "kpm", work: 7_000_000_000, compute_ms: 900 }, { "cf-connecting-ip": ip }));
      }
    } else if (roll < 0.8 && held.has(v) && held.get(v).tasks[0].single) {
      const pack = held.get(v), t = pack.tasks[0].single;
      held.delete(v);
      say(`verify ${dev.indexOf(v)}`, await call(verifyRoute, "/api/board/verify", { lease: pack.lease, verifier: v, engine: pack.engine,
        score: 3.5, metric: "kpm", work: 7_000_000_000, compute_ms: 900, identity: t.identity }, { "cf-connecting-ip": ip }));
    } else if (roll < 0.8 && held.has(v)) {
      const pack = held.get(v);
      held.delete(v);
      if (rnd() < 0.2) say(`renew ${dev.indexOf(v)}`, await call(verifyRoute, "/api/board/renew",
        { verifier: v, lease: pack.lease, question: pack.tasks[0].question }, { "cf-connecting-ip": ip }));
      const rows = pack.tasks.map((t) => ({ question: t.question, metric: "kpm", compute_ms: 900, counts: { runs: 3 },
        score: v === liar ? truth(t.question) + 1 : truth(t.question), work: 7_000_000_000 }));
      say(`answers ${dev.indexOf(v)}`, await call(verifyRoute, "/api/board/answers",
        { lease: pack.lease, verifier: v, engine: pack.engine, done: true, answers: rows.slice(0, rnd() < 0.15 ? 1 : rows.length) }, { "cf-connecting-ip": ip }));
    } else if (roll < 0.86 && held.has(v)) {
      say(`release ${dev.indexOf(v)}`, await call(verifyRoute, "/api/board/release", { verifier: v, lease: held.get(v).lease }));
      held.delete(v);
    } else if (roll < 0.93) {
      const orders = ids.slice(0, 12).map((identity) => ({ identity, ruler: rulers[0], mode: "base" }));
      say(`mine ${dev.indexOf(v)}`, await call(verifyRoute, "/api/board/mine", { verifier: v, orders }));
    } else {
      say(`points ${dev.indexOf(v)}`, await call(contributionRoute, "/api/board/points", { verifier: v, since: "2026-10-12T00" }));
    }
    if (step % 10 === 9) await dump(`step ${step}`);
  }

  // A DISPUTE FOR CERTAIN: the liar and two honest computers on the same rows.
  for (const v of [liar, dev[3], dev[4]]) {
    clock += 41_000;
    const ip = `198.51.100.${dev.indexOf(v) + 1}`;
    const r = await call(verifyRoute, "/api/board/work", { verifier: v, engine: "e1", protocol: PROTOCOL, consent: YES, lanes: 4 }, { "cf-connecting-ip": ip });
    say(`dispute work ${dev.indexOf(v)}`, r);
    if (!r || !r.pack) continue;
    const rows = r.pack.tasks.map((t) => ({ question: t.question, metric: "kpm", compute_ms: 900,
      score: v === liar ? truth(t.question) + 1 : truth(t.question), work: 7_000_000_000 }));
    say(`dispute answers ${dev.indexOf(v)}`, await call(verifyRoute, "/api/board/answers",
      { lease: r.pack.lease, verifier: v, engine: "e1", done: false, answers: rows }, { "cf-connecting-ip": ip }));
    // …AND SENT AGAIN, as a page does when the reply was lost — its rows handed
    // on meanwhile, so it is the previous lease that still lets it answer.
    const fdb = env.FACTORY || LIBRARY;
    for (const x of rows) {
      await fdb.prepare("UPDATE questions SET lease_prev = ?, leased_to_prev = ? WHERE id = ?").bind(r.pack.lease, v, x.question).run();
    }
    say(`dispute answers again ${dev.indexOf(v)}`, await call(verifyRoute, "/api/board/answers",
      { lease: r.pack.lease, verifier: v, engine: "e1", done: true, answers: rows }, { "cf-connecting-ip": ip }));
  }
  await dump("disputed");

  // THE SCRIPTS' READS of the factory (worker/factory.js `report`).
  const queueRows = L.prepare("SELECT batch, build_id, ruler, mode FROM queue ORDER BY batch, build_id, ruler, mode").all();
  say("report claimed", await call(officialRoute, "/api/tasks/official/report", { witness: "official:diff", report: "claimed", rows: queueRows }, TOKEN));
  say("report refresh_live", await call(officialRoute, "/api/tasks/official/report", { witness: "official:diff", report: "refresh_live" }, TOKEN));
  const unv = await call(officialRoute, "/api/tasks/official/report", { witness: "official:diff", report: "unverified" }, TOKEN);
  say("report unverified", { n: unv.rows.length, rows: unv.rows.map((r) => JSON.stringify(r)).sort() });
  say("report calibration", await call(officialRoute, "/api/tasks/official/report",
    { witness: "official:diff", report: "calibration", since: "2026-01-01T00:00:00.000Z", offset: 0 }, TOKEN));

  // THE OFFICIAL MACHINES: past every deadline, they settle disputes and open rows.
  clock += 8 * 86_400_000;
  const W = "official:diff";
  const claimed = await call(officialRoute, "/api/tasks/official/claim", { witness: W, engine: "e1", max: 500, producers: ["new_build", "sweep", "refresh"] }, TOKEN);
  say("claim", { n: claimed.tasks.length, states: claimed.tasks.map((t) => [t.question, t.state]).sort() });
  const answers = claimed.tasks.slice(0, 60).map((t) => ({ ref: t.demands[0].ref, result: { score: truth(t.question), metric: "kpm" }, work: 7_000_000_000 }));
  say("official answers", await call(officialRoute, "/api/tasks/official/answer", { witness: W, engine: "e1", answers, producers: ["new_build", "sweep", "refresh"] }, TOKEN));
  say("official release", await call(officialRoute, "/api/tasks/official/release", { witness: W }, TOKEN));
  await dump("settled");

  // WHAT READS THE FACTORY: points, ledgers, the ranking, the fair table, the audit.
  for (const v of dev) {
    say(`points ${dev.indexOf(v)}`, await call(contributionRoute, "/api/board/points", { verifier: v }));
    say(`ledger ${dev.indexOf(v)}`, await call(contributionRoute, "/api/board/ledger", { verifier: v, from: "" }));
  }
  say("ranking", await call(contributionRoute, "/api/contributors?limit=50", null, {}, "GET"));
  say("computing", await call(contributionRoute, "/api/board/computing", null, {}, "GET"));
  say("demand", await call(contributionRoute, "/api/board/demand", null, {}, "GET"));
  say("account ledger", await call(contributionRoute, "/api/account/ledger?from=", null, ANN, "GET"));
  say("account devices", await call(contributionRoute, "/api/account/devices", null, ANN, "GET"));
  say("fair", await tableOf(env, "e1", clock));
  say("audit", await facts().audit(env, clock, "e1"));
  await dump("end");
  if (pgc) await pgc.close();
  process.stdout.write(`\n${JSON.stringify({ trace })}\n`);
}
