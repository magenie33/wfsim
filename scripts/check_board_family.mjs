import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { testDb } from "./pg/test_db.mjs";
import { familyRoute, askFamilyTransfers, FAMILY_PRODUCERS } from "../worker/board_family.js";
import { facts, workOf } from "../worker/factory.js";
import { userTasksRoute, purgeUserCalculations } from "../worker/user_tasks.js";
import { syncRoute } from "../worker/sync.js";
import { setDial } from "../worker/dials.js";
import { shipFamily } from "./ship_family.mjs";
import worker from "../worker/index.js";

const db = await testDb("library"), engine = "test-engine";
const env = { LIBRARY: db, FACTORY_TOKEN: "check", ASSETS: { fetch: async () => Response.json({ engine }) } };
const source = "a".repeat(32), ruler = "standard_single_target";
const job = { source, weapon: "burston", ruler, request: { weapon: "burston", strategy: "quick", runs: 2, finalists: 1,
  starts: [{ slots: ["serration", null, "riven:board"], fixed: [{ kind: "mods", idx: 2 }] }] },
  context: { weapon: "burston", benchmark: ruler, mode: "base", rivens: { "riven:board": { riven_pos: ["critical_damage", "multishot"], riven_neg: "" } } } };
const call = async (jobs = [job], token = "check", pinned) => {
  const response = await familyRoute(new Request("https://wfsim.test/api/board/family", { method: "POST",
    headers: { authorization: `Bearer ${token}` }, body: JSON.stringify({ jobs, ...(pinned === undefined ? {} : { engine: pinned }) }) }), env);
  return { status: response.status, ...await response.json() };
};
let passed = 0;
const check = async (name, run) => { await run(); passed++; console.log(`  ok  ${name}`); };
const count = async table => (await db.get(`SELECT count(*) AS n FROM ${table}`)).n;
await check("only the trusted intake can admit family work", async () => {
  assert.equal((await call([job], "wrong")).status, 401);
  assert.equal(await count("family_transfers"), 0);
});
await check("the source build must land before its family requests", async () => {
  assert.equal((await call()).status, 409);
  assert.equal(await count("family_transfers"), 0);
  await db.run("INSERT INTO entity_weapon_builds (id, at, record) VALUES (?, ?, ?)", source, "2026-11-01", JSON.stringify({ weapon: "burston_prime" }));
});
await check("an empty admission never scans historical builds", async () => {
  assert.equal((await call([])).status, 200);
  assert.equal(await count("family_transfers"), 0);
});
await check("durable admission never waits for task registration", async () => {
  const demand = facts().demand, demandAll = facts().demandAll;
  facts().demand = facts().demandAll = async () => { throw new Error("registration unavailable"); };
  try {
    const got = await call();
    assert.equal(got.status, 200); assert.equal(got.engine, engine); assert.equal(got.accepted, 1);
    assert.equal(await count("family_transfers"), 1); assert.equal(await count("demands"), 0);
  } finally { facts().demand = demand; facts().demandAll = demandAll; }
});
await check("interrupted admission resumes only its durable transfer", async () => {
  env.FACTORY_PAUSED = "1";
  assert.equal((await call()).status, 200);
  assert.equal(await count("family_transfers"), 1); assert.equal(await count("demands"), 0);
  delete env.FACTORY_PAUSED;
  await askFamilyTransfers(env);
  assert.equal(await count("demands"), 1);
});
await check("concurrent resubmissions coalesce into one independent background search", async () => {
  await Promise.all([call(), call()]);
  assert.equal(await count("family_transfers"), 1); assert.equal(await count("questions"), 1);
  const demand = await db.get("SELECT * FROM relation_task_consumers");
  assert.equal(demand.producer, "family_build"); assert.equal(demand.waits, 0); assert.equal(demand.initiator, "system");
  assert.equal((await db.get("SELECT * FROM state_compute_work")).verb, "optimize_report");
});
await check("an interrupted engine rebind remains retryable after its metadata write", async () => {
  const row = await db.get("SELECT * FROM entity_family_search_requests");
  await db.run("UPDATE entity_family_search_requests SET asked_engine = NULL WHERE id = ?", row.id);
  const demandAll = facts().demandAll;
  facts().demandAll = async () => { throw new Error("interrupted ask"); };
  try { await assert.rejects(askFamilyTransfers(env), /interrupted ask/); }
  finally { facts().demandAll = demandAll; }
  assert.equal((await db.get("SELECT asked_engine FROM entity_family_search_requests")).asked_engine, null);
  await askFamilyTransfers(env);
  assert.equal((await db.get("SELECT asked_engine FROM entity_family_search_requests")).asked_engine, engine);
  assert.equal(await count("questions"), 1);
});
await check("user report retention preserves a live family search", async () => {
  await purgeUserCalculations(env);
  assert.equal(await count("questions"), 1); assert.equal(await count("demands"), 1);
});
await check("only capable clients lease family optimization reports on either sync path", async () => {
  for (const proc of [0, 1]) {
    await setDial(env, "sync_proc", proc, "check");
    const sync = async (capabilities, digit) => {
      const response = await syncRoute(new Request("https://wfsim.test/api/board/sync", { method: "POST", body: JSON.stringify({
        verifier: digit.repeat(24), engine, protocol: 8, capabilities, consent: { v: 3, at: new Date().toISOString() }, lanes: 2, want_ms: 1000 }) }), env);
      return response.json();
    };
    assert.ok(!(await sync([], String(proc + 1))).work);
    const got = await sync(["calculation-reports-v1"], String(proc + 3));
    assert.equal(got.work?.kind, "user_calculation", JSON.stringify(got));
    assert.equal(got.work.calculation, "optimization"); assert.deepEqual(got.work.request, job.request);
    await facts().release(env, String(proc + 3).repeat(24), got.work.lease);
  }
  await setDial(env, "sync_proc", 0, "check");
});
const witness = digit => ({ device: digit.repeat(24), owner: `owner-${digit}`, siblings: [digit.repeat(24), "cap:calculation-reports-v1"], net: `net-${digit}` });
const best = { mods: ["serration", "speed_trigger", "riven:board"], evolutions: [], arcane: ["primary_merciless"],
  mode: "base", replay: { weapon: "burston" }, forma: {} };
