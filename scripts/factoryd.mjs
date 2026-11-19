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
import { pgDb, postgresClient, POSTGRES_OPTIONS, placeholders } from "../worker/db.js";
import { signalTick, snapshotTick, fairTick, readFair, compareFair, settleShadow, settleBatch, findingsTick, signalBody, workWaiting } from "../worker/settler.js";
import { servedRelease, CLIENTS_PER_FACT, PROTOCOL, PROTOCOLS_SERVED } from "../worker/verify.js";
import { dialsOf, forgetDials } from "../worker/dials.js";
import {measureQueueCosts,recordQueueDay} from '../worker/queue_clearance.js';
import {seedQueueZones} from '../worker/queue_counts.js';
import {currentStatistics} from '../worker/statistics_clock.js';
import { putObject } from "./sigv4.mjs";
import { iso } from "../worker/instant.js";
import { KEPT_MS } from "../worker/tasks.js";
import { BOARD_KINDS } from "../worker/board_kinds.js";
import { recoverBoardTasks } from "../worker/board_tasks.js";
import { DEMAND_KEY, DEMAND_CACHE, queueDemandOf, queueFleetOf, tallyOf, tallyBase, scorePace, TALLY_KEY, TALLY_CACHE, TALLY_BASE_MS } from "../worker/contribution.js";
import { capacity, backgroundLane, paceCache, claimDaemon } from "./factory_capacity.mjs";
import { facts } from "../worker/factory.js";
import { pipelineClock } from "./pipeline_clock.mjs";
import { TASK_PIPELINE_STAGES, readPipelineControl } from "../worker/task_pipeline.js";
import { availabilityWriter, publishAvailability } from "../worker/factory_availability.js";
import {createStatisticsProvider,STATISTICS_PUMP_MS} from '../worker/statistics_provider.js';

/// HOW OFTEN EACH TICK RUNS, and where the served release is read.
export const SNAPSHOT_MS = 10_000, FAIR_MS = 60_000, DEMAND_MS = 60_000, STATS_TICK_MS = 10_000, SETTLE_MS = 2_000, RELEASE_MS = 60_000, WAVE_MS = 2_000, SIGNAL_CHECK_MS = 60_000;

