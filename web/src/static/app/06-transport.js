// ---- transport: api(path, body) --------------------------------------
// Native: fetch to the local server. Wasm: a Web Worker owns the engine, and
// the optimize start/status/cancel triad is emulated against a DEDICATED
// optimize worker whose progress fills the status shape the poller renders.
// Cancel = terminate that worker, since all its state is inside it.
/// ONE POOL OF WORKERS, and every wasm call this page makes runs on it —
/// `api`'s RPC, the simulator's shards and the quick calc's lanes. Three pools
/// is up to seventeen wasm instances on an eight-core machine, each with its
/// own copy of the engine's memory. SIZE: every core but one, capped at eight;
/// THE OPTIMIZER KEEPS ITS OWN, because a search owns them for minutes. A LANE
/// IS A QUEUE — one message at a time, so `busy` is queue depth.
/// HOW MUCH OF THE MACHINE THIS PAGE MAY USE — one setting, and it is GLOBAL.
///
/// Every heavy thing here is N workers running the engine flat out — the right
/// default on a desktop and a hand-warmer on a phone. The lever is the WORKER
/// COUNT: a longer run on fewer cores is cooler than a shorter one on all of
/// them, for the same answer. A PERCENTAGE rather than a named tier, since the
/// core count is something the page can ASK FOR, so one setting means the same
/// on a phone and a workstation. IT IS NOT A QUALITY SETTING: one lane and
/// eight produce the same numbers, and the run COUNT is the accuracy knob.
const COMPUTE_KEY = "wfsim-compute";
/// HALF, by decision. It is cooler than what this page did
/// before on a phone and faster on a desktop with more than sixteen threads,
/// which is the same change in both directions: the old default was a fixed
/// eight whatever the machine was. It is a DEFAULT, not a limit — see the
/// ceiling note below, of which there is now none.
const COMPUTE_DEFAULT_PCT = 50;
/// THERE IS NO CEILING. 100% means every logical processor
/// the machine reports, on the desktop build and in a browser alike.
///
/// NOT capped at sixteen. Each lane is a Web Worker holding its own instance
/// of the wasm module, so the last few cost memory rather than heat. MEASURED,
/// that instance is **48 MB** (0/1/4/8/16 lanes,
/// linear to within 2%) — so a 28-core machine spends 1.3 GB to use all of
/// itself, about 4% of what such a machine has. A cap was protecting nobody
/// and taking 43% of the CPU away from precisely the readers who had the most
/// of it, because core count and installed memory travel together: nobody
/// builds 64 cores onto 8 GB.
///
/// The share stays the reader's the whole way, which is this control's own
/// principle — "a share the reader can move, rather than a number this file
/// works out for them" — and a ceiling was the one place that principle had an
/// exception.

/// WHAT THE MACHINE SAYS IT HAS, and how much that is worth believing.
///
/// `navigator.hardwareConcurrency` is the only thing a page can ask, and three
/// things about it matter here:
///
///   · it counts LOGICAL processors, not cores — a four-core CPU with SMT
///     answers 8, and a phone counts its efficiency cores among them, so a
///     phone's "8" is not eight cores' worth of work;
///   · the spec lets a browser report FEWER on purpose ("don't treat this as an
///     absolute measurement"), and Firefox's resistFingerprinting and Tor do;
///   · it is absent on iOS Safari 11 through 15.3 — five years of iPhones —
///     where `undefined` falls back to 4 and the control says it is guessing.
///
/// All three argue for the same thing: a share the reader can move, rather than
/// a number this file works out for them.
const detectedCores = () => {
  const n = Number(navigator.hardwareConcurrency);
  return Number.isFinite(n) && n >= 1
    ? { n: Math.floor(n), known: true }
    : { n: 4, known: false };
};

/// The reader's share, 10–100. Migrates the three named tiers this shipped with
/// for a few hours on 2026-08-18, so nobody's stored setting reads as garbage.
const readComputePct = () => {
  const raw = localStorage.getItem(COMPUTE_KEY);
  const legacy = { saver: 10, balanced: 50, full: 100 }[raw];
  const n = legacy ?? Number(raw);
  return Number.isFinite(n) && n >= 10 && n <= 100
    ? Math.round(n / 10) * 10
    : COMPUTE_DEFAULT_PCT;
};
let computePct = readComputePct();

let pool = [];
/// The lane count that share comes to on this machine — at least one, and never
/// more than the ceiling.
const poolSize = () => Math.max(1, Math.round((detectedCores().n * computePct) / 100));

