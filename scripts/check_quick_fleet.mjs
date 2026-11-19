// THE QUICK DESCENT ACROSS THE FLEET answers the same whatever cores score it
// (08-checkpoint-api.js `quickFleet`): one worker, three, and a count that
// changes every round give the same build, the same number and the same work,
// and so do workers that rest between calls (a share below 100%).
// Two volunteers' answers agree only to the bit (worker/appraise.js), so a
// search that depended on its slicing would never be confirmed. A second
// background search starts on the workers the first handed back.
//   node scripts/check_quick_fleet.mjs        (WFSIM_BASE=<origin> for a dev server)
import { openApp } from "./cdp.mjs";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createContext, runInContext } from "node:vm";

const transport = readFileSync(new URL("../web/src/static/app/06-transport.js", import.meta.url), "utf8");
const watch = transport.slice(transport.indexOf("const LANE_WATCHDOG"), transport.indexOf("/// WHERE A WORKER"));

async function checkSignals() {
  let now = 1000;
  const workers = [], messages = [];
  const drain = async () => { for (let i = 0; i < 16; i++) await Promise.resolve(); };
  const sandbox = createContext({
    Date: class extends Date { static now() { return now; } }, performance: { now: () => now },
    setTimeout, clearTimeout, poolSize: () => 2, computeRun: async (_, work, live) => live() ? work() : { cancelled: true },
    watchSilence: () => ({ heard() {}, done() {}, start() {}, clear() {} }),
    spawnEngine: () => {
      const worker = { postMessage: (m) => messages.push({ worker, ...m }), terminate() {},
        emit(data) { this.onmessage({ data }); } };
      workers.push(worker);
      return worker;
    },
  });
  runInContext("let wopt = null, woptNextId = 1;", sandbox);
  runInContext(readFileSync(new URL("../web/src/static/app/08-checkpoint-api.js", import.meta.url), "utf8"), sandbox);
  const job = runInContext("wopt = quickFleet({ strategy: 'quick' }, 2)", sandbox);
  await drain();
  assert.equal(job.status.activity, "loading");
  assert.equal(job.status.workers_loading, 1);
  assert.equal(job.status.engine_last_at, null);
  workers[0].emit({ kind: "alive", source: "loading" });
  assert.equal(job.status.response_last_at, now);
  assert.equal(job.status.engine_last_at, null, "a download timer is no evidence of engine activity");
  now = 1500;
  workers[0].emit({ kind: "progress", payload: { phase: "searching", sims_done: 0, enumerated: 0 } });
  assert.equal(job.status.engine_last_at, now, "a real calculation callback is feedback without a heartbeat");
  assert.equal(job.status.workers_active, 1);
  now = 2000;
  workers[0].emit({ kind: "alive", source: "engine" });
  assert.equal(job.status.engine_last_at, now);
  assert.equal(job.status.activity, "planning");
  assert.equal(job.status.workers_active, 1);
  assert.equal(job.status.sims_done, 0, "a heartbeat completes no additional fights");
  workers[0].emit({ kind: "result", payload: { pending: ["a", "b", "c", "d"], progress: [{ settled: false }] } });
  await drain();
  assert.equal(job.status.activity, "scoring");
  assert.equal(job.status.batch_builds_total, 4);
  assert.equal(job.status.batch_slices_total, 4);
  workers[1].emit({ kind: "progress", payload: { sims_done: 2, enumerated: 0 } });
  workers[0].emit({ kind: "result", payload: { scores: ["a"], fights: 3, work: 7 } });
  await drain();
  assert.equal(job.status.sims_done, 5, "banked and in-flight fights are disjoint");
  assert.equal(job.status.enumerated, 1);
  assert.equal(job.status.batch_slices_done, 1);
  workers[0].emit({ kind: "progress", payload: { sims_done: 2, enumerated: 0 } });
  workers[1].emit({ kind: "result", payload: { scores: ["b"], fights: 3, work: 7 } });
  await drain();
  assert.equal(job.status.sims_done, 8);
  workers[0].emit({ kind: "result", payload: { scores: ["c"], fights: 3, work: 7 } });
  workers[1].emit({ kind: "result", payload: { scores: ["d"], fights: 3, work: 7 } });
  await drain();
  assert.equal(job.status.batches_completed, 1);
  assert.equal(job.status.sims_done, 12);
  assert.equal(job.status.enumerated, 4);
  assert.equal(job.status.batch_slices_total, 0, "the completed batch is no longer in flight");
  const lead = messages.filter((m) => m.body?.quick_fleet?.lead).at(-1);
  assert.equal(lead.body.quick_fleet.scores.join(","), "a,b,c,d", "observation preserves the slice ordering");
  workers[0].emit({ kind: "progress", payload: { phase: "running", rounds: 0, jobs: 2, sims_done: 0 } });
  assert.equal(job.status.activity, "final_simulation", "the quick final round has no funnel rounds");
  assert.equal(job.status.jobs, 2);
  assert.equal(job.status.sims_planned, 0, "an unknown final total stays unknown");
  workers[0].emit({ kind: "progress", payload: { phase: "running", rounds: 0, jobs: 2, sims_done: 3 } });
  assert.equal(job.status.sims_done, 15);
  now = 4000;
  workers[0].emit({ kind: "alive", source: "engine" });
  assert.equal(job.status.activity, "final_simulation");
  assert.equal(job.status.sims_done, 15);
  assert.equal(job.status.engine_last_at, now);
  now = 5000;
  workers[0].emit({ kind: "alive", source: "loading" });
  assert.equal(job.status.activity, "final_simulation", "a late loading signal cannot replace the final phase");
  assert.equal(job.status.engine_last_at, 4000);
  assert.equal(job.status.response_last_at, now);
  workers[0].emit({ kind: "progress", payload: { phase: "running", rounds: 0, jobs: 2, sims_done: 6 } });
  assert.equal(job.status.sims_done, 18, "final progress adds to the banked search once");
  workers[0].emit({ kind: "result", payload: { ok: true, final_work: 5, results: [] } });
  await drain();
  assert.equal(job.result.work, 33);
  assert.equal(job.result.fights, 12);
  assert.equal(job.result.builds, 4);
  assert.equal(runInContext("woptStatus().phase", sandbox), "done");
  const resting = runInContext("quickFleet({ strategy: 'quick' }, 1, async () => {}, { rest: () => 1 })", sandbox);
  await drain();
  assert.equal(resting.status.engine_last_at, null);
  workers[2].emit({ kind: "result", payload: { pending: ["x"] } });
  await drain();
  assert.equal(resting.status.engine_last_at, now, "an actual engine return is feedback without a heartbeat");
  assert.equal(resting.status.activity, "resting");
  assert.equal(resting.status.workers_active, 0, "a resting worker is not scoring");
  now = 6000;
  workers[2].emit({ kind: "alive", source: "loading" });
  assert.equal(resting.status.engine_last_at, 5000, "a loading timer never refreshes calculation feedback");
  workers[2].emit({ kind: "rested" });
  resting.cancel();
  await drain();
  assert.equal(resting.status.engine_last_at, 5000, "finishing a rest never refreshes calculation feedback");
  assert.equal(resting.status.response_last_at, now);
  runInContext("woptStart({ strategy: 'walk' })", sandbox);
  assert.equal(runInContext("woptStatus().workers_loading", sandbox), 2);
  workers[3].emit({ kind: "alive", source: "engine" });
  assert.equal(runInContext("woptStatus().workers_active", sandbox), 1);
  assert.equal(runInContext("woptStatus().workers_loading", sandbox), 1);
  workers[3].emit({ kind: "progress", payload: { phase: "searching", sims_done: 3, enumerated: 2 } });
  workers[4].emit({ kind: "progress", payload: { phase: "searching", sims_done: 5, enumerated: 4 } });
  assert.equal(runInContext("woptStatus().sims_done", sandbox), 8);
  workers[3].emit({ kind: "result", payload: { ok: true, results: [] } });
  assert.equal(runInContext("woptStatus().workers_active", sandbox), 1);
  assert.equal(runInContext("woptStatus().sims_done", sandbox), 8);
  workers[4].onerror({ message: "worker error" });
  assert.equal(runInContext("woptStatus().workers_active", sandbox), 0);
  assert.equal(runInContext("woptStatus().phase", sandbox), "error");

  const finals = runInContext("wopt = quickFleet({strategy:'quick'}, 2)", sandbox);
  await drain();
  const [leader, other] = workers.slice(-2);
  leader.emit({kind:"result", payload:{ok:true, verify:["a","b","c"], generation:7, runs:1000}});
  await drain();
  assert.deepEqual(messages.filter(m => m.kind === "verify_final").map(m => m.index), [0, 1]);
  other.emit({kind:"progress", payload:{sims_done:500}});
  assert.equal(finals.status.sims_done, 500);
  other.emit({kind:"result", payload:{ok:true,receipt:"second"}});
  await drain();
  assert.equal(finals.status.sims_done, 1000);
  assert.equal(messages.at(-1).index, 2);
  other.emit({kind:"result", payload:{ok:true,receipt:"third"}});
  leader.emit({kind:"result", payload:{ok:true,receipt:"first"}});
  await drain();
  assert.equal(finals.status.sims_done, 3000);
  assert.equal(messages.at(-1).kind, "finish_finals");
  assert.equal(messages.at(-1).receipts.join(","), "first,second,third");
  leader.emit({kind:"result", payload:{ok:true,final_work:99,results:[]}});
  await drain();
  assert.equal(finals.result.work, 99);

  const failedFinal = runInContext("wopt = quickFleet({strategy:'quick'}, 2)", sandbox);
  await drain();
  const [firstFinal, badFinal] = workers.slice(-2);
  firstFinal.emit({kind:"result", payload:{ok:true, verify:["a","b"], generation:8, runs:2}});
  await drain();
  badFinal.emit({kind:"result", payload:{ok:true}});
  await drain();
  assert.match(failedFinal.result.error, /Missing finalist measurement/);
  assert.equal(failedFinal.workers.length, 0);

  const posted = [];
  let arriving, beat;
  const wasm = Object.assign(async () => {}, { set_heartbeat: (f) => { beat = f; } });
  const workerContext = createContext({ wasm_bindgen: wasm, importScripts() {},
    postMessage: (m) => posted.push(m), performance: { now: () => now },
    setInterval: (f) => { arriving = f; return 1; }, clearInterval() {}, setTimeout });
  runInContext(readFileSync(new URL("../web/src/static/worker.js", import.meta.url), "utf8"), workerContext);
  arriving();
  assert.equal(posted.at(-1).source, "loading");
  await workerContext.onmessage({ data: { kind: "engine", module: {} } });
  await drain();
  assert.equal(typeof beat, "function");
  beat();
  assert.equal(posted.at(-1).source, "engine", "the real worker identifies engine callbacks separately");
  console.log("  ok  optimizer signals distinguish loading, scoring, final simulations and rest without recounting work");
}

