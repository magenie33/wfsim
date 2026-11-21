// SPDX-License-Identifier: AGPL-3.0-or-later
// THE UTILITY PAGES' WORLD, from DE's world state — docs/UI.md §"Utility".
// DE's host refuses a request from a Cloudflare Worker (403 whatever it sends),
// and a page cannot read it across origins, so the bot server relays it: each
// minute it PUTs DE's file here, this turns each thing a page lists into one
// ITEM, names it, and keeps the result in R2 for anyone to read. Arbitrations
// are not in DE's file: they come from a schedule (`arbitrationsOf`).
//
//   GET /api/world                                   the items, as last relayed, and the arbitration now
//   GET /api/world/names                             every era and mission type, named, and the fissures seen
//   GET /api/world/arbitrations                      the arbitrations of the coming days
//   POST /api/world/reminder-history                 past openings matching reminder rules
//   PUT /api/world   (bearer BOT_RELAY_TOKEN)         DE's worldState.php, as read
import NAMES from "./world_names.json" with { type: "json" };

const KEY = "world/items.json";
/// The relay runs each minute; a reader is never more than this behind it.
const FRESH_S = 30;
const MAX_BYTES = 4 * 1024 * 1024;
const OPEN = { "access-control-allow-origin": "*" };
const HISTORY_CORS = { "access-control-allow-methods": "POST, OPTIONS", "access-control-allow-headers": "content-type" };

const json = (body, status = 200, extra = {}) => new Response(JSON.stringify(body), {
  status, headers: { "content-type": "application/json; charset=utf-8", ...OPEN, ...extra } });
const msOf = (d) => Number(d && d.$date && d.$date.$numberLong) || 0;

/// The same UTC date and time one year earlier; February 29 clamps to February 28.
function fissureWindow(now) {
  const start = new Date(now), month = start.getUTCMonth();
  start.setUTCFullYear(start.getUTCFullYear() - 1);
  if (start.getUTCMonth() !== month) start.setUTCDate(0);
  return { since: start.toISOString(), window_ms: now - start.getTime() };
}

export async function purgeFissures(env) {
  if (!env.LIBRARY) return;
  const { since } = fissureWindow(Date.now());
  await env.LIBRARY.prepare("DELETE FROM record_world_fissures WHERE started_at < ?1").bind(since).run();
}

export async function worldRoute(request, env, ctx, path) {
  // WHAT A REMINDER CAN NAME BEFORE IT IS OPEN: the table the items are named
  // from, every combination of list, era, mission and node in the retained
  // calendar year, its count and first and last openings, and the nodes and planets it names.
  if (path === "/api/world/names") {
    const now = Date.now();
    const { since, window_ms } = fissureWindow(now);
    const seen = env.LIBRARY && await env.LIBRARY.prepare(`SELECT list, tier, mission, node,
        count(*) AS recent, max(started_at) AS last, min(started_at) AS first
        FROM record_world_fissures WHERE started_at >= ?1 AND started_at <= ?2 GROUP BY list, tier, mission, node`)
      .bind(since, new Date(now).toISOString()).all()
      .then((r) => r.results.map((x) => [x.list, x.tier, x.mission, x.node, Number(x.recent), Date.parse(x.last), Date.parse(x.first)]))
      .catch(() => null);
    const nodes = {};
    for (const [, , , node] of seen || []) {
      const n = NAMES.nodes[node];
      if (n) nodes[node] = { name: n.name, planet: n.planet };
    }
    return json({ ok: true, tiers: NAMES.tiers, missions: NAMES.missions, planets: NAMES.planets, nodes,
      fissures: seen || [], window_ms }, 200, { "cache-control": "public, max-age=600" });
  }
  if (path === "/api/world/arbitrations") return arbitrationRoute(request, ctx, path);
  if (path === "/api/world/reminder-history") return reminderHistoryRoute(request, env);
  if (path !== "/api/world") return json({ ok: false, error: "not found" }, 404);
  if (request.method === "PUT") return worldRelay(request, env);
  if (request.method !== "GET") return json({ ok: false, error: "GET or PUT" }, 405);
  const cache = caches.default;
  const key = new Request(new URL(path, request.url).toString());
  const hit = await cache.match(key);
  if (hit) return hit;
  const obj = env.UPLOADS && await env.UPLOADS.get(KEY).catch(() => null);
  if (!obj) return json({ ok: false, error: "the world state has not been relayed yet" }, 503);
  const relayed = await obj.json();
  // AN UNREACHABLE SCHEDULE COSTS THE ARBITRATION, never the fissures.
  const now = Date.now();
  const arbitration = await arbitrationSchedule().then((s) => arbitrationsOf(s, now, now + 1)).catch(() => []);
  const res = json({ ...relayed, items: [...relayed.items, ...arbitration] }, 200,
    { "cache-control": `public, max-age=${FRESH_S}` });
  ctx.waitUntil(cache.put(key, res.clone()));
  return res;
}