/// THE SHARES WORTH OFFERING — one per distinct lane count.
///
/// Ten fixed steps read badly at both ends: on a 4-core machine two steps share
/// a lane count, and rows that buy the same thing say the same thing twice. So the list is built from
/// what the shares actually BUY on this machine, keeping the cheapest share
/// that reaches each count — which also makes the last row always mean "all of
/// them", whatever the machine is.
const computeSteps = () => {
  const { n: cores } = detectedCores();
  const lanes = (pct) => Math.max(1, Math.round((cores * pct) / 100));
  const out = [];
  const seen = new Set();
  for (let pct = 10; pct <= 100; pct += 10) {
    const n = lanes(pct);
    if (seen.has(n)) continue;
    seen.add(n);
    out.push({ pct, lanes: n });
  }
  return out;
};

/// Change it, and MEAN IT NOW. The pool is built once and held, so a setting
/// that only applied to the next page load would read as broken on the machine
/// it was set to rescue. Dropping the pool is safe at any moment — every waiter
/// is settled with `cancelled` and the callers that can re-ask do (`laneAsk`).
function setComputePct(pct) {
  const want = Math.max(10, Math.min(100, Math.round(Number(pct) / 10) * 10));
  // ONTO AN OFFERED STEP, so the list always has the current value in it — a
  // share that buys the same lanes as a cheaper one is that cheaper one.
  const step = computeSteps().find((s) => s.pct >= want) || computeSteps().slice(-1)[0];
  const n = step ? step.pct : want;
  if (!Number.isFinite(n) || n === computePct) return;
  computePct = n;
  try { localStorage.setItem(COMPUTE_KEY, String(n)); } catch (_) { /* private mode */ }
  cancelSim();
  renderComputePicker();
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

/// HOW LONG A LANE MAY SAY NOTHING before it is presumed gone, in ms.
///
/// Generous on purpose: a false kill costs one rebuilt worker, and the window
/// only has to be shorter than "for ever". `loading` covers the wait before a
/// worker's first word, which is a multi-megabyte module DOWNLOAD; `stall`
/// covers a worker that has already spoken, which beats once a run.
///
/// Lowered by `check_calc_recovers.mjs`, which has to wedge a worker and then
/// outlive the window to prove the recovery happens at all.
const LANE_WATCHDOG = { loading: 90000, stall: 45000 };

/// A WORKER THAT NEITHER ANSWERS NOR FAILS, watched — ONE implementation, used
/// by both fleets.
///
/// `onerror` settles its waiters and a terminate settles its waiters; a worker
/// the browser reclaims mid-call does neither, so whatever waited on it waited
/// for ever, with no error to catch. Silence is made a BOUNDED outcome instead:
/// a request starts a clock, any word from the worker resets it, and a worker
/// past its window is handed to `giveUp` and settled like any other dead one.
///
/// BOTH FLEETS BEAT. A simulate reports progress once a run and the optimizer
/// posts at about 4 Hz, so being slow and being gone are told apart by what the
/// worker SAYS rather than by how long it is taking.
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
  let owed = 0, last = 0, dog = null, spoke = false;
  const disarm = () => { if (dog) { clearInterval(dog); dog = null; } };
  const clear = () => { owed = 0; disarm(); };
  return {
    clear,
    heard() { spoke = true; last = Date.now(); },
    done() { owed = Math.max(0, owed - 1); last = Date.now(); if (!owed) disarm(); },
    start() {
      owed += 1;
      last = Date.now();
      if (dog) return;
      dog = setInterval(() => {
        const cap = spoke ? LANE_WATCHDOG.stall : LANE_WATCHDOG.loading;
        if (!owed) { disarm(); return; }
        if (Date.now() - last > cap) { clear(); giveUp(); }
      }, 5000);
    },
  };
}

