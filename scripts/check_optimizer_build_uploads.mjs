import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { webcrypto } from "node:crypto";
import vm from "node:vm";

const read = name => readFileSync(new URL("../web/src/static/" + name, import.meta.url), "utf8");
let storeSource = read("app/53-task-store.js"), taskSource = read("app/57-optimization-tasks.js");
let appraisalSource = read("app/63-riven-tasks.js");
let fleetSource = read("app/08-checkpoint-api.js");
const negative = process.env.WFSIM_BUILD_UPLOAD_NEGATIVE;
if (negative === "unbounded-batch") storeSource = storeSource.replace("takeValues(prefix, 128)", "takeValues(prefix, 1000000)");
else if (negative === "early-ack") taskSource = taskSource.replace("await keepOptimizationBuilds(status.builds || [], link.source);",
  "await queue.store.value(link.key, { ...link, cursor: Number(status.builds_next) || 0 });\n    await keepOptimizationBuilds(status.builds || [], link.source);");
else if (negative === "late-consent") appraisalSource = appraisalSource.replace("board_upload: boardUpload", "board_upload: boardConsent() === 'yes'");
else if (negative === "delayed-error") fleetSource = fleetSource.replace("job.result = job.parts[i];", "job.result = job.parts.every(Boolean) ? job.parts[i] : null;");
else if (negative === "no-coalescing") storeSource = storeSource.replace("await this.coalesce();", "");
else if (negative) throw new Error("Unknown negative control: " + negative);
const plain = value => JSON.parse(JSON.stringify(value));
const gate = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };
const locks = () => {
  const held = new Map();
  return (key, work) => {
    const next = (held.get(key) || Promise.resolve()).catch(() => {}).then(work);
    held.set(key, next); return next;
  };
};
const context = vm.createContext({ crypto: webcrypto, TextEncoder, AbortController, setTimeout, clearTimeout, console });
vm.runInContext(storeSource + "\nglobalThis.classes = { UserTaskMemoryStore, OptimizationBuildOutbox };", context);
const { UserTaskMemoryStore, OptimizationBuildOutbox } = context.classes;
const build = (id, extra = {}) => ({ weapon: "braton_prime", mods: ["serration", `card_${id}`], arcanes: ["primary_merciless@2"], ...extra });
function fixture({ store = new UserTaskMemoryStore(), enabled = true, send, lock = locks(), coalesce = async () => {} } = {}) {
  const calls = [], errors = [], timers = [];
  const state = { enabled };
  const outbox = new OptimizationBuildOutbox({ store, lock, engine: "engine-a", consent: () => state.enabled, coalesce,
    send: async (source, rows) => { calls.push({ source: plain(source), rows: plain(rows) }); return send ? send(source, rows) : rows.map(() => ({ ok: true })); },
    failed: error => errors.push(error.message), schedule: (work, ms) => { const item = { work, ms }; timers.push(item); return item; } });
  return { outbox, store, state, calls, errors, timers };
}
async function idle(outbox) { while (outbox.running) await outbox.running; }
async function until(test) {
  for (let i = 0; i < 200; i++) { if (test()) return; await new Promise(resolve => setImmediate(resolve)); }
  assert.fail("the expected asynchronous step did not happen");
}
let passed = 0;
async function check(name, work) { await work(); passed++; console.log(`  ok  ${name}`); }

await check("all element orders survive dedup while promotions and property order collapse", async () => {
  const f = fixture({ enabled: false });
  const a = build(1), same = { arcanes: a.arcanes, mods: a.mods, weapon: a.weapon };
  const reversed = { ...a, mods: [...a.mods].reverse() };
  await f.outbox.add([a, same, reversed, build(2)], { kind: "public" }); await idle(f.outbox);
  assert.equal((await f.store.takeValues(f.outbox.prefix, 100)).length, 3);
  assert.equal(f.calls.length, 0, "declining upload must leave the network unused");
  f.state.enabled = true; f.outbox.wake(); await idle(f.outbox);
  assert.equal(f.calls.flatMap(call => call.rows).length, 3);
  await f.outbox.add([same, reversed, build(2)], { kind: "public" }); await idle(f.outbox);
  assert.equal(f.calls.length, 1, "settled identities remain deduplicated");
});

await check("records and trusted ownership are frozen before asynchronous storage", async () => {
  const f = fixture({ enabled: false }), row = build(3), source = { kind: "community", harvest: { receipt: "original" } };
  const saving = f.outbox.add([row], source);
  row.mods[0] = "edited"; source.harvest.receipt = "other";
  await saving; await idle(f.outbox);
  const [held] = await f.store.takeValues(f.outbox.prefix, 10);
  assert.equal(held.value.build.mods[0], "serration");
  assert.equal(held.value.source.harvest.receipt, "original");
});

