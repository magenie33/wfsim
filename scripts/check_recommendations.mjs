import assert from "node:assert/strict";
import { recommendationsRoute, recommendationExport } from "../worker/recommendations.js";
import { sha256 } from "../worker/accounts.js";
import { testDb } from "./pg/test_db.mjs";

const db = await testDb("accounts"), library = await testDb("library");
const env = { ACCOUNTS: db, LIBRARY: library };
const identity = "a".repeat(32), other = "b".repeat(32), token = "recommendation-session";
const check = (name, condition) => { assert.ok(condition, name); console.log("  ok  " + name); };
await db.run("INSERT INTO accounts (id, created_at, username) VALUES (?, ?, ?)", "reader", "2026-01-01T00:00:00.000Z", "reader");
await db.run("INSERT INTO sessions (token_hash, account, expires_at) VALUES (?, ?, ?)", await sha256(token), "reader", "2099-01-01T00:00:00.000Z");
for (const id of [identity, other]) {
  await library.run("INSERT INTO builds (id, at, record) VALUES (?, ?, ?)", id, "2026-11-04", JSON.stringify({ weapon: "furis", mods: ["hornet_strike"] }));
  await library.run(`INSERT INTO scores (identity, ruler, mode, measured_by, score, metric, cost_seconds, started_at, finished_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`, id, "standard_single_target", "cycle", "test", 100, "kpm", 0, "2026-11-04", "2026-11-04");
}
let currentDay;
const day = async value => {
  currentDay = value;
  return db.exec("CREATE OR REPLACE FUNCTION recommendation_day() RETURNS date LANGUAGE sql STABLE AS $$ SELECT '" + value + "'::date $$");
};
await day("2026-11-04");
const request = (path, body, headers = {}) => new Request("https://wfsim.app" + path, {
  method: body ? "POST" : "GET", headers: { cookie: "wfsim_session=" + token, ...(body ? { "content-type": "application/json", origin: "https://wfsim.app" } : {}), ...headers },
  ...(body ? { body: JSON.stringify(body) } : {}),
});
const call = async (path, body, headers) => {
  if (path === "/api/recommendations" && body) body = { day: currentDay, ...body };
  const response = await recommendationsRoute(request(path, body, headers), env, path.split("?")[0]);
  return { status: response.status, headers: response.headers, ...await response.json() };
};
const vote = { identity, ruler: "standard_single_target", mode: "cycle", action: "recommend" };
const preflight = await recommendationsRoute(new Request("https://wfsim.app/api/recommendations", { method: "OPTIONS", headers: { origin: "https://client.invalid", "access-control-request-headers": "authorization,content-type" } }), env, "/api/recommendations");
check("Browser clients can preflight bearer requests without cookie credentials", preflight.status === 204 && preflight.headers.get("access-control-allow-origin") === "*" && !preflight.headers.has("access-control-allow-credentials"));
check("A vote requires a registered account", (await call("/api/recommendations", vote, { cookie: "" })).status === 401);
check("Cookie writes reject a foreign origin", (await call("/api/recommendations", vote, { origin: "https://elsewhere.invalid" })).status === 403);
check("A client cannot invent a build", (await call("/api/recommendations", { ...vote, identity: "c".repeat(32) })).status === 404);
const voted = await call("/api/recommendations", vote);
check("A new vote is pending and earns no settled point", voted.ok && voted.pending && (await call("/api/recommendations/state", { identities: [identity] })).points === 0);
check("The daily allowance is per build rather than global", (await call("/api/recommendations", { ...vote, identity: other })).pending);
await call("/api/recommendations", { ...vote, identity: other, action: "withdraw" });
await Promise.all(Array.from({ length: 8 }, () => call("/api/recommendations", vote)));
check("Repeated and concurrent submissions are idempotent", (await db.get("SELECT count(*) AS n FROM recommendations")).n === 1);
check("Pending votes do not enter public heat", !Object.keys((await call("/api/recommendations/heat/furis")).heat).length);
check("The account sees its pending state", (await call("/api/recommendations/state", { identities: [identity, other] })).pending.join() === identity);
await call("/api/recommendations", { ...vote, action: "withdraw" });
check("A pending vote can be withdrawn", (await db.get("SELECT count(*) AS n FROM recommendations")).n === 0);
await call("/api/recommendations", vote);
await day("2026-11-05");
check("A retry after midnight cannot cast an unintended second-day vote", (await call("/api/recommendations", { ...vote, day: "2026-11-04" })).status === 409 && (await db.get("SELECT count(*) AS n FROM recommendations")).n === 1);
let heat = await call("/api/recommendations/heat/furis");
check("The first settlement credits exactly 1.0", heat.heat[identity] === 1 && heat.day === "2026-11-05");
check("Settlement credits one permanent recommendation point", (await call("/api/recommendations/state", { identities: [identity] })).points === 1);
await call("/api/recommendations", { ...vote, action: "withdraw" });
check("A settled vote cannot be withdrawn", (await db.get("SELECT count(*) AS n FROM recommendations")).n === 1);
await call("/api/recommendations", vote);
check("The new UTC day grants a new vote", (await db.get("SELECT count(*) AS n FROM recommendations")).n === 2);
await day("2026-11-06");
heat = await call("/api/recommendations/heat/furis");
check("Each vote ages independently, in exact tenths", heat.heat[identity] === 1.9);
await day("2026-11-15");
heat = await call("/api/recommendations/heat/furis");
check("A vote is zero on its eleventh settled day", heat.heat[identity] === 0.1);
await day("2026-11-16");
heat = await call("/api/recommendations/heat/furis");
check("Fully expired votes leave heat", heat.heat[identity] === undefined);
const history = await call("/api/account/recommendations?filter=available");
check("Expired builds remain in history with permanent points", history.points === 2 && history.builds.length === 1 && history.builds[0].points === 2 && history.available === 1);
check("History preserves the authoritative build", history.builds[0].record.weapon === "furis" && history.builds[0].identity === identity);
check("The complete history includes expired vote dates", (await call("/api/account/recommendations/votes?identity=" + identity)).votes.length === 2);
check("Account data never enters public heat", !JSON.stringify(heat).includes("reader") && !JSON.stringify(heat).includes("record"));
const key = "wf_test_recommendation_agent";
await db.run("INSERT INTO agent_keys (id, key_hash, name, created_at, account, last_used_at) VALUES (?, ?, ?, ?, ?, ?)",
  "agent", await sha256(key), "Client", "2026-01-01", "reader", "2099-01-01");
