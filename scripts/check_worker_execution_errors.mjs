// SPDX-License-Identifier: AGPL-3.0-or-later
// The real browser worker and page watchdog with a tiny local WASM trap.
import assert from "node:assert/strict";
import { once } from "node:events";
import { readFileSync } from "node:fs";
import { createServer } from "node:http";
import { openApp } from "./cdp.mjs";

const read = path => readFileSync(new URL(`../web/src/static/${path}`, import.meta.url), "utf8");
const transport = read("app/06-transport.js");
const watch = transport.slice(transport.indexOf("function watchSilence("), transport.indexOf("/// WHERE A WORKER"));
const lanes = transport.slice(transport.indexOf("/// WHERE A LANE WAS LOST"), transport.indexOf("/// ONE CALL ON A LANE THAT MAY DIE UNDER IT."));
const worker = `
  const probe = new BroadcastChannel("worker-rejections");
  self.addEventListener("unhandledrejection", e => probe.postMessage(String(e.reason?.message ?? e.reason)));
  ${read("worker.js")}
`;
const glue = `
  // (module (func (export "trap") unreachable))
  const module = new WebAssembly.Module(Uint8Array.of(
    0,97,115,109,1,0,0,0, 1,4,1,96,0,0, 3,2,1,0,
    7,8,1,4,116,114,97,112,0,0, 10,5,1,3,0,0,11
  ));
  const { trap } = new WebAssembly.Instance(module).exports;
  let instance;
  const wasm_bindgen = Object.assign(async ({ module_or_path }) => { instance = module_or_path; }, {
    set_heartbeat() {},
    optimize() { trap(); },
    optimize_resume() { throw new Error("resume exception"); },
    api(path, body) {
      if (path === "/throw") throw new TypeError("RPC exception");
      if (path === "/refuse") return JSON.stringify({ ok: false, error: "business refusal", detail: 42 });
      if (path === "/queued") probe.postMessage({ queued: instance });
      if (path === "/instance") {
        if (JSON.parse(body).trapInstance === instance) trap();
        return JSON.stringify({ ok: true, instance });
      }
      return JSON.stringify({ ok: true, path });
    },
    simulate_shard(body) { if (JSON.parse(body).trap) trap(); throw new Error("shard exception"); },
    simulate_merged(body) { if (JSON.parse(body).trap) trap(); throw new Error("merge exception"); },
    verify_final() { throw new Error("replay exception"); },
    finish_finals(_, receipts) {
      if (receipts === "null") return "invalid JSON";
      return JSON.stringify({ ok: false, error: "invalid or incomplete finalist receipts" });
    },
  });
`;
const page = `
  let woptNextId = 1;
  const LANE_WATCHDOG = { loading: 1000, stall: 1000, tick: 50 };
  const computeRun = (_, work, live) => live() ? work() : { cancelled: true };
  const WASM = true, ENGINE_VIEWS = {}, poolSize = () => 1, pool = [];
  const track = () => {}, releaseAfterLaneFailed = () => {};
  let readerInFlight = 0, workerCount = 0;
  const workerErrors = [], rejections = [], queuedRuns = [];
  const probe = new BroadcastChannel("worker-rejections");
  probe.onmessage = e => {
    if (typeof e.data === "string") rejections.push(e.data);
    else queuedRuns.push(e.data.queued);
  };
  function spawnEngine() {
    const worker = new Worker("/worker.js");
    worker.addEventListener("error", e => workerErrors.push(e.message));
    worker.postMessage({ kind: "engine", module: ++workerCount });
    return worker;
  }
  ${watch}
  ${lanes}
  ${read("app/08-checkpoint-api.js")}
`;

async function quickTrap() {
  const job = quickFleet({ strategy: "quick" }, 1);
  try {
    const until = performance.now() + 3000;
    while (!job.result && performance.now() < until) await new Promise(resolve => setTimeout(resolve, 10));
    return { result: job.result, workers: job.workers.length, workerErrors, rejections };
  } finally { job.workers.forEach(worker => worker.terminate()); }
}

async function sharedLaneTrap() {
  const before = await api("/instance", {});
  const old = pool[0];
  const refusal = await api("/refuse", {});
  const reusedAfterRefusal = pool[0] === old;
  const execution = api("/instance", { trapInstance: before.instance });
  const pending = old.call("/queued", {});
  const [retried, waiting] = await Promise.all([execution, pending]);
  const after = { dead: old.dead, busy: old.busy, readerInFlight };
  const shards = [];
  for (const kind of ["shard", "merge"]) {
    const lane = pool[0];
    const result = await lane.sendNow({ kind, body: { trap: true } });
    shards.push({ kind, result, dead: lane.dead, fresh: (await api("/instance", {})).instance });
  }
  const last = pool[0];
  last.worker.terminate();
  return { before, refusal, reusedAfterRefusal, retried, waiting, after, shards, queuedRuns, rejections };
}