await check("network delivery never blocks storage or runs two requests at once", async () => {
  const remote = gate(); let active = 0, peak = 0, first = true;
  const f = fixture({ send: async (_, rows) => {
    active++; peak = Math.max(peak, active);
    if (first) { first = false; await remote.promise; }
    active--; return rows.map(() => ({ ok: true }));
  } });
  await f.outbox.add([build(4)]); await until(() => f.calls.length === 1);
  await f.outbox.add([build(5)]);
  assert.equal((await f.store.takeValues(f.outbox.prefix, 10)).length, 2);
  assert.equal(f.calls.length, 1);
  remote.resolve(); await idle(f.outbox);
  assert.equal(peak, 1); assert.equal(f.calls.flatMap(call => call.rows).length, 2);
});

await check("sparse arrivals share one background window while storage remains independent", async () => {
  const window = gate(); let windows = 0;
  const f = fixture({ coalesce: async () => { windows++; await window.promise; } });
  await f.outbox.add([build(30)]);
  await until(() => windows || f.calls.length);
  assert.equal(f.calls.length, 0, "the first sparse arrival must wait for its background batch window");
  await f.outbox.add([build(31)]); await f.outbox.add([build(32)]);
  assert.equal((await f.store.takeValues(f.outbox.prefix, 128)).length, 3, "storage must not await the batch window");
  assert.equal(windows, 1); assert.equal(f.calls.length, 0);
  window.resolve(); await idle(f.outbox);
  assert.equal(f.calls.length, 1); assert.equal(f.calls[0].rows.length, 3);
});

await check("full batches bypass coalescing and only their sparse tail waits", async () => {
  const window = gate(); let windows = 0;
  const f = fixture({ coalesce: async () => { windows++; await window.promise; } });
  await f.outbox.add(Array.from({ length: 257 }, (_, i) => build(i)));
  await until(() => windows || f.calls.length > 2);
  assert.deepEqual(f.calls.map(call => call.rows.length), [128, 128]);
  assert.equal(windows, 1);
  window.resolve(); await idle(f.outbox);
  assert.deepEqual(f.calls.map(call => call.rows.length), [128, 128, 1]);
});

await check("declining upload during coalescing pauses local records before HTTP", async () => {
  const window = gate(); let waiting = false;
  const f = fixture({ coalesce: async () => { waiting = true; await window.promise; } });
  await f.outbox.add([build(33)]); await until(() => waiting);
  f.state.enabled = false; window.resolve(); await idle(f.outbox);
  assert.equal(f.calls.length, 0); assert.equal(await f.store.countValues(f.outbox.prefix), 1);
  f.state.enabled = true; f.outbox.wake(); await idle(f.outbox);
  assert.equal(f.calls.length, 1);
});

await check("uncapped totals are split by both item count and UTF-8 request size", async () => {
  for (const extra of [{}, { padding: "紫".repeat(1000) }]) {
    const f = fixture();
    await f.outbox.add(Array.from({ length: 513 }, (_, i) => build(i, extra)));
    await idle(f.outbox);
    assert.equal(f.calls.flatMap(call => call.rows).length, 513);
    assert(f.calls.length > 4);
    for (const call of f.calls) {
      assert(call.rows.length <= 128);
      assert(Buffer.byteLength(JSON.stringify({ builds: call.rows })) <= 131072);
      assert(call.rows.every(row => row.arcanes[0] === "primary_merciless@2"));
    }
  }
});

await check("reload and task cancellation preserve queued payloads and their dedup markers", async () => {
  const f = fixture({ enabled: false });
  await f.outbox.add([build(6), build(7)]); await idle(f.outbox);
  await f.store.change(state => { state.tasks = []; });
  const reloaded = fixture({ store: f.store });
  reloaded.outbox.wake(); await idle(reloaded.outbox);
  assert.equal(reloaded.calls.flatMap(call => call.rows).length, 2);
  await reloaded.outbox.add([build(6), build(7)]); await idle(reloaded.outbox);
  assert.equal(reloaded.calls.length, 1);
});

