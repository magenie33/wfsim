import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { testDb } from "./pg/test_db.mjs";
import { facts } from "../worker/factory.js";
import { appraiseRoute, askRiven } from "../worker/appraise.js";
import { accountRoute, sha256 } from "../worker/accounts.js";
import { contributorProfilesRoute } from "../worker/contributor_profiles.js";
const { userTasksRoute } = await import(process.env.WFSIM_USER_TASK_MODULE || "../worker/user_tasks.js");
const env = { LIBRARY: await testDb("library"), ACCOUNTS: await testDb("accounts"), AUTH_SECRET: "delivery-fixture",
  ASSETS: { fetch: async () => Response.json({ engine: "test-engine" }) } };
const owner = "a".repeat(64), engine = "test-engine", profiles = {};
for (const [index, name] of ["first", "second"].entries()) {
  const profile = randomUUID().replaceAll("-", ""), device = String(index + 1).repeat(24);
  profiles[name] = profile;
  await env.ACCOUNTS.run("INSERT INTO entity_accounts(id,created_at,username,display_name,profile_id) VALUES (?,'2026-01-01',?,?,?)", name, name, name, profile);
  await env.ACCOUNTS.run("INSERT INTO state_login_sessions(token_hash,account,expires_at) VALUES (?,?,'2099-01-01')", await sha256(name), name);
  await env.ACCOUNTS.run("INSERT INTO relation_account_compute_devices(verifier,account,claimed_at) VALUES (?,?,'2026-01-01')", device, name);
  await env.ACCOUNTS.run("INSERT INTO record_compute_device_ownership_intervals(verifier,account,from_hour) VALUES (?,?,'')", device, name);
  await env.ACCOUNTS.run("INSERT INTO state_account_public_attribution(account,named,chosen_at) VALUES (?,1,'initial')", name);
  await env.LIBRARY.run("INSERT INTO entity_compute_devices(id,seen) VALUES (?,'2026-01-01')", device);
}
const role = { kind: "fixture", label: { en: "Fixture identity", zh: "测试身份" }, tier: null, titles: [] };
env.CLOUD = { fetch: async request => {
  if (new URL(request.url).pathname === "/internal/identity") return Response.json({ ok: true, labels: [{ id: role.kind, label: role.label }] });
  const { accounts } = await request.json(), marks = {};
  for (const id of accounts) {
    const choice = await env.ACCOUNTS.get("SELECT named,label FROM state_account_public_attribution WHERE account=?", id);
    if (choice?.named && choice.label === role.kind) marks[id] = role;
  }
  return Response.json({ ok: true, marks });
} };
const req = (path, body, account = "") => new Request("https://wfsim.test" + path, {
  method: body ? "POST" : "GET", headers: { origin: "https://wfsim.test", "content-type": "application/json", cookie: "wfsim_session=" + account },
  ...(body ? { body: JSON.stringify(body) } : {}),
});
const call = async (action, body) => { const path = "/api/user-tasks/" + action, response = await userTasksRoute(req(path, { owner, ...body }), env, path); return { status: response.status, ...await response.json() }; };
const identity = async (name, body) => { const path = "/api/account/identity", response = await accountRoute(req(path, body, name), env, path); return { status: response.status, ...await response.json() }; };
const proof = async id => { const path = "/api/computations/" + id, response = await contributorProfilesRoute(req(path), env, path); return { status: response.status, ...await response.json() }; };
let passed = 0;
const check = async (name, fn) => { await fn(); passed++; console.log("  ok  " + name); };
const input = { input_schema: 1, id: randomUUID(), task: randomUUID(), kind: "appraisal", engine,
  request: { weapon: "torid", runs: 2, strategy: "quick", starts: [], rivens: [{ id: "fixture" }] } };
