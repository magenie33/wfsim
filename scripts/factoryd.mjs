// THE SETTLER'S DAEMON — docs/BOARD.md §"The settler". Runs on the database's
// own machine (deploy/wfsim-factoryd.service), so its transactions cost no
// round trip across an ocean, and calls the worker's own code (worker/settler.js)
// on statement handles over the local socket.
//   PG_URL=postgres://wfsim_factoryd@localhost/wfsim PG_SOCKET=/var/run/postgresql node scripts/factoryd.mjs
//   … --once   every tick once, then exit (what a deploy checks)
// PG_ACCOUNTS_URL (read-only) lets the fair table see whose devices are whose;
// without it every device is a walk-in, as the worker's tables without ACCOUNTS.
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import postgres from "./pg/node_modules/postgres/src/index.js";
import { pgDb, postgresClient, POSTGRES_OPTIONS } from "../worker/db.js";
import { snapshotTick, fairTick, readFair, compareFair, settleShadow, settleBatch, findingsTick, signalBody, workWaiting } from "../worker/settler.js";
import { servedRelease, CLIENTS_PER_FACT, PROTOCOL, PROTOCOLS_SERVED } from "../worker/verify.js";
import { dialsOf, forgetDials } from "../worker/dials.js";
import { putObject } from "./sigv4.mjs";
import { iso } from "../worker/instant.js";
import { KEPT_MS } from "../worker/tasks.js";
import { BOARD_KINDS } from "../worker/board_kinds.js";
import { demandOf, DEMAND_KEY, DEMAND_CACHE, DEMAND_COST_EVERY_MS, tallyOf, tallyBase, scorePace, TALLY_KEY, TALLY_CACHE, TALLY_BASE_MS } from "../worker/contribution.js";
import { capacity, backgroundLane, paceCache, claimDaemon } from "./factory_capacity.mjs";

/// HOW OFTEN EACH TICK RUNS, and where the served release is read.
export const SNAPSHOT_MS = 10_000, FAIR_MS = 60_000, DEMAND_MS = 60_000, STATS_TICK_MS = 10_000, SETTLE_MS = 2_000, RELEASE_MS = 60_000, WAVE_MS = 2_000, SIGNAL_CHECK_MS = 60_000;
/// WHAT IS PAST THE WINDOW GOES this many questions a call, a few calls a
/// minute (worker/pg/sync.sql `factory_forget_past`): a day's facts in hours,
/// and no call near the statement timeout.
export const PAST_MS = 60_000, PAST_BATCH = 2_000, PAST_CALLS = 5;
const RELEASE_URL = process.env.RELEASE_URL || "https://wfsim.app/release.json";
/// THE WORK SIGNAL: the object written to the public bucket, and where readers get it.
export const SIGNAL_KEY = "work.json", SIGNAL_CACHE = "public, max-age=5";
const SIGNAL_URL = process.env.SIGNAL_URL || "https://cdn.wfsim.app/work.json";
/// A SIGNAL OLDER THAN THIS AT THE EDGE is stale; this many stale or uncached
/// reads in a row are a finding the watchdog tells the owner.
export const SIGNAL_FRESH_MS = 30_000, SIGNAL_BAD_RUNS = 3;
/// A CACHE STATUS THAT MEANS THE EDGE KEEPS THE FILE; DYNAMIC or BYPASS mean every read reaches R2.
const CACHED = new Set(["HIT", "MISS", "EXPIRED", "REVALIDATED", "UPDATING", "STALE"]);

/// WHERE THE SIGNAL IS WRITTEN, from R2_* in the environment, or null when the
/// token is not configured: the signal is then skipped, and said so once.
export function signalTarget(env = process.env) {
  const t = { endpoint: env.R2_ENDPOINT, bucket: env.R2_BUCKET, accessKey: env.R2_ACCESS_KEY_ID, secretKey: env.R2_SECRET_ACCESS_KEY };
  return Object.values(t).every((v) => typeof v === "string" && v.trim()) ? t : null;
}
/// A STATEMENT THAT HANGS, or a transaction left open, is cut here rather than
/// holding locks the request path waits on.
/// Every network call the daemon makes gives up after this.
const FETCH_MS = 15_000;
const CONNECTION = { statement_timeout: 30_000, idle_in_transaction_session_timeout: 60_000, application_name: "wfsim-factoryd" };