const report = { ok: true, target: "check", duration: 2, results: [best] };
await check("one witness cannot publish a derived build", async () => {
  const who = witness("5"), work = await facts().next(env, who, engine);
  assert.equal(workOf(work).calculation, "optimization");
  const path = "/api/user-tasks/answer";
  const response = await userTasksRoute(new Request(`https://wfsim.test${path}`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({
    verifier: who.device, engine, lease: work.lease, question: work.question, work: 100, report }) }), env, path);
  assert.equal(response.status, 200); assert.equal(await count("inbox"), 0);
});
await check("independent agreement submits the target build with trusted nonrecursive provenance", async () => {
  const who = witness("6"), work = await facts().next(env, who, engine);
  assert.ok(work); await facts().answer(env, who, { lease: work.lease, engine, result: { report }, work: 100 });
  assert.equal(await count("inbox"), 1); assert.equal(await count("scores"), 0);
  const inbox = await db.get("SELECT * FROM record_build_submissions"), transfer = await db.get("SELECT * FROM entity_family_search_requests");
  assert.equal(record_build_submissions.family_source, transfer.id); assert.equal(record_build_submissions.initiator, "system");
  assert.equal(transfer.state, "succeeded");
  const record = JSON.parse(record_build_submissions.record);
  assert.equal(record.weapon, "burston"); assert.deepEqual(record.mods, ["serration", "speed_trigger", "riven"]);
  assert.deepEqual(record.riven_pos, ["critical_damage", "multishot"]);
  assert.equal(record.family_source, undefined);
});
await check("completed transfers and replayed facts cannot submit again", async () => {
  await call();
  const row = await db.get("SELECT * FROM entity_family_search_requests");
  await FAMILY_PRODUCERS.family_build.on_fact(env, { ref: row.id }, { engine, result: { report } });
  assert.equal(await count("inbox"), 1); assert.equal(await count("questions"), 1);
});
await check("a crash after derived submission resumes without a second inbox record", async () => {
  const row = await db.get("SELECT * FROM entity_family_search_requests"), prepare = db.prepare;
  await db.run("UPDATE entity_family_search_requests SET state = 'waiting' WHERE id = ?", row.id);
  db.prepare = sql => {
    if (sql.includes("SET state = 'succeeded'")) throw new Error("interrupted completion");
    return prepare(sql);
  };
  try {
    await assert.rejects(FAMILY_PRODUCERS.family_build.on_fact(env, { ref: row.id }, { engine, result: { report } }), /interrupted completion/);
  } finally { db.prepare = prepare; }
  await FAMILY_PRODUCERS.family_build.on_fact(env, { ref: row.id }, { engine, result: { report } });
  assert.equal(await count("inbox"), 1);
});
await check("the public submission cannot forge derivation provenance", async () => {
  const response = await worker.fetch(new Request("https://wfsim.test/api/board/submit", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({
    weapon: "burston", mods: ["serration"], family_source: "forged" }) }), env, { waitUntil() {} });
  assert.equal(response.status, 200, await response.clone().text());
  const rows = await db.all("SELECT * FROM record_build_submissions WHERE family_source IS NULL");
  assert.equal(rows.length, 1); assert.equal(JSON.parse(rows[0].record).family_source, undefined);
});
await check("an empty search terminates without inventing a score", async () => {
  const row = await db.get("SELECT * FROM entity_family_search_requests");
  await db.run("UPDATE entity_family_search_requests SET state = 'waiting' WHERE id = ?", row.id);
  await FAMILY_PRODUCERS.family_build.on_fact(env, { ref: row.id }, { engine, result: { report: { results: [] } } });
  assert.equal((await db.get("SELECT state FROM entity_family_search_requests")).state, "failed");
  assert.equal(await count("scores"), 0);
});
await check("transfer transport preserves requests and requires acknowledged durable admission", async () => {
  const sent = [];
  await shipFamily([{ family: [job] }], { token: "check", send: async (url, options) => {
    if (url.endsWith("/release.json")) return Response.json({ engine });
    const body = JSON.parse(options.body); sent.push(body);
    return Response.json({ ok: true, engine, accepted: body.jobs.length });
  } });
  assert.deepEqual(sent, [{ engine, jobs: [job] }]);
  await assert.rejects(shipFamily([{ family: [job] }], { token: "check", wait: async () => {},
    send: async () => Response.json({ ok: false }, { status: 503 }) }));
  await shipFamily([{ record: {} }], { send: () => { throw new Error("empty transfers must not send"); } });
});
await check("the deployed daemon role can deliver family facts", async () => {
  const role = `family_daemon_${Date.now()}`;
  await db.exec(readFileSync(new URL("../deploy/factoryd_grants.sql", import.meta.url), "utf8")
    .replaceAll("wfsim_factoryd", role).replace(/GRANT CONNECT ON DATABASE wfsim TO [^;]+;/, ""));
  const schema = (await db.get("SELECT current_schema() AS name")).name;
  await db.exec(`GRANT USAGE ON SCHEMA ${schema} TO ${role}; SET ROLE ${role}`);
  try {
    const row = await db.get("SELECT * FROM entity_family_search_requests");
    await db.run("UPDATE entity_family_search_requests SET state = 'waiting' WHERE id = ?", row.id);
    await FAMILY_PRODUCERS.family_build.on_fact(env, { ref: row.id }, { engine, result: { report } });
    assert.equal((await db.get("SELECT state FROM entity_family_search_requests")).state, "succeeded");
  } finally { await db.exec("RESET ROLE"); }
});
await check("a future arrival can seed the same family on a new engine without a historical sweep", async () => {
  env.ASSETS = { fetch: async () => Response.json({ engine: "next-engine" }) };
  await call([]);
  assert.equal(await count("family_transfers"), 1);
  await call();
  assert.equal(await count("family_transfers"), 2);
  assert.equal((await db.get("SELECT count(*) AS n FROM entity_family_search_requests WHERE engine = 'next-engine'")).n, 1);
});
await check("leased search receipts deliver atomic build batches after the lease closes", async () => {
  env.AUTH_SECRET = "family-harvest-check-only";
  const question = "e".repeat(32), lease = "f".repeat(32), verifier = "e".repeat(24);
  const at = new Date().toISOString(), until = new Date(Date.now() + 60_000).toISOString();
  await db.run(`INSERT INTO state_compute_work (id, verb, request, state, due_at, opened_at, lease, lease_until, leased_to)
    VALUES (?, 'optimize_report', ?, 'open', ?, ?, ?, ?, ?)`, question, JSON.stringify({ weapon: "torid", strategy: "quick", __board_upload: true }), at, at, lease, until, verifier);
  await db.run(`INSERT INTO relation_task_consumers (producer, ref, question, state, asked_at, due_at, initiator)
    VALUES ('user_calculation', 'harvest-service', ?, 'live', ?, ?, 'user:harvest-requester')`, question, at, at);
  const submit = body => worker.fetch(new Request("https://wfsim.test/api/board/submit", { method: "POST",
    headers: { "content-type": "application/json" }, body: JSON.stringify(body) }), env, { waitUntil() {} });
  const grantedResponse = await submit({ builds: [], harvest: { lease, verifier, question, engine: "next-engine" } });
  assert.equal(grantedResponse.status, 200, await grantedResponse.clone().text());
  const grant = await grantedResponse.json();
  assert.equal(grant.question, question);
  await db.run("UPDATE state_compute_work SET lease = NULL, leased_to = NULL, lease_until = NULL WHERE id = ?", question);
  await db.run("UPDATE relation_task_consumers SET state = 'served' WHERE question = ?", question);
  const before = await count("inbox");
  const builds = Array.from({ length: 128 }, () => ({ weapon: "torid", mods: ["serration"], score: 123, initiator: "system", family_source: "fake" }));
  const calls = db.cost.calls;
  const response = await submit({ builds, harvest: { receipt: grant.receipt, question, engine: "next-engine" } });
  assert.equal(response.status, 200, await response.clone().text());
  assert.equal(db.cost.calls - calls, 1, "one multi-row insert is one database round trip");
  assert.equal((await response.json()).results.filter(item => item.ok).length, builds.length);
  assert.equal(await count("inbox"), before + builds.length);
  const rows = await db.all("SELECT record, initiator, family_source FROM record_build_submissions WHERE initiator = 'user:harvest-requester'");
  assert.equal(rows.length, builds.length);
  assert.ok(rows.every(row => row.family_source === null && JSON.parse(row.record).score === undefined));
  assert.equal(await count("scores"), 0);
  const wrong = await submit({ builds, harvest: { receipt: grant.receipt, question: "a".repeat(32) } });
  assert.equal(wrong.status, 403);
  assert.equal(await count("inbox"), before + builds.length);
  await db.run("UPDATE state_compute_work SET lease = ?, leased_to = ?, lease_until = ? WHERE id = ?", lease, verifier, until, question);
  await db.run(`INSERT INTO relation_task_consumers (producer, ref, question, state, asked_at, due_at, initiator)
    VALUES ('family_build', 'harvest-family-source', ?, 'live', ?, ?, 'system')`, question, at, at);
  const familyGrant = await (await submit({ builds: [], harvest: { lease, verifier, question, engine: "next-engine" } })).json();
  const familyBatch = await submit({ builds: [{ weapon: "torid", mods: ["serration"] }], harvest: { receipt: familyGrant.receipt } });
  assert.equal(familyBatch.status, 200, await familyBatch.clone().text());
  const familyInbox = await db.get("SELECT initiator, family_source FROM record_build_submissions WHERE family_source = 'harvest-family-source'");
  assert.deepEqual(familyInbox, { initiator: "system", family_source: "harvest-family-source" });
});

