import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import { webcrypto } from "node:crypto";

const source = readFileSync(new URL("../web/src/static/app/53-task-store.js", import.meta.url), "utf8");
function classes(code = source) {
  const context = vm.createContext({ crypto: webcrypto, AbortController, console });
  vm.runInContext(code + "\nglobalThis.classes = { UserTaskQueue, UserTaskMemoryStore };", context);
  return context.classes;
}
const locks = () => {
  const held = new Map();
  return (name, work) => {
    const next = (held.get(name) || Promise.resolve()).catch(() => {}).then(work);
    held.set(name, next);
    return next;
  };
};
function fixture({ run = async () => ({ value: 1 }), implementation = classes(), store, lock = locks() } = {}) {
  const artifacts = new Map();
  const type = { schema: 1, validate: (request) => { if (!(request.count > 0)) throw new Error("Invalid request"); },
    execute: run, save: async (task, value) => { artifacts.set(task.attempt, value); return task.attempt; },
    recover: async (task) => artifacts.has(task.attempt) ? task.attempt : null };
  store ||= new implementation.UserTaskMemoryStore();
  const queue = new implementation.UserTaskQueue({ store, types: new Map([["new-calculation", type]]), engine: "engine-a", lock });
  queue.wake = () => {};
  return { queue, store, type, artifacts };
}
const origin = { context: { weapon: "original", preset: "build-a", scenario: "fight-a", key: "original" } };
const create = (queue, count = 5) => queue.create("new-calculation", { count, nested: { value: "original" } }, origin);
const deferred = () => { let resolve; const promise = new Promise((done) => { resolve = done; }); return { promise, resolve }; };
let passed = 0;
async function check(name, run) { await run(); passed++; console.log(`  ok  ${name}`); }

