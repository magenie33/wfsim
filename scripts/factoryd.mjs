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
import { snapshotTick, fairTick, readFair, compareFair, settleBatch, findingsTick, signalBody, workWaiting } from "../worker/settler.js";
import { servedRelease, CLIENTS_PER_FACT, PROTOCOL, PROTOCOLS_SERVED } from "../worker/verify.js";
import { dialsOf, forgetDials } from "../worker/dials.js";
import { putObject } from "./sigv4.mjs";
import { iso } from "../worker/instant.js";

/// HOW OFTEN EACH TICK RUNS, and where the served release is read.
export const SNAPSHOT_MS = 10_000, FAIR_MS = 60_000, SETTLE_MS = 2_000, RELEASE_MS = 60_000, WAVE_MS = 2_000, SIGNAL_CHECK_MS = 60_000;
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
const CONNECTION = { statement_timeout: 30_000, idle_in_transaction_session_timeout: 60_000, application_name: "wfsim-factoryd" };

const commitOf = () => {
  if (process.env.DAEMON_COMMIT) return process.env.DAEMON_COMMIT;
  try { return readFileSync(new URL("../COMMIT", import.meta.url), "utf8").trim(); } catch { return "unknown"; }
};
const log = (...a) => console.log(new Date().toISOString(), ...a);

export async function openEnv(env = process.env) {
  if (!env.PG_URL) throw new Error("PG_URL is required");
  const clients = [];
  const db = (url) => {
    // PG_SOCKET (a directory) reaches Postgres over its unix socket, as the
    // unit does: peer authentication, no password on the machine.
    const c = postgresClient(postgres(url, { ...POSTGRES_OPTIONS, max: 2, idle_timeout: 0, connection: CONNECTION, onnotice: () => {},
      ...(env.PG_SOCKET ? { host: env.PG_SOCKET } : {}) }));
    clients.push(c);
    return pgDb(c);
  };
  const out = { LIBRARY: db(env.PG_URL), ...(env.PG_ACCOUNTS_URL ? { ACCOUNTS: db(env.PG_ACCOUNTS_URL) } : {}) };
  return { env: out, close: () => Promise.all(clients.map((c) => c.end())) };
}

/// THE RELEASE SERVED NOW, read through the worker's own `servedRelease` (a
/// fresh ASSETS each time, since it keeps what it read per ASSETS).
async function served() {
  return servedRelease({ ASSETS: { fetch: () => fetch(RELEASE_URL) } });
}

/// `opened` and `servedNow` stand in for the databases and the release in a
/// check; `put` (one object to R2) and `readEdge` (the signal as the CDN serves it) for R2 and the edge.
export async function run({ once = false, opened = null, servedNow = served, target = signalTarget(),
  put = (t, o) => putObject({ ...t, ...o }), readEdge = readSignalEdge } = {}) {
  const { env, close } = opened || await openEnv();
  const commit = commitOf();
  // THE SHADOW GOES ON WHERE IT STOPPED: its cursor is kept in its finding.
  const kept = await env.LIBRARY.prepare("SELECT body FROM factory_findings WHERE kind = 'settle_shadow'").first().catch(() => null);
  let cursor = { at: iso(Date.now() - 10 * 60_000), id: "" };
  try { if (kept) cursor = JSON.parse(kept.body).cursor || cursor; } catch { /* from ten minutes ago */ }
  let release = await servedNow(), stopping = false;
  const failed = new Map();
  const written = { open: true, urgent: true }, edge = { stale: 0, uncached: 0 };
  let unconfiguredSaid = false;
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
    try { await fn(); if (failed.get(name)) log(name, "recovered"); failed.set(name, 0); }
    catch (e) { const n = (failed.get(name) || 0) + 1; failed.set(name, n); if (n === 1 || n % 30 === 0) log(name, "failed", n, e && e.message); }
  };
  const jobs = {
    release: async () => { release = await servedNow(); forgetDials(); },
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
    },
    settle: async () => { cursor = (await settleBatch(env, { threshold: CLIENTS_PER_FACT, from: cursor })).cursor; },
  };
  if (once) {
    for (const name of ["snapshot", "fair", "settle"]) await tick(name, jobs[name]);
    await close();
    const bad = [...failed].filter(([, n]) => n > 0).map(([k]) => k);
    log(bad.length ? `once: failed ${bad.join(", ")}` : "once: ok", commit);
    return bad.length ? 1 : 0;
  }
  const timers = new Map(), inflight = new Set();
  const every = (ms, name) => {
    const loop = async () => {
      if (stopping) return;
      const p = tick(name, jobs[name]);
      inflight.add(p);
      await p;
      inflight.delete(p);
      if (!stopping) timers.set(name, setTimeout(loop, ms));
    };
    timers.set(name, setTimeout(loop, 0));
  };
  every(RELEASE_MS, "release"); every(SNAPSHOT_MS, "snapshot"); every(FAIR_MS, "fair"); every(SETTLE_MS, "settle");
  every(WAVE_MS, "wave"); every(SIGNAL_CHECK_MS, "signalCheck");
  log("factoryd up", commit, release.engine);
  await new Promise((resolve) => {
    const stop = () => { stopping = true; for (const t of timers.values()) clearTimeout(t); resolve(); };
    process.once("SIGTERM", stop);
    process.once("SIGINT", stop);
  });
  // A TICK UNDER WAY FINISHES, so no transaction is cut by the connection closing.
  await Promise.allSettled([...inflight]);
  await close();
  log("factoryd down");
  return 0;
}

/// THE SIGNAL AS A READER GETS IT: its `at` and the edge's cache status.
export async function readSignalEdge(url = SIGNAL_URL) {
  const r = await fetch(url, { cache: "no-store" });
  let at = null;
  try { at = (await r.json()).at || null; } catch { /* unreadable reads as stale */ }
  return { status: r.status, at, cache: r.headers.get("cf-cache-status") };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exitCode = await run({ once: process.argv.includes("--once") });
}