await check("failed batches retry durably while mixed permanent refusals settle independently", async () => {
  let phase = 0;
  const f = fixture({ send: async (_, rows) => {
    phase++;
    if (phase === 1) throw new Error("503");
    if (phase === 2) return [{ ok: true }, { ok: false, error: "refused", retryable: false }, { ok: false, retryable: true }];
    return rows.map(() => ({ ok: true }));
  } });
  await f.outbox.add([build(8), build(9), build(10)]); await idle(f.outbox);
  assert.equal((await f.store.takeValues(f.outbox.prefix, 10)).length, 3);
  assert.equal(f.timers.length, 1); f.timers.shift().work(); await idle(f.outbox);
  assert.equal((await f.store.takeValues(f.outbox.prefix, 10)).length, 1);
  assert(f.errors.includes("refused"));
  const [failed] = await f.store.entries("optimization-build-refused:");
  assert.equal(failed.error, "refused"); assert.equal(failed.build.weapon, "braton_prime");
  assert.equal(await f.store.countValues("optimization-build-refused:"), 1);
  f.timers.shift().work(); await idle(f.outbox);
  assert.equal((await f.store.takeValues(f.outbox.prefix, 10)).length, 0);
  assert.equal(f.calls[2].rows.length, 1, "successful and refused entries must not be retried");
});

await check("community receipts survive opt-out without impersonating the computing account", async () => {
  const f = fixture({ enabled: false });
  const source = { kind: "community", explicit: true, harvest: { receipt: "signed-asker", question: "q", engine: "engine-a" } };
  await f.outbox.add([build(11)], source); await idle(f.outbox);
  assert.equal(f.calls.length, 1);
  assert.deepEqual(plain(f.outbox.envelope(source)), { harvest: source.harvest });
  assert(!JSON.stringify(f.calls[0]).includes("initiator"));
});

await check("fleet slices persist every returned build before advancing and retain late deliveries after cancel", async () => {
  const saved = [], durable = gate(), workers = []; let leads = 0;
  class Worker {
    postMessage(message) {
      if (this.dead) return;
      queueMicrotask(() => {
        const fleet = message.body.quick_fleet;
        let payload;
        if (fleet.lead) {
          leads++;
          payload = leads === 1 ? { ok: true, pending: [[1]], progress: [] }
            : { ok: true, results: [{ mods: ["ranked"] }], final_work: 1 };
        } else payload = { ok: true, scores: [{ key: "one", score: [1, 1] }], builds: [build(12), build(13)], fights: 2, work: 5 };
        this.onmessage?.({ data: { kind: "result", payload } });
      });
    }
    terminate() { this.dead = true; }
  }
  const scope = vm.createContext({ console, performance, Date, setTimeout, clearTimeout,
    watchSilence: () => ({ heard() {}, start() {}, clear() {} }),
    spawnEngine: () => { const worker = new Worker(); workers.push(worker); return worker; } });
  vm.runInContext("let wopt = null, woptNextId = 1;\n" + fleetSource + "\nglobalThis.quickFleetTest = quickFleet;", scope);
  const job = scope.quickFleetTest({ strategy: "quick" }, 1, undefined, { onBuilds: async rows => {
    await durable.promise; saved.push(...plain(rows));
  } });
  await until(() => leads === 1 && workers.length === 1 && job.status.batch_slices_total > 0);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(leads, 1, "the next leader call must wait for the local commit");
  job.cancelled = true; workers.forEach(worker => worker.terminate()); durable.resolve();
  await until(() => saved.length === 2);
  assert.equal(job.result, null); assert.equal(leads, 1);
});

await check("native cursor ACK follows local commit and cancellation pages remain recoverable", async () => {
  const store = new UserTaskMemoryStore(), durable = gate(), requests = [], saved = [];
  store.persistent = true;
  const scope = vm.createContext({ crypto: webcrypto, console, TextEncoder, AbortController, setTimeout, clearTimeout,
    window: { addEventListener() {} }, WASM: false, userTaskLock: locks(), initUserTasks: async () => ({ store }),
    userTaskFailed: () => {}, keepOptimizationBuilds: async rows => { if (rows.length) await durable.promise; saved.push(...plain(rows)); },
    api: async (_, request) => { requests.push(plain(request)); return requests.length === 1
      ? { ok: true, job_token: "original-job", phase: "cancelled", builds: [build(14), build(15)], builds_next: 2, builds_more: false }
      : { ok: true, job_token: "original-job", phase: "cancelled", builds: [], builds_next: 2, builds_more: false }; } });
  vm.runInContext(storeSource + "\n" + taskSource + "\nglobalThis.readNative = nativeOptimizationStatus;", scope);
  const link = { key: "optimization-build-source:test", id: 7, token: "original-job", cursor: 0, source: { kind: "public" } };
  await store.value(link.key, link);
  const pending = scope.readNative(link); await until(() => requests.length === 1);
  assert.equal((await store.value(link.key)).cursor, 0, "a page must not ACK uncommitted records");
  durable.resolve(); await pending;
  assert.equal((await store.value(link.key)).cursor, 2); assert.equal(saved.length, 2);
  await scope.readNative(link);
  assert.equal(requests[1].builds_after, 2); assert.equal(requests[1].job_token, "original-job");
  assert.equal(await store.value(link.key), null);
});