const accepted = await call("accept", input);
assert.equal(accepted.ok, true);
const report = { ok: true, results: [{ mods: ["serration"], forma: {}, replay: { weapon: "torid" }, score: 10 }], work: 100 };
const who = (digit, name) => ({ device: digit.repeat(24), owner: name, net: "net-" + digit, siblings: [digit.repeat(24), "cap:calculation-reports-v1"] });
const first = who("1", "first"), firstLease = await facts().next(env, first, engine);
await facts().answer(env, first, { lease: firstLease.lease, engine, work: 100, result: { report } });
let early;
await check("one appraisal candidate is deliverable before independent confirmation", async () => {
  early = (await call("status", { ids: [input.id], include_results: true })).tasks[0];
  assert.equal(early.state, "waiting"); assert.ok(early.candidate); assert.deepEqual(early.result.results, report.results);
  assert.match(early.delivery, /^[a-f0-9]{32}$/);
  assert.equal((await env.LIBRARY.get("SELECT state FROM state_compute_work WHERE id=?", accepted.question)).state, "open");
  assert.equal((await env.LIBRARY.get("SELECT sum(signatures) AS n FROM entity_compute_devices")).n, 0);
});
await check("delivery identifies the actual first calculator and cannot borrow typed thanks", async () => {
  const value = await proof(early.delivery);
  assert.equal(value.record.kind, "delivery"); assert.equal(value.record.witnesses.length, 1);
  assert.equal(value.record.witnesses[0].name, "first");
  assert.ok(!JSON.stringify(value).includes(first.device) && !JSON.stringify(value).includes('"owner"'));
});
await check("current privacy and eligible label selection govern the same delivery", async () => {
  assert.equal((await identity("first", { named: true, label: "forged" })).status, 403);
  assert.equal((await identity("first", { named: true, label: role.kind })).ok, true);
  assert.equal((await proof(early.delivery)).record.witnesses[0].mark.kind, role.kind);
  await identity("first", { named: false, label: role.kind });
  const anonymous = (await proof(early.delivery)).record.witnesses[0];
  assert.equal(anonymous.name, null); assert.equal(anonymous.profile, null); assert.equal(anonymous.mark, undefined);
  await identity("first", { named: true, label: "none" });
  await env.ACCOUNTS.run("UPDATE entity_accounts SET display_name='renamed first' WHERE id='first'");
  const renamed = (await proof(early.delivery)).record.witnesses[0];
  assert.equal(renamed.name, "renamed first"); assert.equal(renamed.mark, undefined);
});
const second = who("2", "second"), secondLease = await facts().next(env, second, engine);
assert.equal(secondLease.question, accepted.question);
await facts().answer(env, second, { lease: secondLease.lease, engine, work: 100, result: { report } });
await check("both witnesses earn credit and retain actual completion order separately from delivery", async () => {
  assert.equal((await env.LIBRARY.get("SELECT sum(signatures) AS n FROM entity_compute_devices")).n, 2);
  const verified = await proof(accepted.question);
  assert.equal(verified.record.kind, "optimization");
  assert.deepEqual(verified.record.witnesses.map(w => w.name), ["renamed first", "second"]);
  for (const name of ["first", "second"]) {
    const path = "/api/contributors/profile/" + profiles[name];
    const response = await contributorProfilesRoute(req(path + "?kind=optimization&view=records"), env, path), value = await response.json();
    assert.equal(value.candidates.length, 1);
    assert.equal(value.candidates[0].question, accepted.question);
    assert.equal(value.candidates[0].witnesses.length, 2);
  }
  assert.equal((await proof(early.delivery)).record.witnesses.length, 1);
});
await check("a rejected candidate advances to the next calculator without changing consensus", async () => {
  const value = (await call("status", { ids: [input.id], include_results: true, after: { [input.id]: early.candidate } })).tasks[0];
  assert.ok(value.candidate > early.candidate);
  assert.equal((await proof(value.delivery)).record.witnesses[0].name, "second");
  assert.equal((await env.LIBRARY.get("SELECT sum(signatures) AS n FROM entity_compute_devices")).n, 2);
});
await check("another browser cannot read the first candidate or its report", async () => {
  assert.equal((await call("status", { owner: "b".repeat(64), ids: [input.id], include_results: true })).tasks.length, 0);
});
await check("local delivery uses the captured signed-in account and adds no volunteer credit", async () => {
  const path = "/api/user-tasks/source", body = { owner, task: randomUUID(), profile: profiles.first, engine, weapon: "torid" };
  const response = await userTasksRoute(req(path, body, "first"), env, path), value = await response.json();
  assert.ok(value.source); assert.equal((await proof(value.source)).record.witnesses[0].name, "renamed first");
  const wrong = await userTasksRoute(req(path, { ...body, task: randomUUID(), profile: profiles.second }, "first"), env, path);
  assert.equal((await wrong.json()).source, null);
  assert.equal((await env.LIBRARY.get("SELECT sum(signatures) AS n FROM entity_compute_devices")).n, 2);
});
await check("FX delivers and names its first actual calculator before consensus, then preserves both witnesses", async () => {
  env.BOT_RELAY_TOKEN = "delivery-bot";
  const botCall = async (path, body) => {
    const response = await appraiseRoute(new Request("https://wfsim.test" + path, {method:"POST",
      headers:{authorization:"Bearer delivery-bot"}, body:JSON.stringify(body)}), env, path);
    assert.equal(response.status,200); return response.json();
  };
  const opened = await botCall("/api/appraise/new", {channel:"qq",asker:"test",room:"fixture",weapon:"torid",
    ruler:"standard_single_target",riven:{bonuses:[{id:"multishot",roll:1}],rank:8}});
  const code = opened.code;
  await askRiven(env, {code,channel:"qq",asker:"test",weapon:"torid",ruler:"standard_single_target"},
    {request:{weapon:"torid",runs:2},context:{weapon:"torid"}}, Date.now());
  const calculator = {...first,lanes:8}, leased = await facts().next(env,calculator,engine);
  assert.ok(leased && leased.demands.some(d=>d.ref === code));
  const result = {build:{weapon:"torid",mods:["serration"]},score:12};
  await facts().answer(env,calculator,{lease:leased.lease,engine,work:123,result});
  assert.equal((await env.LIBRARY.get("SELECT state FROM state_compute_work WHERE id=?",leased.question)).state,"open");
  const pending = await botCall("/api/appraise/claim",{channel:"qq"});
  assert.equal(pending.judge.length,1);
  await identity("first",{named:true,label:role.kind});
  const told = await botCall("/api/appraise/claim",{channel:"qq",judged:[{id:pending.judge[0].id,ok:true,verdict:{score:12}}]});
  assert.equal(told.tell[0].calculator.name,"renamed first");
  assert.equal(told.tell[0].calculator.mark.kind,role.kind);
  assert.equal((await env.LIBRARY.get("SELECT sum(signatures) AS n FROM entity_compute_devices")).n,2);
  const verifying = {...second,lanes:8}, lease = await facts().next(env,verifying,engine);
  assert.equal(lease.question,leased.question);
  await facts().answer(env,verifying,{lease:lease.lease,engine,work:123,result});
  const agreed = await proof(leased.question);
  assert.equal(agreed.record.weapon,"torid");
  assert.deepEqual(agreed.record.witnesses.map(w=>w.name),["renamed first","second"]);
  assert.equal((await env.LIBRARY.get("SELECT sum(signatures) AS n FROM entity_compute_devices")).n,4);
  const row = await env.LIBRARY.get("SELECT source FROM record_appraisal_candidates WHERE id=?",pending.judge[0].id);
  assert.equal((await proof(row.source)).record.witnesses.length,1);
});
await env.LIBRARY.close(); await env.ACCOUNTS.close();
console.log(`\n${passed} delivery and identity checks passed`);