async function checkFailures() {
  const source = readFileSync(new URL("../web/src/static/app/08-checkpoint-api.js", import.meta.url), "utf8");
  const unhandled = [];
  const rejected = error => unhandled.push(String(error.message || error));
  process.on("unhandledRejection", rejected);
  try {
    for (const fault of ["partial spawn", "leader post", "scorer post", "rest post", "pace", "null rejection", "rest callback", "rest silence",
      "worker error", "worker silence", "empty batch", "same batch", "incomplete batch", "slow alive"]) {
      let now = 1000, timerId = 0, lead = 0, paced = 0, spawns = 0;
      const timers = new Map(), workers = [], held = [];
      const fail = () => { throw new Error(fault); };
      const sandbox = createContext({
        Date: class extends Date { static now() { return now; } }, performance: { now: () => now },
        setTimeout, clearTimeout, poolSize: () => 2, computeRun: async (_, work, live) => live() ? work() : { cancelled: true },
        setInterval: f => { timers.set(++timerId, f); return timerId; }, clearInterval: id => timers.delete(id),
        spawnEngine: () => {
          if (++spawns === 2 && fault === "partial spawn") fail();
          const worker = { terminated: false, terminate() { this.terminated = true; },
            emit(data) { this.onmessage?.({ data }); },
            postMessage(message) {
              if (message.kind === "rest") {
                if (lead === 2 && fault === "rest post") fail();
                if (lead === 2 && fault === "rest silence") return;
                queueMicrotask(() => this.emit({ kind: "rested", payload: { ok: true } }));
                return;
              }
              const q = message.body.quick_fleet;
              if (q.lead) {
                lead++;
                if (lead === 2 && fault === "leader post") fail();
                const pending = lead === 1 || fault === "same batch" ? ["a", "b"] : fault === "empty batch" ? [] : ["c", "d"];
                queueMicrotask(() => this.emit({ kind: "result", payload: lead > 2
                  ? { ok: true, results: [], final_work: 0 } : { pending } }));
              } else {
                if (lead === 2) {
                  if (fault === "scorer post") fail();
                  if (fault === "worker error") { queueMicrotask(() => this.onerror?.({ message: fault, preventDefault() {} })); return; }
                  if (fault === "worker silence") return;
                  if (fault === "slow alive") { held.push({ worker: this, score: q.score }); return; }
                  if (fault === "incomplete batch") { queueMicrotask(() => this.emit({ kind: "result", payload: { scores: [] } })); return; }
                }
                queueMicrotask(() => this.emit({ kind: "result", payload: { scores: q.score, fights: q.score.length * 10, work: 1 } }));
              }
            } };
          workers.push(worker);
          return worker;
        },
        pace: async () => { if (++paced === 2) { if (fault === "pace") fail(); if (fault === "null rejection") throw null; } },
        rest: () => { if (lead === 2 && fault === "rest callback") fail(); return fault.startsWith("rest") ? 1 : 0; },
      });
      runInContext("let wopt = null, woptNextId = 1;", sandbox);
      runInContext(watch, sandbox);
      runInContext(source, sandbox);
      const job = runInContext("wopt = quickFleet({strategy:'quick'}, 2, pace, {rest, keep:true})", sandbox);
      await new Promise(setImmediate);
      for (let tick = 0; tick < 40; tick++) {
        now += 5000;
        if (fault === "slow alive") workers.forEach(w => w.emit({ kind: "alive", source: "engine" }));
        [...timers.values()].forEach(f => f());
        await Promise.resolve();
      }
      if (fault === "slow alive") {
        assert.equal(job.result, null, "a slow worker with heartbeats must keep computing");
        for (const { worker, score } of held) worker.emit({ kind: "result", payload: { scores: score, fights: score.length * 10, work: 1 } });
        await new Promise(setImmediate);
        assert.equal(runInContext("woptStatus().phase", sandbox), "done");
        assert.equal(job.result.fights, 40);
        assert.equal(job.result.builds, 4);
      } else {
        assert.equal(runInContext("woptStatus().phase", sandbox), "error", `${fault} must settle after completed work`);
        assert.equal(job.result.ok, false);
        assert.match(job.result.error, new RegExp(fault.includes("silence") ? "worker stopped answering" : fault === "null rejection" ? "null" : fault));
        assert.equal(job.status.enumerated, fault === "partial spawn" ? 0 : 2, `${fault} keeps completed build counts`);
        assert.equal(job.status.sims_done, fault === "partial spawn" ? 0 : 20, `${fault} keeps completed fight counts`);
        if (fault === "partial spawn") { assert.equal(spawns, 2); assert.equal(workers.length, 1, "one worker was created before the spawn failure"); }
        assert.equal(job.workers.length, 0);
        assert(workers.every(w => w.terminated), `${fault} terminates every worker`);
        assert.equal(runInContext("fleetKept.length", sandbox), 0, "failed workers must not be reused");
      }
      assert.equal(timers.size, 0, `${fault} clears every watchdog`);
      assert.equal(unhandled.length, 0, `${fault} must not escape as an unhandled rejection`);
    }
  } finally {
    process.removeListener("unhandledRejection", rejected);
  }
  console.log("  ok  quick fleet failures settle after completed work; live slow calls retain their answer and counts");
}

