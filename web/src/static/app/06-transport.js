// ---- compute ranges and transport ------------------------------------
const COMPUTE_RANGES_KEY = "wfsim-compute-ranges";
const computePercent = value => ["number", "string"].includes(typeof value) && Number.isFinite(Number(value)) && String(value).trim() !== ""
  ? Math.max(0, Math.min(100, Math.round(Number(value)))) : null;
const computeRangeValue = value => value && computePercent(value.min) !== null && computePercent(value.max) !== null
  ? { min: Math.min(computePercent(value.min), computePercent(value.max)), max: computePercent(value.max) } : null;
function readComputeRanges() {
  let saved = {};
  try { saved = JSON.parse(localStorage.getItem(COMPUTE_RANGES_KEY)) || {}; } catch (_) { /* use the device's settings */ }
  let previousConsent = false;
  try { const consent = JSON.parse(localStorage.getItem("wfsim-compute-consent")); previousConsent = consent?.v === 3 && consent.on === true; } catch (_) { /* no prior consent */ }
  const legacy = (key, fallback) => {
    let raw = null;
    try { raw = localStorage.getItem(key); } catch (_) { /* storage unavailable */ }
    const value = raw === null ? null : computePercent(({ saver: 10, balanced: 50, full: 100 })[raw] ?? raw);
    return value === null ? fallback : { min: value, max: value };
  };
  return {
    solo: computeRangeValue(saved.solo) || legacy("wfsim-compute", { min: 0, max: 50 }),
    together: computeRangeValue(saved.together) || legacy("wfsim-community-share", previousConsent ? { min: 30, max: 30 } : { min: 0, max: 50 }),
  };
}
let computeRanges = readComputeRanges();
const computeRange = kind => computeRanges[kind];

// Ranges describe requested shares; concurrent requests above 100 share proportionally.
// Headroom is a host observation in 0..1. Browsers supply responsiveness, not OS CPU usage.
class ComputeBudget {
  constructor({ cores, range, active = () => false, now = () => performance.now(), wake = (fn, ms) => setTimeout(fn, ms), clear = id => clearTimeout(id) }) {
    Object.assign(this, { cores, range, active, now, wake, clear });
    this.headroom = 1;
    this.queue = [];
    this.running = { solo: 0, together: 0 };
    this.debt = { solo: 0, together: 0 };
    this.at = now();
    this.timer = null;
  }
  target(kind) {
    const { min, max } = this.range(kind);
    return min + (max - min) * this.headroom;
  }
  shares(requested) {
    const live = kind => kind === requested || this.active(kind) || this.running[kind] > 0 || this.queue.some(job => job.kind === kind);
    const solo = live("solo") ? this.target("solo") : 0;
    const together = live("together") ? this.target("together") : 0;
    const scale = Math.max(100, solo + together);
    return { solo: solo * 100 / scale, together: together * 100 / scale };
  }
  settle() {
    const now = this.now(), elapsed = Math.max(0, now - this.at), shares = this.shares();
    for (const kind of ["solo", "together"]) {
      const share = shares[kind] || this.target(kind);
      this.debt[kind] = Math.max(0, this.debt[kind] + elapsed * (this.running[kind] - this.cores() * share / 100));
    }
    this.at = now;
  }
  pressure(lag, recovery = 0.02) {
    this.settle();
    this.headroom = lag > 50 ? Math.max(0, this.headroom - Math.min(0.5, lag / 500)) : Math.min(1, this.headroom + recovery);
    this.pump();
  }
  changed() { this.at = this.now(); this.pump(); }
  run(kind, work, live = () => true) {
    return new Promise((resolve, reject) => {
      this.settle();
      this.queue.push({ kind, work, live, resolve, reject });
      this.pump();
    });
  }
  pump() {
    if (this.timer !== null) { this.clear(this.timer); this.timer = null; }
    this.settle();
    for (let i = 0; i < this.queue.length;) {
      const job = this.queue[i], kind = job.kind;
      if (!job.live()) { this.queue.splice(i, 1); job.resolve({ ok: false, cancelled: true }); continue; }
      const power = this.cores() * this.shares()[kind] / 100;
      const slots = Math.ceil(power - 1e-9);
      if (power <= 0 || this.running[kind] >= slots || this.running.solo + this.running.together >= this.cores() || this.debt[kind] > 0.01) { i++; continue; }
      this.queue.splice(i, 1);
      this.running[kind]++;
      Promise.resolve().then(job.work).then(job.resolve, job.reject).finally(() => {
        this.settle();
        this.running[kind]--;
        this.pump();
      });
    }
    if (this.queue.length) {
      const shares = this.shares();
      const delays = this.queue.map(({ kind }) => {
        const power = this.cores() * shares[kind] / 100;
        return power > 0 && this.debt[kind] > 0 ? this.debt[kind] / power : 100;
      });
      this.timer = this.wake(() => { this.timer = null; this.pump(); }, Math.max(1, Math.min(100, ...delays)));
    }
  }
}
const computeBudget = new ComputeBudget({
  cores: () => detectedCores().n, range: computeRange,
  active: kind => kind === "solo" ? readerBusy() : !!computeNow && boardVerifyOn() && !computeHeld(),
});
const COMPUTE_TIMING = Symbol("compute timing");
const computeRun = (kind, work, live) => computeBudget.run(kind, async () => {
  const began = performance.now(), result = await work();
  if (result && typeof result === "object" && Object.isExtensible(result))
    Object.defineProperty(result, COMPUTE_TIMING, { value: performance.now() - began, configurable: true });
  return result;
}, live);
const computeElapsed = (result, began) => result?.[COMPUTE_TIMING] ?? performance.now() - began;
const computeShare = kind => computeBudget.shares(kind)[kind];
const computeLaneCount = kind => Math.max(1, Math.ceil(detectedCores().n * computeRange(kind).max / 100 - 1e-9));
function setComputeRange(kind, end, value) {
  const number = computePercent(value);
  if (!(kind in computeRanges) || !["min", "max"].includes(end) || number === null) return false;
  computeBudget.settle();
  const range = { ...computeRange(kind), [end]: number };
  if (range.min > range.max) range[end === "min" ? "max" : "min"] = number;
  computeRanges[kind] = range;
  computePct = computeRanges.solo.max;
  try { localStorage.setItem(COMPUTE_RANGES_KEY, JSON.stringify(computeRanges)); } catch (_) { /* this page only */ }
  computeBudget.changed();
  renderComputePicker();
  return true;
}
// Hidden-tab timer throttling is not evidence of machine pressure.
let computePressureAt = performance.now();
setInterval(() => {
  const now = performance.now(), elapsed = now - computePressureAt, lag = elapsed - 500;
  computePressureAt = now;
  if (computeBudget.queue.length || computeBudget.running.solo || computeBudget.running.together
    || boardVerifyOn() && computeRange("together").min !== computeRange("together").max) {
    computeBudget.pressure(document.hidden ? 0 : lag, Math.min(1, elapsed / 25_000));
    if (!document.hidden) renderComputePicker();
  }
}, 500);
document.addEventListener("visibilitychange", () => { computePressureAt = performance.now(); });
// Native: fetch to the local server. Wasm: a Web Worker owns the engine, and
// the optimize start/status/cancel triad is emulated against a DEDICATED
// optimize worker whose progress fills the status shape the poller renders.
// Cancel = terminate that worker, since all its state is inside it.
/// ONE POOL OF WORKERS serves the reader's calls. Shared budgets pace heavy calls.
const detectedCores = () => {
  const n = Number(navigator.hardwareConcurrency);
  return Number.isFinite(n) && n >= 1
    ? { n: Math.floor(n), known: true }
    : { n: 4, known: false };
};
let computePct = computeRange("solo").max;
let pool = [];
const poolSize = () => computeLaneCount("solo");
const computeSteps = () => {
  const out = [], seen = new Set();
  for (let pct = 1; pct <= 100; pct++) {
    const lanes = Math.max(1, Math.ceil(detectedCores().n * pct / 100));
    if (!seen.has(lanes)) { seen.add(lanes); out.push({ pct, lanes }); }
  }
  return out;
};
function setComputePct(pct) {
  if (computePercent(pct) === null) return;
  setComputeRange("solo", "min", pct);
  setComputeRange("solo", "max", pct);
}