async function worldRelay(request, env) {
  const auth = request.headers.get("authorization") || "";
  if (!env.BOT_RELAY_TOKEN || auth !== `Bearer ${env.BOT_RELAY_TOKEN}`) return json({ ok: false, error: "unauthorized" }, 401);
  const body = await request.text();
  if (body.length > MAX_BYTES) return json({ ok: false, error: "too large" }, 413);
  let ws;
  try { ws = JSON.parse(body); } catch (_) { return json({ ok: false, error: "not json" }, 400); }
  if (!ws || !Array.isArray(ws.ActiveMissions)) return json({ ok: false, error: "not a world state" }, 400);
  const now = Date.now();
  const items = worldItems(ws, now);
  await env.UPLOADS.put(KEY, JSON.stringify({ ok: true, read_at_ms: now, items }),
    { httpMetadata: { contentType: "application/json" } });
  // A LOG THAT FAILS COSTS THE LOG, never the relay.
  await fissureLog(env, items).catch((e) => console.error(`fissure log: ${e && e.message || e}`));
  return json({ ok: true, items: items.length });
}

/// EVERY FISSURE THE GAME OPENS, one row per DE id, written the first minute it
/// is seen (`fissures`, worker/pg/library.sql). It is the only source of which
/// combinations of list, era and mission exist — no export or wiki table says
/// which era opens which mission type — and of how often each comes.
async function fissureLog(env, items) {
  if (!env.LIBRARY) return;
  const insert = env.LIBRARY.prepare("INSERT INTO record_world_fissures (id, list, tier, mission, node, started_at, ends_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT DO NOTHING");
  const rows = items.filter((x) => x.kind === "fissure" && x.id).map((x) => insert.bind(x.id,
    x.attributes.list, x.attributes.tier, x.attributes.mission, x.attributes.node,
    new Date(x.started_at_ms).toISOString(), new Date(x.ends_at_ms).toISOString()));
  if (rows.length) await env.LIBRARY.batch(rows);
}

/// ONE SHAPE FOR EVERYTHING A UTILITY PAGE LISTS: `{kind, id, attributes,
/// names, started_at_ms, ends_at_ms}`. A reminder matches on `attributes` and
/// nothing else, so a new kind is a parser here and a page, and reminders reach
/// it unchanged. `names` holds `{en, zh}` for each attribute it can name.
const KINDS = { fissure: fissuresOf };

export const worldItems = (ws, now) => Object.entries(KINDS)
  .flatMap(([kind, of]) => of(ws).map((x) => ({ kind, ...x })))
  .filter((x) => x.ends_at_ms > now);

/// EVERY OPEN FISSURE. `list` is which of the game's three it is in: a Steel
/// Path fissure is an ordinary one marked `Hard`, and a Void Storm is a Railjack
/// node's. An id the table does not know has no name, and the page shows the id.
function fissuresOf(ws) {
  const missions = (ws.ActiveMissions || []).map((m) => ({ id: m._id && m._id.$oid,
    list: m.Hard ? "steel_path" : "normal", tier: m.Modifier, node: m.Node, mission: m.MissionType,
    started_at_ms: msOf(m.Activation), ends_at_ms: msOf(m.Expiry) }));
  const storms = (ws.VoidStorms || []).map((s) => ({ id: s._id && s._id.$oid,
    list: "railjack", tier: s.ActiveMissionTier, node: s.Node, mission: null,
    started_at_ms: msOf(s.Activation), ends_at_ms: msOf(s.Expiry) }));
  return [...missions, ...storms].map(fissureItem);
}

function fissureItem(f) {
  const n = NAMES.nodes[f.node] || {};
  // A storm names its node's mission; the world state gives it no mission type.
  return { id: f.id,
    attributes: { list: f.list, tier: f.tier, mission: f.mission, planet: n.planet || null, node: f.node },
    names: { tier: NAMES.tiers[f.tier] || null,
      mission: (f.mission ? NAMES.missions[f.mission] : n.mission) || null,
      node: n.name || null, system: n.system || null },
    started_at_ms: f.started_at_ms, ends_at_ms: f.ends_at_ms };
}

// ---- ARBITRATIONS -----------------------------------------------------------
//
// ONE NODE AN HOUR, on a rotation the game computes ahead; browse.wf publishes
// it as `<unix seconds>,<node>` lines for years to come (docs/DATA_SOURCES.md
// §"THE WORLD STATE"). An item is one hour of it, and an hour's mission type
// and faction are its node's.