async function checkDeliveries() {
  const source = readFileSync(new URL("../web/src/static/app/08-checkpoint-api.js", import.meta.url), "utf8");
  const flush = () => new Promise(setImmediate);
  for (const outcome of ["saved", "stream save failed", "final save failed", "worker stopped answering"]) {
    let now = 1000, timerId = 0;
    const timers = new Map(), saves = [];
    const worker = { postMessage() {}, terminate() { this.terminated = true; } };
    const emit = data => worker.onmessage({ data });
    const sandbox = createContext({
      Date: class extends Date { static now() { return now; } }, performance: { now: () => now },
      setInterval: (fn, ms) => { timers.set(++timerId, { fn, ms }); return timerId; },
      clearInterval: id => timers.delete(id), poolSize: () => 1, spawnEngine: () => worker,
      computeRun: async (_, work, live) => live() ? work() : { cancelled: true },
      keepOptimizationBuilds: (builds, source) => new Promise((resolve, reject) => {
        assert.equal(source.kind, "public", "this task has build upload enabled");
        saves.push({ build: builds[0], resolve, reject });
      }),
    });
    runInContext("let wopt = null, woptNextId = 1;", sandbox);
    runInContext(watch, sandbox);
    runInContext(source, sandbox);
    runInContext("woptStart({ strategy: 'quick' })", sandbox);
    const job = runInContext("wopt", sandbox);
    const { stall, tick } = runInContext("LANE_WATCHDOG", sandbox);
    await flush();
    assert.equal(timers.size, 1, "the worker is watched while its reply is owed");

    emit({ kind: "builds", payload: ["streamed"] });
    await flush();
    const silent = outcome === "worker stopped answering";
    if (!silent) emit({ kind: "result", payload: { ok: true, results: ["answer"], builds: ["final"] } });
    for (let elapsed = 0; elapsed <= stall; elapsed += tick) {
      now += tick;
      for (const { fn, ms } of timers.values()) {
        assert.equal(ms, tick);
        fn();
      }
    }
    await flush();
    assert.deepEqual(saves.map(save => save.build), ["streamed"], "a pending stream save holds back the final save");

    if (silent) {
      assert.equal(job.result.error, outcome, "a builds message does not complete the worker call");
      assert.equal(job.result.worker_dead, true);
    } else {
      assert.equal(job.result?.error ?? null, null, "a terminal worker reply must not time out during build persistence");
      assert.equal(job.result, null, "the job still waits for durable saves");
      assert.equal(timers.size, 0, "no worker-silence timer remains after the reply");
    }
    if (outcome === "stream save failed") saves[0].reject(new Error(outcome));
    else saves[0].resolve();
    await flush();
    if (outcome === "saved" || outcome === "final save failed") {
      assert.deepEqual(saves.map(save => save.build), ["streamed", "final"]);
      assert.equal(job.result, null, "the job waits for the final save too");
      if (outcome === "saved") saves[1].resolve();
      else saves[1].reject(new Error(outcome));
      await flush();
    }
    assert.equal(job.result.ok, outcome === "saved");
    if (outcome === "saved") assert.deepEqual(job.result.results, ["answer"]);
    else assert.equal(job.result.error, outcome);
    assert.equal(timers.size, 0);
    assert.equal(job.workers.length, 0);
    assert.equal(worker.terminated, true);
  }
  console.log("  ok  quick fleet stops watching on result, waits for ordered saves and reports actual delivery failures");
}

