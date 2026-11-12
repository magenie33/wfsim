import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createContext, runInContext } from "node:vm";
import { headlessRivenGainRequest } from "../mcp/headless.js";
import { questionId } from "../worker/task_contract.js";

const source = name => readFileSync(`web/src/static/app/${name}.js`, "utf8");
const scope = createContext({ console });
for (const name of ["83-headless-card", "92-riven-input", "87-headless", "78-optimizer-page", "80-optimizer-preset", "08-checkpoint-api"]) runInContext(source(name), scope);
const read = text => JSON.parse(runInContext(`JSON.stringify(${text})`, scope));
runInContext(`
  const META = {
    weapons: [{ id: "test", modes: ["base"], mods: ["heat_card", "cold_card", "electric_card", "toxin_card"],
      evolutions: [{ tier: 1, options: [{ id: "first" }, { id: "second" }] }] }],
    search_presets: {
      strategies: [{ id: "four_elements", name: "Four elements strategy" }],
      element_starts: [["heat_card"], ["cold_card"], ["electric_card"], ["toxin_card"]],
      presets: [{ id: "riven_appraisal", riven: true, finalists: 1, candidate_runs: 10, search_strategy: "four_elements" }]
    },
    mod_pools: { rifle: ["heat_card", "cold_card", "electric_card", "toxin_card"].map(id => ({ id, max_rank: 3 })) },
    benchmarks: [{ id: "test_ruler", scenario: { duration: 10, runs: 1000 } }],
    defaults: {}, riven_rules: { max_rank: 8 }
  };
  const select = { value: "", innerHTML: "" };
  const elements = {
    weapon: { value: "test" }, "opt-search-strategy": select,
    "opt-results": { innerHTML: "" }, "opt-cancel": { addEventListener() {} }
  };
  const $ = id => elements[id] || null;
  const tr = text => text, DT = text => text;
  const escHtml = text => String(text).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/"/g, "&quot;");
  let opt = { starts: [], limits: null }, optRun = { candidate_runs: 10, finalists: 10 };
  const OPT_RUN_DEFAULTS = { finalists: 10 };
  normalizeLimits = limits => JSON.parse(JSON.stringify(limits || { exclude: { mods: [] } }));
  const normalizeStart = start => JSON.parse(JSON.stringify(start));
  const blankStart = () => headlessBuiltinStarts(META, "test")[0];
  const startsPayload = () => opt.starts.map(start => headlessStartPayload(META, start));
  const rivenPayload = () => [], everyRank = () => ({ mods: [], arcanes: [] });
  const mode = "base", valence = { element: "", bonus: 0 }, assembly = null;
  const evoSel = { 1: "second" };
  const slots = [], STANCE = 9, slotModId = () => null;
  let activeScenario = null;
  const scenarioNamed = id => id ? { builtin: id } : null, benchList = () => META.benchmarks, simRuns = () => 73;
  const theFight = (overrides = {}) => ({ duration: 10, runs: simRuns(), ...overrides }), startPayload = start => headlessStartPayload(META, start);
  let updates = 0;
  updateOptEstimate = () => { updates++; };
  const document = { activeElement: null };
  const optimizationFinalStage = () => false;
  const optimizationTaskProgress = () => ({ phase: "Optimization", count: "", detail_html: "" });
  const watchOptimizationClocks = () => {};
`, scope);