const commitOf = () => {
  if (process.env.DAEMON_COMMIT) return process.env.DAEMON_COMMIT;
  try { return readFileSync(new URL("../COMMIT", import.meta.url), "utf8").trim(); } catch { return "unknown"; }
};
const log = (...a) => console.log(new Date().toISOString(), ...a);

export async function openEnv(env = process.env, connect = postgres) {
  if (!env.PG_URL) throw new Error("PG_URL is required");
  const clients = [];
  const db = (url, background = false) => {
    // PG_SOCKET (a directory) reaches Postgres over its unix socket, as the
    // unit does: peer authentication, no password on the machine.
    const connection = { ...CONNECTION, ...(background ? { transaction_timeout: 120_000 } : {}),
      application_name: background ? "wfsim-factoryd-background" : CONNECTION.application_name };
    const c = postgresClient(connect(url, { ...POSTGRES_OPTIONS, max: background ? 1 : 2, idle_timeout: 0, connection, onnotice: () => {},
      ...(env.PG_SOCKET ? { host: env.PG_SOCKET } : {}) }));
    clients.push(c);
    return pgDb(c);
  };
  const out = { LIBRARY: db(env.PG_URL), ...(env.PG_ACCOUNTS_URL ? { ACCOUNTS: db(env.PG_ACCOUNTS_URL) } : {}) };
  const claim = async (onLost) => {
    let owner;
    const sql = connect(env.PG_URL, { ...POSTGRES_OPTIONS, max: 1, idle_timeout: 0,
      connection: { ...CONNECTION, application_name: "wfsim-factoryd-owner" }, onnotice: () => {},
      onclose: () => owner?.lost(), ...(env.PG_SOCKET ? { host: env.PG_SOCKET } : {}) });
    owner = await claimDaemon(sql, onLost);
    return owner;
  };
  return { env: out, backgroundEnv: { LIBRARY: db(env.PG_URL, true) }, claim, close: () => Promise.all(clients.map((c) => c.end())) };
}

/// THE RELEASE SERVED NOW, read through the worker's own `servedRelease` (a
/// fresh ASSETS each time, since it keeps what it read per ASSETS).
async function served() {
  return servedRelease({ ASSETS: { fetch: () => fetch(RELEASE_URL, { signal: AbortSignal.timeout(FETCH_MS) }) } });
}