/// WHO OUTRANKS WHOM WHEN TWO SURFACES WANT THE SAME CORES.
///
/// A person pressing Run outranks a ranking that measures itself. Both take the
/// whole pool, so without this they interleave by luck: the reader who just
/// asked for a number waits behind eighty candidates nobody asked for out loud,
/// on the fights where waiting is worst.
///
/// PRIORITY IS A PROPERTY OF THE ACTION, not of the transport — held by the
/// thing a person did, released when it is done, so `simulateFleet` stays one
/// function that does not have to guess who called it.
///
/// BACKGROUND WORK YIELDS BETWEEN PIECES, never mid-piece. A piece is a quarter
/// second by construction (see `simulateFleet`), so the foreground waits for
/// that and not for the scan — and nothing is cancelled, killed or recomputed
/// to make room.
let foregroundHeld = 0;
const holdForeground = () => {
  foregroundHeld += 1;
  let done = false;
  return () => { if (!done) { done = true; foregroundHeld = Math.max(0, foregroundHeld - 1); } };
};
/// Background work calls this before claiming a lane. `live` lets a scan that
/// has been superseded stop waiting and stand down.
const yieldToForeground = async (live) => {
  while (foregroundHeld > 0 && (!live || live())) {
    await new Promise((r) => setTimeout(r, 25));
  }
};

/// THE READER'S OWN CALLS IN FLIGHT, counted on the lane rather than listed by
/// feature: every call on the pool is the reader's unless the community's work
/// tagged it (`api(…, { community: true })`), so a new feature that computes is
/// counted without anyone remembering to add it. Community work yields to this.
let readerInFlight = 0;

/// HOW LONG A LANE MAY SAY NOTHING before it is presumed gone, in ms.
///
/// Generous on purpose: a false kill costs one rebuilt worker, and the window
/// only has to be shorter than "for ever". `loading` covers the wait before a
/// worker's first word, which is a multi-megabyte module DOWNLOAD; `stall`
/// covers a worker that has already spoken, which beats once a run.
///
/// Lowered by `check_calc_recovers.mjs`, which has to wedge a worker and then
/// outlive the window to prove the recovery happens at all.
/// `tick` is how often it looks.
/// `stall` is 30 s, fifteen of the engine's beats (`wfsim_engine::alive`, one in
/// 2 s at most): silence that long is a worker the browser reclaimed.
const LANE_WATCHDOG = { loading: 90000, stall: 30000, tick: 5000 };

