// SPDX-License-Identifier: AGPL-3.0-or-later
// `node scripts/check_gain_branch.mjs` tests scheduling without a worker.
// Add `--wasm <pkg> --before <git-ref> [--baseline-wasm <pkg>] [--rows]` for the
// full old/new receipt. `--data-pack <file>` (repeatable) supplies shared data
// when the artifact folders contain only wasm-bindgen output.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";
import vm from "node:vm";
import { loadEngine } from "./wasm_engine.mjs";

const sources = ["54-gains", "56-quick-calc", "58-quick-calc-controls"];
const app = (name) => `web/src/static/app/${name}.js`;
const source = (name) => readFileSync(app(name), "utf8");
const plain = (v) => JSON.parse(JSON.stringify(v));
const tick = () => new Promise((resolve) => setImmediate(resolve));
const axis = (idx) => ({ kind: "mods", idx });
const riven = (id, roll) => ({ id, spec: { rank: 4, bonuses: [{ id: "critical_chance", roll }] } });
const fixture = (weapon = "ocucor", slots = []) => ({ weapon, slots, rivenDefs: [], mode: "base",
  evos: {}, ranks: { mods: [], arcanes: [] }, fight: { enemy: "corrupted_heavy_gunner", level: 100, duration: 2, metric: "dps" } });

function harness(state, hooks, before) {
  const calls = { list: 0, base: 0, simulate: 0, runs: 0 };
  const c = vm.createContext({ state, taskClone: plain, USER_TASK_TYPES: new Map(), userTaskSources: new Map(),
    localStorage: { getItem: () => null }, tr: String, escHtml: String, setTimeout: () => 1,
    $: () => ({ value: state.weapon }), scenarioList: () => [], presetId: (p) => p.id, activeScenario: "test",
    theFight: (over) => ({ ...state.fight, ...over }), slotModId: (id) => id, rivenPayload: () => state.rivenDefs,
    arcanes: [], arcaneRanks: [], valence: {}, assembly: null, everyRank: () => state.ranks,
    pickerPrefs: { dir: "desc" }, metricOf: (metric) => ({ per_minute: metric === "kpm" }),
    WASM: false, poolSize: () => 1, yieldToForeground: async () => {}, freeLane: () => ({}),
    api: (path, req) => { assert.equal(path, "/api/candidates"); calls.list++; return hooks.list(req); },
    simulateFleet: (req, progress) => { calls.base++; calls.runs += req.runs; return hooks.base(req, progress); },
    laneSimulate: (_, req) => { calls.simulate++; calls.runs += req.runs; return hooks.sim(req); },
  });
  const run = (code) => vm.runInContext(code, c);
  run(`const { slots, mode, evos: evoSel } = state;
    function buildPayload() { return { weapon: state.weapon, mods: slots.filter(Boolean),
      evolutions: Object.values(evoSel), mode, rivens: rivenPayload() }; }`);
  const store = source("53-task-store");
  run(store.slice(store.indexOf("class UserTransientTask")));
  for (const name of sources) run(before ? execFileSync("git", ["show", `${before}:${app(name)}`], { encoding: "utf8" }) : source(name));
  const tasks = source("59-simulation-tasks");
  run(tasks.slice(tasks.indexOf("const quickTask ="), tasks.indexOf("let simulationReport =")));
  const scan = (i) => run(`scanGains({ kind: "mods", idx: ${i} })`);
  const read = () => plain(run(`({ done: gainScan.done, total: gainScan.total, base: gainScan.base, floor: gainScan.floor,
    failed: !!gainScan.failed, running: gainScan.running, complete: !!gainScan.key, note: gainScan.note,
    ranked: [...gainScan.ids].map(id => ({id, name: id})).sort((a,b) => gainSort(a,b,["gain","drain","name"]))
      .map(({id}) => [id, gainScan.by[id] || gainScan.refused?.[id] || null]) })`));
  return { run, read, scan, calls };
}

const score = (v) => ({ ok: true, dps: v, dps_se: 1, dps_runs: [v - 1, v + 1] });
const row = (id, mods) => ({ id, payload: { mods } });
const list = (candidates) => ({ ok: true, candidates });
const delta = (a, b) => Object.fromEntries(Object.keys(b).map((k) => [k, b[k] - a[k]]));