const blank = read(`headlessBuiltinStarts(META, "test")`);
assert.equal(blank.length, 1);
assert.ok(blank[0].build.slots.every(slot => slot.mod == null));
assert.deepEqual(blank[0].fixed, []);
const pinned = read(`headlessBuiltinStarts(META, "test", "riven:R", { 1: "second" })`);
assert.equal(pinned.length, 1);
assert.deepEqual(pinned[0].build.slots.filter(slot => slot.mod).map(slot => slot.mod), ["riven:R"]);
assert.deepEqual(pinned[0].fixed, ["mods:0"]);
assert.deepEqual(pinned[0].build.evoSel, { 1: "second" });
const appraisal = read(`headlessRivenGainRequest(META, { weapon: "test", ruler: "test_ruler", card: {}, rivenId: "R" }).request`);
assert.equal(appraisal.strategy, "quick");
assert.equal(appraisal.search_strategy, "four_elements");
assert.equal(appraisal.starts.length, 1);
assert.deepEqual(appraisal.starts[0].slots.filter(Boolean), ["riven:R"]);
assert.deepEqual(appraisal.starts[0].fixed, [{ kind: "mods", idx: 0 }]);
assert.deepEqual(JSON.parse(JSON.stringify(headlessRivenGainRequest(read("META"),
  { weapon: "test", ruler: "test_ruler", card: {}, rivenId: "R" }).request)), appraisal);
runInContext(`applyOptState(builtinSearchState(META.search_presets.presets[0], "riven:R"));`, scope);
assert.equal(read(`optimizeBody().search_strategy`), appraisal.search_strategy);
assert.equal(read(`optimizeBody().starts.length`), 1);
assert.deepEqual(read(`optimizeBody().starts[0].fixed`), appraisal.starts[0].fixed);
assert.equal(read(`optimizeBody().runs`), 73);
runInContext('activeScenario = "test_ruler";', scope);
assert.equal(read(`optimizeBody().runs`), 1000);
runInContext('activeScenario = null;', scope);
console.log("  ok  built-ins and headless appraisal supply one user start, retaining its riven and evolutions");

const card = { bonuses: [{ id: "critical_damage", roll: 1.07 }, { id: "multishot", roll: 0.94 }],
  malus: { id: "status_duration", roll: 1.03 }, rank: 8, polarity: "madurai" };
scope.identityCard = card;
runInContext(`
  const savedClock = Date.now, savedRandom = Math.random;
  let identityClock = 1700000000000;
  Date.now = () => identityClock++;
  Math.random = () => 0.25;
`, scope);
const freeze = () => read(`headlessRivenGainRequest(META, { weapon: "test", ruler: "test_ruler", card: identityCard })`);
const firstFreeze = freeze(), secondFreeze = freeze();
runInContext(`Date.now = savedClock; Math.random = savedRandom;`, scope);
assert.deepEqual(firstFreeze, secondFreeze, "separate appraisals of the same inputs must freeze the same question");
const meta = read("META");
const wire = value => JSON.parse(JSON.stringify(value));
const frozen = wire(headlessRivenGainRequest(meta, { weapon: "test", ruler: "test_ruler", card }));
assert.deepEqual(frozen, firstFreeze, "the bot and page share default request identity");
const identity = await questionId("optimize", frozen);
assert.equal(await questionId("optimize", secondFreeze), identity);
const riven = frozen.request.rivens[0];
assert.equal(frozen.request.starts[0].slots[0], `riven:${riven.id}`);
assert.deepEqual(Object.keys(frozen.context.rivens), [`riven:${riven.id}`]);
for (const change of [
  copy => { copy.bonuses[0].roll = 1.08; }, copy => { copy.bonuses[0].id = "damage"; },
  copy => { copy.malus.roll = 1.02; }, copy => { copy.malus.id = "zoom"; },
  copy => { copy.rank = 7; }, copy => { copy.polarity = "naramon"; },
]) {
  const changed = structuredClone(card); change(changed);
  const other = wire(headlessRivenGainRequest(meta, { weapon: "test", ruler: "test_ruler", card: changed }));
  assert.notEqual(await questionId("optimize", other), identity, "card values must remain distinct questions");
}
for (const key of ["weapon", "duration", "runs", "seed", "candidate_runs", "search_strategy"]) {
  const changed = structuredClone(frozen);
  changed.request[key] = typeof changed.request[key] === "number" ? changed.request[key] + 1 : "other";
  assert.notEqual(await questionId("optimize", changed), identity, `${key} remains part of the question`);
}
for (const id of ["saved-card-A", "saved-card-B"]) {
  const explicit = headlessRivenGainRequest(meta, { weapon: "test", ruler: "test_ruler", card, rivenId: id });
  assert.equal(explicit.request.rivens[0].id, id, "owned card references are preserved");
  const normalized = JSON.parse(JSON.stringify(explicit).replaceAll(id, riven.id));
  assert.deepEqual(normalized, JSON.parse(JSON.stringify(frozen)), "only the task-local name differs");
}
console.log("  ok  fresh appraisal identity is deterministic; card values, fight and explicit saved-card references stay distinct");