export function statisticsLimitsFromEnv(env) {
  const maximum = env.FACTORY_STATISTICS_MAX_BYTES;
  const reserve = env.FACTORY_STATISTICS_RESERVE_BYTES;
  if (maximum === undefined && reserve === undefined) return {};
  const maxStorageBytes = Number(maximum), storageReserveBytes = Number(reserve);
  if (![maximum, reserve].every(value => /^[1-9][0-9]*$/.test(value))
      || ![maxStorageBytes, storageReserveBytes].every(Number.isSafeInteger)
      || storageReserveBytes >= maxStorageBytes) {
    throw Error("set valid FACTORY_STATISTICS_MAX_BYTES and FACTORY_STATISTICS_RESERVE_BYTES");
  }
  return { maxStorageBytes, storageReserveBytes };
}
/// Bounded maintenance passes; archived task pruning runs separately through
/// scripts/pg_task_archive.py and the unarchived retention call returns zero.
export const PAST_MS = 60_000, PAST_BATCH = 100, PAST_CALLS = 5;
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
  let writeSnapshot,writeAvailability,statisticsTransaction,statisticsQuery,statisticsControlQuery;
  const boundedQuery=c=>async(sql,args=[],{signal,remainingMs=1000}={})=>{
    let expired=false,timer;
    const check=()=>{if(expired||signal?.aborted)throw Error('statistics lookup stopped');};
    const timeout=new Promise((_,reject)=>{timer=setTimeout(()=>{expired=true;reject(Error('statistics lookup deadline'));},Math.max(1,Math.min(1000,remainingMs)));});
    try{return await Promise.race([c.transaction(async tx=>{
      check();await tx.query('SET LOCAL statement_timeout='+Math.max(1,Math.min(1000,Math.floor(remainingMs))),[]);
      check();const result=await tx.query(sql,args);check();return result.rows;
    }),timeout]);}finally{clearTimeout(timer);}
  };
  const db = (url, background = false, stage = null, max = 1, signal = false) => {
    // PG_SOCKET (a directory) reaches Postgres over its unix socket, as the
    // unit does: peer authentication, no password on the machine.
    // Background batches can contain several bounded statements; the connection stays alive between them.
    const connection = { ...CONNECTION, ...(background ? { transaction_timeout: 0 } : {}),
      ...(stage ? {statement_timeout:5000,transaction_timeout:30000} : {}),
      application_name: signal ? "wfsim-factoryd-signal" : stage ? "wfsim-factoryd-"+stage : background ? "wfsim-factoryd-background" : CONNECTION.application_name };
    const c = postgresClient(connect(url, { ...POSTGRES_OPTIONS, max, idle_timeout: 0, connection, onnotice: () => {},
      ...(env.PG_SOCKET ? { host: env.PG_SOCKET } : {}) }));
    clients.push(c);
    if(signal)writeAvailability=availabilityWriter(c);
    if(background&&!statisticsTransaction)statisticsTransaction=fn=>c.transaction(fn);
    if(stage==='statistics-control')statisticsQuery=statisticsControlQuery=boundedQuery(c);
    if(!background&&!stage&&!signal&&!writeSnapshot&&url===env.PG_URL)writeSnapshot=async(statement,allowed)=>{
      const stopped=new Error('statistics stopped or paused');
      const began=performance.now();let expired=false,timer;
      const guard=()=>{if(expired||performance.now()-began>=3000)throw stopped;};
      const timeout=new Promise((_,reject)=>{timer=setTimeout(()=>{expired=true;reject(stopped);},3000);});
      try{return await Promise.race([c.transaction(async tx=>{
        guard();
        const limited={query:async(sql,args=[])=>{
          guard();await tx.query('SET LOCAL statement_timeout='+Math.max(1,Math.min(1000,Math.floor(3000-(performance.now()-began)))),[]);
          const result=await tx.query(sql,args);guard();return result;
        }};
        if(!await allowed(limited))throw stopped;
        await limited.query(placeholders(statement.sql),statement.args);
        if(!await allowed(limited))throw stopped;
        guard();
        return true;
      }),timeout]);}catch(error){if(error===stopped)return false;throw error;}finally{clearTimeout(timer);}
    };
    return pgDb(c);
  };
  const out = { LIBRARY: db(env.PG_URL), ...(env.PG_ACCOUNTS_URL ? { ACCOUNTS: db(env.PG_ACCOUNTS_URL,false,null,2) } : {}) };
  const signalEnv={LIBRARY:db(env.PG_URL,false,null,1,true)};
  const statisticsEnv={LIBRARY:db(env.PG_URL,false,'statistics-control')};
  const claim = async (onLost) => {
    let owner;
    const sql = connect(env.PG_URL, { ...POSTGRES_OPTIONS, max: 1, idle_timeout: 0,
      connection: { ...CONNECTION, application_name: "wfsim-factoryd-owner" }, onnotice: () => {},
      onclose: () => owner?.lost(), ...(env.PG_SOCKET ? { host: env.PG_SOCKET } : {}) });
    owner = await claimDaemon(sql, onLost);
    return owner;
  };
  const backgroundEnv={LIBRARY:db(env.PG_URL,true)};
  const stageEnvs=Object.fromEntries(Object.keys(TASK_PIPELINE_STAGES).map(name=>[name,{...out,LIBRARY:db(env.PG_URL,false,name)}]));
  const listen=async wake=>{
    const sql=connect(env.PG_URL,{...POSTGRES_OPTIONS,max:1,idle_timeout:0,connect_timeout:5,
      connection:{...CONNECTION,application_name:"wfsim-factoryd-events"},onnotice:()=>{},
      ...(env.PG_SOCKET?{host:env.PG_SOCKET}:{})});
    let handle;
    clients.push({end:async()=>{await handle?.unlisten();await sql.end({timeout:5});}});
    handle=await sql.listen("wfsim_task_pipeline",wake);
  };
  return {env:out,signalEnv,statisticsEnv,backgroundEnv,stageEnvs,writeSnapshot,writeAvailability,statisticsTransaction,statisticsQuery,statisticsControlQuery,claim,listen,close:()=>Promise.all(clients.map(c=>c.end()))};
}

/// THE RELEASE SERVED NOW, read through the worker's own `servedRelease` (a
/// fresh ASSETS each time, since it keeps what it read per ASSETS).
async function served() {
  return servedRelease({ ASSETS: { fetch: () => fetch(RELEASE_URL, { signal: AbortSignal.timeout(FETCH_MS) }) } });
}