// The public key prefix is read from the existing credential contract.
const { readFileSync } = await import("node:fs");
const prefix = readFileSync(new URL("../worker/agents.js", import.meta.url), "utf8").match(/const KEY_PREFIX = "([^"]+)"/)[1];
const agentKey = prefix + "recommendation-check";
await db.run("UPDATE agent_keys SET key_hash = ? WHERE id = ?", await sha256(agentKey), "agent");
const headers = { authorization: "Bearer " + agentKey, cookie: "", origin: "https://client.invalid" };
check("A claimed third-party client can recommend", (await call("/api/recommendations", vote, headers)).ok);
await db.run("UPDATE agent_keys SET account = NULL WHERE id = ?", "agent");
check("An unclaimed credential cannot recommend", (await call("/api/recommendations", vote, headers)).status === 401);
await db.run("UPDATE agent_keys SET account = ? WHERE id = ?", "reader", "agent");
await call("/api/recommendations", vote);
check("The browser and its client share the same daily allowance", (await db.get("SELECT count(*) AS n FROM recommendations WHERE day = '2026-11-16'")).n === 1);
await db.run("INSERT INTO accounts (id, created_at, username) VALUES (?, ?, ?)", "another-reader", "2026-01-01", "another_reader");
await db.run("INSERT INTO sessions (token_hash, account, expires_at) VALUES (?, ?, ?)", await sha256("another-session"), "another-reader", "2099-01-01");
const another = { cookie: "wfsim_session=another-session" };
check("Accounts cannot read another account's recommendation history", (await call("/api/account/recommendations?filter=all", undefined, another)).total === 0
  && (await call("/api/account/recommendations/votes?identity=" + identity, undefined, another)).votes.length === 0);
check("Malformed batches are rejected", (await call("/api/recommendations/state", { identities: Array(201).fill(identity) })).status === 400);
check("Malformed object identities are rejected", (await call("/api/recommendations", { ...vote, identity: { toString: null } })).status === 400);
const malformed = await recommendationsRoute(new Request("https://wfsim.app/api/recommendations", { method: "POST", headers: { cookie: "wfsim_session=" + token, origin: "https://wfsim.app", "content-type": "application/json" }, body: "null" }), env, "/api/recommendations");
check("A null JSON body is rejected without an exception", malformed.status === 400);
const oversized = await recommendationsRoute(new Request("https://wfsim.app/api/recommendations", { method: "POST", headers: { cookie: "wfsim_session=" + token, origin: "https://wfsim.app", "content-type": "application/json" }, body: "x".repeat(10001) }), env, "/api/recommendations");
check("Streaming bodies are bounded before JSON parsing", oversized.status === 413);
check("Histories are paginated and validate cursors", (await call("/api/account/recommendations?offset=-1")).status === 400);
check("History rejects invalid calendar dates", (await call("/api/account/recommendations/votes?identity=" + identity + "&before=2026-02-30")).status === 400);
check("The account export includes recommendation dates", (await recommendationExport(env, "reader")).recommendations.length === 3);
const OriginalDate = Date, originalCache = globalThis.caches;
try {
  const instant = OriginalDate.parse('2026-11-04T23:59:58.500Z');
  globalThis.Date = class extends OriginalDate { constructor(...args) { super(...(args.length ? args : [instant])); } static now() { return instant; } };
  globalThis.caches = { default: { match: async () => Response.json({ ok: true, heat: {} }, { headers: { 'cache-control': 'public, max-age=30' } }) } };
  const calls = db.cost.calls;
  const cached = await call('/api/recommendations/heat/furis');
  check('A cached response cannot renew freshness across midnight', cached.headers.get('cache-control') === 'public, max-age=1');
  check('Public cache hits do not query either database', db.cost.calls === calls);
} finally {
  globalThis.Date = OriginalDate;
  if (originalCache === undefined) delete globalThis.caches; else globalThis.caches = originalCache;
}
env.RECOMMENDATION_LIMIT = { limit: async () => ({ success: false }) };
const throttled = await call("/api/recommendations", vote);
check("Request limits return a retry time instead of banning clients", throttled.status === 429 && throttled.headers.get("retry-after") === "60");
delete env.RECOMMENDATION_LIMIT;
await db.run("DELETE FROM accounts WHERE id = ?", "reader");
check("Deleting an account erases its recommendation history", (await db.get("SELECT count(*) AS n FROM recommendations")).n === 0);
await Promise.all([db.close(), library.close()]);
console.log("\nRecommendation rules passed");
