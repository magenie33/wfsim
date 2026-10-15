// USAGE POINTS — the page's half against the worker's, and the endpoint against
// a stub dataset. What must hold (docs/ANALYTICS.md):
//   - every event the page sends is one the worker accepts, and every one the
//     worker accepts is sent from somewhere: a name on one side only is a time
//     series that silently never starts;
//   - the page writes the schema the worker reads;
//   - a point carries the visitor's hash as its index and nothing that identifies them;
//   - every point lands in the usage log, a browser's own request deletes its
//     rows, and the log keeps nothing past its days;
//   - anything that is not a point is refused and never written;
//   - no dev server or check host is live, so no test writes the live dataset.
//
//   node scripts/check_usage_events.mjs
import worker, { USAGE_EVENTS, USAGE_SCHEMA, USAGE_POINT_SCHEMA, usagePoint } from "../worker/index.js";
import { usageVisitor, USAGE_LOG_DAYS } from "../worker/usage_log.js";
import { appSource } from "./app_source.mjs";
import { USAGE_RESULTS, USAGE_LOG_FROM, usageDayRows, rollupUsage } from "../worker/usage_days.js";
import { testDb } from "./pg/test_db.mjs";
import { readFileSync, readdirSync } from "node:fs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const src = appSource();
const calls = [...src.matchAll(/(?<!function )\btrack\(([^,)]*)/g)].map((m) => m[1].trim());
const named = calls.filter((a) => /^"[^"]*"$/.test(a)).map((a) => a.slice(1, -1));
check("every track() call names its event as a literal", named.length === calls.length,
  calls.filter((a) => !/^"[^"]*"$/.test(a)).join(", "));
const unknown = named.filter((e) => !USAGE_EVENTS.includes(e));
check("every event the page sends is one the worker accepts", !unknown.length, unknown.join(", "));
// NONA'S, named as literals at her `count(...)` calls under `nona/`.
const walk = (d) => readdirSync(d, { withFileTypes: true })
  .flatMap((x) => (x.isDirectory() ? walk(`${d}/${x.name}`) : x.name.endsWith(".js") ? [`${d}/${x.name}`] : []));
const nonaSrc = walk("web/src/static/nona").map((f) => readFileSync(f, "utf8")).join("\n");
const nonaCalls = [...nonaSrc.matchAll(/\bcount\(([^,)]*)/g)].map((m) => m[1].trim());
const nonaNamed = nonaCalls.filter((a) => /^"[^"]*"$/.test(a)).map((a) => a.slice(1, -1));
check("every event Nona sends is a literal of her own the worker accepts",
  nonaNamed.length > 0 && nonaNamed.length === nonaCalls.length
    && nonaNamed.every((e) => e.startsWith("nona.") && USAGE_EVENTS.includes(e)),
  nonaCalls.join(", "));
const unsent = USAGE_EVENTS.filter((e) => !named.includes(e) && !nonaNamed.includes(e));
check("every event the worker accepts is sent by the page", !unsent.length, unsent.join(", "));

const v = src.match(/\bv: (\d+), e: event\b/);
check("the page writes the worker's schema", v && Number(v[1]) === USAGE_SCHEMA,
  v ? `page ${v[1]}, worker ${USAGE_SCHEMA}` : "no `v:` in track()");
// A ROUTE WITHOUT A VIEW KIND sends an empty subject, which the worker accepts.
const kinds = (name) => {
  const m = src.match(new RegExp(`const ${name} = \\{([^}]*)\\}`));
  return m ? [...m[1].matchAll(/:\s*"([a-z_]+)"/g)].map((x) => x[1]) : [];
};
const viewKeys = (() => {
  const m = src.match(/const AUTH_VIEWS = \{([^}]*)\}/);
  return m ? [...m[1].matchAll(/\b([a-z_]+):/g)].map((x) => x[1]) : [];
})();
const authKinds = kinds("AUTH_PATHS");
check("every account route has its own view kind", authKinds.length > 0
  && authKinds.every((k) => viewKeys.includes(k)) && new Set(kinds("AUTH_VIEWS")).size === viewKeys.length,
  `routes ${authKinds.join(",")}; views ${viewKeys.join(",")}`);