async function exerciseProtocol() {
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  const until = async check => {
    const deadline = performance.now() + 3000;
    while (!check() && performance.now() < deadline) await sleep(10);
    if (!check()) throw new Error("worker fixture did not answer");
  };
  const open = (url = "/worker.js") => {
    const worker = new Worker(url), messages = [], errors = [];
    worker.onmessage = e => messages.push(e.data);
    worker.onerror = e => { e.preventDefault(); errors.push(e.message); };
    return { worker, messages, errors, send: message => worker.postMessage(message) };
  };
  const rpc = open();
  try {
    for (const message of [
      { kind: "unknown", id: 99 }, null, { kind: "engine" },
      { kind: "api", id: 1, path: "/throw" },
      { kind: "shard", id: 2 }, { kind: "merge", id: 3 },
      { kind: "verify_final", generation: 7, index: 0 },
      { kind: "finish_finals", generation: 7, receipts: null },
      { kind: "finish_finals", generation: 7, receipts: [] },
      { kind: "optimize", checkpoint: "{}" },
      { kind: "rest", id: 4, ms: 1n },
      { kind: "api", id: 5, path: "/still-alive" },
    ]) rpc.send(message);
    await until(() => rpc.messages.some(message => message.id === 5) && rpc.errors.length >= 3);
    const routed = rpc.messages.slice();
    rpc.send({ kind: "rest", id: 6, ms: 0 });
    await until(() => rpc.messages.some(message => message.id === 6));
    const rested = rpc.messages.at(-1);

    rpc.send({ kind: "rest", id: 7, ms: 500 });
    rpc.send({ kind: "api", id: 8, path: "/before-terminate" });
    await until(() => rpc.messages.some(message => message.id === 8));
    rpc.worker.terminate();
    await sleep(550);
    const cancelReplies = rpc.messages.filter(message => message.id === 7);

    const failedLoad = async (url, cause) => {
      const lane = open(url);
      try {
        lane.send({ kind: "api", id: 100, path: "/pending" });
        if (cause) lane.send({ kind: "engine", cause });
        await until(() => lane.errors.length && lane.messages.length);
        return { messages: lane.messages, errors: lane.errors };
      } finally { lane.worker.terminate(); }
    };
    const dataLoad = await failedLoad("/worker.js", "data");
    const glueLoad = await failedLoad("/broken/worker.js");
    return { routed, rested, cancelReplies, dataLoad, glueLoad, errors: rpc.errors, rejections };
  } finally { rpc.worker.terminate(); }
}

const assets = new Map([
  ["/", ["text/html", '<script src="/fixture.js"></script>']],
  ["/fixture.js", ["text/javascript", page]],
  ["/worker.js", ["text/javascript", worker]],
  ["/broken/worker.js", ["text/javascript", worker]],
  ["/pkg/wfsim_wasm.js", ["text/javascript", glue]],
]);
const server = createServer((request, response) => {
  const asset = assets.get(request.url);
  if (!asset) { response.writeHead(404).end(); return; }
  response.writeHead(200, { "content-type": asset[0], "cache-control": "no-store" }).end(asset[1]);
});
server.listen(0, "127.0.0.1");
await once(server, "listening");
let app;
try {
  app = await openApp({ base: `http://127.0.0.1:${server.address().port}`, boot: 100 });
  const trap = await app.evaluate(`(${quickTrap})()`);
  console.log("Quick WASM trap:", JSON.stringify(trap));
  assert.equal(trap.result?.ok, false);
  assert.match(trap.result.error, /unreachable/);
  assert.equal(trap.result.worker_dead, true);
  assert.equal(trap.workers, 0);
  assert.deepEqual(trap.workerErrors, [trap.result.error]);
  assert.deepEqual(trap.rejections, []);

  const lane = await app.evaluate(`(${sharedLaneTrap})()`);
  console.log("Shared lane WASM trap:", JSON.stringify(lane));
  assert.equal(lane.before.ok, true);
  assert(Number.isInteger(lane.before.instance));
  assert.equal(lane.waiting.worker_dead, true);
  assert.match(lane.waiting.error, /RuntimeError: unreachable/);
  assert.deepEqual(lane.after, { dead: true, busy: 0, readerInFlight: 0 });
  assert.deepEqual(lane.retried, { ok: true, instance: lane.before.instance + 1 });
  assert.deepEqual(lane.refusal, { ok: false, error: "business refusal", detail: 42 });
  assert.equal(lane.reusedAfterRefusal, true);
  assert.deepEqual(lane.queuedRuns, []);
  assert.deepEqual(lane.rejections, []);
  for (const [i, { result, dead, fresh }] of lane.shards.entries()) {
    assert.deepEqual(result, lane.waiting);
    assert.equal(dead, true);
    assert.equal(fresh, lane.retried.instance + i + 1);
  }

  const wire = await app.evaluate(`(${exerciseProtocol})()`);
  const refusal = error => ({ ok: false, error });
  assert.equal(wire.routed.length, 6);
  assert.deepEqual(wire.routed.slice(0, 4), [
    { id: 1, payload: refusal("RPC exception") },
    { id: 2, payload: refusal("shard exception") },
    { id: 3, payload: refusal("merge exception") },
    { kind: "result", payload: refusal("invalid or incomplete finalist receipts") },
  ]);
  assert.equal(wire.routed[4].id, 4);
  assert.equal(wire.routed[4].kind, "rested");
  assert.equal(wire.routed[4].payload.ok, false);
  assert.match(wire.routed[4].payload.error, /BigInt/);
  assert.deepEqual(wire.routed[5], { id: 5, payload: { ok: true, path: "/still-alive" } });
  assert.deepEqual(wire.rested, { id: 6, kind: "rested", payload: { ok: true } });
  assert.deepEqual(wire.cancelReplies, []);
  assert.deepEqual(wire.dataLoad.messages, [{ kind: "load_failed", cause: "data" }]);
  assert.deepEqual(wire.dataLoad.errors, ["Uncaught Error: engine data"]);
  assert.deepEqual(wire.glueLoad.messages, [{ kind: "load_failed", cause: "glue" }]);
  assert.match(wire.glueLoad.errors[0], /importScripts/);
  assert.equal(wire.errors.length, 3);
  assert.match(wire.errors[0], /replay exception/);
  assert.match(wire.errors[1], /SyntaxError.*JSON/);
  assert.match(wire.errors[2], /resume exception/);
  assert.deepEqual(wire.rejections, []);
  console.log("ok: execution errors retain RPC, optimizer and rest routing, load failure, unknown messages and termination");
} finally {
  try { await app?.close(); } finally { server.close(); }
}