async function scheduling() {
  const s = fixture("torid", ["cold", null, "heat"]);
  s.rivenDefs.push(riven("generated", 1));
  let candidates = [row("a", ["cold", "a", "heat"]), row("alias", ["cold", "a", "heat"]),
    row("a", ["cold", "a", "heat"]), row("rank", ["cold", "a@1", "heat"]),
    row("order", ["heat", "a", "cold"]), row("riven:generated", ["cold", "riven:generated", "heat"]), row("refused", ["refused"])];
  let fault = "lose", bodies = [];
  const h = harness(s, {
    list: () => list(candidates), base: () => score(8), sim: (req) => {
      bodies.push(plain(req));
      if (req.mods.includes("a@1") && fault === "lose") { fault = ""; return null; }
      if (req.mods.includes("refused")) return { ok: false, error: "cannot equip" };
      return score(10 + bodies.length);
    },
  });
  await h.scan(1);
  const first = h.read();
  assert.equal(first.complete, true);
  assert.equal(first.done, 6);
  assert.equal(first.done, first.total);
  const gains = Object.fromEntries(first.ranked);
  assert.deepEqual(gains.a, gains.alias);
  assert.notDeepEqual(gains.a, gains.rank);
  assert.notDeepEqual(gains.a, gains.order);
  assert.equal(gains.refused, "cannot equip");
  assert.equal(bodies.length, 6); // five distinct requests, one lost attempt

  const oldKey = h.run("gainKey()");
  s.slots.splice(1, 0, null); // Same flat build, different physical layout.
  assert.notEqual(h.run("gainKey()"), oldKey);
  let prev = { ...h.calls };
  await h.scan(1);
  assert.deepEqual(delta(prev, h.calls), { list: 1, base: 0, simulate: 1, runs: 10 });
  assert.deepEqual(h.read(), first);
  prev = { ...h.calls };
  s.rivenDefs[0] = riven("generated", 1.08);
  await h.scan(1);
  assert.deepEqual(delta(prev, h.calls), { list: 1, base: 1, simulate: 5, runs: 60 });
  prev = { ...h.calls };
  s.fight.duration++;
  await h.scan(1);
  assert.deepEqual(delta(prev, h.calls), { list: 0, base: 1, simulate: 5, runs: 60 });
  prev = { ...h.calls };
  h.run("gainPrefs.runs = 11");
  await h.scan(1);
  assert.deepEqual(delta(prev, h.calls), { list: 0, base: 1, simulate: 5, runs: 66 });
  prev = { ...h.calls };
  s.ranks.mods.push("a");
  candidates = [];
  await h.scan(1);
  assert.deepEqual(delta(prev, h.calls), { list: 1, base: 0, simulate: 0, runs: 0 });
  assert.equal(h.read().complete, true);

  let release, lateProgress, picked = 0, listFails = true;
  const r = harness(fixture(), {
    list: () => listFails ? { ok: false, error: "listing failed" } : list([row("a", ["a"]), row("b", ["b"]), row("c", ["c"])]),
    base: (_, progress) => { lateProgress = progress; return score(4); },
    sim: () => (++picked === 2 ? new Promise((_, reject) => { release = reject; }) : score(5)),
  });
  await r.scan(0);
  assert.match(r.read().note, /listing failed/);
  assert.equal(r.read().complete, false);
  listFails = false;
  r.run(`scenarioList = () => [{ id: "test", name: "Test" }]; ensureGains(${JSON.stringify(axis(0))}, null, true)`);
  await tick();
  assert.equal(r.read().done, 1);
  r.run('USER_TASK_TYPES.get("quick").act("cancel"); ensureGains({kind:"mods",idx:0})');
  assert.equal(r.run("quickTask.current.state"), "cancelled");
  assert.equal(r.read().running, false);
  assert.equal(r.calls.simulate, 2);
  r.run('ensureGains({kind:"mods",idx:0},null,true)');
  await tick();
  release(new Error("cancelled generation"));
  lateProgress(4, 10);
  await tick();
  assert.equal(r.read().complete, true);
  assert.equal(r.read().done, 3);
  assert.equal(r.read().failed, false);
  assert.equal(r.run("gainScan.phase"), "");
  assert.deepEqual(r.calls, { list: 2, base: 1, simulate: 4, runs: 50 });

  let recover = false;
  const holes = harness(fixture(), { list: () => list([row("a", ["a"]), row("b", ["b"]), row("c", ["c"])]),
    base: () => score(4), sim: (req) => req.mods[0] === "a" || recover ? score(6) : { cancelled: true } });
  await holes.scan(0);
  assert.equal(holes.read().done, 1);
  assert.equal(holes.read().complete, false);
  recover = true;
  prev = { ...holes.calls };
  await holes.scan(0);
  assert.equal(holes.read().done, 3);
  assert.equal(holes.read().complete, true);
  assert.deepEqual(delta(prev, holes.calls), { list: 0, base: 0, simulate: 2, runs: 20 });

  const edit = fixture();
  edit.rivenDefs.push(riven("generated", 1));
  const rolls = [];
  const frozen = harness(edit, { list: () => list([row("a", ["a"]), row("b", ["b"])]),
    base: () => score(4), sim: (req) => {
      rolls.push(req.rivens[0].spec.bonuses[0].roll);
      edit.rivenDefs[0].spec.bonuses[0].roll = 2;
      return score(5);
    } });
  await frozen.scan(0);
  assert.deepEqual(rolls, [1, 1]);
  assert.equal(frozen.run("gainScan.key === gainKey()"), false);
  await frozen.scan(0);
  assert.deepEqual(rolls, [1, 1, 2, 2]);
  console.log("PASS exact inputs, fan-out, refusal, counts, holes, recovery, cancellation and stale callbacks");
}

