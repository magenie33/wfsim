// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import { headlessBoardRecord, pooledRanking, pooledRivens, rankBoard } from "../mcp/headless.js";

const weapon = { id: "torid", modes: ["base", "cycle"] }, ruler = "standard_single_target";
const benches = [{ id: ruler, scenario: { runs: 1000, metric: "kpm" } }];
const rows = Array.from({ length: 24 }, (_, i) => ({
  mods: ["fixture-" + i], benchmark: ruler, mode: i % 2 ? "base" : "cycle",
  score: 100 - (i % 12), measured_by: i % 2 ? "current" : "old",
  ...(i >= 12 ? { riven: { bonuses: ["multishot", "stat-" + i], malus: "zoom", rolls: [0.95, 1.05, 1.1] } } : {}),
})).reverse().concat({ mods: ["other-ruler"], benchmark: "other", score: 999 });
const events = [], results = [], current = rows.map(row => ({ ...row, score: row.score + 10 }));
const requests = new Map();
let board = rows, queue;
const ctx = {
  crypto, TextEncoder, AbortController, structuredClone, URLSearchParams,
  userTaskSources: new Map(), resultMem: new Map(), results, ENGINE_ID: "current", RIVENS: true,
  META: { benchmarks: benches, search_presets: { presets: [{ riven: true }] } }, localStorage: { getItem: () => null },
  window: { addEventListener() {} }, document: { addEventListener() {}, querySelector() {} },
  benchList: () => benches, metricOf: id => ({ id }), metricValue: (_metric, result) => result.score,
  headlessBoardRecord, pooledRanking, pooledRivens, rankBoard,
  presetWeapon: () => weapon.id, weaponInfo: () => weapon,
  boardConsent: () => "yes", loadPresetList: () => [{ id: "one", name: "Riven", state: { bonuses: [{ id: "critical_chance" }, { id: "multishot" }] } }],
  headlessRivenGainRequest: () => ({ request: { weapon: weapon.id, starts: [], rivens: [{}] }, context: {} }),
  accountState: {},
  pageBoardReader: () => ({ weapon: async (_meta, id, selected) => {
    assert.equal(id, weapon.id); assert.equal(selected, ruler); events.push(board === current ? "new board" : "old board"); return board;
  } }),
  api: async (path, body) => {
    if (path === "/api/riven") return { ok: true };
    assert.equal(path, "/api/board/order");
    const row = rows.find(row => row.mods[0] === body.record.mods[0]);
    assert(row); assert.equal(body.ruler, ruler); assert.equal(body.mode, row.mode);
    if (row.riven) assert.deepEqual(body.record.riven_rolls, row.riven.rolls);
    const i = Number(row.mods[0].split("-")[1]);
    const request = { weapon: weapon.id, mode: row.mode, runs: 1000, seed: 123 + i };
    requests.set(request.seed, row);
    const identity = i.toString(16).padStart(32, "0");
    if (body.observation) assert.equal(JSON.stringify(body.observation), JSON.stringify(request));
    return { ok: true, request, identity, matches_observation: !!body.observation };
  },
  initUserTasks: async () => queue, withPoolOf: (_weapon, work) => work(),
  boardRowState: (_, row) => ({ mods: row.mods }), holdForeground: () => () => {},
  track() {}, noteSimRun() {}, scheduleUserTasks() {}, frozenTaskBuild: build => build,
  resultMemKey: (id, preset) => id + "/" + preset,
  resultAddCaptured: async (r, build, context, id) => {
    const record = { id, r, build, ...context }; results.push(record); return record;
  },
  simulateFleet: async request => {
    const row = requests.get(request.seed);
    assert(row); events.push(row.mods[0]);
    return { ok: true, score: row.score + (request.seed === 124 ? 1 : 0) };
  },
  executeOptimization: async () => {
    events.push("optimize");
    board = current;
    return { results: [{ mods: ["serration"] }], delivery: {} };
  },
  boardPayloadFromResult: () => ({}), pageSay: () => ({}),
  headlessAppraisalReplay: async (_say, w, job, _rows, host) => ({ ok: true, reference_rows: await host.board(w.id, job.ruler) }),
};
const part = name => readFileSync(new URL("../web/src/static/app/" + name + ".js", import.meta.url), "utf8");
const headless = part("87-headless");
const create = runInNewContext([
  headless.match(/function headlessRulerScenario\([^]*?\n\}/)[0],
  "const headlessWeaponOf = () => weaponInfo();",
  part("53-task-store"),
  "USER_TASK_TYPES.set('optimization', { schema: 1, validate() {}, save: async () => 'graded' });",
  part("59-simulation-tasks"),
  part("63-riven-tasks"),
  "renderBoardReviewNote = () => {}; scheduleBoardReviews = () => {};",
  "({ queue: new UserTaskQueue({ store: new UserTaskMemoryStore(), types: USER_TASK_TYPES, engine: ENGINE_ID, lock: (_name, fn) => fn() }),",
  "submit: submitSavedAppraisals, trackers: boardReviewTrackers, type: USER_TASK_TYPES.get('appraisal') })",
].join("\n"), ctx);
queue = create.queue;

let recorded;
create.type.save = async (_task, result) => { recorded = result; return "graded"; };
const [appraisal] = await create.submit(["one"], ruler);
assert.equal((await queue.wait(appraisal.id)).state, "succeeded");
await queue.deliverEvents();
const selected = [...Array.from({ length: 10 }, (_, i) => i), ...Array.from({ length: 10 }, (_, i) => i + 12)];
assert.deepEqual(events, ["old board", ...selected.map(i => "fixture-" + i), "optimize", "new board"]);
assert.deepEqual([...queue.list()].map(task => task.kind), [...selected.map(() => "simulation"), "appraisal"]);
assert.equal(results.length, 20);
assert.equal(create.trackers.size, 1);
const [review] = create.trackers.values();
assert.equal(review.baseline_score, 99);
assert.equal(review.observed_score, 100);
assert.equal(review.request.runs, 1000);
assert.equal(recorded.verdict.reference_rows[0].score, current[0].score);

const duringFailure = events.length;
ctx.simulateFleet = async () => ({ ok: false, error: "Reference simulation refused" });
await assert.rejects(create.submit(["one"], ruler), /Reference simulation refused/);
assert.deepEqual(events.slice(duringFailure), ["new board"]);
console.log("ok — 10 + 10 ordinary simulations, exact ruler and equipped rolls, normal numeric review, then optimization and a fresh grading board");