async function checkCancellation() {
  const readApp = file => readFileSync(new URL(`../web/src/static/app/${file}.js`, import.meta.url), "utf8");
  const source = readApp("08-checkpoint-api");
  const boardWork = readApp("69-board-work");
  const rivenGain = boardWork.slice(boardWork.indexOf("const RIVEN_RENEW_MS"), boardWork.indexOf("/// ONE ROW, fought here"));
  const community = readApp("93-community-calculations");
  const communityOptimize = community.slice(community.indexOf("async function communityOptimizationReport("), community.indexOf("const COMMUNITY_CALCULATIONS"));
  const budget = transport.slice(0, transport.indexOf("const computeElapsed"));
  const flush = () => new Promise(setImmediate);

  for (const caller of ["foreground", "riven gain", "community report"]) {
    for (const saving of [true, false]) {
      const label = `${caller} cancels while ${saving ? "saving" : "computing"}`;
      let timerId = 0, giveBacks = 0;
      const watchdogs = new Map(), polls = new Map(), workers = [], saves = [];
      const controller = new AbortController();
      const request = { strategy: "quick" }, buildSource = { kind: "public" };
      const sandbox = createContext({
        Date: class extends Date { static now() { return 1000; } }, performance: { now: () => 1000 },
        setInterval: f => { watchdogs.set(++timerId, f); return timerId; }, clearInterval: id => watchdogs.delete(id),
        setTimeout: f => { polls.set(++timerId, f); return timerId; }, clearTimeout: id => polls.delete(id),
        detectedCores: () => ({ n: 1 }), poolSize: () => 1, communityLanes: () => 1, readerBusy: () => false,
        computeNow: null, boardVerifyOn: () => true, computeHeld: () => false, computeFrozen: false,
        computeStart() {}, computeProgress() {}, computeEnd() {}, giveBack() { giveBacks++; },
        request, buildSource, signal: controller.signal, communityOptimizationBuildSource: async () => buildSource,
        keepOptimizationBuilds: () => new Promise((resolve, reject) => saves.push({ resolve, reject })),
        spawnEngine: () => {
          const worker = { requests: 0, terminations: 0, postMessage() { this.requests++; }, terminate() { this.terminations++; } };
          workers.push(worker);
          return worker;
        },
      });
      runInContext("let wopt = null, woptNextId = 1, heldLease = null, computeBeats = 0, observedJob;", sandbox);
      runInContext(budget, sandbox);
      runInContext("computeRanges = { solo: { min: 100, max: 100 }, together: { min: 100, max: 100 } };", sandbox);
      runInContext(watch, sandbox);
      runInContext(source, sandbox);
      runInContext(rivenGain, sandbox);
      runInContext(communityOptimize, sandbox);
      runInContext("const startFleet = quickFleet; quickFleet = (...args) => observedJob = startFleet(...args);", sandbox);
      const start = { foreground: "woptStart(request)", "riven gain": "rivenGainOnce({ request, code: 'test' }, 'verifier')",
        "community report": "communityOptimizationReport(request, { signal, buildSource })" }[caller];
      const completion = Promise.resolve(runInContext(start, sandbox)).then(value => ({ value }), error => ({ error }));
      const running = runInContext("computeBudget.running", sandbox);
      const occupied = () => running.solo + running.together;
      await flush();
      const job = runInContext("observedJob", sandbox), [worker] = workers;
      assert.equal(worker.requests, 1, label);
      assert.equal(occupied(), 1, label);
      const message = worker.onmessage, error = worker.onerror;
      if (saving) message({ data: { kind: "result", payload: { ok: true, builds: ["final"] } } });
      await flush();
      assert.equal(saves.length, saving ? 1 : 0, label);
      assert.equal(watchdogs.size, saving ? 0 : 1, label);
      assert.equal(occupied(), 1, "the call holds a slot until its reply and save settle");

      if (caller === "foreground") {
        runInContext("woptCancel(); woptCancel();", sandbox);
      } else {
        if (caller === "riven gain") sandbox.computeFrozen = true;
        else controller.abort();
        for (const [id, poll] of polls) { polls.delete(id); poll(); }
        const { value, error } = await completion;
        if (caller === "riven gain") { assert.equal(value, true); assert.equal(giveBacks, 1); }
        else assert.equal(error?.message, "Cancelled");
      }
      await flush();
      assert.equal(occupied(), 0, `${label} must return the slot without waiting for the save or watchdog`);
      assert.equal(job.cancelled, true, label);
      assert.equal(job.result, null, label);
      assert.equal(job.workers.length, 0, label);
      assert.equal(worker.terminations, 1, label);
      assert.equal(watchdogs.size, 0, label);
      assert.equal(runInContext("fleetKept.length", sandbox), 0, label);
      if (caller === "foreground") assert.equal(runInContext("woptStatus().phase", sandbox), "cancelled");

      runInContext("woptStart(request)", sandbox);
      await flush();
      assert.equal(workers[1].requests, 1, "a new search starts before the cancelled save settles");
      const cancelledStatus = job.status;
      message({ data: { kind: "result", payload: { ok: true, builds: ["late"] } } });
      error({ message: "late worker error" });
      if (saving) {
        if (caller === "riven gain") saves[0].resolve();
        else saves[0].reject(new Error("late save error"));
      }
      await flush();
      assert.equal(saves.length, saving ? 1 : 0, "late replies do not start another save");
      assert.equal(job.status, cancelledStatus, "late replies and failures do not update the cancelled job");
      assert.equal(job.result, null, label);
      workers[1].onmessage({ data: { kind: "result", payload: { ok: true } } });
      await flush();
      assert.equal(runInContext("woptStatus().phase", sandbox), "done");
      assert.equal(occupied(), 0, label);
      assert.equal(watchdogs.size, 0, label);
    }
  }
  console.log("  ok  foreground and community quick fleet cancellation return the compute slot and ignore late replies and failures");
}