async function fullScan(pkg, baselinePkg, before, packs, showRows) {
  const open = (dir) => packs.length ? loadEngine({ files: {
    js: join(dir, "wfsim_wasm.js"), wasm: join(dir, "wfsim_wasm_bg.wasm"), packs,
  } }) : loadEngine({ pkg: dir });
  const artifact = (dir) => ({ dir, sha256: createHash("sha256")
    .update(readFileSync(join(dir, "wfsim_wasm_bg.wasm"))).digest("hex") });
  const oldEngine = open(baselinePkg), newEngine = baselinePkg === pkg ? oldEngine : open(pkg);
  const hooks = (engine, catalogs) => {
    const simulate = (req) => engine.api("/api/simulate", req);
    return { base: simulate, sim: simulate, list: (req) => {
      const r = engine.api("/api/candidates", req);
      assert.equal(r.ok, true, r.error);
      catalogs.set(req.axis.idx, r.candidates);
      return r;
    } };
  };
  console.log(JSON.stringify({ baseline: artifact(baselinePkg), current: artifact(pkg), before, sharedDataPacks: packs }));
  for (const [weapon, slots, ranked] of [
    ["torid", ["hellfire", null, null, "infected_clip", null, "serration", "vital_sense", "split_chamber"], ["cryo_rounds"]],
    ["ocucor", ["heated_charge", null, null, "convulsion", null, "hornet_strike", "barrel_diffusion"], ["pistol_gambit"]],
    ["magistar", ["shocking_touch", null, null, "fever_strike", null, "pressure_point", null, null, null, "crushing_ruin"], ["organ_shatter"]],
  ]) {
    const state = fixture(weapon, slots);
    if (weapon === "ocucor") state.fight.metric = "kpm";
    state.ranks.mods = ranked;
    state.rivenDefs = [riven("generated:one", 0.95), riven("generated:two@3", 1.05)];
    const oldCatalogs = new Map(), newCatalogs = new Map();
    const old = harness(state, hooks(oldEngine, oldCatalogs), before);
    const next = harness(state, hooks(newEngine, newCatalogs));
    for (const [visit, idx] of [1, 2, 4, 1].entries()) {
      const a = { ...old.calls }, b = { ...next.calls };
      await old.scan(idx);
      await next.scan(idx);
      const expected = old.read(), got = next.read();
      assert.equal(got.complete, true, got.note);
      assert.equal(got.done, got.total);
      const scored = (id) => expected.ranked.some(([key, g]) => id(key) && typeof g === "object" && g !== null);
      assert(scored((id) => id.startsWith(`${ranked[0]}@`)));
      assert(scored((id) => id === "riven:generated:one"));
      assert(scored((id) => id === "riven:generated:two@3"));
      assert.deepEqual(newCatalogs.get(idx), oldCatalogs.get(idx), `${weapon} slot ${idx} ordered candidate payloads`);
      assert.deepEqual(got, expected, `${weapon} slot ${idx}`);
      const hash = createHash("sha256").update(JSON.stringify(got.ranked)).digest("hex");
      const refused = got.ranked.filter(([, g]) => typeof g === "string").length;
      const beforeWork = delta(a, old.calls), afterWork = delta(b, next.calls);
      assert.deepEqual(beforeWork, { list: 1, base: 1, simulate: got.total, runs: (got.total + 1) * 10 });
      const sim = visit === 0 || visit === 2 ? got.total : refused;
      assert.deepEqual(afterWork, { list: Number(visit !== 3), base: Number(visit === 0),
        simulate: sim, runs: (sim + Number(visit === 0)) * 10 });
      console.log(JSON.stringify({ weapon, idx, candidates: got.total, refused, sha256: hash,
        before: beforeWork, after: afterWork, ...(showRows && { rows: got.ranked }) }));
    }
  }
}

await scheduling();
const { values: args } = parseArgs({ options: { wasm: { type: "string" }, before: { type: "string" },
  "baseline-wasm": { type: "string" }, "data-pack": { type: "string", multiple: true }, rows: { type: "boolean" } } });
if (Object.keys(args).length) {
  assert(args.wasm && args.before, "expected --wasm <pkg> --before <git-ref>");
  await fullScan(args.wasm, args["baseline-wasm"] ?? args.wasm, args.before, args["data-pack"] ?? [], args.rows);
}