const ARBITRATION_SCHEDULE = "https://browse.wf/arbys.txt";
/// The file changes when the rotation does, which is rarely.
const ARBITRATION_SCHEDULE_S = 6 * 3600;
/// What `/api/world/arbitrations` lists ahead of the hour now.
const ARBITRATION_AHEAD_MS = 14 * 24 * 3600 * 1000;

let schedule = null;
let scheduleAt = 0;
/// THE SCHEDULE, `[[start_ms, node]]` in time order, read once per isolate.
async function arbitrationSchedule() {
  if (schedule && Date.now() - scheduleAt < ARBITRATION_SCHEDULE_S * 1000) return schedule;
  const r = await fetch(ARBITRATION_SCHEDULE, { cf: { cacheTtl: ARBITRATION_SCHEDULE_S, cacheEverything: true } });
  if (!r.ok) throw new Error(`arbitration schedule ${r.status}`);
  schedule = parseArbitrations(await r.text());
  scheduleAt = Date.now();
  return schedule;
}

export const parseArbitrations = (text) => text.split("\n")
  .map((l) => l.trim().split(","))
  .filter(([t, node]) => node && Number(t) > 0)
  .map(([t, node]) => [Number(t) * 1000, node])
  .sort((a, b) => a[0] - b[0]);

/// EVERY ARBITRATION OPEN AT SOME MOMENT OF `[from, to)`. An hour ends where the
/// next begins.
export function arbitrationsOf(sched, from, to) {
  const out = [];
  for (let i = 0; i < sched.length; i++) {
    const [start, node] = sched[i];
    const end = i + 1 < sched.length ? sched[i + 1][0] : start + 3600_000;
    if (end <= from) continue;
    if (start >= to) break;
    out.push(arbitrationItem(node, start, end));
  }
  return out;
}

function arbitrationItem(node, start, end) {
  const n = NAMES.nodes[node] || {};
  return { kind: "arbitration", id: `arbitration-${start}`,
    attributes: { mission: n.type || null, faction: n.faction || null, node },
    names: { mission: (n.type && NAMES.missions[n.type]) || n.mission || null,
      faction: (n.faction && NAMES.factions[n.faction]) || null,
      node: n.name || null, system: n.system || null },
    started_at_ms: start, ends_at_ms: end };
}

const HISTORY_PAGE = 30;
const HISTORY_ATTRIBUTES = { fissure: ["list", "tier", "mission", "planet", "node"], arbitration: ["mission", "faction", "node"] };
const historyTime = (ms) => Number.isSafeInteger(ms) && ms >= 0 && ms <= 8.64e15;
const historyId = (v) => typeof v === "string" && /^[\w./-]{1,256}$/.test(v);

/// One row per mission, even when several reminders match it. Both sources
/// use their own opening clocks and the same descending (start, id) cursor.
async function reminderHistoryRoute(request, env) {
  if (request.method === "OPTIONS") return new Response(null, { status: 204, headers: { ...OPEN, ...HISTORY_CORS } });
  if (request.method !== "POST") return json({ ok: false, error: "POST only" }, 405);
  let input;
  try {
    const body = await request.text();
    if (body.length > MAX_BYTES) return json({ ok: false, error: "too large" }, 413);
    input = JSON.parse(body);
  } catch (_) { return json({ ok: false, error: "not json" }, 400); }
  const { rules, before: cursor } = input || {};
  if (!Array.isArray(rules) || !rules.every((r) => typeof r?.kind === "string" && Object.hasOwn(HISTORY_ATTRIBUTES, r.kind)
    && historyTime(r.made_at_ms) && r.attributes && typeof r.attributes === "object" && !Array.isArray(r.attributes)
    && Object.entries(r.attributes).every(([k, v]) => HISTORY_ATTRIBUTES[r.kind].includes(k) && historyId(v)))
    || (cursor != null && (!historyTime(cursor.started_at_ms) || !historyId(cursor.id)))) {
    return json({ ok: false, error: "invalid reminder history query" }, 400);
  }
  const now = Date.now();
  const since = Date.parse(fissureWindow(now).since);
  const before = cursor?.started_at_ms <= now ? cursor : null;
  const fissures = rules.filter((r) => r.kind === "fissure" && r.made_at_ms <= now);
  const arbitrations = rules.filter((r) => r.kind === "arbitration" && r.made_at_ms <= now);
  try {
    const rows = (await Promise.all([
      fissures.length ? fissureHistory(env.LIBRARY, fissures, since, now, before) : [],
      arbitrations.length ? arbitrationHistory(arbitrations, since, now, before) : [],
    ])).flat();
    rows.sort((a, b) => b.started_at_ms - a.started_at_ms || (a.id < b.id ? 1 : a.id > b.id ? -1 : 0));
    const items = rows.slice(0, HISTORY_PAGE);
    const last = items.at(-1);
    return json({ ok: true, items, before: rows.length > HISTORY_PAGE
      ? { id: last.id, started_at_ms: last.started_at_ms } : null }, 200, { "cache-control": "no-store" });
  } catch (_) { return json({ ok: false, error: "reminder history unavailable" }, 503); }
}