await checkSignals();
await checkFailures();
await checkDeliveries();
await checkCancellation();
if (process.argv.includes("--signals-only")) process.exit(0);

// A FROZEN SURVEY QUESTION, cut short: its fight shorter and fewer runs and starts.
const REQUEST = {"weapon":"furis","rivens":[{"id":"rmv0r8x1z402","name":"riven 1","spec":{"bonuses":[{"id":"cold","roll":1.1},{"id":"critical_chance","roll":1.1}],"malus":null,"rank":8,"polarity":"madurai","shape":"2","drafts":{"2":{"bonuses":[{"id":"cold","roll":1.1},{"id":"critical_chance","roll":1.1}],"malus":null},"3":{"bonuses":[{"id":null,"roll":1},{"id":null,"roll":1},{"id":null,"roll":1}],"malus":null},"3+1":{"bonuses":[{"id":null,"roll":1},{"id":null,"roll":1},{"id":null,"roll":1}],"malus":{"id":null,"roll":1}},"2+1":{"bonuses":[{"id":null,"roll":1},{"id":null,"roll":1}],"malus":{"id":null,"roll":1}}}}}],"every_rank":{"arcanes":[],"mods":["hunter_track","continuous_misery","lingering_torment","perpetual_agony","augur_seeker","lasting_sting"]},"mode":"cycle","valence_element":"","valence_bonus":0,"stance":"","enemy":"thrax_centurion","level":9999,"steel_path":true,"guardian_aura":false,"ancient_protector_aura":false,"eximus":null,"squad_size":1,"headshot_pct":100,"aiming":true,"player_at":[0,0],"target_at":[0,0.5],"formation":[],"also_acting":[],"aim_at":null,"invisible":false,"airborne":false,"sliding":false,"aim_gliding":false,"overshields":false,"channeling":false,"melee_equipped":true,"solo_weapon":false,"buff_triggers_off":[],"frame":"","infinite_ammo":true,"metric":"kpm","duration":20,"buffs":{},"ability_strength":null,"abilities":[],"apl":[],"extra_stats":{},"auras":[],"shards":[],"seed":12648430,"spectral_form":false,"wf_energy_pct":1,"__weapon":"furis","runs":4,"custom_enemies":[],"finalists":1,"strategy":"quick","starts":[{"slots":["riven:rmv0r8x1z402","scorch",null,null,null,null,null,null,null,null],"evolutions":["furis_evo1_incarnon_form","furis_haven_foray","furis_practiced_grip","furis_headcracker"],"arcane":["none"],"arcane_rank":[null],"mode":null,"valence_element":null,"fixed":[{"kind":"mods","idx":0}]},{"slots":["riven:rmv0r8x1z402","frostbite",null,null,null,null,null,null,null,null],"evolutions":["furis_evo1_incarnon_form","furis_haven_foray","furis_practiced_grip","furis_headcracker"],"arcane":["none"],"arcane_rank":[null],"mode":null,"valence_element":null,"fixed":[{"kind":"mods","idx":0}]}],"candidate_runs":2,"limits":{"exclude":{"mods":[],"arcanes":[],"evolutions":[],"modes":[],"valence":[]},"mods":8,"exilus":true,"arcane_seats":[true]}};

