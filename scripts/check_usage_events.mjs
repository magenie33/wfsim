// USAGE POINTS — the page's half against the worker's, and the endpoint against
// a stub dataset. What must hold (docs/ANALYTICS.md):
//   - every event the page sends is one the worker accepts, and every one the
//     worker accepts is sent from somewhere: a name on one side only is a time
//     series that silently never starts;
//   - the page writes the schema the worker reads;
//   - a point carries the visitor as its index and nothing that identifies them;
//   - anything that is not a point is refused and never written;
//   - no dev server or check host is live, so no test writes the live dataset.
//
//   node scripts/check_usage_events.mjs
import worker, { USAGE_EVENTS, USAGE_SCHEMA, usagePoint } from "../worker/index.js";
import { appSource } from "./app_source.mjs";
import { USAGE_RESULTS, usageDayRows, rollupUsage } from "../worker/usage_days.js";
import { DatabaseSync } from "node:sqlite";
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
check("…and written once, indexed by the visitor", written.length === 1 && p.indexes[0] === cid);
check("…carrying the event, subject, route, language, shell, release and country",
  p && JSON.stringify(p.blobs) === JSON.stringify(["simulator.run", cid, "torid", "weapons", "zh", "web", "r20260927.1", "CN"]),
  p && JSON.stringify(p.blobs));
check("…and the schema and the count", p && p.doubles[0] === USAGE_SCHEMA && p.doubles[1] === 1000);
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
  const db = new DatabaseSync(":memory:");
  db.exec(readFileSync(new URL("../worker/schema.sql", import.meta.url), "utf8"));
  const stmt = (sql, args = []) => ({
    bind: (...a) => stmt(sql, a),
    all: async () => ({ results: db.prepare(sql).all(...args) }),
    run: async () => db.prepare(sql).run(...args),
  });
  const LIBRARY = { prepare: (sql) => stmt(sql), batch: async (ss) => { for (const x of ss) await x.run(); } };
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
  db.exec("DELETE FROM usage_days WHERE day = '2026-10-02' AND event = 'visitors' AND subject = 'all'");
  const redo = await rollupUsage(env, now);
  check("…but a day without its marker is written again", JSON.stringify(redo) === '["2026-10-02"]',
    JSON.stringify(redo));
  check("no secrets is no run", (await rollupUsage({ LIBRARY }, now)).length === 0);
  globalThis.fetch = realFetch;
}

if (failures) {
  console.log(`\n${failures} check(s) failed`);
  process.exit(1);
}
console.log("\nusage points: page and worker agree");