await check("a cancelled long-running worker keeps its build delivery without restoring a result", async () => {
  const durable = gate(), saved = [];
  class Worker {
    postMessage() {
      queueMicrotask(() => {
        this.onmessage({ data: { kind: "builds", payload: [build(16)] } });
        this.onmessage({ data: { kind: "result", payload: { ok: true, results: [{ mods: ["ranked"] }] } } });
      });
    }
    terminate() {}
  }
  const scope = vm.createContext({ console, performance, Date, setTimeout, clearTimeout, poolSize: () => 1,
    localStorage: { removeItem() {} }, spawnEngine: () => new Worker(),
    watchSilence: () => ({ heard() {}, start() {}, clear() {}, done() {} }) });
  vm.runInContext("let wopt = null, woptNextId = 1;\n" + fleetSource
    + "\nglobalThis.longWorker = { start: woptStart, cancel: woptCancel, job: () => wopt };", scope);
  scope.longWorker.start({ strategy: "exhaust" }, null, null, async rows => { await durable.promise; saved.push(...plain(rows)); });
  await new Promise(resolve => setImmediate(resolve));
  const job = scope.longWorker.job(); scope.longWorker.cancel(); durable.resolve();
  await until(() => saved.length === 1); await new Promise(resolve => setImmediate(resolve));
  assert.equal(job.cancelled, true); assert.equal(job.result, null);
});

await check("worker failure settles before pending local commits and clears every watchdog", async () => {
  for (const fault of ["worker", "watchdog", "result"]) {
    const durable = gate(), entered = gate(), workers = [], watchdogs = []; let saved = 0;
    const scope = vm.createContext({ console, performance, Date, setTimeout, clearTimeout, poolSize: () => 2,
      localStorage: { removeItem() {} }, spawnEngine: () => {
        const worker = { dead: false, postMessage() {}, terminate() { this.dead = true; }, emit(data) { this.onmessage({ data }); } };
        workers.push(worker); return worker;
      },
      watchSilence: expired => {
        const wd = { cleared: false, expired, heard() {}, start() {}, clear() { this.cleared = true; }, done() {} };
        watchdogs.push(wd); return wd;
      } });
    vm.runInContext("let wopt = null, woptNextId = 1;\n" + fleetSource
      + "\nglobalThis.longWorker = { start: woptStart, status: woptStatus, job: () => wopt };", scope);
    scope.longWorker.start({ strategy: "walk" }, null, null, async rows => {
      entered.resolve(); await durable.promise; saved += rows.length;
    });
    workers[0].emit({ kind: "builds", payload: [build(20)] });
    workers[0].emit({ kind: "result", payload: { ok: true, results: [] } });
    await entered.promise;
    assert.equal(scope.longWorker.job().parts[0], null, "the healthy shard is awaiting the local commit");
    if (fault === "worker") workers[1].onerror({ message: "worker error", preventDefault() {} });
    else if (fault === "result") workers[1].emit({ kind: "result", payload: { ok: false, error: "engine refused", builds: [build(21)] } });
    else watchdogs[1].expired();
    assert.equal(scope.longWorker.status().phase, "error", "an engine failure cannot wait for unrelated persistence");
    assert.equal(scope.longWorker.job().workers.length, 0);
    assert(workers.every(worker => worker.dead)); assert(watchdogs.every(wd => wd.cleared));
    assert.equal(saved, 0);
    durable.resolve(); await until(() => saved === (fault === "result" ? 2 : 1)); await new Promise(resolve => setImmediate(resolve));
    assert.equal(scope.longWorker.status().phase, "error", "late persistence must not overwrite the failed search");
  }
});