const hosts = src.match(/const LIVE_HOSTS = (\[[^\]]*\])/);
const live = hosts ? JSON.parse(hosts[1]) : [];
check("no dev or check host is live", live.length > 0
  && !live.some((h) => /^(127\.|localhost$|0\.0\.0\.0$)/.test(h)), hosts ? hosts[1] : "no LIVE_HOSTS");

const written = [];
const CTX = { waitUntil: () => {} };
const env = { USAGE: { writeDataPoint: (p) => written.push(p) } };
const cid = "0123456789abcdef0123456789abcdef";
const good = { v: USAGE_SCHEMA, e: "simulator.run", cid, subject: "torid", route: "weapons",
  lang: "zh", shell: "web", release: "r20260927.1", n: 1000 };
const post = (b, e = env, method = "POST") => worker.fetch(Object.assign(new Request("https://wfsim.app/api/e", {
  method, ...(method === "POST" ? { body: typeof b === "string" ? b : JSON.stringify(b) } : {}),
}), { cf: { country: "CN" } }), e, CTX);

let r = await post(good);
check("a point is accepted", r.status === 204, `status ${r.status}`);
const p = written[0];
const hashed = await usageVisitor(cid);
check("…and written once, indexed by the visitor's hash", written.length === 1 && p.indexes[0] === hashed
  && /^[0-9a-f]{32}$/.test(hashed) && hashed !== cid);
check("…carrying the event, the hash, subject, route, language, shell, release and country",
  p && JSON.stringify(p.blobs) === JSON.stringify(["simulator.run", hashed, "torid", "weapons", "zh", "web", "r20260927.1", "CN"]),
  p && JSON.stringify(p.blobs));
check("…and the dataset's schema and the count", p && p.doubles[0] === USAGE_POINT_SCHEMA && p.doubles[1] === 1000);
check("…and never the browser's id", !JSON.stringify(p).includes(cid));
check("…answering any origin, so the desktop shell can post",
  r.headers.get("access-control-allow-origin") === "*");

const refused = [
  ["an event nobody declared", { ...good, e: "builder.click" }],
  ["a visitor id that is not one", { ...good, cid: "me@example.com" }],
  ["a subject with a path in it", { ...good, subject: "../x" }],
  ["another schema", { ...good, v: USAGE_SCHEMA + 1 }],
  ["a shell that does not exist", { ...good, shell: "bot" }],
  ["a negative count", { ...good, n: -1 }],
  ["not json", "{"],
];
for (const [what, b] of refused) {
  const before = written.length;
  r = await post(b);
  check(`refused and not written: ${what}`, r.status === 400 && written.length === before, `status ${r.status}`);
}
check("the page's own route fallback is a route the worker accepts",
  ["home", "other"].every((route) => usagePoint({ ...good, route }, "")));
for (const ua of [
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_5) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/13.1.1 Safari/605.1.15 (Applebot/0.1; +http://www.apple.com/go/applebot)",
  "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; Googlebot/2.1; +http://www.google.com/bot.html) Chrome/145.0.0.0 Safari/537.36",
  "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/145.0.0.0 Safari/537.36",
]) {
  const before = written.length;
  r = await worker.fetch(new Request("https://wfsim.app/api/e", { method: "POST", body: JSON.stringify(good),
    headers: { "user-agent": ua } }), env, CTX);
  check(`a crawler that runs the page is answered and not counted: ${ua.match(/\w+(bot|Chrome)\b/i)[0]}`,
    r.status === 204 && written.length === before);
}
{
  const before = written.length;
  r = await worker.fetch(new Request("https://wfsim.app/api/e", { method: "POST", body: JSON.stringify(good),
    headers: { "user-agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36 Edg/145.0.0.0" } }), env, CTX);
  check("…and a reader's browser is", r.status === 204 && written.length === before + 1);
}
r = await post(good, {});
check("no dataset bound is a 503, not a silent 204", r.status === 503, `status ${r.status}`);
r = await post(null, env, "OPTIONS");
check("a preflight is answered", r.status === 204 && r.headers.get("access-control-allow-methods"));

