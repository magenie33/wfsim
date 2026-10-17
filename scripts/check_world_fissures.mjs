// THE FISSURE LOG A REMINDER IS BUILT FROM (worker/world.js), with no network:
// a relayed fissure carries its node's planet, is logged once, and
// `/api/world/names` hands back each combination seen with how many opened in
// the window and when the last did, and the nodes and planets they name.
//   node scripts/check_world_fissures.mjs
import { worldRoute } from "../worker/world.js";
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
const call = (path, init) => worldRoute(new Request(`https://x${path}`, init), env, ctx, path).then((r) => r.json());

const now = Date.now();
const date = (ms) => ({ $date: { $numberLong: String(ms) } });
const fissure = (id, node, tier, mission, startMs) => ({ _id: { $oid: id }, Node: node, Modifier: tier, MissionType: mission,
  Activation: date(startMs), Expiry: date(now + 3600_000) });
// An old fissure on the same node, logged straight to the table: outside the window.
await LIBRARY.prepare("INSERT INTO fissures (id, list, tier, mission, node, started_at, ends_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")
  .bind("old", "normal", "VoidT3", "MT_CAPTURE", "SolNode1", new Date(now - 9 * 86400_000).toISOString(),
    new Date(now - 9 * 86400_000 + 3600_000).toISOString()).run();
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
const logged = await LIBRARY.prepare("SELECT count(*) AS n FROM fissures").first();
check("a fissure relayed twice is logged once", Number(logged.n) === 3, logged.n);

const names = await call("/api/world/names");
const row = (node) => (names.fissures || []).find((c) => c[3] === node);
const neptune = row("SolNode1");
check("a combination is [list, tier, mission, node, recent, last_ms]", neptune && neptune.slice(0, 4).join() === "normal,VoidT3,MT_CAPTURE,SolNode1",
  JSON.stringify(neptune));
check("…counting only what opened in the window", neptune && neptune[4] === 1, JSON.stringify(neptune));
check("…and when the last opened", neptune && neptune[5] === now - 60_000, JSON.stringify(neptune));
check("a Void Storm's combination has no mission", row("CrewBattleNode519") && row("CrewBattleNode519")[2] === null);
check("each node seen is named with its planet", names.nodes && names.nodes.SolNode1
  && names.nodes.SolNode1.planet === "/Lotus/Language/Locations/Neptune" && Object.keys(names.nodes).length === 2,
  JSON.stringify(names.nodes));
check("every planet a node names is in the planets table", Object.values(names.nodes || {}).every((n) => n.planet in (names.planets || {})));
check("the window is said", names.window_ms === 7 * 86400_000, names.window_ms);

console.log(failures ? `\n${failures} FAILED` : "\nall ok");
process.exit(failures ? 1 : 0);