await check("an unrelated calculation type executes through the same lifecycle", async () => {
  const { queue, artifacts } = fixture({ run: async (task, { report }) => { report({ phase: "a new stage", done: 2, total: task.request.count }); return { value: 17 }; } });
  const task = await create(queue);
  await queue.runNext();
  const done = queue.list()[0];
  assert.equal(done.id, task.id);
  assert.equal(done.state, "succeeded");
  assert.equal(artifacts.get(done.result_ref).value, 17);
});
await check("queued requests and origin context are immutable snapshots", async () => {
  const { queue } = fixture();
  const request = { count: 5, nested: { value: "original" } }, context = structuredClone(origin);
  const task = await queue.create("new-calculation", request, context);
  request.nested.value = "edited"; context.context.weapon = "other";
  task.request.count = 99;
  const held = queue.list()[0];
  assert.equal(held.request.nested.value, "original");
  assert.equal(held.request.count, 5);
  assert.equal(held.origin.context.weapon, "original");
});
await check("editing origin context during an acceptance transaction cannot change the task", async () => {
  const { queue, store } = fixture(), gate = deferred(), entered = deferred();
  const change = store.change.bind(store);
  store.change = async (transform) => { entered.resolve(); await gate.promise; return change(transform); };
  const context = structuredClone(origin);
  const accepted = queue.create("new-calculation", { count: 5 }, context);
  await entered.promise;
  context.context.weapon = "edited while saving";
  gate.resolve(); await accepted;
  assert.equal(queue.list()[0].origin.context.weapon, "original");
});
await check("invalid and unknown requests never enter the queue", async () => {
  const { queue } = fixture();
  await assert.rejects(create(queue, 0));
  await assert.rejects(queue.create("absent", {}, origin));
  await assert.rejects(queue.create("new-calculation", { count: 1 }, origin, "absent"));
  assert.equal(queue.list().length, 0);
});
await check("a new executor can run a type without changing the queue", async () => {
  const { queue, type, artifacts } = fixture();
  type.execute = null;
  queue.executors = new Map([["another-destination", { supports: () => true,
    run: async () => ({ value: "another executor" }) }]]);
  const task = await queue.create("new-calculation", { count: 5 }, origin, "another-destination");
  await queue.runNext();
  assert.equal(queue.list()[0].destination, "another-destination");
  assert.equal(queue.list()[0].state, "succeeded");
  assert.equal(artifacts.get(queue.list()[0].result_ref).value, "another executor");
  assert.equal(queue.list()[0].id, task.id);
});
await check("waiting order determines the next executed task", async () => {
  const { queue } = fixture();
  const first = await create(queue), second = await create(queue);
  await queue.move(second.id, -1);
  await queue.runNext();
  assert.equal(queue.list().find((task) => task.id === second.id).state, "succeeded");
  assert.equal(queue.list().find((task) => task.id === first.id).state, "queued");
});
await check("an arrival as the runner becomes idle is not stranded", async () => {
  const { queue } = fixture();
  await create(queue);
  const runNext = queue.runNext.bind(queue);
  let arrived = false;
  queue.runNext = async () => {
    const ran = await runNext();
    if (!ran && !arrived) { arrived = true; await create(queue); }
    return ran;
  };
  delete queue.wake;
  queue.wake();
  for (let index = 0; index < 200 && queue.list().filter((task) => task.state === "succeeded").length < 2; index++) {
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
  assert.equal(queue.list().filter((task) => task.state === "succeeded").length, 2);
});
async function cancellationInvariant(implementation = classes()) {
  const ready = deferred(), finish = deferred();
  let report;
  const { queue, artifacts } = fixture({ implementation, run: async (task, executor) => { report = executor.report; ready.resolve(); await finish.promise; return { value: "late" }; } });
  const task = await create(queue), running = queue.runNext();
  await ready.promise;
  await queue.cancel(task.id);
  report({ phase: "late", done: 99, total: 100 });
  finish.resolve(); await running;
  assert.equal(queue.list()[0].state, "cancelled");
  assert.equal(artifacts.size, 0);
}
await check("cancelled attempts cannot save late progress or results", () => cancellationInvariant());
await check("a retry retains identity and inputs while replacing its attempt", async () => {
  const { queue, type } = fixture({ run: async () => { throw new Error("Temporary failure"); } });
  const task = await create(queue);
  await queue.runNext();
  const oldAttempt = queue.list()[0].attempt;
  type.execute = async () => ({ value: "retried" });
  await queue.retry(task.id); await queue.runNext();
  assert.equal(queue.list()[0].id, task.id);
  assert.notEqual(queue.list()[0].attempt, oldAttempt);
  assert.equal(queue.list()[0].request.count, 5);
  assert.equal(queue.list()[0].state, "succeeded");
});
await check("removing cancelled work cannot strand the next task", async () => {
  const ready = deferred(), finish = deferred();
  const { queue, type } = fixture({ run: async () => { ready.resolve(); await finish.promise; throw new Error("Aborted"); } });
  const task = await create(queue), running = queue.runNext();
  await ready.promise; await queue.cancel(task.id); await queue.remove(task.id);
  finish.resolve(); await running;
  type.execute = async () => ({ value: 1 });
  await create(queue); await queue.runNext();
  assert.equal(queue.list()[0].state, "succeeded");
});
await check("recovery interrupts abandoned work and adopts a committed result", async () => {
  const { queue, store, artifacts } = fixture();
  const first = await create(queue), second = await create(queue);
  await store.change((state) => {
    Object.assign(state.tasks[0], { state: "running", attempt: "abandoned" });
    Object.assign(state.tasks[1], { state: "running", attempt: "committed" });
  });
  artifacts.set("committed", { value: 7 });
  await queue.refresh(); await queue.recover();
  assert.equal(queue.list().find((task) => task.id === first.id).state, "interrupted");
  assert.equal(queue.list().find((task) => task.id === second.id).result_ref, "committed");
});
await check("a different engine or request schema cannot silently execute", async () => {
  const { queue, type } = fixture();
  const task = await create(queue);
  queue.engine = "engine-b";
  await queue.runNext();
  assert.equal(queue.list()[0].state, "interrupted");
  await assert.rejects(queue.retry(task.id));
  queue.engine = "engine-a"; type.schema = 2;
  await assert.rejects(queue.retry(task.id));
});
await check("a storage failure cannot acknowledge task acceptance", async () => {
  const { queue, store } = fixture();
  store.change = async () => { throw new Error("Storage full"); };
  await assert.rejects(create(queue), /Storage full/);
  assert.equal(queue.list().length, 0);
});
await check("clearing a task keeps its authoritative result artifact", async () => {
  const { queue, artifacts } = fixture();
  const task = await create(queue); await queue.runNext();
  await queue.remove(task.id);
  assert.equal(queue.list().length, 0);
  assert.equal(artifacts.size, 1);
});
await check("captured result records do not read the current editor", async () => {
  const resultSource = readFileSync(new URL("../web/src/static/app/35-results.js", import.meta.url), "utf8");
  const from = resultSource.indexOf("function resultRecord(");
  const until = resultSource.indexOf("/// Every record of one build", from);
  const context = vm.createContext({ RESULT_SCHEMA: 1, ENGINE_ID: "a", RELEASE_ID: "b" });
  vm.runInContext(resultSource.slice(from, until) + "\nglobalThis.capture = resultRecord;", context);
  const recorded = context.capture({ runs: 3 }, { weapon: "original" }, origin.context, "stable-result");
  assert.equal(recorded.weapon, "original");
  assert.equal(recorded.preset, "build-a");
  assert.equal(recorded.scenario, "fight-a");
  assert.equal(recorded.id, "stable-result");
  assert.equal(recorded.r.replay, null);
});
await check("a background board upload uses its captured build without repainting another editor", async () => {
  const boardSource = readFileSync(new URL("../web/src/static/app/68-board-submit.js", import.meta.url), "utf8");
  const from = boardSource.indexOf("async function submitBoardPayload(");
  const until = boardSource.indexOf("/// THE LINE THE BOARD DRAWS", from);
  let sent, renders = 0;
  const context = vm.createContext({
    presetWeapon: () => "another-weapon", activePreset: "another-build", simKey: () => "another-key",
    boardConsent: () => "yes", boardState: "unchanged", boardOnBoard: null, boardBoards: null, boardDoor: null, boardRefusal: "",
    boardVerdict: async () => ({ ok: true, accepted: true, boards: 1, of: 1 }),
    boardRowMatching: () => { throw new Error("The other weapon's rows must not be read"); },
    boardPayload: () => { throw new Error("The current build must not be read"); },
    fetch: async (url, options) => { sent = JSON.parse(options.body); return { ok: true }; }, track: () => {},
    renderBoardConsent: () => { renders++; }, renderBoardOutcome: () => { renders++; },
  });
  vm.runInContext(boardSource.slice(from, until) + "\nglobalThis.offer = offerBoardSubmit;", context);
  await context.offer({ body: { weapon: "original", mods: ["original-mod"] }, context: origin.context });
  assert.equal(sent.weapon, "original");
  assert.equal(sent.mods[0], "original-mod");
  assert.equal(context.boardState, "unchanged");
  assert.equal(renders, 0);
});
await check("the cancellation assertion fails with its attempt guard removed", async () => {
  const guard = 'held && held.state === "running" && held.attempt === task.attempt && held.generation === task.generation';
  assert(source.includes(guard));
  const broken = classes(source.replace(guard, "held"));
  await assert.rejects(() => cancellationInvariant(broken), /AssertionError/);
});
console.log(`\n${passed} user task queue checks passed`);