await check("a quick scorer's refusal does not wait for its returned builds to persist", async () => {
  const durable = gate(), workers = []; let saved = 0;
  const scope = vm.createContext({ console, performance, Date, setTimeout, clearTimeout,
    watchSilence: () => ({ heard() {}, start() {}, clear() {} }), spawnEngine: () => {
      const worker = { dead: false, terminate() { this.dead = true; }, postMessage(message) {
        const payload = message.body.quick_fleet.lead ? { pending: [[1]] }
          : { ok: false, error: "engine refused", builds: [build(22)] };
        queueMicrotask(() => this.onmessage({ data: { kind: "result", payload } }));
      } };
      workers.push(worker); return worker;
    } });
  vm.runInContext("let wopt = null, woptNextId = 1;\n" + fleetSource + "\nglobalThis.quickTest = quickFleet;", scope);
  const job = scope.quickTest({ strategy: "quick" }, 1, undefined, { onBuilds: async rows => { await durable.promise; saved += rows.length; } });
  await until(() => job.result?.ok === false);
  assert.equal(job.result.error, "engine refused"); assert.equal(saved, 0);
  assert.equal(job.workers.length, 0); assert(workers.every(worker => worker.dead));
  durable.resolve(); await until(() => saved === 1);
  assert.equal(job.result.error, "engine refused");
});

await check("long-worker setup and post failures stop every created worker", async () => {
  for (const fault of ["spawn", "post"]) {
    const workers = [], watchdogs = [];
    const scope = vm.createContext({ console, performance, Date, setTimeout, clearTimeout, poolSize: () => 2,
      spawnEngine: () => {
        if (fault === "spawn" && workers.length === 1) throw new Error("spawn failed");
        const worker = { dead: false, postMessage() { if (fault === "post") throw new Error("post failed"); }, terminate() { this.dead = true; } };
        workers.push(worker); return worker;
      }, watchSilence: () => {
        const wd = { cleared: false, heard() {}, start() {}, clear() { this.cleared = true; }, done() {} };
        watchdogs.push(wd); return wd;
      } });
    vm.runInContext("let wopt = null, woptNextId = 1;\n" + fleetSource
      + "\nglobalThis.longWorker = { start: woptStart, status: woptStatus, job: () => wopt };", scope);
    scope.longWorker.start({ strategy: "walk" });
    assert.equal(scope.longWorker.status().phase, "error");
    assert.equal(scope.longWorker.job().workers.length, 0);
    assert.equal(workers.length, 1); assert(workers.every(worker => worker.dead)); assert(watchdogs.every(wd => wd.cleared));
  }
});

await check("a completed shard's late error cannot stop the remaining healthy worker", async () => {
  const workers = [];
  const scope = vm.createContext({ console, performance, Date, setTimeout, clearTimeout, poolSize: () => 2,
    localStorage: { removeItem() {} }, watchSilence: () => ({ heard() {}, start() {}, clear() {}, done() {} }),
    spawnEngine: () => {
      const worker = { postMessage() {}, terminate() {}, emit(data) { this.onmessage({ data }); } };
      workers.push(worker); return worker;
    } });
  vm.runInContext("let wopt = null, woptNextId = 1;\n" + fleetSource
    + "\nglobalThis.longWorker = { start: woptStart, status: woptStatus };", scope);
  scope.longWorker.start({ strategy: "walk" });
  workers[1].emit({ kind: "alive", source: "engine" });
  workers[0].emit({ kind: "result", payload: { ok: true, results: [] } });
  workers[0].onerror({ message: "late error", preventDefault() {} });
  assert.equal(scope.longWorker.status().workers_active, 1);
  assert.notEqual(scope.longWorker.status().phase, "error");
  workers[1].emit({ kind: "result", payload: { ok: true, results: [] } });
  assert.equal(scope.longWorker.status().phase, "done");
});

await check("a restarted native server cannot receive an old job cursor for a reused id", async () => {
  const store = new UserTaskMemoryStore(), requests = []; let acknowledged = 0, saved = 0;
  store.persistent = true;
  const scope = vm.createContext({ crypto: webcrypto, console, TextEncoder, AbortController, setTimeout, clearTimeout,
    window: { addEventListener() {} }, WASM: false, userTaskLock: locks(), initUserTasks: async () => ({ store }),
    userTaskFailed: () => {}, keepOptimizationBuilds: async () => { saved++; },
    api: async (_, request) => {
      requests.push(plain(request));
      if (request.job_token !== "restarted-job") return { ok: false, error: "optimization job no longer matches its captured token" };
      acknowledged = request.builds_after; return { ok: true, job_token: "restarted-job", phase: "running", builds: [] };
    } });
  vm.runInContext(storeSource + "\n" + taskSource + "\nglobalThis.readNative = nativeOptimizationStatus;", scope);
  const old = { key: "optimization-build-source:old", id: 0, token: "old-process-job", cursor: 200, source: { kind: "public" } };
  await store.value(old.key, old);
  assert.equal((await scope.readNative(old)).ok, false);
  assert.equal(requests[0].job_token, "old-process-job");
  assert.equal(acknowledged, 0); assert.equal(saved, 0); assert.equal((await store.value(old.key)).cursor, 200);
  const legacy = { ...old, key: "optimization-build-source:legacy", token: null };
  await store.value(legacy.key, legacy);
  assert.equal((await scope.readNative(legacy)).ok, false); assert.equal(requests.length, 1);
});