function makeLane() {
  const w = new Worker("/worker.js");
  const pending = new Map();
  const progress = new Map();
  let seq = 0, dead = false, busy = 0, warm = false, warming = null;
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
  const perish = (why) => {
    dead = true;
    pending.forEach((res) => res({ ok: false, error: why, worker_dead: true }));
    pending.clear(); progress.clear(); wd.clear(); busy = 0;
  };
  const wd = watchSilence(() => perish("worker stopped answering"));
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
  };
  w.onmessage = (e) => {
    // ANY word from the worker is proof of life, including one about a request
    // whose progress nobody asked to see.
    wd.heard();
    if (e.data.kind === "progress") {
      const f = progress.get(e.data.id);
      if (f) f(e.data.done, e.data.total);
      return;
    }
    const r = pending.get(e.data.id);
    if (r) {
      pending.delete(e.data.id);
      progress.delete(e.data.id);
      wd.done();
      busy = Math.max(0, busy - 1);
      r(e.data.payload);
    }
  };
  const lane = {
    worker: w,
    get busy() { return busy; },
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
      wd.clear();
      busy = 0;
    },
    send(msg, onProgress) {
      return new Promise((res) => {
        if (dead) { res({ ok: false, cancelled: true }); return; }
        const id = ++seq;
        pending.set(id, res);
        wd.start();
        busy += 1;
        if (onProgress) progress.set(id, onProgress);
        w.postMessage({ ...msg, id });
      });
    },
    /// An ENDPOINT on this lane. Progress is asked for explicitly because only
    /// `/api/simulate` reports it — see the worker.
    call(path, body, onProgress) {
      return lane.send(
        { kind: "api", path, body: body ?? {}, progress: !!onProgress },
        onProgress,
      );
    },
    get warm() { return warm; },
    // THE CHEAPEST QUESTION THERE IS, asked only to find out when this worker's
    // module has finished loading. `lanes` opens the first lane alone and waits
    // on this, so the rest are served the module from the HTTP cache instead of
    // asking the network for it N times at once. A failure still resolves: this
    // is a starting gun, not a health check, and `laneAsk` handles a dead lane.
    warmed() {
      if (!warming) {
        warming = lane.call("/api/meta")
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
const lanes = async () => {
  const n = poolSize();
  const first = laneAt(0);
  if (n > 1 && !first.warm) await first.warmed();
  // N LIVE LANES, DENSE BY CONSTRUCTION. `laneAt` already replaces a slot that
  // is missing or dead, so asking it for every index says exactly that — where
  // slicing the pool and patching the gaps said it twice and could still hand
  // back an `undefined` when the pool was sparse, which every caller then
  // treated as a lane and crashed on.
  return Array.from({ length: n }, (_, i) => laneAt(i));
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
/// FALLS BACK TO ONE CALL when there is nothing to gain — a single lane, or so
/// few runs that a slice would be one engagement. Sharding costs a shard each
/// on the wire and a merge at the end, not worth paying to halve a millisecond.
async function simulateFleet(body, onProgress, opts) {
  // ONLY IN THE WASM BUILD: `/worker.js` is generated into `site/` and the
  // native dev server has never served it. Without this, a simulation there
  // built workers that 404 and then waited for answers that could not come —
  // the Run button locked, on the one server the owner develops against and no
  // check script uses. On native there is nothing to gain either:
  // `api` is a fetch and the fetches already parallelise.
  if (!WASM) return api("/api/simulate", body, onProgress);
  const runs = Math.max(1, Number(body.runs) || 1);
  // THE SIZE IS DECIDED BEFORE THE WORKERS EXIST, so a simulation that falls
  // back does not pay for a pool it will not use.
  const n = poolSize();
  if (n < 2 || runs < n * 2) {
    return api("/api/simulate", body, onProgress);
  }
  const ls = await lanes();
  // EVERY RUN COVERED EXACTLY ONCE, remainder to the first lanes. A gap or an
  // overlap is not a slow answer, it is a wrong one.
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

  await Promise.all(ls.map(async (lane, k) => {
    for (;;) {
      if (stopped || failed || cursor >= runs) return;
      // …AND BACKGROUND WORK WAITS FOR THE FOREGROUND FIRST. Between pieces
      // only: the piece already running is a quarter second, so a person's Run
      // waits for that rather than for the scan.
      if (opts && opts.background) await yieldToForeground();
      if (stopped || failed || cursor >= runs) return;
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
      while (lane.busy > 0 && Date.now() < until && cursor < runs && !stopped && !failed) {
        await new Promise((r) => setTimeout(r, 25));
      }
      if (stopped || failed || cursor >= runs) return;
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
      let r = await lane.send({ kind: "shard", body, from, count },
        (d) => { live.set(k, d); tick(); });
      // A LOST LANE'S PIECE IS RE-ASKED ONCE, on a fresh worker, over the SAME
      // runs — a retry over a different range is a different answer wearing the
      // same name. Without the retry one reclaimed worker fails the whole
      // simulation, and for the quick calc this call is the baseline every
      // candidate is measured against.
      if (r && r.worker_dead) {
        r = await freeLane().send({ kind: "shard", body, from, count },
          (d) => { live.set(k, d); tick(); });
      }
      live.set(k, 0);
      // A CANCELLED PIECE CANCELS THE SIMULATION — there is no answer to merge
      // from a fraction of the runs, and the reader asked for it to stop.
      if (r && r.cancelled) { stopped = true; return; }
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
  }));

  if (stopped) return { ok: false, cancelled: true };
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
  return freeLane().send({ kind: "merge", body, shards: parts });
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