/// A WORKER THAT NEITHER ANSWERS NOR FAILS, watched — ONE implementation, used
/// by both fleets.
///
/// `onerror` settles its waiters and a terminate settles its waiters; a worker
/// the browser reclaims mid-call does neither, so whatever waited on it waited
/// for ever, with no error to catch. Silence is made a BOUNDED outcome instead:
/// a request starts a clock, any word from the worker resets it, and a worker
/// past its window is handed to `giveUp` and settled like any other dead one.
///
/// EVERY CALL BEATS. The engine says it is alive from inside its work, every
/// endpoint (worker.js `set_heartbeat`), a simulate reports progress too and the
/// optimizer posts at about 4 Hz, so being slow and being gone are told apart by
/// what the worker SAYS rather than by how long it is taking.
function watchSilence(giveUp) {
  // SILENCE IS THE WORKER'S, NOT A REQUEST'S — one clock, not one per id.
  //
  // A clock per request declared the whole lane dead as soon as ANY id went
  // quiet, and a queued request is quiet by definition: a worker runs its
  // messages one at a time and the wasm call blocks, so a scan request sitting
  // behind a long simulate shard hears nothing until that shard is done. Past
  // the window the lane was killed — taking the shard with it — while the
  // worker was demonstrably alive and reporting progress on the very job that
  // was making it wait. That is both surfaces dying together under exactly the
  // load they are worth having: a heavy fight, few lanes, a simulate and a scan
  // wanting the same pool.
  //
  // A worker that is talking is alive, whatever it is talking about.

  // SILENCE IS COUNTED ONLY WHILE THE PAGE RUNS. A frozen tab or a sleeping
  // machine stops the worker with it, and the wall clock read on waking would
  // kill every lane of a machine left computing; a look adds at most two ticks.
  let owed = 0, quiet = 0, looked = 0, dog = null, spoke = false;
  const disarm = () => { if (dog) { clearInterval(dog); dog = null; } };
  const clear = () => { owed = 0; disarm(); };
  return {
    clear,
    heard() { spoke = true; quiet = 0; },
    done() { owed = Math.max(0, owed - 1); quiet = 0; if (!owed) disarm(); },
    start() {
      owed += 1;
      quiet = 0;
      if (dog) return;
      looked = Date.now();
      dog = setInterval(() => {
        const now = Date.now();
        quiet += Math.min(now - looked, 2 * LANE_WATCHDOG.tick);
        looked = now;
        const cap = spoke ? LANE_WATCHDOG.stall : LANE_WATCHDOG.loading;
        if (!owed) { disarm(); return; }
        if (quiet > cap) { clear(); giveUp(spoke); }
      }, LANE_WATCHDOG.tick);
    },
  };
}

/// WHERE A WORKER'S LOAD BROKE, as `worker.js` says it in `load_failed`, and the
/// suffix of the `engine.fail` subject (docs/ANALYTICS.md): `glue` the script
/// beside the module, `network` the module's fetch, `http` its status, `cut` its
/// body mid-download, `compile` the bytes, `memory` an instance it could not have,
/// `data` a pack the engine would not install.
const LOAD_CAUSES = ["glue", "network", "http", "cut", "compile", "memory", "data", "unclassified"];

/// THE ENGINE IS DOWNLOADED AND COMPILED ONCE A PAGE, and every worker is handed
/// the compiled module — docs/WASM.md §"One engine a page". `build_site_app.py`
/// writes in the hashed names and the size; the dev server serves no workers.
const ENGINE_WASM = "/pkg/wfsim_wasm_bg.wasm";
/// …AND ITS DATA: the module is code, and these packs are installed into every
/// instance before its first call (docs/WASM.md §"Code and data"), so a data
/// edit is a new pack and the module stays cached.
const ENGINE_PACKS = [];
/// The module's and the packs' bytes together, for the boot screen.
const ENGINE_WASM_BYTES = 0;
/// THE ANSWERS THE BUILD ALREADY KNOWS, path to file (scripts/engine_views.mjs):
/// a page reads its catalog from these and draws before the engine has
/// downloaded, which then only has to be there by the first computation.
const ENGINE_VIEWS = {};
/// A DOWNLOAD THAT RECEIVES NOTHING FOR THIS LONG IS CUT, so a stalled line ends
/// in a failure the lanes can report rather than in lanes that wait for ever.
const ENGINE_STALL_MS = 60_000;
/// One download at a time: the lanes asked together share it, and a failed one
/// is forgotten, so the next lane made is the next try.
let engineModule = null;
function engineLoad() {
  if (!engineModule) {
    const once = () => {
      engineGot = 0;
      return Promise.all([engineCompile(), ...ENGINE_PACKS.map(enginePack)])
        .then(([module, ...packs]) => ({ module, packs }))
        .catch((err) => { if (!err.loadCause) err.loadCause = engineCauseOf(err); throw err; });
    };
    engineModule = once().catch((err) => (err.loadCause === "cut" || err.loadCause === "network" ? once() : Promise.reject(err)));
    engineModule.catch(() => { engineModule = null; });
    // HOW LONG THE ENGINE TOOK, now that the page no longer waits for it to draw.
    engineModule.then(() => track("engine.ready", "", Math.round(performance.now())), () => {});
  }
  return engineModule;
}
const engineCauseOf = (err) => (err instanceof WebAssembly.CompileError ? "compile"
  : err instanceof RangeError ? "memory" : "unclassified");
