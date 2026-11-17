// THE FISSURE LOG A REMINDER IS BUILT FROM (worker/world.js), with no network:
// a relay names and logs each fissure once; reads and daily cleanup share one
// rolling UTC calendar year, including leap-day and expiry boundaries.
//   node scripts/check_world_fissures.mjs
import * as world from "../worker/world.js";
import worker from "../worker/index.js";
import { testDb } from "./pg/test_db.mjs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const LIBRARY = await testDb("library");
const r2 = new Map();
const UPLOADS = { put: async (k, v) => { r2.set(k, v); }, get: async (k) => r2.has(k) ? { json: async () => JSON.parse(r2.get(k)) } : null };
const env = { BOT_RELAY_TOKEN: "relay", LIBRARY, UPLOADS };
globalThis.caches = { default: { match: async () => null, put: async () => {} } };
const ctx = { waitUntil() {} };
const call = (path, init) => world.worldRoute(new Request(`https://x${path}`, init), env, ctx, path).then((r) => r.json());

const originalNow = Date.now;
const now = Date.parse("2025-03-01T12:34:56.789Z");
const since = Date.parse("2024-03-01T12:34:56.789Z");
Date.now = () => now;
const date = (ms) => ({ $date: { $numberLong: String(ms) } });
const fissure = (id, node, tier, mission, startMs) => ({ _id: { $oid: id }, Node: node, Modifier: tier, MissionType: mission,
  Activation: date(startMs), Expiry: date(now + 3600_000) });
for (const [id, node, start] of [["old", "SolNode1", now - 400 * 86400_000], ["middle", "SolNode1", now - 180 * 86400_000],
  ["boundary", "SolNode1", since], ["edge-only", "SolNode45", since], ["retired", "SolNode107", since - 1]]) {
  await LIBRARY.prepare("INSERT INTO record_world_fissures (id, list, tier, mission, node, started_at, ends_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")
    .bind(id, "normal", "VoidT3", "MT_CAPTURE", node, new Date(start).toISOString(),
      new Date(start + 3600_000).toISOString()).run();
}
const before = await call("/api/world/names");
check("reads exclude expired combinations before daily cleanup", !before.fissures.some((c) => c[3] === "SolNode107"));
check("…and count retained records from months ago, including the exact cutoff", before.fissures.find((c) => c[3] === "SolNode1")?.[4] === 2,
  JSON.stringify(before.fissures));
const ws = { ActiveMissions: [fissure("a", "SolNode1", "VoidT3", "MT_CAPTURE", now - 60_000)],
  VoidStorms: [{ _id: { $oid: "b" }, Node: "CrewBattleNode519", ActiveMissionTier: "VoidT1", Activation: date(now - 60_000), Expiry: date(now + 3600_000) }] };
const put = { method: "PUT", headers: { authorization: "Bearer relay" }, body: JSON.stringify(ws) };
await call("/api/world", put);
await call("/api/world", put);

const items = JSON.parse(r2.get("world/items.json")).items;
const capture = items.find((x) => x.id === "a"), storm = items.find((x) => x.id === "b");
check("a fissure carries its node's planet", capture && capture.attributes.planet === "/Lotus/Language/Locations/Neptune",
  JSON.stringify(capture && capture.attributes));
check("…and a Void Storm its Proxima's, not the planet's", storm && storm.attributes.planet === "/Lotus/Language/Locations/Earth_SPACE",
  JSON.stringify(storm && storm.attributes));
const logged = await LIBRARY.prepare("SELECT count(*) AS n FROM record_world_fissures").first();
check("a fissure relayed twice is logged once", Number(logged.n) === 7, logged.n);

const names = await call("/api/world/names");
const row = (node) => (names.fissures || []).find((c) => c[3] === node);
const neptune = row("SolNode1");
check("a combination is [list, tier, mission, node, recent, last_ms]", neptune && neptune.slice(0, 4).join() === "normal,VoidT3,MT_CAPTURE,SolNode1",
  JSON.stringify(neptune));
check("…counting only the retained calendar year's occurrences", neptune && neptune[4] === 3, JSON.stringify(neptune));
check("…and when the last opened", neptune && neptune[5] === now - 60_000, JSON.stringify(neptune));
check("a Void Storm's combination has no mission", row("CrewBattleNode519") && row("CrewBattleNode519")[2] === null);
check("only retained nodes are named with their planets", names.nodes && names.nodes.SolNode1
  && names.nodes.SolNode1.planet === "/Lotus/Language/Locations/Neptune" && Object.keys(names.nodes).length === 3,
  JSON.stringify(names.nodes));
check("every planet a node names is in the planets table", Object.values(names.nodes || {}).every((n) => n.planet in (names.planets || {})));
const purge = world.purgeFissures;
check("daily maintenance can expire fissure records", typeof purge === "function");
const scheduled = [];
await worker.scheduled({ cron: "0 3 * * *" }, env, { waitUntil: (work) => scheduled.push(work) });
await Promise.all(scheduled);
const retained = await LIBRARY.prepare("SELECT count(*) AS n FROM record_world_fissures").first();
check("daily maintenance deletes expired records while retaining the exact cutoff", Number(retained.n) === 5, retained.n);
check("the cutoff record itself remains", !!await LIBRARY.prepare("SELECT id FROM record_world_fissures WHERE id = ?1").bind("boundary").first());
Date.now = () => now + 86400_000;
const later = await call("/api/world/names");
check("a combination retires as soon as its last occurrence leaves the window", !later.fissures.some((c) => c[3] === "SolNode45"));
if (purge) await purge(env);
const advanced = await LIBRARY.prepare("SELECT count(*) AS n FROM record_world_fissures").first();
check("cleanup follows the moving cutoff", Number(advanced.n) === 3, advanced.n);

for (const [at, days] of [["2024-02-28T12:34:56.789Z", 365], ["2024-02-29T12:34:56.789Z", 366],
  ["2024-03-01T12:34:56.789Z", 366], ["2025-02-28T12:34:56.789Z", 366], ["2025-03-01T12:34:56.789Z", 365],
  ["2000-03-01T12:34:56.789Z", 366], ["2100-03-01T12:34:56.789Z", 365]]) {
  Date.now = () => Date.parse(at);
  const window = await call("/api/world/names");
  check(`UTC calendar year ending ${at} spans ${days} days`, window.window_ms === days * 86400_000, window.window_ms);
}
Date.now = originalNow;
await LIBRARY.close();

console.log(failures ? `\n${failures} FAILED` : "\nall ok");
process.exit(failures ? 1 : 0);