runInContext(`
  const legacy = {
    starts: [
      { build: { weapon: "test", slots: [{ mod: "heat_card", rank: 2 }, { mod: null }] }, fixed: ["mods:1"] },
      { build: { weapon: "test", slots: [{ mod: "cold_card" }] }, fixed: ["mods:0"] }
    ],
    final: { build: { weapon: "test", slots: [{ mod: "heat_card" }] }, fixed: ["mods:0"] },
    limits: { exclude: { mods: ["excluded_card"] } }, candidate_runs: 1, finalists: 3
  };
  applyOptState(legacy);
  renderOptStrategy();
`, scope);
assert.equal(read(`optimizeBody().strategy`), "quick");
assert.equal(read(`"search_strategy" in optimizeBody()`), false);
assert.equal(read(`"search_strategy" in snapshotOpt()`), false);
assert.deepEqual(read(`snapshotOpt().starts`), read(`legacy.starts`));
assert.deepEqual(read(`snapshotOpt().final`), read(`legacy.final`));
assert.deepEqual(read(`optimizeBody().limits`), read(`legacy.limits`));
assert.match(read(`select.innerHTML`), /value="" selected disabled>Saved starts only/);
runInContext(`select.value = "four_elements"; select.onchange();`, scope);
assert.equal(read(`optimizeBody().search_strategy`), "four_elements");
assert.equal(read(`snapshotOpt().search_strategy`), "four_elements");
assert.deepEqual(read(`snapshotOpt().starts`), read(`legacy.starts`));
assert.deepEqual(read(`snapshotOpt().final`), read(`legacy.final`));
assert.deepEqual(read(`optimizeBody().limits`), read(`legacy.limits`));
runInContext(`applyOptState(blankOpt()); renderOptStrategy();`, scope);
assert.equal(read(`optimizeBody().starts.length`), 1);
assert.equal(read(`optimizeBody().search_strategy`), "four_elements");
assert.match(read(`select.innerHTML`), /value="four_elements" selected>Four elements strategy/);
assert.equal(read(`select.innerHTML.includes("Saved starts only")`), false);
console.log("  ok  legacy searches stay legacy; selecting a strategy preserves starts, pins and final constraints");

runInContext(`renderOptProgress({ phase: "running", rounds: 0, starts: [
  { start: 0, branch: "original", branch_index: 0, settled: true },
  { start: 0, branch: "heat", branch_index: 1, settled: false },
  { start: 0, branch: "heat", branch_index: 2, settled: false },
  { start: 1, branch: "cold", branch_index: 3, settled: true }
] });`, scope);
const progress = read(`elements["opt-results"].innerHTML`);
assert.match(progress, /2 of 4 branches settled/);
assert.equal((progress.match(/start 1 · heat/g) || []).length, 2);
assert.match(progress, /start 2 · cold/);
assert.doesNotMatch(progress, /start [34]/);
runInContext(`renderOptProgress({ phase: "running", rounds: 0, starts: [{ settled: true }, { settled: false }] });`, scope);
assert.match(read(`elements["opt-results"].innerHTML`), /1 of 2 starts settled/);
assert.match(read(`elements["opt-results"].innerHTML`), /start 2:/);
console.log("  ok  branch progress retains original start indexes and distinguishes branches from user starts");

