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

async function checkSignals() {
  let now = 1000;
  const workers = [], messages = [];
  const drain = async () => { for (let i = 0; i < 16; i++) await Promise.resolve(); };
  const sandbox = createContext({
    Date: class extends Date { static now() { return now; } }, performance: { now: () => now },
    setTimeout, clearTimeout, poolSize: () => 2,
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
  workers[2].emit({ kind: "result", payload: { pending: ["x"] } });
  await drain();
  assert.equal(resting.status.activity, "resting");
  assert.equal(resting.status.workers_active, 0, "a resting worker is not scoring");
  resting.cancelled = true;
  workers[2].emit({ kind: "rested" });
  await drain();
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

await checkSignals();
if (process.argv.includes("--signals-only")) process.exit(0);

// A FROZEN SURVEY QUESTION, cut short: its fight shorter and fewer runs and starts.
const REQUEST = {"weapon":"furis","rivens":[{"id":"rmv0r8x1z402","name":"riven 1","spec":{"bonuses":[{"id":"cold","roll":1.1},{"id":"critical_chance","roll":1.1}],"malus":null,"rank":8,"polarity":"madurai","shape":"2","drafts":{"2":{"bonuses":[{"id":"cold","roll":1.1},{"id":"critical_chance","roll":1.1}],"malus":null},"3":{"bonuses":[{"id":null,"roll":1},{"id":null,"roll":1},{"id":null,"roll":1}],"malus":null},"3+1":{"bonuses":[{"id":null,"roll":1},{"id":null,"roll":1},{"id":null,"roll":1}],"malus":{"id":null,"roll":1}},"2+1":{"bonuses":[{"id":null,"roll":1},{"id":null,"roll":1}],"malus":{"id":null,"roll":1}}}}}],"every_rank":{"arcanes":[],"mods":["hunter_track","continuous_misery","lingering_torment","perpetual_agony","augur_seeker","lasting_sting"]},"mode":"cycle","valence_element":"","valence_bonus":0,"stance":"","enemy":"thrax_centurion","level":9999,"steel_path":true,"guardian_aura":false,"ancient_protector_aura":false,"eximus":null,"squad_size":1,"headshot_pct":100,"aiming":true,"player_at":[0,0],"target_at":[0,0.5],"formation":[],"also_acting":[],"aim_at":null,"invisible":false,"airborne":false,"sliding":false,"aim_gliding":false,"overshields":false,"channeling":false,"melee_equipped":true,"solo_weapon":false,"buff_triggers_off":[],"frame":"","infinite_ammo":true,"metric":"kpm","duration":20,"buffs":{},"ability_strength":null,"abilities":[],"apl":[],"extra_stats":{},"auras":[],"shards":[],"seed":12648430,"spectral_form":false,"wf_energy_pct":1,"__weapon":"furis","runs":4,"custom_enemies":[],"finalists":1,"strategy":"quick","starts":[{"slots":["riven:rmv0r8x1z402","scorch",null,null,null,null,null,null,null,null],"evolutions":["furis_evo1_incarnon_form","furis_haven_foray","furis_practiced_grip","furis_headcracker"],"arcane":["none"],"arcane_rank":[null],"mode":null,"valence_element":null,"fixed":[{"kind":"mods","idx":0}]},{"slots":["riven:rmv0r8x1z402","frostbite",null,null,null,null,null,null,null,null],"evolutions":["furis_evo1_incarnon_form","furis_haven_foray","furis_practiced_grip","furis_headcracker"],"arcane":["none"],"arcane_rank":[null],"mode":null,"valence_element":null,"fixed":[{"kind":"mods","idx":0}]}],"candidate_runs":2,"limits":{"exclude":{"mods":[],"arcanes":[],"evolutions":[],"modes":[],"valence":[]},"mods":8,"exilus":true,"arcane_seats":[true]}};

const app = await openApp({ boot: 13000, base: process.env.WFSIM_BASE });
const { evaluate, check } = app;
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