await check("session-only storage cannot acknowledge a native build stream", async () => {
  const store = new UserTaskMemoryStore(), requests = []; let kept = 0;
  const scope = vm.createContext({ crypto: webcrypto, console, TextEncoder, AbortController, setTimeout, clearTimeout,
    window: { addEventListener() {} }, WASM: false, userTaskLock: locks(), initUserTasks: async () => ({ store }),
    keepOptimizationBuilds: async () => { kept++; }, api: async (_, request) => {
      requests.push(plain(request)); return { ok: true, job_token: "job", phase: "done", result: { ok: true } };
    } });
  vm.runInContext(storeSource + "\n" + taskSource + "\nglobalThis.readNative = nativeOptimizationStatus;", scope);
  const link = { key: "optimization-build-source:memory", id: 0, token: "job", cursor: 200, source: { kind: "public" } };
  await store.value(link.key, link);
  assert.equal((await scope.readNative(link)).phase, "done");
  assert.equal(Object.hasOwn(requests[0], "builds_after"), false);
  assert.equal(kept, 0); assert.equal((await store.value(link.key)).cursor, 200);
});

await check("native collection is an explicit volatile subscription only with durable storage and upload authorization", async () => {
  for (const [persistent, kind, expected] of [[true, "public", true], [false, "public", false], [true, "disabled", false]]) {
    const store = new UserTaskMemoryStore(), calls = []; store.persistent = persistent;
    const scope = vm.createContext({ crypto: webcrypto, console, TextEncoder, AbortController, setTimeout, clearTimeout,
      window: { addEventListener() {} }, WASM: false, userTaskLock: locks(), initUserTasks: async () => ({ store }),
      optimizationTaskBuildSource: async () => ({ kind }), keepOptimizationBuilds: async () => {}, holdForeground: () => () => {},
      userTaskFailed: error => { throw error; }, track() {}, optProgressOf: () => null,
      api: async (path, request) => {
        calls.push({ path, request: plain(request) });
        return path === "/api/optimize" ? { ok: true, job_id: 0, job_token: "job" }
          : { ok: true, job_token: "job", phase: "done", builds: [], builds_next: 0, builds_more: false, result: { ok: true, results: [] } };
      } });
    vm.runInContext(storeSource + "\n" + taskSource + "\nglobalThis.executeNative = executeOptimization;", scope);
    const task = { id: "task", attempt: "attempt", engine: "engine-a", request_schema: 1, origin: { board_upload: true }, request: { weapon: "braton_prime", starts: [] } };
    const checkpoint = async value => value || null;
    const result = await scope.executeNative(task, { signal: new AbortController().signal, report() {}, checkpoint });
    assert.equal(result.ok, true); assert.equal(calls[0].request.__collect_builds, expected);
    assert.equal(Object.hasOwn(task.request, "__collect_builds"), false, "collection transport cannot mutate the saved calculation");
    const status = calls.find(call => call.path === "/api/optimize/status");
    assert.equal(Object.hasOwn(status.request, "builds_after"), expected);
    assert.equal(status.request.job_token, "job");
  }
});

await check("optimizer and riven-analysis notices share the existing consent switch", async () => {
  const settings = new Map(), boxes = [{ button: {} }, { button: {} }];
  boxes.forEach(box => { box.querySelector = () => box.button; });
  const scope = vm.createContext({ console, setTimeout: () => 1, clearTimeout() {},
    localStorage: { getItem: key => settings.get(key) || null, setItem: (key, value) => settings.set(key, value) },
    document: { querySelectorAll: () => boxes }, $: () => null, tr: value => value, escHtml: value => value });
  vm.runInContext(read("app/68-board-submit.js")
    + "\nrefreshBoardDoor = () => {}; globalThis.drawConsent = renderOptimizationBuildConsent;", scope);
  scope.drawConsent(); assert(boxes.every(box => box.innerHTML.includes("Legal builds found during optimization")));
  boxes[0].button.onclick(); assert.equal(settings.get("wfsim-board-consent"), "no");
  assert(boxes.every(box => box.innerHTML.includes("New tasks keep build uploads off.")));
  boxes[1].button.onclick(); assert.equal(settings.get("wfsim-board-consent"), "yes");
});