await check("one admission batch and one resumer batch have bounded database calls", async () => {
  const db = await testDb("library");
  const env = { LIBRARY: db, FACTORY_TOKEN: "check", ASSETS: { fetch: async () => Response.json({ engine }) } };
  await db.exec(`INSERT INTO entity_weapon_builds (id,at,record) SELECT lpad(to_hex(i),32,'0'),'2026-11-01','{}' FROM generate_series(1,100) i`);
  const jobs = Array.from({ length: 100 }, (_, i) => ({ ...job, source: (i + 1).toString(16).padStart(32, "0") }));
  const post = async () => familyRoute(new Request("https://wfsim.test/api/board/family", { method: "POST",
    headers: { authorization: "Bearer check" }, body: JSON.stringify({ engine, jobs }) }), env);
  let calls = db.cost.calls;
  const first = await post();
  assert.equal(first.status, 200); assert.equal((await first.json()).accepted, 100);
  assert.ok(db.cost.calls - calls <= 2, String(db.cost.calls - calls));
  assert.equal((await db.get("SELECT count(*) AS n FROM relation_task_consumers")).n, 0);
  calls = db.cost.calls;
  assert.equal((await post()).status, 200);
  assert.ok(db.cost.calls - calls <= 2, String(db.cost.calls - calls));
  calls = db.cost.calls;
  await askFamilyTransfers(env);
  assert.ok(db.cost.calls - calls <= 5, String(db.cost.calls - calls));
  assert.equal((await db.get("SELECT count(*) AS n FROM relation_task_consumers")).n, 100);
  assert.equal((await db.get("SELECT count(*) AS n FROM entity_family_search_requests WHERE asked_engine=?", engine)).n, 100);
  await db.close();
});
await check("every source must land before a partial admission can be acknowledged", async () => {
  const db = await testDb("library"), env = { LIBRARY: db, FACTORY_TOKEN: "check", ASSETS: { fetch: async () => Response.json({ engine }) } };
  const second = "b".repeat(32), jobs = [job, { ...job, source: second }];
  await db.run("INSERT INTO entity_weapon_builds(id,at,record) VALUES (?,?,?)", source, "2026-11-01", "{}");
  const post = () => familyRoute(new Request("https://wfsim.test/api/board/family", { method: "POST",
    headers: { authorization: "Bearer check" }, body: JSON.stringify({ engine, jobs }) }), env);
  assert.equal((await post()).status, 409);
  assert.equal((await db.get("SELECT count(*) AS n FROM entity_family_search_requests")).n, 1);
  await db.run("INSERT INTO entity_weapon_builds(id,at,record) VALUES (?,?,?)", second, "2026-11-01", "{}");
  const replay = await post();
  assert.equal(replay.status, 200); assert.equal((await replay.json()).accepted, 2);
  assert.equal((await db.get("SELECT count(*) AS n FROM entity_family_search_requests")).n, 2);
  assert.equal((await db.get("SELECT count(*) AS n FROM relation_task_consumers")).n, 0);
  await db.close();
});
await check("interrupted batched marking reuses demands without dropping waiting transfers", async () => {
  const db = await testDb("library"), env = { LIBRARY: db, FACTORY_TOKEN: "check", ASSETS: { fetch: async () => Response.json({ engine }) } };
  await db.run("INSERT INTO entity_weapon_builds(id,at,record) VALUES (?,?,?)", source, "2026-11-01", "{}");
  const post = () => familyRoute(new Request("https://wfsim.test/api/board/family", { method: "POST",
    headers: { authorization: "Bearer check" }, body: JSON.stringify({ engine, jobs: [job] }) }), env);
  assert.equal((await post()).status, 200);
  const prepare = db.prepare;
  db.prepare = text => {
    if (text.includes("SET asked_engine =")) throw new Error("marking interrupted");
    return prepare(text);
  };
  try { await assert.rejects(askFamilyTransfers(env), /marking interrupted/); }
  finally { db.prepare = prepare; }
  assert.equal((await db.get("SELECT asked_engine FROM entity_family_search_requests")).asked_engine, null);
  assert.equal((await db.get("SELECT count(*) AS n FROM relation_task_consumers")).n, 1);
  await askFamilyTransfers(env);
  assert.equal((await db.get("SELECT count(*) AS n FROM relation_task_consumers")).n, 1);
  assert.equal((await db.get("SELECT asked_engine FROM entity_family_search_requests")).asked_engine, engine);
  await db.close();
});
await check("lost admission ACK is replayable across an engine release without a second transfer", async () => {
  const db = await testDb("library"), env = { LIBRARY: db, FACTORY_TOKEN: "check", ASSETS: { fetch: async () => Response.json({ engine }) } };
  await db.run("INSERT INTO entity_weapon_builds(id,at,record) VALUES (?,?,?)", source, "2026-11-01", "{}");
  const bodies = [];
  await shipFamily([{ family: [job] }], { token: "check", wait: async () => {}, send: async (url, options) => {
    if (url.endsWith("/release.json")) return Response.json({ engine });
    bodies.push(options.body);
    const response = await familyRoute(new Request(url, options), env);
    if (bodies.length === 1) {
      env.ASSETS = { fetch: async () => Response.json({ engine: "next-engine" }) };
      return new Response(new ReadableStream({ start(controller) {
        controller.enqueue(new TextEncoder().encode('{"ok":true,'));
        controller.error(new TypeError("lost ACK body", { cause: { code: "ECONNRESET" } }));
      } }));
    }
    return response;
  } });
  assert.equal(bodies.length, 2); assert.equal(bodies[0], bodies[1]);
  assert.equal((await db.get("SELECT count(*) AS n FROM entity_family_search_requests")).n, 1);
  assert.equal((await db.get("SELECT engine FROM entity_family_search_requests")).engine, engine);
  const other = { ...job, ruler: "heavy_gunner", context: { ...job.context, benchmark: "heavy_gunner" } };
  const refused = await familyRoute(new Request("https://wfsim.test/api/board/family", { method: "POST",
    headers: { authorization: "Bearer check" }, body: JSON.stringify({ engine, jobs: [other] }) }), env);
  assert.equal(refused.status, 409);
  assert.equal((await db.get("SELECT count(*) AS n FROM entity_family_search_requests")).n, 1);
  await db.close();
});
await check("streamed family transport deduplicates globally and bounds count and UTF-8 bytes", async () => {
  let produced = 0, firstPostAt = null;
  const sent = [], target = i => ({ ...job, source: i.toString(16).padStart(32, "0") });
  async function* builds() {
    for (let i = 1; i <= 251; i++) { produced++; yield { family: [target(i), target(1)] }; }
  }
  const send = async (url, options) => {
    if (url.endsWith("/release.json")) return Response.json({ engine });
    firstPostAt ??= produced;
    const body = JSON.parse(options.body); sent.push(options.body);
    return Response.json({ ok: true, engine, accepted: body.jobs.length });
  };
  const result = await shipFamily(builds(), { token: "check", send });
  assert.ok(firstPostAt < produced); assert.equal(result.jobs, 251);
  const jobs = sent.flatMap(body => JSON.parse(body).jobs);
  assert.equal(jobs.length, 251); assert.equal(new Set(jobs.map(item => item.source)).size, 251);
  assert.ok(sent.every(body => JSON.parse(body).jobs.length <= 100 && Buffer.byteLength(body) <= 512 * 1024));
  sent.length = 0;
  await shipFamily(Array.from({ length: 40 }, (_, i) => ({ family: [{ ...target(i), context: { ...job.context, text: "雪".repeat(10_000) } }] })), { token: "check", send });
  assert.ok(sent.length > 1 && sent.every(body => Buffer.byteLength(body) <= 512 * 1024));
  const posts = sent.length;
  await assert.rejects(shipFamily([{ family: [{ ...job, context: { ...job.context, text: "雪".repeat(200_000) } }] }], { token: "check", send }), /byte limit/);
  assert.equal(sent.length, posts);
});
await check("only transient transport errors retry, with bounded attempts and stable batch bytes", async () => {
  const bodies = [], delays = [];
  await shipFamily([{ family: [job] }], { token: "check", wait: async ms => delays.push(ms), send: async (url, options) => {
    if (url.endsWith("/release.json")) return Response.json({ engine });
    bodies.push(options.body);
    if (bodies.length === 1) throw Object.assign(new Error("network cut"), { code: "ECONNRESET" });
    if (bodies.length === 2) return Response.json({ ok: false }, { status: 503 });
    return Response.json({ ok: true, engine, accepted: 1 });
  } });
  assert.equal(bodies.length, 3); assert.equal(new Set(bodies).size, 1); assert.deepEqual(delays, [2000, 4000]);
  let tries = 0;
  await assert.rejects(shipFamily([{ family: [job] }], { token: "check", wait: async () => {}, send: async (url) => {
    if (url.endsWith("/release.json")) return Response.json({ engine });
    tries++; return Response.json({ ok: false }, { status: 503 });
  } }));
  assert.equal(tries, 3);
  for (const status of [400, 401, 409, 413]) {
    tries = 0;
    await assert.rejects(shipFamily([{ family: [job] }], { token: "check", wait: async () => {}, send: async url => {
      if (url.endsWith("/release.json")) return Response.json({ engine });
      tries++; return Response.json({ ok: false }, { status });
    } }));
    assert.equal(tries, 1);
  }
  for (const ack of [{ ok: true }, { ok: true, engine: "other-engine", accepted: 1 }, { ok: true, engine, accepted: 0 }]) {
    tries = 0;
    await assert.rejects(shipFamily([{ family: [job] }], { token: "check", send: async url => {
      if (url.endsWith("/release.json")) return Response.json({ engine });
      tries++; return Response.json(ack);
    } }), /acknowledged/);
    assert.equal(tries, 1);
  }
  tries = 0;
  await assert.rejects(shipFamily([{ family: [job] }], { token: "check", send: async url => {
    if (url.endsWith("/release.json")) return Response.json({ engine });
    tries++; return new Response('{"ok":');
  } }), SyntaxError);
  assert.equal(tries, 1);
  tries = 0;
  await shipFamily([{ family: [job] }], { token: "check", wait: async () => {}, send: async url => {
    if (url.endsWith("/release.json")) return Response.json({ engine });
    if (++tries === 1) return new Response(new ReadableStream({ start(controller) {
      controller.error(new DOMException("ACK timed out", "AbortError"));
    } }));
    return Response.json({ ok: true, engine, accepted: 1 });
  } });
  assert.equal(tries, 2);
  tries = 0; delays.length = 0;
  await shipFamily([{ family: [job] }], { token: "check", wait: async ms => delays.push(ms), send: async url => {
    if (url.endsWith("/release.json")) return Response.json({ engine });
    if (++tries === 1) return Response.json({ ok: false }, { status: 429, headers: { "retry-after": "9999" } });
    return Response.json({ ok: true, engine, accepted: 1 });
  } });
  assert.equal(tries, 2); assert.deepEqual(delays, [30_000]);
});
await db.close();
console.log(`\n${passed} family spread checks passed`);