// THE KEPT DAYS (worker/usage_days.js): one definition of a result, totals
// right, no visitor id written, and a day half written is written again.
const py = readFileSync(new URL("./usage.py", import.meta.url), "utf8").match(/^RESULTS = \(([^)]*)\)/m);
const pyResults = py ? [...py[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]) : [];
check("the kept days and usage.py count a result the same way",
  JSON.stringify(pyResults) === JSON.stringify(USAGE_RESULTS), `${pyResults} vs ${USAGE_RESULTS}`);
const A = "a".repeat(32), B = "b".repeat(32), C = "c".repeat(32);
const pt = (day, cid, e, subject = "", country = "CN") =>
  ({ day: `${day} 00:00:00`, cid, e, subject, country, si: 1, n: 1 });
const fixture = [
  pt("2026-10-01", A, "app.boot"),
  pt("2026-10-02", A, "app.boot"), pt("2026-10-02", A, "simulator.run", "torid"),
  pt("2026-10-02", A, "optimizer.run", "torid"),
  pt("2026-10-02", B, "app.boot", "", "US"), pt("2026-10-02", B, "simulator.run", "furis", "US"),
  pt("2026-10-02", C, "app.view", "home"),
];
const keptDay = usageDayRows("2026-10-02", fixture);
const cell = (event, subject, market) => (keptDay.find((o) => o.event === event && o.subject === subject
  && (!market || o.market === market)) || {}).visitors || 0;
check("a day counts its visitors, those with a result and those returning",
  cell("visitors", "all", "china") === 2 && cell("visitors", "all", "overseas") === 1
  && cell("visitors", "result", "china") === 1 && cell("visitors", "returning", "china") === 1
  && cell("simulator.run", "torid") === 1 && cell("visitors.country", "US") === 1, JSON.stringify(keptDay));
check("a weapon is tested once a day by a reader who simulated AND searched on it",
  cell("tested", "torid", "china") === 1 && cell("tested", "furis", "overseas") === 1, JSON.stringify(keptDay.filter((o) => o.event === "tested")));