const compute = source("73-compute"), computeAt = compute.indexOf("function computeLanes(");
runInContext(compute.slice(computeAt, compute.indexOf("\n}\n", computeAt) + 3), scope);
runInContext(`
  const computeBox = { children: [0, 1].map(() => {
    const label = { textContent: "" }, bar = { style: {} };
    return { label, classList: { toggle() {} }, querySelector: name => name === "span" ? label : bar };
  }) };
  computeLanes(computeBox, [
    { start: 0, branch: "heat", settled: true },
    { start: 0, branch: "cold", settled: true }
  ]);
`, scope);
assert.deepEqual(read(`computeBox.children.map(child => child.label.textContent)`), [
  "start 1 · heat · settled", "start 1 · cold · settled"
]);
console.log("  ok  compute-page lanes retain the same original start and branch labels");

runInContext(`
  const result = (score, start, branch) => ({
    mods: [String(score)], kill_progress: score, from_starts: [{ start, branch }]
  });
  const parts = [
    { ok: true, search_strategy: "four_elements", starts: 1, branches: 2, finalists: 3,
      results: [result(4, 0, "original")], start_results: [{ start: 0, result: result(4, 0, "original") }] },
    { ok: true, search_strategy: "four_elements", starts: 2, branches: 2, finalists: 3,
      results: [result(7, 0, "heat"), result(3, 1, "cold")],
      start_results: [{ start: 0, result: result(7, 0, "heat") }, { start: 1, result: result(3, 1, "cold") }] },
    { ok: true, search_strategy: "four_elements", starts: 1, branches: 1, finalists: 3,
      results: [], failed_starts: [{ start: 1, why: "no legal build" }],
      failed_branches: [{ start: 1, branch: "toxin", why: "no legal build" }] }
  ];
`, scope);
const merged = read(`woptMerge(parts)`);
assert.equal(merged.starts, 2);
assert.equal(merged.branches, 5);
assert.deepEqual(merged.failed_starts, []);
assert.deepEqual(merged.failed_branches, [{ start: 1, branch: "toxin", why: "no legal build" }]);
assert.deepEqual(read(`woptMerge([parts[2]], { search_strategy: "four_elements", starts: [{}, {}] }).failed_starts`),
  [{ start: 1, why: "no legal build" }]);
assert.deepEqual(merged.start_results.map(answer => [answer.start, answer.result.kill_progress]), [[0, 7], [1, 3]]);
assert.equal(read(`woptMerge(parts.map(({ search_strategy, ...part }) => part)).starts`), 4);
assert.equal(read(`woptMerge([{ starts: 1, branches: 2 }, { starts: 1, branches: 3 }],
  { search_strategy: "four_elements", starts: [{}, {}] }).starts`), 2);
console.log("  ok  fleet merging counts distinct original starts, sums branches and retains each start's best answer");

const workers = [];
const fleet = createContext({
  performance, Date, poolSize: () => 2,
  computeRun: async (_, work, live) => live() ? work() : { cancelled: true },
  watchSilence: () => ({ heard() {}, start() {}, done() {}, clear() {} }),
  spawnEngine: () => {
    const worker = { postMessage() {}, terminate() {} };
    workers.push(worker); return worker;
  }
});
runInContext("let wopt = null, woptNextId = 1;", fleet);
runInContext(source("08-checkpoint-api"), fleet);
runInContext(`woptStart({ strategy: "descent", search_strategy: "four_elements", starts: [{}, {}] });`, fleet);
workers[0].onmessage({ data: { kind: "progress", payload: {
  phase: "searching", starts: [{ start: 0, branch: "heat", branch_index: 1 }]
} } });
workers[1].onmessage({ data: { kind: "progress", payload: {
  phase: "searching", starts: [{ start: 0, branch: "cold", branch_index: 2 }, { start: 1, branch: "original", branch_index: 3 }]
} } });
const lanes = JSON.parse(runInContext("JSON.stringify(woptStatus().starts)", fleet));
assert.deepEqual(lanes.map(lane => [lane.start, lane.branch_index]), [[0, 1], [0, 2], [1, 3]]);
console.log("  ok  sharded progress includes all branches without renumbering original starts");