// Two pinned single-card starts cannot converge to the same build.
const FINALS_REQUEST = {
  weapon: "braton", enemy: "corrupted_heavy_gunner", level: 100, duration: 1,
  metric: "dps", infinite_ammo: true, headshot_pct: 0, seed: 0x5eed,
  strategy: "quick", build_size: 1, build_min: 1, candidate_runs: 2, runs: 4,
  finalists: 2, screen_ratio: 0, mods: { serration: "search", point_strike: "search" },
  arcanes: { none: "fixed" }, every_rank: { mods: [], arcanes: [] },
  limits: { mods: 1, exilus: false, arcane_seats: [false] },
  starts: ["serration", "point_strike"].map(id => ({
    slots: [id, null, null, null, null, null, null, null, null, null],
    evolutions: [], arcane: ["none"], arcane_rank: [null],
    fixed: [{ kind: "mods", idx: 0, placement: "slot" }],
  })),
};

const app = await openApp({ boot: 13000, base: process.env.WFSIM_BASE });
try {
  const { evaluate, check } = app;
  const finals = await evaluate(`(async () => {
    const body = ${JSON.stringify(FINALS_REQUEST)};
    const run = async (count, policy) => {
      const job = quickFleet(body, count);
      const workers = job.workers.slice(), wire = [], cleanup = [];
      for (const [index, worker] of workers.entries()) {
        const send = worker.postMessage;
        let pending = null;
        const observe = event => {
          if (event.data.kind !== "result" || !pending) return;
          const payload = event.data.payload || {};
          pending.reply = { ok: payload.ok, error: payload.error, receipt: payload.receipt,
            verify: payload.verify, generation: payload.generation, runs: payload.runs };
          pending = null;
        };
        // Observe the actual worker result before quickFleet handles its completion.
        worker.addEventListener("message", observe, true);
        worker.postMessage = function (message, ...transfer) {
          let sent = message;
          if (["optimize", "verify_final", "finish_finals", "api"].includes(message.kind)) {
            const call = { worker: index, kind: message.kind, path: message.path,
              index: message.index, generation: message.generation, replay: message.replay,
              lead: message.body?.quick_fleet?.lead === true };
            if (message.kind === "finish_finals") {
              const receipts = message.receipts.slice();
              call.before = receipts.map(receipt => JSON.parse(receipt)[1]);
              if (policy === "reverse" || policy === "missing") receipts.reverse();
              if (policy === "missing") receipts.pop();
              sent = { ...message, receipts };
              call.receipts = receipts;
            }
            wire.push(call);
            pending = call;
          }
          return send.call(worker, sent, ...transfer);
        };
        cleanup.push(() => {
          worker.postMessage = send;
          worker.removeEventListener("message", observe, true);
        });
      }
      try {
        const deadline = performance.now() + 45000;
        while (!job.result && performance.now() < deadline) await new Promise(resolve => setTimeout(resolve, 20));
        if (!job.result) throw Error("bounded finalist fixture did not finish");
        const result = job.result;
        return { ok: result.ok !== false, error: result.error,
          answer: { results: result.results, start_results: result.start_results, final_work: result.final_work }, wire };
      } finally {
        cleanup.forEach(restore => restore());
        job.cancel();
      }
    };
    return { serial: await run(1), split: await run(2, "reverse"), missing: await run(2, "missing") };
  })()`, 150000);

  const calls = (run, kind) => run.wire.filter(call => call.kind === kind);
  assert.equal(finals.serial.ok, true, finals.serial.error);
  assert.equal(finals.serial.answer.results.length, 2, "fixture must retain two distinct finalists");
  assert.equal(finals.serial.answer.start_results.length, 2, "both pinned starts must be reported");
  const replays = finals.serial.answer.results.map(row => JSON.stringify(row.replay));
  assert.equal(new Set(replays).size, 2, "the finalists must carry distinct complete replay identities");
  assert.notEqual(finals.serial.answer.results[0].dps, finals.serial.answer.results[1].dps,
    "the two scores must distinguish a misplaced receipt");
  assert(finals.serial.answer.final_work > 0, "the serial oracle must measure its finalists");
  assert.equal(calls(finals.serial, "verify_final").length, 0, "one worker is the serial final-round oracle");
  assert.equal(calls(finals.serial, "finish_finals").length, 0);

  for (const run of [finals.split, finals.missing]) {
    const verify = calls(run, "verify_final"), finish = calls(run, "finish_finals");
    assert.equal(verify.length, 2, "the actual WASM lane path must verify both finalists");
    assert.equal(new Set(verify.map(call => call.worker)).size, 2, "both actual workers must measure a finalist");
    const dispatch = run.wire.find(call => Array.isArray(call.reply?.verify));
    assert(dispatch?.lead && dispatch.reply.verify.length === 2, "the actual leader must dispatch two replays");
    for (const call of verify) {
      assert.equal(call.generation, dispatch.reply.generation);
      assert.equal(call.replay, dispatch.reply.verify[call.index]);
      assert.equal(call.reply?.ok, true, "verify_final must return an actual WASM receipt");
      const [generation, index, score] = JSON.parse(call.reply.receipt);
      assert.equal(generation, call.generation);
      assert.equal(index, call.index);
      assert(Array.isArray(score.Measured) && score.Measured.length === 7, "legal finalists must actually be measured");
    }
    assert.deepEqual(verify.map(call => JSON.stringify(JSON.parse(call.replay))).sort(), replays.slice().sort());
    assert.equal(finish.length, 1, "the actual leader must execute finish_finals exactly once");
    assert.equal(finish[0].worker, dispatch.worker);
    assert.equal(finish[0].generation, dispatch.reply.generation);
    assert.deepEqual(finish[0].before, [0, 1], "the page assembles receipts by candidate index");
  }
  assert.equal(finals.split.ok, true, finals.split.error);
  const finish = calls(finals.split, "finish_finals")[0];
  assert.deepEqual(finish.receipts.map(receipt => JSON.parse(receipt)[1]), [1, 0],
    "the real WASM collector must restore intentionally reversed receipt order");
  assert.equal(finish.reply?.ok, true, "finish_finals must return the measured report");
  assert.deepEqual(finals.split.answer, finals.serial.answer,
    "all ordered results, start results, replay identities and final work must equal the serial oracle");
  assert.equal(calls(finals.missing, "finish_finals")[0].receipts.length, 1);
  assert.equal(calls(finals.missing, "finish_finals")[0].reply?.ok, false);
  assert.equal(finals.missing.ok, false, "an actual missing receipt must fail the whole final round");
  assert.match(finals.missing.error, /invalid or incomplete finalist receipts/);
  console.log("  ok  actual WASM finalist dispatch, ordered replay reports, reversed receipts and missing-receipt rejection");

  const r = await evaluate(`(async () => {
    const body = ${JSON.stringify(REQUEST)};
    const run = async (n, opts) => {
      const job = quickFleet(body, n, async () => {}, opts);
      while (!job.result) await new Promise((ok) => setTimeout(ok, 200));
      const best = (job.result.results || [])[0] || {};
      return { ok: job.result.ok !== false, error: job.result.error || null, work: job.result.work,
        kill: best.kill_progress, mods: JSON.stringify(best.mods), arcanes: JSON.stringify(best.arcanes), workers: job.workers.length };
    };
    const one = await run(1);
    const three = await run(3, { keep: true });
    const kept = fleetKept.length;
    let flip = 0;
    const changing = await run(() => (flip++ % 2 ? 4 : 1), { keep: true });
    const keptAfter = fleetKept.length;
    const rested = await run(2, { rest: (ms) => Math.min(20, ms) });
    return { one, three, kept, changing, keptAfter, rested };
  })()`, 600000);

  const same = (a, b) => a.ok && b.ok && a.work === b.work && a.kill === b.kill && a.mods === b.mods && a.arcanes === b.arcanes;
  check("the search finds a build", r.one.ok && r.one.mods && r.one.mods !== "[]" && r.one.work > 0, JSON.stringify(r.one));
  check("three workers give the same build, number and work as one", same(r.one, r.three), JSON.stringify([r.one, r.three]));
  check("...and a count that changes every round gives them too", same(r.one, r.changing), JSON.stringify([r.one, r.changing]));
  check("...and so do workers resting between calls, as a share below 100% has them", same(r.one, r.rested), JSON.stringify([r.one, r.rested]));
  check("a background search hands its workers back for the next one", r.kept === 3 && r.keptAfter >= 3, JSON.stringify(r));
  await app.finish("a quick search answers the same on any number of cores, and a volunteer's next one starts warm");
} finally {
  await app.close();
}