/// `opened` and `servedNow` stand in for the databases and the release in a
/// check; `put` (one object to R2) and `readEdge` (the signal as the CDN serves it) for R2 and the edge.
export async function run({ once = false, opened = null, servedNow = served, target = signalTarget(),
  put = (t, o) => putObject({ ...t, ...o }), readEdge = readSignalEdge, statisticsFactory=createStatisticsProvider,
  statisticsLimits = statisticsLimitsFromEnv(process.env) } = {}) {
  const { env, signalEnv = env, statisticsEnv = signalEnv, backgroundEnv = env, writeSnapshot,writeAvailability,statisticsTransaction,statisticsQuery,statisticsControlQuery,stageEnvs = Object.fromEntries(Object.keys(TASK_PIPELINE_STAGES).map(name=>[name,env])), claim, listen, close } = opened || await openEnv();
  let stopping = false, ownershipLost = false, stopRun;
  let owner;
  try {
    if (!once && claim) owner = await claim(() => { ownershipLost = true; stopping = true; stopRun?.(); });
  } catch (e) { await close(); throw e; }
  const budget = capacity({ critical: { snapshot: SNAPSHOT_MS, settle: SETTLE_MS } }), background = backgroundLane();
  const pace = paceCache((now) => scorePace(backgroundEnv, now));
  const commit = commitOf();
  // THE SHADOW GOES ON WHERE IT STOPPED: its cursor is kept in its finding.
  const kept = await env.LIBRARY.prepare("SELECT body FROM state_factory_findings WHERE kind = 'settle_shadow'").first().catch(() => null);
  let cursor = { at: iso(Date.now() - 10 * 60_000), id: "" };
  try { if (kept) cursor = JSON.parse(kept.body).cursor || cursor; } catch { /* from ten minutes ago */ }
  let release = await servedNow();
  const statistics=statisticsFactory({env,controlEnv:statisticsEnv,transaction:statisticsTransaction,query:statisticsQuery,controlQuery:statisticsControlQuery,
    release:()=>release,stopped:()=>stopping,limits:statisticsLimits});
  let statisticsAt=0, queueFleet=null, queuePrices=null;
  let wakeStatistics=()=>{};
  const publishSnapshot=counts=>snapshotTick(env,{engine:release.engine,release:release.release},Date.now(),{counts,allowed:statistics.allowed,
    ...(writeSnapshot?{write:statement=>writeSnapshot(statement,async tx=>{
      try{return await statistics.assert(counts,{lookup:async(sql,args)=>(await tx.query(placeholders(sql),args)).rows});}catch{return false;}
    })}:{})});
  const allowStatistics=async name=>{
    if(stopping)return false;
    const control=await readPipelineControl(statisticsEnv,'statistics');
    if(stopping)return false;
    if(control.paused){log(name,'paused','statistics',control.changed_at,control.changed_by);return false;}
    return true;
  };
  const failed = new Map();
  const written = { open: true, urgent: true }, edge = { stale: 0, uncached: 0 };
  let unconfiguredSaid = false, tallyAt = 0, tallyKept = null, forgotAt = 0, admittedAt = 0;
  // Only the signal clock and wave probe share this writer.
  const writeSignal = async (snap) => {
    if(stopping)return null;
    return publishAvailability(signalEnv.LIBRARY,snap,{write:writeAvailability,allowed:()=>!stopping,publish:async()=>{
    if (!target) {
      if (!unconfiguredSaid) { unconfiguredSaid = true; await findingsTick(signalEnv.LIBRARY, "signal_unconfigured", () => ({ at: snap.at })); }
      return null;
    }
    const body = signalBody(snap, await dialsOf(signalEnv), { protocol: PROTOCOL, protocols: PROTOCOLS_SERVED });
    if(stopping)return null;
    await put(target, { key: SIGNAL_KEY, body: JSON.stringify(body), contentType: "application/json", cacheControl: SIGNAL_CACHE });
    written.open = body.open > 0; written.urgent = body.urgent > 0;
    return body;
    }});
  };
  // EVERY TICK ON ITS OWN CLOCK, and one that throws only logs: a bad minute of
  // the fair table must not stop the heartbeat.
  const tick = async (name, fn) => {
    const began = performance.now();
    try {
      const ran = await fn();
      if (ran === false) return;
      if (failed.get(name)) log(name, "recovered"); failed.set(name, 0);
      if(name!=='statistics')budget.observe(name, performance.now() - began);
    } catch (e) {
      if(name!=='statistics')budget.observe(name, performance.now() - began, true);
      const n = (failed.get(name) || 0) + 1; failed.set(name, n); if (n === 1 || n % 30 === 0) log(name, "failed", n, e && e.message);
    }
  };
  let signalFlight=null;
  const signal=()=>{
    if(stopping)return Promise.resolve(false);
    if(!signalFlight)signalFlight=signalTick(signalEnv,{engine:release.engine,release:release.release})
      .then(writeSignal).finally(()=>{signalFlight=null;});
    return signalFlight;
  };
  const jobs = {
    release: async () => {const next=await servedNow();if(next.engine!==release.engine)statistics.changed();release=next;forgetDials();},
    admission: async () => {
      if (!release.engine) return false;
      const now = Date.now();
      if (now - admittedAt < budget.interval("admission", SETTLE_MS) - SETTLE_MS / 2) return false;
      admittedAt = now;
      const result = await recoverBoardTasks(backgroundEnv, release.engine);
      return result.mode !== "disabled";
    },
    heartbeat: async () => owner ? owner.pulse(commit) : false,
    snapshot: async () => {
      const allowed=()=>allowStatistics('snapshot');
      if(!await allowed())return false;
      const counts=await statistics.get();if(!counts)return false;
      return publishSnapshot(counts);
    },
    statistics: async()=>{
      if(Date.now()<statisticsAt)return false;
      const began=performance.now();
      let failed=true,catchup=false;
      try{
        const result=await statistics.advance();
        failed=false;
        catchup=result.processed>0&&statistics.state().state==='advancing';
        if(result.sealed)wakeStatistics();
        return result;
      }finally{
        budget.observe('statistics',performance.now()-began,failed);
        statisticsAt=Date.now()+budget.interval('statistics',STATISTICS_PUMP_MS,undefined,{catchup});
      }
    },
    signal,
    // A WAVE FROM NOTHING is signalled now, not at the next snapshot.
    wave: async () => {
      const w = await workWaiting(signalEnv.LIBRARY);
      if ((w.open && !written.open) || (w.urgent && !written.urgent)) await jobs.signal();
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
    settle: async () => settleBatch(stageEnvs.verification),
    workflowAdmission: async () => facts().runStage(stageEnvs.admission,"admission"),
    delivery: async () => facts().runStage(stageEnvs.delivery,"delivery"),
    contribution: async () => facts().runStage(stageEnvs.contribution,"contribution"),
    shadow: async () => {
      const result=await settleShadow(backgroundEnv,{threshold:CLIENTS_PER_FACT,from:cursor});
      if(result.status==="held"){log("shadow","held",result.reason);return false;}
      cursor=result.cursor;
    },
    // THE COMPUTE PAGE'S PICTURE, one object every reader gets (worker/contribution.js `demandOf`).
    queueMaintenance: async () => {
      if(!statisticsTransaction||!await allowStatistics('queue maintenance'))return false;
      const present=await env.LIBRARY.prepare("SELECT to_regclass('state_queue_zone_seed') IS NOT NULL installed").first();
      if(!present?.installed)return false;
      await statisticsTransaction(async tx=>{
        await tx.query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ",[]);
        await tx.query("SET LOCAL statement_timeout='1800ms'",[]);
        await tx.query("SET LOCAL transaction_timeout='90s'",[]);
        await tx.query("SET LOCAL lock_timeout='250ms'",[]);
        await tx.query('SET LOCAL jit=off',[]);
        const query=async(sql,args=[])=>{
          if(stopping)throw Error('queue maintenance stopped');
          return (await tx.query(sql,args)).rows;
        };
        await query('SELECT queue_count_fold()');
        const [seeded]=await query("SELECT 1 FROM state_queue_zone_seed WHERE epoch=coalesce(compute_order_cut_epoch(),'') AND workload IS NOT NULL");
        if(!seeded)await seedQueueZones(query,{pageRows:4096});
      });
    },
    queueCosts: async () => {
      if(!await allowStatistics('queue costs'))return false;
      queuePrices=await measureQueueCosts(backgroundEnv.LIBRARY,queuePrices);
      await recordQueueDay(backgroundEnv.LIBRARY,queuePrices);
    },
    queueFleet: async () => {
      if(!await allowStatistics('queue fleet'))return false;
      queueFleet=await queueFleetOf(backgroundEnv,pace);
    },
    demand: async () => {
      if(!await allowStatistics('demand'))return false;
      if (!target) return;
      let body=await queueDemandOf(env,{fleet:queueFleet,prices:queuePrices,detail:statistics.cachedDemand?.()});
      if(body===undefined){
        const counts=await statistics.get();if(!counts?.demand)return false;
        if(!await publishSnapshot(counts)||!await allowStatistics('demand'))return false;
        await statistics.assert(counts);body=counts.demand;
      }
      if(!body||!await allowStatistics('demand')||stopping)return false;
      if(body.queue){
        const cut=await env.LIBRARY.prepare('SELECT compute_order_cut_epoch() AS epoch').first();
        if(!cut||body.order_epoch!==cut.epoch||!currentStatistics(body)||stopping)return false;
      }
      await put(target, { key: DEMAND_KEY, body: JSON.stringify(body), contentType: "application/json", cacheControl: DEMAND_CACHE });
    },
    // THE HOME PAGE'S COUNT (worker/contribution.js `tallyOf`), every `stats_tally_ms`;
    // the whole-table count under it is taken again each `TALLY_BASE_MS`.
    tally: async () => {
      if (!target) return;
      const now = Date.now();
      if (now - tallyAt < budget.interval("tally", (await dialsOf(env, now)).stats_tally_ms, 10 * 60_000) - STATS_TICK_MS / 2) return false;
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
    // Unarchived cleanup returns zero; private R2 archival owns pruning.
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
    for (const name of ["signal","statistics", "snapshot", "fair", "workflowAdmission", "settle", "delivery", "contribution", "shadow", "admission", "queueMaintenance", "queueCosts", "queueFleet", "demand", "tally", "forget", "past", "moot"]) await tick(name, jobs[name]);
    statistics.stop();
    await close();
    const bad = [...failed].filter(([, n]) => n > 0).map(([k]) => k);
    log(bad.length ? `once: failed ${bad.join(", ")}` : "once: ok", commit,'statistics='+statistics.state().state);
    return bad.length ? 1 : 0;
  }
  if (stopping) { await close(); await owner?.release(); return 1; }
  const clock=pipelineClock({onError:(name,error)=>log(name,"clock failed",error.message)});
  wakeStatistics=()=>{clock.wake('snapshot');clock.wake('demand');};
  const every = (ms, name) => {
    clock.add(name,ms,async()=>{
      if (stopping) return;
      await (["statistics","admission", "queueMaintenance", "queueCosts", "queueFleet", "tally", "forget", "past", "moot", "shadow"].includes(name)
        ? background(() => stopping ? undefined : tick(name, jobs[name])) : tick(name, jobs[name])
      );
    });
  };
  every(SNAPSHOT_MS, "heartbeat");
  every(STATISTICS_PUMP_MS,'statistics');
  every(RELEASE_MS, "release"); every(SNAPSHOT_MS, "signal"); every(SNAPSHOT_MS, "snapshot"); every(FAIR_MS, "fair"); every(SETTLE_MS, "settle"); every(FAIR_MS,"queueMaintenance"); every(DEMAND_MS,"queueCosts"); every(DEMAND_MS,"queueFleet"); every(DEMAND_MS, "demand"); every(STATS_TICK_MS, "tally"); every(STATS_TICK_MS, "forget"); every(PAST_MS, "past"); every(PAST_MS, "moot");
  every(SETTLE_MS, "admission");
  every(SETTLE_MS,"workflowAdmission");
  every(SETTLE_MS,"delivery");every(SETTLE_MS,"contribution");every(FAIR_MS,"shadow");
  every(WAVE_MS, "wave"); every(SIGNAL_CHECK_MS, "signalCheck");
  let stop;
  const stopped=new Promise((resolve) => {
    stop = () => { stopping = true;statistics.stop();clock.stop();resolve(); };
    stopRun = stop;
    process.once("SIGTERM", stop);
    process.once("SIGINT", stop);
  });
  if(stopping)stop();
  if(listen && !stopping)try {await listen(stage=>{if(stage==='statistics'){statistics.changed();statisticsAt=0;clock.wake('statistics');}
    else clock.wake({admission:"workflowAdmission",verification:"settle",delivery:"delivery",contribution:"contribution"}[stage]);});}
  catch(error){log("pipeline notification unavailable; polling retained",error.message);}
  if(!stopping)log("factoryd up",commit,release.engine);
  await stopped;
  process.removeListener("SIGTERM",stop);process.removeListener("SIGINT",stop);
  // A TICK UNDER WAY FINISHES, so no transaction is cut by the connection closing.
  await clock.drain();
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