const engineFailed = (cause, err) => Object.assign(err || new Error(`engine ${cause}`), { loadCause: cause });
/// THE DOWNLOAD IS COUNTED for the boot screen (index.html), module and packs
/// together, so a slow line reads as progress rather than as a hang.
let engineGot = 0, engineSaid = 0;
const engineSay = () => {
  engineSaid = Date.now();
  if (window.__wfsimBootProgress) window.__wfsimBootProgress(engineGot, ENGINE_WASM_BYTES);
};
/// One engine file's response, read a part at a time: `next()` resolves each
/// part, and a body that stalls is cut with the cause recorded on `cut`.
async function engineFetch(url) {
  // THE CLOCK IS ON THE HEADERS ONLY: a signal left armed would cut a slow
  // body that is still arriving.
  const ctl = new AbortController();
  const late = setTimeout(() => ctl.abort(), ENGINE_STALL_MS);
  let r;
  try {
    r = await fetch(url, { signal: ctl.signal });
  } catch (err) {
    throw engineFailed("network", err);
  } finally {
    clearTimeout(late);
  }
  if (!r.ok) throw engineFailed("http", new Error(`${url} ${r.status}`));
  const reader = r.body && r.body.getReader();
  const got = { r, reader, cut: null };
  got.next = async () => {
    let stall;
    const part = await Promise.race([
      reader.read(),
      new Promise((_, no) => { stall = setTimeout(() => no(new Error("engine download stalled")), ENGINE_STALL_MS); }),
    ]).catch((err) => { reader.cancel().catch(() => {}); throw (got.cut = engineFailed("cut", err)); })
      .finally(() => clearTimeout(stall));
    if (part.done) engineSay();
    else {
      engineGot += part.value.byteLength;
      if (Date.now() - engineSaid > 250) engineSay();
    }
    return part;
  };
  return got;
}
async function engineBytes(f) {
  const parts = [];
  let n = 0;
  for (;;) {
    const part = await f.next();
    if (part.done) break;
    parts.push(part.value);
    n += part.value.byteLength;
  }
  const bytes = new Uint8Array(n);
  let at = 0;
  for (const p of parts) { bytes.set(p, at); at += p.byteLength; }
  return bytes;
}
async function engineCompile() {
  const f = await engineFetch(ENGINE_WASM);
  if (!f.reader) return WebAssembly.compile(await f.r.arrayBuffer());
  // COMPILED AS IT ARRIVES where the browser can, so the compile is not a
  // second wait after the last byte. A cut body rejects the compile with the
  // browser's own error, so the cause is the one this stream recorded.
  if (WebAssembly.compileStreaming) {
    const body = new ReadableStream({
      async pull(ctl) {
        const part = await f.next();
        if (part.done) ctl.close();
        else ctl.enqueue(part.value);
      },
      cancel() { f.reader.cancel().catch(() => {}); },
    });
    return WebAssembly.compileStreaming(new Response(body, { headers: { "content-type": "application/wasm" } }))
      .catch((err) => { throw f.cut || err; });
  }
  return WebAssembly.compile(await engineBytes(f));
}
/// One data pack, as the bytes the engine's `install_data_bytes` reads.
async function enginePack(url) {
  const f = await engineFetch(url);
  return f.reader ? engineBytes(f) : new Uint8Array(await f.r.arrayBuffer());
}

/// EVERY ENGINE WORKER IS MADE HERE — `worker.js` waits for its `engine`
/// message and loads nothing itself. A module the browser will not post across
/// is sent as its address instead, and that worker fetches its own; the packs
/// always travel as bytes.
function spawnEngine() {
  const w = new Worker("/worker.js");
  engineLoad().then(({ module, packs }) => {
    try {
      w.postMessage({ kind: "engine", module, packs });
    } catch (_) {
      w.postMessage({ kind: "engine", url: ENGINE_WASM, packs });
    }
  }, (err) => w.postMessage({ kind: "engine", cause: err.loadCause }));
  return w;
}

/// WHERE A LANE WAS LOST, beside its `engine.fail`: which pool's (`reader`, the
/// reader's own; `board`, the community's), whether the page was in sight, and
/// the call it was running (`load` before it ever answered) — `n` the minutes it
/// had been out of sight. A hidden tab is throttled or frozen by the browser, so
/// without these a lane it starved reads as a lane that broke, and a lane lost
/// mid-call names the call to look at.
let hiddenSince = document.hidden ? Date.now() : null;
document.addEventListener("visibilitychange", () => { hiddenSince = document.hidden ? (hiddenSince ?? Date.now()) : null; });
function laneLost(pool, call) {
  track("engine.fail_lane", `${pool}_${hiddenSince === null ? "shown" : "hidden"}_${call}`.slice(0, 64),
    hiddenSince === null ? 0 : Math.round((Date.now() - hiddenSince) / 60_000));
}