/// `opened` and `servedNow` stand in for the databases and the release in a
/// check; `put` (one object to R2) and `readEdge` (the signal as the CDN serves it) for R2 and the edge.
export async function run({ once = false, opened = null, servedNow = served, target = signalTarget(),
  put = (t, o) => putObject({ ...t, ...o }), readEdge = readSignalEdge } = {}) {
  const { env, backgroundEnv = env, claim, close } = opened || await openEnv();
  let stopping = false, ownershipLost = false, stopRun;
  let owner;
  try {
    if (!once && claim) owner = await claim(() => { ownershipLost = true; stopping = true; stopRun?.(); });
  } catch (e) { await close(); throw e; }
  const budget = capacity({ critical: { snapshot: SNAPSHOT_MS, settle: SETTLE_MS } }), background = backgroundLane();
  const pace = paceCache((now) => scorePace(backgroundEnv, now));
  const commit = commitOf();
  // THE SHADOW GOES ON WHERE IT STOPPED: its cursor is kept in its finding.
  const kept = await env.LIBRARY.prepare("SELECT body FROM factory_findings WHERE kind = 'settle_shadow'").first().catch(() => null);
  let cursor = { at: iso(Date.now() - 10 * 60_000), id: "" };
  try { if (kept) cursor = JSON.parse(kept.body).cursor || cursor; } catch { /* from ten minutes ago */ }
  let release = await servedNow();
  const failed = new Map();
  const written = { open: true, urgent: true }, edge = { stale: 0, uncached: 0 };
  let unconfiguredSaid = false, tallyAt = 0, tallyKept = null, forgotAt = 0;
  // THE SIGNAL FOLLOWS EVERY SNAPSHOT, so it is never older than one.
  const writeSignal = async (snap) => {
    if (!target) {
      if (!unconfiguredSaid) { unconfiguredSaid = true; await findingsTick(env.LIBRARY, "signal_unconfigured", () => ({ at: snap.at })); }
      return null;
    }
    const body = signalBody(snap, await dialsOf(env), { protocol: PROTOCOL, protocols: PROTOCOLS_SERVED });
    await put(target, { key: SIGNAL_KEY, body: JSON.stringify(body), contentType: "application/json", cacheControl: SIGNAL_CACHE });
    written.open = body.open > 0; written.urgent = body.urgent > 0;
    return body;
  };
  // EVERY TICK ON ITS OWN CLOCK, and one that throws only logs: a bad minute of
  // the fair table must not stop the heartbeat.
  const tick = async (name, fn) => {
    const began = performance.now();
    try {
      const ran = await fn();
      if (ran === false) return;
      if (failed.get(name)) log(name, "recovered"); failed.set(name, 0);
      budget.observe(name, performance.now() - began);
    } catch (e) {
      budget.observe(name, performance.now() - began, true);
      const n = (failed.get(name) || 0) + 1; failed.set(name, n); if (n === 1 || n % 30 === 0) log(name, "failed", n, e && e.message);
    }
  };
  const jobs = {
    release: async () => { release = await servedNow(); forgetDials(); },
    // THE HEARTBEAT IS WRITTEN FIRST: a signal that cannot be written fails its
    // own tick and leaves the snapshot, and the daemon_at it carries, current.
    snapshot: async () => writeSignal(await snapshotTick(env, { engine: release.engine, release: release.release, commit })),
    // A WAVE FROM NOTHING is signalled now, not at the next snapshot.
    wave: async () => {
      const w = await workWaiting(env.LIBRARY);
      if ((w.open && !written.open) || (w.urgent && !written.urgent)) await jobs.snapshot();
    },
    signalCheck: async () => {
      if (!target) return;
      const seen = await readEdge();
      const stale = !seen.at || Date.now() - Date.parse(seen.at) > SIGNAL_FRESH_MS;
      edge.stale = stale ? edge.stale + 1 : 0;
      edge.uncached = CACHED.has(String(seen.cache || "").toUpperCase()) ? 0 : edge.uncached + 1;
      if (edge.stale === SIGNAL_BAD_RUNS) await findingsTick(env.LIBRARY, "signal_stale", (p) => ({ runs: (p.runs || 0) + 1, last: seen }));
      if (edge.uncached === SIGNAL_BAD_RUNS) await findingsTick(env.LIBRARY, "signal_uncached", (p) => ({ runs: (p.runs || 0) + 1, last: seen }));
    },
    fair: async () => {
      if (!release.engine) return;
      // WHAT WAS WRITTEN READS BACK AS WHAT WAS WORKED OUT: the round trip
      // through Postgres's doubles is itself a shadow check, at no tolerance.
      const table = await fairTick(env, release.engine);
      const cmp = compareFair(table, (await readFair(env.LIBRARY, release.engine)).table);
      if (cmp.differ) await findingsTick(env.LIBRARY, "fair_roundtrip", (p) => ({ differ: (p.differ || 0) + 1, last: cmp }));
      await findingsTick(env.LIBRARY, "capacity", () => budget.reading());
    },
    // THE QUEUE FIRST (dial `settle_async`), then the shadow's judging of what settled.
    settle: async () => { await settleBatch(env); cursor = (await settleShadow(env, { threshold: CLIENTS_PER_FACT, from: cursor })).cursor; },
    // THE COMPUTE PAGE'S PICTURE, one object every reader gets (worker/contribution.js `demandOf`).
    demand: async () => {
      if (!target) return;
      const now = Date.now();
      const body = await demandOf(backgroundEnv, now, undefined, { pace: await pace(now),
        costEveryMs: budget.interval("demandCosts", DEMAND_COST_EVERY_MS),
        onCosts: (ms) => budget.observe("demandCosts", ms) });
      await put(target, { key: DEMAND_KEY, body: JSON.stringify(body), contentType: "application/json", cacheControl: DEMAND_CACHE });
    },
    // THE HOME PAGE'S COUNT (worker/contribution.js `tallyOf`), every `stats_tally_ms`;
    // the whole-table count under it is taken again each `TALLY_BASE_MS`.
    tally: async () => {
      if (!target) return;
      const now = Date.now();
      if (now - tallyAt < Math.min(10 * 60_000, budget.interval("tally", (await dialsOf(env, now)).stats_tally_ms)) - STATS_TICK_MS / 2) return false;
      if (!tallyKept || now - Date.parse(tallyKept.at) >= TALLY_BASE_MS) tallyKept = await tallyBase(backgroundEnv.LIBRARY, now);
      await put(target, { key: TALLY_KEY, body: JSON.stringify(await tallyOf(backgroundEnv, tallyKept, now, await pace(now))), contentType: "application/json", cacheControl: TALLY_CACHE });
      tallyAt = now;
    },
    // AN ASK THAT IS OVER KEEPS NO CODE (worker/pg/sync.sql `factory_forget_codes`), every `initiator_forget_ms`.
    forget: async () => {
      const now = Date.now();
      if (now - forgotAt < (await dialsOf(env, now)).initiator_forget_ms - STATS_TICK_MS / 2) return;
      await backgroundEnv.LIBRARY.prepare("SELECT factory_forget_codes() AS n").first();
      forgotAt = now;
    },
    // AN OFFICIAL CHECK OF ANOTHER ENGINE IS NONE (worker/pg/sync.sql `factory_moot_checks`):
    // a dispute goes back to the volunteers, a spot back to its fact.
    moot: async () => {
      if (!release.engine) return;
      for (let i = 0; i < PAST_CALLS; i++) {
        const { n } = await backgroundEnv.LIBRARY.prepare("SELECT factory_moot_checks(?::text, ?::bigint) AS n").bind(release.engine, PAST_BATCH).first();
        if (Number(n) < PAST_BATCH) break;
      }
    },
    // WHAT IS SETTLED AND PAST `KEPT_MS` IS FORGOTTEN; the served engine's facts stay.
    past: async () => {
      if (!release.engine) return;
      const before = iso(Date.now() - KEPT_MS);
      for (let i = 0; i < PAST_CALLS; i++) {
        const { n } = await backgroundEnv.LIBRARY.prepare("SELECT factory_forget_past(?::text, ?::text, ?::text, ?::bigint) AS n")
          .bind(before, release.engine, JSON.stringify(BOARD_KINDS), PAST_BATCH).first();
        if (Number(n) < PAST_BATCH) break;
      }
    },
  };
  if (once) {
    for (const name of ["snapshot", "fair", "settle", "demand", "tally", "forget", "past", "moot"]) await tick(name, jobs[name]);
    await close();
    const bad = [...failed].filter(([, n]) => n > 0).map(([k]) => k);
    log(bad.length ? `once: failed ${bad.join(", ")}` : "once: ok", commit);
    return bad.length ? 1 : 0;
  }
  if (stopping) { await close(); await owner?.release(); return 1; }
  const timers = new Map(), inflight = new Set();
  const every = (ms, name) => {
    const loop = async () => {
      if (stopping) return;
      const p = ["demand", "tally", "forget", "past", "moot"].includes(name)
        ? background(() => stopping ? undefined : tick(name, jobs[name])) : tick(name, jobs[name]);
      inflight.add(p);
      await p;
      inflight.delete(p);
      if (!stopping) timers.set(name, setTimeout(loop, ms));
    };
    timers.set(name, setTimeout(loop, 0));
  };
  every(RELEASE_MS, "release"); every(SNAPSHOT_MS, "snapshot"); every(FAIR_MS, "fair"); every(SETTLE_MS, "settle"); every(DEMAND_MS, "demand"); every(STATS_TICK_MS, "tally"); every(STATS_TICK_MS, "forget"); every(PAST_MS, "past"); every(PAST_MS, "moot");
  every(WAVE_MS, "wave"); every(SIGNAL_CHECK_MS, "signalCheck");
  log("factoryd up", commit, release.engine);
  await new Promise((resolve) => {
    const stop = () => { stopping = true; for (const t of timers.values()) clearTimeout(t); resolve(); };
    stopRun = stop;
    process.once("SIGTERM", stop);
    process.once("SIGINT", stop);
  });
  // A TICK UNDER WAY FINISHES, so no transaction is cut by the connection closing.
  await Promise.allSettled([...inflight]);
  await close();
  await owner?.release();
  log("factoryd down");
  return ownershipLost ? 1 : 0;
}

/// THE SIGNAL AS A READER GETS IT: its `at` and the edge's cache status.
export async function readSignalEdge(url = SIGNAL_URL) {
  const r = await fetch(url, { cache: "no-store", signal: AbortSignal.timeout(FETCH_MS) });
  let at = null;
  try { at = (await r.json()).at || null; } catch { /* unreadable reads as stale */ }
  return { status: r.status, at, cache: r.headers.get("cf-cache-status") };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exitCode = await run({ once: process.argv.includes("--once") });
}
