// THE ACCOUNTS PAUSE HOLDS WHAT WRITES AND LETS WHAT READS GO ON — worker/index.js
// `accountWrite`, docs/BOARD.md §"Postgres": while `ACCOUNTS_PAUSED` is set a
// sign-in step, an account change and the paid half's writes are answered 503
// with a time to try again, and nothing reaches the accounts; a read does.
//   node scripts/check_accounts_pause.mjs
import worker from "../worker/index.js";

let failed = 0;
const check = (name, ok, detail = "") => {
  console.log(`  ${ok ? "ok  " : "FAIL"}  ${name}${ok ? "" : `  — ${detail}`}`);
  if (!ok) failed++;
};

/// ACCOUNTS THAT FAIL ON ANY USE: a request that got past the pause and
/// touched them throws, and is not answered 503 with `maintenance`.
const touched = [];
const ACCOUNTS = { prepare: (sql) => { touched.push(sql); throw new Error("the accounts were touched"); },
  batch: () => { touched.push("batch"); throw new Error("the accounts were touched"); } };
const env = { ACCOUNTS, ACCOUNTS_PAUSED: "1", AUTH_SECRET: "s",
  ASSETS: { fetch: async () => new Response("<!doctype html>", { headers: { "content-type": "text/html" } }) } };
const call = async (method, path) => {
  const r = await worker.fetch(new Request(`https://wfsim.app${path}`, { method,
    ...(method === "POST" ? { body: "{}", headers: { "content-type": "application/json" } } : {}) }), env, null);
  let body = null;
  try { body = await r.clone().json(); } catch (_) { /* not json */ }
  return { status: r.status, retry: r.headers.get("retry-after"), reason: body && body.reason };
};

const held = [["GET", "/api/auth/google/callback"], ["POST", "/api/auth/email/code"], ["POST", "/api/account/devices/claim"],
  ["POST", "/api/account/contribution"], ["POST", "/api/account/delete"], ["POST", "/api/cloud/sync"], ["POST", "/api/stripe/webhook"]];
for (const [method, path] of held) {
  const r = await call(method, path);
  check(`${method} ${path} waits: 503, maintenance, try again later`,
    r.status === 503 && r.reason === "maintenance" && Number(r.retry) > 0, JSON.stringify(r));
}
check("...and none of them reached the accounts", touched.length === 0, touched.join(" | "));

// WHAT ONLY READS goes on: answered by its own route, never by the pause.
for (const [method, path] of [["GET", "/api/account/devices"], ["GET", "/api/cloud/page.js"], ["GET", "/api/cloud/thanks"]]) {
  const r = await call(method, path).catch((e) => ({ status: 0, reason: String(e.message) }));
  check(`${method} ${path} is not held by the pause`, r.reason !== "maintenance", JSON.stringify(r));
}

// UNSET, NOTHING IS HELD.
delete env.ACCOUNTS_PAUSED;
const free = await call("POST", "/api/account/contribution").catch((e) => ({ status: 0, reason: String(e.message) }));
check("without the switch a write is not held", free.reason !== "maintenance", JSON.stringify(free));

console.log(failed ? `\n${failed} failed` : "\nthe accounts pause holds every write and no read");
process.exit(failed ? 1 : 0);
