// SPDX-License-Identifier: AGPL-3.0-or-later
// COMPUTERS ON BUT IDLE (worker/idle_devices.js), found and said by the watchdog.
// Plain node on PGlite: ids that asked in the last half hour are sorted into an
// old engine, an asking loop and not computing; one that holds a row, answered,
// only just opened or went away is none of them; the watchdog says each kind in
// Chinese once, and again only when a device it has not said joins it.
import { testDb } from "./pg/test_db.mjs";
import { idleFindings, loopAsksPerHour } from "../worker/idle_devices.js";
import { watchdog } from "../worker/factory.js";
import { watchdogText } from "../worker/watchdog_say.js";
import { DIALS } from "../worker/dials.js";
import { iso } from "../worker/instant.js";

let failures = 0;
const check = (name, ok, detail) => {
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || detail === undefined ? "" : `  — ${detail}`}`);
  if (!ok) failures++;
};

const db = await testDb("library"), sent = [];
const env = { LIBRARY: db,
  ASSETS: { fetch: async () => new Response(JSON.stringify({ engine: "e1", release: "r1", accepted: [] })) },
  CLOUD: { fetch: async (r) => { sent.push((await r.json()).text); return new Response('{"ok":true}'); } } };
const NOW = Date.now(), hour = iso(NOW).slice(0, 13);
const id = (c) => c.repeat(24);
const on = async (c, { engine = "e1", ago = 60_000, asks = 3 } = {}) => {
  await db.run("INSERT INTO verifiers (id, seen, asked_at, asked_engine, asked_lanes) VALUES (?, ?, ?, ?, 8)", id(c), hour.slice(0, 10), iso(NOW - ago), engine);
  if (asks) await db.run("INSERT INTO verifier_asks (verifier, hour, asks) VALUES (?, ?, ?)", id(c), hour, asks);
};
await on("a", { engine: "e0/p7", asks: 0 });
await on("b", { asks: loopAsksPerHour({ sync_floor_ms: DIALS.sync_floor_ms.default }) + 1 });
await on("c");
await on("d");
await db.run(`INSERT INTO answers (question, engine, device, owner, canon, result, work, at) VALUES ('q1', 'e1', ?, ?, 'x', '{}', 1, ?)`, id("d"), id("d"), iso(NOW - 60_000));
await on("e", { asks: 1 });
await on("f", { ago: 2 * 3_600_000 });

console.log("who is idle");
const found = await idleFindings(env, "e1", NOW);
const of = (kind) => (found.find((f) => f.kind === kind) || { devices: [] }).devices.map((x) => x.device);
check("an id asking with another engine is on an old engine", JSON.stringify(of("idle:old_engine")) === JSON.stringify([id("a")]), JSON.stringify(found));
check("one asking more than twice what a page makes with nothing is a loop", JSON.stringify(of("idle:asking_loop")) === JSON.stringify([id("b")]));
check("one on the served engine, asking, holding and answering nothing is not computing", JSON.stringify(of("idle:not_computing")) === JSON.stringify([id("c")]));
check("one that answered, only just asked once, or went away is none of them",
  ![id("d"), id("e"), id("f")].some((x) => found.some((f) => f.devices.some((y) => y.device === x))));

console.log("\nwhat the watchdog says");
await watchdog(env, NOW);
const saidIdle = async () => (await db.all("SELECT said FROM watchdog_said ORDER BY at, cause")).map((x) => x.said).filter((t) => /idle:/.test(t));
let idleSaid = await saidIdle();
check("each kind is written down once, in Chinese", idleSaid.length === 3 && idleSaid.some((t) => t.includes("停在旧引擎"))
  && idleSaid.some((t) => t.includes("频繁请求")) && idleSaid.some((t) => t.includes("在线但没在算")), JSON.stringify(idleSaid));
check("…and sent to nobody", sent.length === 0, JSON.stringify(sent));
await watchdog(env, NOW + 60_000);
idleSaid = await saidIdle();
check("…and not again while only the same devices are idle", idleSaid.length === 3, String(idleSaid.length));
await on("g");
await watchdog(env, NOW + 120_000);
const again = (await saidIdle()).slice(3);
check("…but again when a device it has not said joins a kind, listing both", again.length === 1 && again[0].includes(id("c").slice(0, 8))
  && again[0].includes(id("g").slice(0, 8)) && again[0].includes("新出现 1 台"), JSON.stringify(again));
check("every idle kind has its own words, none the fallback", ["old_engine", "asking_loop", "not_computing"].every((why) =>
  !watchdogText({ kind: `idle:${why}`, devices: [{ device: id("z"), engine: "e1", protocol: 8, lanes: 4, asks: 3, last_answer: null, label: null, why }],
    online: 10, served: "e1", loop_per_hour: 120 }, NOW).includes("没有这一类的说明")));

console.log(failures ? `\n${failures} failed` : "\nan idle computer is found by what the server keeps, and said once");
process.exit(failures ? 1 : 0);