check("…and no row carries a visitor id", !JSON.stringify(keptDay).includes(A));
const tail = keptDay[keptDay.length - 1];
check("…and its kept marker is the last row written", tail.event === "visitors" && tail.subject === "all");
{
  const LIBRARY = await testDb("library");
  // THE DATASET, STUBBED: it holds three days, and a day's query returns every
  // point before the end of the day it asks for.
  const realFetch = globalThis.fetch;
  globalThis.fetch = async (_url, init) => {
    const end = (init.body.match(/timestamp < toDateTime\('(\d{4}-\d\d-\d\d)/) || [])[1];
    const data = end ? fixture.filter((r) => r.day.slice(0, 10) < end)
      : ["2026-10-01", "2026-10-02", "2026-10-03"].map((d) => ({ day: `${d} 00:00:00` }));
    return new Response(JSON.stringify({ data }));
  };
  const env = { LIBRARY, USAGE_ACCOUNT: "x", USAGE_READ_TOKEN: "y" };
  const now = Date.parse("2026-10-03T00:30:00Z");
  const first = await rollupUsage(env, now);
  check("a run keeps every finished day it lacks, and not today",
    JSON.stringify(first) === '["2026-10-01","2026-10-02"]', JSON.stringify(first));
  check("…and a second run keeps nothing again", (await rollupUsage(env, now)).length === 0);
  await LIBRARY.exec("DELETE FROM usage_days WHERE day = '2026-10-02' AND event = 'visitors' AND subject = 'all'");
  const redo = await rollupUsage(env, now);
  check("…but a day without its marker is written again", JSON.stringify(redo) === '["2026-10-02"]',
    JSON.stringify(redo));
  check("no secrets is no run", (await rollupUsage({ LIBRARY }, now)).length === 0);
  globalThis.fetch = realFetch;
}

// THE LOG (worker/usage_log.js): every point, under the hash; the browser's own
// erasure; the days past keeping dropped; and a day after USAGE_LOG_FROM kept
// from the log, unsampled.
{
  const LIBRARY = await testDb("library");
  const logged = { ...env, LIBRARY };
  r = await post(good, logged);
  r = await post({ ...good, e: "app.boot", subject: "direct" }, logged);
  const rows = await LIBRARY.all("SELECT * FROM usage_points");
  check("every point lands in the log", r.status === 204 && rows.length === 2, JSON.stringify(rows));
  check("…under the visitor's hash, never the browser's id",
    rows.every((x) => x.visitor === hashed) && !JSON.stringify(rows).includes(cid));
  check("…with its day, event, subject, country and count",
    rows.some((x) => x.event === "simulator.run" && x.subject === "torid" && x.country === "CN" && x.n === 1000
      && x.day === x.at.slice(0, 10)));
  const other = "f".repeat(32);
  await post({ ...good, cid: other }, logged);
  const forget = (b, e = logged) => worker.fetch(new Request("https://wfsim.app/api/e/forget",
    { method: "POST", body: JSON.stringify(b) }), e, CTX);
  r = await forget({ cid: "me@example.com" });
  check("a forget that names no id is refused", r.status === 400);
  r = await forget({ cid });
  const j = await r.json();
  const left = await LIBRARY.all("SELECT visitor FROM usage_points");
  check("a browser's forget deletes its rows and says how many", r.status === 200 && j.deleted === 2, JSON.stringify(j));
  check("…and nobody else's", left.length === 1 && left[0].visitor === await usageVisitor(other));
  check("…answering any origin, so the desktop shell can ask", r.headers.get("access-control-allow-origin") === "*");
  r = await forget({ cid }, env);
  check("no database bound is a 503", r.status === 503);

  // A DAY FROM THE LOG: the fixture's shape, two weeks on, written as points.
  const shift = (d) => new Date(Date.parse(d) + 22 * 86400000).toISOString().slice(0, 10);
  check("the fixture's days move to the log's", shift("2026-10-01") === USAGE_LOG_FROM);
  await LIBRARY.exec("DELETE FROM usage_points");
  for (const x of fixture) {
    const day = shift(x.day.slice(0, 10));
    await LIBRARY.run(`INSERT INTO usage_points (at, day, visitor, event, subject, route, lang, shell, release, country, n)
      VALUES (?, ?, ?, ?, ?, 'home', 'zh', 'web', 'r1', ?, 0)`, `${day}T12:00:00.000Z`, day, x.cid, x.e, x.subject, x.country);
  }
  const kept = await rollupUsage({ LIBRARY }, Date.parse(`${shift("2026-10-03")}T00:30:00Z`));
  const day2 = shift("2026-10-02");
  const cellOf = async (event, subject, market) => (await LIBRARY.get(
    "SELECT visitors, sampled FROM usage_days WHERE day = ? AND event = ? AND subject = ? AND market = ?",
    day2, event, subject, market)) || {};
  check("a day after the log began is kept from the log",
    JSON.stringify(kept) === JSON.stringify([shift("2026-10-01"), day2]), JSON.stringify(kept));
  check("…counting what the dataset's day counted",
    (await cellOf("visitors", "all", "china")).visitors === 2 && (await cellOf("visitors", "returning", "china")).visitors === 1
    && (await cellOf("tested", "torid", "china")).visitors === 1);
  check("…and marked unsampled", (await cellOf("visitors", "all", "china")).sampled === 0);

  // THE LOG KEEPS ITS DAYS AND NO MORE.
  const now = Date.parse(`${shift("2026-10-03")}T00:30:00Z`) + (USAGE_LOG_DAYS + 1) * 86400000;
  await rollupUsage({ LIBRARY }, now);
  check("a point past the log's days is dropped", (await LIBRARY.all("SELECT 1 FROM usage_points")).length === 0);
}

if (failures) {
  console.log(`\n${failures} check(s) failed`);
  process.exit(1);
}
console.log("\nusage points: page and worker agree");