const historyStart = (rules, since) => rules.reduce((start, r) => Math.min(start, Math.max(since, r.made_at_ms)), Infinity);

async function fissureHistory(db, rules, since, now, before) {
  const nodesOf = new Map();
  const filters = rules.map(({ attributes: { planet, ...attributes }, made_at_ms }) => {
    if (planet && !nodesOf.has(planet)) {
      nodesOf.set(planet, Object.keys(NAMES.nodes).filter((id) => NAMES.nodes[id].planet === planet));
    }
    return { ...attributes, nodes: nodesOf.get(planet), made_at: new Date(made_at_ms).toISOString() };
  });
  const lower = new Date(historyStart(rules, since)).toISOString();
  const upper = new Date(before?.started_at_ms ?? now + 1).toISOString();
  const { results } = await db.prepare(`SELECT f.id, f.list, f.tier, f.mission, f.node, f.started_at, f.ends_at
    FROM record_world_fissures f
    WHERE f.started_at >= ?1 AND (f.started_at, f.id) < (?2, ?3)
      AND EXISTS (SELECT 1 FROM jsonb_to_recordset((?4::text)::jsonb)
        AS r(made_at text, list text, tier text, mission text, node text, nodes jsonb)
        WHERE f.started_at >= r.made_at
          AND (r.list IS NULL OR f.list = r.list) AND (r.tier IS NULL OR f.tier = r.tier)
          AND (r.mission IS NULL OR f.mission = r.mission) AND (r.node IS NULL OR f.node = r.node)
          AND (r.nodes IS NULL OR jsonb_exists(r.nodes, f.node)))
    ORDER BY f.started_at DESC, f.id DESC LIMIT ?5`)
    .bind(lower, upper, before?.id ?? "", JSON.stringify(filters), HISTORY_PAGE + 1).all();
  return results.map((f) => ({ kind: "fissure", ...fissureItem({ ...f,
    started_at_ms: Date.parse(f.started_at), ends_at_ms: Date.parse(f.ends_at) }) }));
}

async function arbitrationHistory(rules, since, now, before) {
  const from = historyStart(rules, since);
  const through = before?.started_at_ms ?? now;
  if (from > through) return [];
  const sched = await arbitrationSchedule();
  const matches = [];
  for (let i = sched.findLastIndex(([start]) => start <= through); i >= 0 && matches.length <= HISTORY_PAGE; i--) {
    const [start, node] = sched[i];
    if (start < from) break;
    const item = arbitrationItem(node, start, sched[i + 1]?.[0] ?? start + 3600_000);
    if (before && start === through && item.id >= before.id) continue;
    if (rules.some((r) => start >= r.made_at_ms && Object.entries(r.attributes).every(([k, v]) => item.attributes[k] === v))) {
      matches.push(item);
    }
  }
  return matches;
}

/// THE COMING DAYS, for the arbitration page and a reminder's next time; and
/// every mission type, faction and node the rest of the schedule holds, for a
/// reminder made from nothing — each node with its own mission type and faction,
/// which are what make a combination of the three possible.
async function arbitrationRoute(request, ctx, path) {
  if (request.method !== "GET") return json({ ok: false, error: "GET only" }, 405);
  const cache = caches.default;
  const key = new Request(new URL(path, request.url).toString());
  const hit = await cache.match(key);
  if (hit) return hit;
  let sched;
  try { sched = await arbitrationSchedule(); } catch (e) {
    return json({ ok: false, error: String(e && e.message || e) }, 502);
  }
  const now = Date.now();
  const items = arbitrationsOf(sched, now, now + ARBITRATION_AHEAD_MS);
  const choices = { mission: {}, faction: {}, node: {} };
  for (const x of arbitrationsOf(sched, now, Infinity)) {
    for (const k of ["mission", "faction", "node"]) {
      const v = x.attributes[k];
      if (v && !(v in choices[k])) choices[k][v] = k === "node"
        ? { name: x.names.node, system: x.names.system, mission: x.attributes.mission, faction: x.attributes.faction } : x.names[k];
    }
  }
  const res = json({ ok: true, read_at_ms: now, items, choices }, 200, { "cache-control": "public, max-age=600" });
  ctx.waitUntil(cache.put(key, res.clone()));
  return res;
}