await check("an opted-out community task still computes without any build POST", async () => {
  let computed = 0, posts = 0, delivery;
  const scope = vm.createContext({ crypto: webcrypto, console, TextEncoder, AbortController, setTimeout, clearTimeout,
    MutationObserver: class { observe() {} }, setInterval: () => 0,
    document: { documentElement: {}, addEventListener() {} },
    window: { addEventListener() {} }, userTaskSources: new Map(), communityLanes: () => 1, communityRest: () => 0,
    ownTabBusy: () => false, initUserTasks: async () => { throw new Error("opt-out must not open the upload store"); },
    fetch: async () => { posts++; throw new Error("opt-out must not send a build POST"); },
    quickFleet: (request, lanes, pace, options) => {
      computed++; assert.equal(request.__board_upload, false);
      delivery = options.onBuilds([build(17)]);
      return { result: { ok: true, results: [{ mods: ["measured"] }] }, workers: [] };
    } });
  vm.runInContext(storeSource + "\n" + read("app/68-board-submit.js") + "\n" + read("app/93-community-calculations.js")
    + "\nglobalThis.privateCompute = { source: communityOptimizationBuildSource, run: communityOptimizationReport };", scope);
  const request = { weapon: "braton_prime", __board_upload: false };
  const source = await scope.privateCompute.source({ request }, "executor");
  const result = await scope.privateCompute.run(request, { signal: new AbortController().signal, buildSource: source });
  await delivery;
  assert.equal(result.ok, true); assert.equal(computed, 1); assert.equal(posts, 0);
});

await check("community retries retain the task's saved upload setting instead of the current toggle", async () => {
  const calls = [];
  const scope = vm.createContext({ console, crypto: webcrypto, setTimeout, clearTimeout,
    window: { addEventListener() {} }, boardConsent: () => "yes",
    USER_TASK_TYPES: new Map([["optimization", { community: { kind: "optimization" } }], ["appraisal", { community: { kind: "optimization" } }]]),
    USER_TASK_EXECUTORS: new Map([["local", {}]]),
    capture: async (action, body) => { calls.push(plain(body)); return { ok: true }; } });
  vm.runInContext(read("app/61-community-tasks.js") + "\ncommunityTaskCall = capture; globalThis.acceptFrozen = communityTaskAccept;", scope);
  const task = { id: "task", kind: "optimization", engine: "engine-a", request: { weapon: "braton_prime" }, origin: { board_upload: false } };
  await scope.acceptFrozen(task, "same-command"); scope.boardConsent = () => "no";
  await scope.acceptFrozen(task, "same-command");
  assert.deepEqual(calls[0], calls[1]); assert.equal(calls[0].request.__board_upload, false);
  assert.equal(task.request.__board_upload, undefined);
  await scope.acceptFrozen({ ...task, kind: "appraisal", origin: { board_upload: true } }, "another-command");
  assert.equal(calls[2].request.__board_upload, true);
  await scope.acceptFrozen({ ...task, origin: { board_upload: false, appraisal: { code: "chat" } } }, "chat-command");
  assert.equal(calls[3].request.__board_upload, true);
  const legacy = { ...task, origin: {} };
  scope.boardConsent = () => "yes"; await scope.acceptFrozen(legacy, "legacy-command");
  scope.boardConsent = () => "no"; await scope.acceptFrozen(legacy, "legacy-command");
  assert.deepEqual(calls[4], calls[5]); assert.deepEqual(calls[4].request, task.request);
  assert.equal(Object.hasOwn(calls[4].request, "__board_upload"), false, "legacy acceptance must retain its original question");
  await scope.acceptFrozen({ ...legacy, origin: undefined }, "missing-origin-command");
  assert.deepEqual(calls[6].request, task.request);
  await scope.acceptFrozen({ ...legacy, request: { ...task.request, __board_upload: false } }, "existing-flag-command");
  assert.equal(calls[7].request.__board_upload, false);
});