/// A CALL NAMED AS A SUBJECT CAN CARRY IT: the endpoint without `/api/`, else the message kind.
const callLabel = (msg) => String(msg.kind === "api" ? String(msg.path || "").replace(/^\/api\//, "") : msg.kind || "other")
  .toLowerCase().replace(/[^a-z0-9_]+/g, "_").slice(0, 32) || "other";

function makeLane(pool = "reader") {
  const w = spawnEngine();
  const pending = new Map();
  const progress = new Map();
  let seq = 0, dead = false, workerFailed = false, busy = 0, queued = 0, warm = false, warming = null, loadCause = null;
  let heavyQueue = Promise.resolve();
  // WHEN THIS WORKER LAST SAID ANYTHING ABOUT A REQUEST — id to timestamp.
  //
  // A WORKER THAT NEITHER ANSWERS NOR FAILS was the one hang nothing caught.
  // `onerror` covers a module that will not load and `abandon` covers a stop,
  // and both SETTLE their waiters; a worker the browser reclaims mid-fight, or
  // one whose wasm traps, does neither — so `lane.call` never settled, the
  // scan's await never returned, `running` stayed true and `ensureGains`
  // refused that list for the life of the page. No error, so the catch that
  // exists for this never ran.
  //
  // The fix is to make SILENCE a bounded outcome rather than an infinite wait:
  // a simulate beats once a run (see `worker.js`), so a lane that says nothing
  // for the window below is presumed gone and settled like any other dead one.
  // Which pending calls are the reader's — see `readerInFlight`.
  const readerIds = new Set();
  // WHAT EACH PENDING CALL IS, oldest first — the oldest is the one running.
  const calls = new Map();
  const running = () => calls.values().next().value;
  const settleReaders = () => { readerInFlight = Math.max(0, readerInFlight - readerIds.size); readerIds.clear(); };
  const perish = (why) => {
    dead = true; workerFailed = true;
    pending.forEach((res) => res({ ok: false, error: why, worker_dead: true }));
    pending.clear(); progress.clear(); calls.clear(); wd.clear(); busy = 0; settleReaders();
  };
  // A WORKER THAT NEVER SPOKE NEVER LOADED: `worker.js` says it is downloading
  // every few seconds, so silence from the start is a load, not a stall.
  const wd = watchSilence((spoke) => {
    track("engine.fail", spoke ? "worker_silent" : "worker_load_timeout", Math.round(performance.now()));
    laneLost(pool, spoke ? (running() || {}).label || "idle" : "load");
    perish("worker stopped answering");
  });
  // A LANE THAT CANNOT LOAD IS A LANE, NOT A DEAD PAGE.
  //
  // `worker.js` fetches a ~6 MB wasm module, and a fetch can fail — a flaky
  // network, a proxy, a CDN refusing the eleventh simultaneous request for the
  // same large file. Reported live from wfsim.app:
  // "Failed to execute 'importScripts' ... The script at
  // 'https://wfsim.app/pkg/wfsim_wasm.js' failed to load", and the whole app
  // showed "WFSim could not start on this browser" because the boot guard
  // treats any window `error` as fatal.
  //
  // It is handled HERE, at the lane, because that is the only place that knows
  // the difference between "this worker is gone" and "the app is broken": the
  // waiters are settled with an error they can act on, the lane marks itself
  // dead so nothing new is queued on it, and the event is stopped rather than
  // left to reach the page's error handler.
  w.onerror = (e) => {
    if (e && e.preventDefault) e.preventDefault();
    dead = true;
    perish(String((e && e.message) || "worker failed to load"));
    laneLost(pool, "load");
    releaseAfterLaneFailed(Math.round(performance.now()), () => loadCause);
  };
  w.onmessage = (e) => {
    // ANY word from the worker is proof of life, including one about a request
    // whose progress nobody asked to see.
    wd.heard();
    if (e.data.kind === "load_failed") {
      loadCause = LOAD_CAUSES.includes(e.data.cause) ? e.data.cause : null;
      return;
    }
    if (e.data.kind === "progress") {
      const f = progress.get(e.data.id);
      if (f) f(e.data.done, e.data.total);
      return;
    }
    const r = pending.get(e.data.id);
    if (r) {
      pending.delete(e.data.id);
      progress.delete(e.data.id);
      calls.delete(e.data.id);
      wd.done();
      busy = Math.max(0, busy - 1);
      if (readerIds.delete(e.data.id)) readerInFlight = Math.max(0, readerInFlight - 1);
      r(e.data.payload);
    }
  };
  const lane = {
    worker: w,
    get busy() { return busy + queued; },
    get dead() { return dead; },
    // NOBODY IS COMING BACK once the worker is terminated, so a cancel settles
    // every waiter itself. Without this the promises hang, the caller never
    // reaches its `finally`, and the Run button stays disabled for ever — a
    // worse hang than the one the stop button exists to end.
    //
    // …AND THE LANE STAYS DEAD, which settles the waiters a cancel could not
    // see because they did not exist yet: a simulation is TWO round trips, and
    // a stop pressed in the gap between them posted the merge to a terminated
    // worker and waited on an answer that could not come.
    abandon() {
      dead = true;
      pending.forEach((res) => res({ ok: false, cancelled: true }));
      pending.clear();
      progress.clear();
      calls.clear();
      wd.clear();
      busy = 0;
      settleReaders();
    },
    async send(msg, onProgress, community = false, live = () => true) {
      const heavy = msg.kind === "shard" || msg.kind === "merge" || msg.kind === "api"
        && ["/api/simulate", "/api/board/runs", "/api/board/fold", "/api/board/score"].includes(msg.path);
      if (!heavy) return lane.sendNow(msg, onProgress, community);
      queued++;
      try {
        await lane.warmed();
        const pending = heavyQueue.then(() => computeRun(community ? "together" : "solo", () => lane.sendNow(msg, onProgress, community),
          () => !dead && live() && (!community || boardVerifyOn() && !computeHeld() && !computeFrozen)));
        heavyQueue = pending.catch(() => {});
        const result = await pending;
        return result?.cancelled && workerFailed ? { ok: false, worker_dead: true } : result;
      } finally { queued--; }
    },
    sendNow(msg, onProgress, community = false) {
      return new Promise((res) => {
        if (dead) { res({ ok: false, cancelled: true }); return; }
        const id = ++seq;
        pending.set(id, res);
        calls.set(id, { label: callLabel(msg) });
        wd.start();
        busy += 1;
        if (!community) { readerIds.add(id); readerInFlight += 1; }
        if (onProgress) progress.set(id, onProgress);
        w.postMessage({ ...msg, id });
      });
    },
    /// An ENDPOINT on this lane. Progress is asked for explicitly because only
    /// `/api/simulate` reports it — see the worker.
    call(path, body, onProgress, community = false, live) {
      return lane.send(
        { kind: "api", path, body: body ?? {}, progress: !!onProgress },
        onProgress,
        community,
        live,
      );
    },
    get warm() { return warm; },
    // A REST OF NOTHING, asked only to find out when this worker's module has
    // finished loading: the worker answers it after `ready` without entering
    // the engine. `/api/meta` here left the lane at the 70 MB that answer costs
    // to build, for life (wasm memory never shrinks). `lanes` opens the first
    // lane alone and waits on this, so the page's one download (`spawnEngine`)
    // is not shared with N instantiations at once. A failure still resolves:
    // this is a starting gun, not a health check, and `laneAsk` handles a dead
    // lane.
    warmed() {
      if (!warming) {
        warming = lane.send({ kind: "rest", ms: 0 })
          .then(() => { warm = true; })
          .catch(() => { warm = true; });
      }
      return warming;
    },
  };
  return lane;
}

/// ONE LANE AT A TIME, made when something actually needs it.
///
/// Building the pool whole on first use means building it on the BOOT's own
/// `/api/meta` — opening the page then fetches a 6 MB wasm module once per
/// core, all at once, before anything is drawn. On a 28-thread machine
/// that is fourteen simultaneous requests for the same large file to answer one
/// small question, and a single one of them failing took the page down. A plain endpoint call needs ONE worker; a sharded
/// simulation is the only thing that wants them all, and it says so.
///
/// A DEAD SLOT IS REPLACED, NEVER HANDED BACK. This returned `pool[i]` whatever
/// state it was in, so a lane that died — the module failed to download, the
/// renderer reclaimed it, a stop abandoned the pool — stayed in its slot for
/// the life of the page and every later call was posted to a worker that could
/// not answer. With `freeLane` falling back to `laneAt(0)`, one dead pool meant
/// the quick calc never produced another number, and a reload rebuilt the same
/// pool the same way and killed it again.
const laneAt = (i) => {
  if (!pool[i] || pool[i].dead) pool[i] = makeLane();
  return pool[i];
};
/// EVERY LANE, for a sharded run — the one caller that wants the whole machine.
///
/// THE FIRST ONE IS WARMED ALONE. Each lane's worker begins by fetching the
/// wasm module, and opening N at once is N simultaneous requests for the same
/// multi-megabyte file: the failure this pool has actually seen is a CDN
/// refusing the eleventh. One lane first puts the module in the HTTP cache, and
/// the rest are served from it — the same total bytes over the wire, one
/// request instead of N.
/// `n` is for the community's work, sized by its own share (69-board-work.js).
const lanes = async (n = poolSize()) => {
  const first = laneAt(0);
  if (n > 1 && !first.warm) await first.warmed();
  // N LIVE LANES, DENSE BY CONSTRUCTION. `laneAt` already replaces a slot that
  // is missing or dead, so asking it for every index says exactly that — where
  // slicing the pool and patching the gaps said it twice and could still hand
  // back an `undefined` when the pool was sparse, which every caller then
  // treated as a lane and crashed on.
  return Array.from({ length: n }, (_, i) => laneAt(i));
};
/// THE COMMUNITY'S OWN LANES (69-board-work.js `measureRow`): workers apart from
/// the reader's, so neither share queues behind the other's calls.
let boardPool = [];
const boardLaneAt = (i) => {
  if (!boardPool[i] || boardPool[i].dead) boardPool[i] = makeLane("board");
  return boardPool[i];
};
const boardLanes = async (n) => {
  const first = boardLaneAt(0);
  if (n > 1 && !first.warm) await first.warmed();
  return Array.from({ length: n }, (_, i) => boardLaneAt(i));
};
/// THE LANE THAT WILL ANSWER SOONEST, and a NEW one only when every lane that
/// exists is already working. One rpc worker meant a long simulate held up every
/// meta and i18n call queued behind it; this parks a long call on its own lane
/// without paying for a pool nobody asked for.
/// HOW MANY LANES A PLAIN ENDPOINT CALL MAY BRING INTO BEING.
///
/// TWO. `api` traffic is short calls, and the boot fires several at once (meta,
/// i18n, targets, the board) — with no cap, each one that found the others busy
/// opened a worker of its own, so opening the page fetched a 6 MB module six
/// times before anything was drawn. One lane serves them; the second exists so
/// a long call that fell back off `simulateFleet` cannot hold up a small one
/// behind it, which is the whole reason the single rpc worker was replaced.
/// Everything above two is for SHARDING, and `lanes()` is what asks for it.
const API_LANES = 2;
const freeLane = () => {
  const live = pool.filter(Boolean).filter((l) => !l.dead);
  const idle = live.find((l) => l.busy === 0);
  if (idle) return idle;
  const made = pool.filter(Boolean).length;
  if (made < Math.min(API_LANES, poolSize())) return laneAt(made);
  if (live.length) return live.reduce((a, b) => (b.busy < a.busy ? b : a));
  // NOTHING ALIVE. `laneAt` replaces a dead slot now, so this builds a worker
  // rather than handing back the corpse in slot 0 — which is what turned one
  // bad minute into a page that never computed again.
  return laneAt(0);
};

/// ONE CALL ON A LANE THAT MAY DIE UNDER IT.
///
/// A stop takes the WHOLE pool, because a simulation occupies every lane — so a
/// scan running beside one gets `cancelled` instead of an answer, and the lane
/// it is holding is dead for good. Retrying ON IT would spin through every
/// remaining candidate in an instant and leave a scan that looks finished with
/// holes in it, which is the worst of the three outcomes: worse than stopping,
/// and worse than being slow.
///
/// So the retry is on a FRESH lane, ONCE. A second refusal returns null and the
/// caller stands that lane's loop down — two stops during one scan means the
/// reader is not waiting for it.
async function laneAsk(lane, path, body, live) {
  // A MISSING LANE IS A DEAD LANE. The caller cannot act on the difference and
  // already has a recovery for the second, so raising here instead threw out
  // every candidate the other thirteen lanes were about to measure — the worst
  // of the three outcomes, and silent, because the throw left the scan's own
  // promise rejected rather than its holes reported.
  const r = lane ? await lane.call(path, body) : { ok: false, worker_dead: true };
  // A LANE THAT DIED AND A LANE THAT WAS STOPPED ARE ONE CASE HERE. They
  // answered in two different shapes — `cancelled` from `abandon`,
  // `worker_dead` from a module that would not load — and only the first was
  // recognised, so a lane whose worker never started returned `{ok:false}` and
  // the caller read it as a measurement that came back empty: the progress
  // counter advanced and the chip never appeared.
  const gone = (x) => !!x && (x.cancelled || x.worker_dead);
  if (!gone(r)) return r;
  if (live && !live()) return null;
  const again = await freeLane().call(path, body);
  return gone(again) ? null : again;
}

/// ONE SIMULATION ON ONE LANE, IN QUARTER-SECOND PIECES — what a background
/// scan measures each candidate with. A whole candidate in one call is the
/// piece a person's Run waits behind, and a crowd fight's candidate is half a
/// second; cut the way `simulateFleet` cuts, it yields between pieces and the
/// shards merge to the same bits. A lane that cannot take pieces (a pinned or
/// native one) is asked whole, and a piece lost is re-asked as `laneAsk` does.
async function laneSimulate(lane, body, live) {
  if (!WASM || !lane || typeof lane.send !== "function") return laneAsk(lane, "/api/simulate", body, live);
  const runs = Math.max(1, Number(body.runs) || 1);
  const gone = (x) => !!x && (x.cancelled || x.worker_dead);
  const parts = [];
  let from = 0, count = 1;
  while (from < runs) {
    await yieldToForeground(live);
    if (live && !live()) return null;
    const n = Math.min(count, runs - from);
    const t0 = performance.now();
    let r = await lane.send({ kind: "shard", body, from, count: n }, null, false, live);
    if (gone(r)) {
      if (live && !live()) return null;
      lane = freeLane();
      r = await lane.send({ kind: "shard", body, from, count: n }, null, false, live);
      if (gone(r)) return null;
    }
    // THE ENGINE'S REFUSAL IS AN ANSWER — passed on whole, as one call's would be.
    if (!r || r.error || r.ok === false) return r;
    parts.push(r);
    from += n;
    count = Math.max(1, Math.round((n * 250) / Math.max(1, performance.now() - t0)));
  }
  const m = await lane.send({ kind: "merge", body, shards: parts }, null, false, live);
  return gone(m) ? laneAsk(freeLane(), "/api/simulate", body, live) : m;
}

/// RUN A SIMULATION ACROSS THE POOL, and merge in Rust.
///
/// The runs of a simulation are INDEPENDENT given their index, so N lanes each
/// take a slice and the shards merge back into exactly what one worker would
/// have produced (`eight_shards_are_one_run`). Measured on a 361-body ruler
/// with a full status build: 14.4 s a hundred runs on one thread, 2.1 on eight
/// — 6.8x.
///
/// The page schedules and collects; every field of the answer is computed by
/// `simulate_merged`, so there is one implementation of the arithmetic.
///
/// A SECOND SEAM, and every observer has to know it: a check that watches
/// simulations by wrapping `api` sees nothing here, because a sharded run never
/// touches it — and worse, captures the quick calc's baseline instead.
/// Anything observing what the page simulates must wrap this too, matching on
/// the request rather than on being the only caller.
///
/// One engagement uses the ordinary endpoint. Longer calculations use pieces
/// even on one lane, so fractional and zero budgets can wait between pieces.
async function simulateFleet(body, onProgress, opts) {
  const cancelled = () => !!opts?.signal?.aborted || !!(opts?.live && !opts.live());
  // A current background scan retries a reclaimed pool; an explicit stop ends it.
  const ask = async (send, retryDead = true) => {
    if (cancelled()) return { ok: false, cancelled: true };
    let result = await send(false);
    if (!cancelled() && result && ((retryDead && result.worker_dead) || (result.cancelled && opts?.live)))
      result = await send(true);
    return cancelled() ? { ok: false, cancelled: true } : result;
  };
  if (cancelled()) return { ok: false, cancelled: true };
  // ONLY IN THE WASM BUILD: `/worker.js` is generated into `site/` and the
  // native dev server has never served it. Without this, a simulation there
  // built workers that 404 and then waited for answers that could not come —
  // the Run button locked, on the one server the owner develops against and no
  // check script uses. On native there is nothing to gain either:
  // `api` is a fetch and the fetches already parallelise.
  if (!WASM) return api("/api/simulate", body, onProgress, opts);
  const runs = Math.max(1, Number(body.runs) || 1);
  // A single engagement is atomic and needs no additional fleet.
  if (runs === 1) return ask(() => api("/api/simulate", body, onProgress, opts), false);
  const ls = await lanes(Math.min(poolSize(), runs));
  // Every run is covered exactly once; lane count never changes the run count.
  const total = runs;
  // CHUNKS A LANE PULLS, SIZED FROM WHAT THIS FIGHT COSTS ON THIS MACHINE.
  //
  // A static `runs / lanes` split is one decision taken before anything is
  // known, and it is wrong in both directions: the per-run cost of a fight in
  // this product spans 1.1 ms to 29 ms, and a machine's lanes are not equal —
  // a browser throttles a background tab, another tab takes a core, one lane
  // draws the long straw. Whatever the cause, that lane held the whole answer
  // and there was nothing to hand its remainder to.
  //
  // So the runs are a QUEUE and a lane takes the next piece when it is free.
  // A slow lane then delays itself and nothing else, and no machine has to be
  // detected for it to work.
  //
  // THE FIRST PIECE IS ONE RUN — the probe. It costs a few milliseconds on a
  // cheap fight and about 30 on the most expensive one in the product, and it
  // buys the only number the sizing needs. Starting from the LAST fight's cost
  // would be free, and wrong: single target to a 361-body Influence fight is a
  // factor of twenty-five, and a first chunk sized from the wrong end is the
  // stall this exists to end.
  const TARGET_MS = 250;
  let est = null;                 // ms per run, measured; null until the probe
  let cursor = 0;                 // the next run nobody has claimed
  let banked = 0;                 // runs whose chunk has come back
  const live = new Map();         // lane -> runs done in the chunk it holds
  const tick = () => {
    if (!onProgress) return;
    let flying = 0;
    live.forEach((d) => { flying += d; });
    onProgress(Math.min(total, banked + flying), total);
  };

  const parts = [];
  let failed = null;
  let stopped = false;

  const runners = [];
  const runLane = async (lane, k) => {
    for (;;) {
      if (stopped || failed || cancelled() || cursor >= runs) return;
      while (ls.length < Math.min(poolSize(), runs)) {
        const index = ls.length, added = laneAt(index);
        ls.push(added); runners.push(runLane(added, index).catch(error => { failed = { ok: false, error: String(error?.message || error) }; }));
      }
      // …AND BACKGROUND WORK WAITS FOR THE FOREGROUND FIRST. Between pieces
      // only: the piece already running is a quarter second, so a person's Run
      // waits for that rather than for the scan.
      if (opts && opts.background) await yieldToForeground();
      if (stopped || failed || cancelled() || cursor >= runs) return;
      // A LANE DOES NOT CLAIM WORK IT CANNOT START. Claiming first and then
      // queueing behind whatever the lane is already holding takes a run out of
      // the pool that nobody else may take, which is the static split's fault
      // reappearing one level up: with one lane occupied, the whole simulation
      // waited for it again.
      //
      // BOUNDED, because a pool where every lane is busy must still make
      // progress — after this a claim is made anyway, and slow beats never.
      const until = Date.now() + 1500;
      // …AND IT STOPS WAITING THE MOMENT THERE IS NOTHING LEFT TO CLAIM, or the
      // pool holds an answer hostage to a lane with no work to do.
      while (lane.busy > 0 && Date.now() < until && cursor < runs && !stopped && !failed && !cancelled()) {
        await new Promise((r) => setTimeout(r, 25));
      }
      if (stopped || failed || cancelled() || cursor >= runs) return;
      const left = runs - cursor;
      // …AND NEVER SO BIG THAT THE TAIL IS ONE CHUNK. A piece is capped at an
      // even share of what is LEFT, so the last round is short whatever the
      // estimate says.
      const want = est ? Math.max(1, Math.round(TARGET_MS / est)) : 1;
      const count = Math.max(1, Math.min(want, Math.ceil(left / ls.length), left));
      const from = cursor;
      cursor += count;
      live.set(k, 0);
      const t0 = performance.now();
      // A LOST LANE'S PIECE IS RE-ASKED ONCE, on a fresh worker, over the SAME
      // runs — a retry over a different range is a different answer wearing the
      // same name. Without the retry one reclaimed worker fails the whole
      // simulation, and for the quick calc this call is the baseline every
      // candidate is measured against.
      const r = await ask((fresh) => (fresh ? freeLane() : lane).send({ kind: "shard", body, from, count },
        (d) => { live.set(k, d); tick(); }, false, () => !cancelled() && !failed && !stopped));
      live.set(k, 0);
      // A CANCELLED PIECE CANCELS THE SIMULATION — there is no answer to merge
      // from a fraction of the runs, and the reader asked for it to stop.
      if (r && r.cancelled) { if (!failed) stopped = true; return; }
      if (!r || r.error) { failed = r || { ok: false, error: "a shard failed" }; return; }
      parts.push(r);
      banked += count;
      // WHAT IT COST, WHICH SIZES THE NEXT ONE. Weighted rather than replaced:
      // one chunk that lands while the machine is busy elsewhere should move
      // the estimate, not become it.
      const per = (performance.now() - t0) / count;
      est = est === null ? per : est * 0.7 + per * 0.3;
      tick();
    }
  };
  runners.push(...ls.map((lane, index) => runLane(lane, index).catch(error => { failed = { ok: false, error: String(error?.message || error) }; })));
  for (let index = 0; index < runners.length; index++) await runners[index];

  if (stopped || cancelled()) return { ok: false, cancelled: true };
  if (failed) return failed;
  // EVERY RUN IS IN. The merge is arithmetic over sums and takes no time worth
  // reporting, so the bar completes here rather than sitting at 97% while the
  // last piece is added up.
  if (onProgress) onProgress(total, total);
  // NOT `ls[0]`, WHICH MAY BE THE LANE THAT DIED. Every run is in and the merge
  // is arithmetic over sums — losing it to a corpse throws away the whole
  // simulation at the last step, and `freeLane` never hands back one.
  //
  // THE PIECES MERGE IN ANY ORDER AND ANY NUMBER: a shard carries sums, and the
  // median is chosen by ranking what every piece brought back. That is what
  // lets the split be decided while the work is already running.
  return ask(() => freeLane().send({ kind: "merge", body, shards: parts }, null, false, () => !cancelled()));
}

/// STOP EVERY WASM CALL IN FLIGHT, which is the only way to interrupt one: it
/// runs to completion inside the engine and there is no yield point to check a
/// flag at. Terminating is instant and costs nothing to recover from — the next
/// call builds a fresh pool, and none of these calls carries state between them.
///
/// IT TAKES THE WHOLE POOL, which is what one pool costs: a simulation occupies
/// every lane, so there is no smaller thing to stop. A quick calc caught in the
/// blast is told `cancelled` and asks again — see `scanGains`.
function cancelSim() {
  const live = pool.filter(Boolean);
  if (!live.length) return false;
  live.forEach((l) => { l.worker.terminate(); l.abandon(); });
  pool = [];
  return true;
}

let wopt = null; // the emulated optimize job: a FLEET of workers over disjoint
                 // strides — { id, workers[], statuses[], parts[], result, … }
let woptNextId = 1;