await check("legacy local tasks need a saved affirmative upload choice", async () => {
  const scope = vm.createContext({ console, setTimeout, clearTimeout, boardConsent: () => "yes" });
  vm.runInContext(read("app/68-board-submit.js") + "\nglobalThis.sourceForTask = optimizationTaskBuildSource;", scope);
  assert.equal((await scope.sourceForTask({ origin: {}, request: {} })).kind, "disabled");
  assert.equal((await scope.sourceForTask({ request: {} })).kind, "disabled");
  assert.equal((await scope.sourceForTask({ origin: { board_upload: true }, request: {} })).kind, "public");
  assert.equal((await scope.sourceForTask({ origin: {}, request: { __board_upload: true } })).kind, "public");
  assert.equal((await scope.sourceForTask({ origin: { board_upload: false }, request: { __board_upload: true } })).kind, "disabled");
});

await check("an enabled community task obtains and persists the original search's signed receipt", async () => {
  const store = new UserTaskMemoryStore(), posts = [];
  const scope = vm.createContext({ console, crypto: webcrypto, TextEncoder, setTimeout, clearTimeout,
    ENGINE_ID: "engine-a", initUserTasks: async () => ({ store }), timeoutSignal: () => new AbortController().signal,
    fetch: async (_, options) => {
      posts.push(JSON.parse(options.body));
      return { ok: true, json: async () => ({ ok: true, receipt: "original-signed-owner", expires_at: "2099-01-01T00:00:00.000Z", question: "q", engine: "engine-a", weapon: "braton_prime" }) };
    } });
  vm.runInContext(read("app/68-board-submit.js") + "\nglobalThis.getReceipt = communityOptimizationBuildSource;", scope);
  const work = { request: { __board_upload: true }, lease: "lease", question: "q" };
  const source = await scope.getReceipt(work, "executor");
  assert.deepEqual(posts[0], { builds: [], harvest: { engine: "engine-a", lease: "lease", verifier: "executor", question: "q" } });
  assert.deepEqual(plain(source.harvest), { receipt: "original-signed-owner", question: "q", engine: "engine-a" });
  assert.equal(source.explicit, true);
  await scope.getReceipt(work, "executor"); assert.equal(posts.length, 1, "the persisted receipt survives a retry");
});

await check("legacy community work can compute when the server declines build collection", async () => {
  const store = new UserTaskMemoryStore(); let posts = 0;
  const scope = vm.createContext({ console, crypto: webcrypto, TextEncoder, setTimeout, clearTimeout,
    ENGINE_ID: "engine-a", initUserTasks: async () => ({ store }), timeoutSignal: () => new AbortController().signal,
    fetch: async () => { posts++; return { ok: true, json: async () => ({ ok: true, disabled: true, results: [] }) }; } });
  vm.runInContext(read("app/68-board-submit.js") + "\nglobalThis.getReceipt = communityOptimizationBuildSource;", scope);
  const source = await scope.getReceipt({ lease: "legacy", question: "legacy", request: {} }, "executor");
  assert.equal(source.kind, "disabled"); assert.equal(posts, 1);
});

await check("saved-riven batches capture upload consent before asynchronous validation", async () => {
  let consent = "yes", captured = null; const validating = gate(), entered = gate();
  const scope = vm.createContext({ crypto: webcrypto, console, TextEncoder, AbortController, setTimeout, clearTimeout,
    ENGINE_ID: "engine-a", boardConsent: () => consent, META: { search_presets: { presets: [{ riven: true, name: "Appraisal" }] } },
    initUserTasks: async () => ({ createMany: async entries => { captured = plain(entries); return entries; } }),
    api: async () => { entered.resolve(); await validating.promise; return { illegal: [] }; },
    headlessRivenGainRequest: () => ({ request: { weapon: "braton_prime" }, context: { weapon: "braton_prime" } }),
    fetch: async () => ({ ok: true }) });
  vm.runInContext(storeSource + "\n" + appraisalSource + "\nglobalThis.submitBatch = submitSavedAppraisals;", scope);
  const card = { id: "owned", name: "Owned", state: { bonuses: [{ id: "critical_chance" }, { id: "critical_damage" }] } };
  const submitting = scope.submitBatch(["owned"], "ruler", "community", { weapon: "braton_prime", cards: [card] });
  await entered.promise; consent = "no"; validating.resolve(); await submitting;
  assert.equal(captured[0].origin.board_upload, true);
});

assert.match(read("worker.js"), /payload\.kind === "builds"/);
assert.match(read("app/93-community-calculations.js"), /communityOptimizationBuildSource\(work, verifier\)/);
assert.match(read("app/69-board-work.js"), /communityOptimizationBuildSource\(w, id\)/);
console.log(`\n${passed} optimizer build-delivery checks passed`);
