// Exercise the shared scheduler on a controlled clock, including fractional-core budgets.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import vm from "node:vm";

const read = () => readFileSync(new URL("../web/src/static/app/06-transport.js", import.meta.url), "utf8");
function fixture(source, saved = {}) {
  let now = 0, id = 0;
  const timers = new Map(), storage = new Map(Object.entries(saved));
  const wake = (fn, ms) => { timers.set(++id, { fn, at: now + ms }); return id; };
  const clear = key => timers.delete(key);
  const scope = vm.createContext({ performance: { now: () => now }, setTimeout: wake, clearTimeout: clear,
    setInterval: () => 0, document: { hidden: false, addEventListener() {} }, renderComputePicker() {},
    localStorage: { getItem: key => storage.get(key) ?? null, setItem: (key, value) => storage.set(key, value) },
    detectedCores: () => ({ n: 1 }), readerBusy: () => false, computeNow: null, boardVerifyOn: () => true,
    computeHeld: () => false, computePct: 50 });
  vm.runInContext(source.slice(0, source.indexOf("/// ONE POOL OF WORKERS"))
    + "\nglobalThis.ranges = { ComputeBudget, computeRange, setComputeRange, computeBudget, computeRun, computeElapsed };", scope);
  const drain = async () => { for (let i = 0; i < 24; i++) await Promise.resolve(); };
  return { ...scope.ranges, storage, wake, clear, now: () => now, drain,
    async advance(ms) {
      const end = now + ms;
      await drain();
      for (;;) {
        const next = [...timers].filter(([, timer]) => timer.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
        if (!next) break;
        now = next[1].at; timers.delete(next[0]); next[1].fn(); await drain();
      }
      now = end; await drain();
    },
  };
}
const equal = (actual, expected, label) => assert.equal(JSON.stringify(actual), JSON.stringify(expected), label);

export async function checkComputeRanges(source = read()) {
  const defaults = fixture(source);
  equal(defaults.computeRange("solo"), { min: 0, max: 50 }, "Solo defaults to 0–50");
  equal(defaults.computeRange("together"), { min: 0, max: 50 }, "Together defaults to 0–50");
  const old = fixture(source, { "wfsim-compute": "balanced", "wfsim-community-share": "30" });
  equal(old.computeRange("solo"), { min: 50, max: 50 }, "legacy explicit Solo is preserved");
  equal(old.computeRange("together"), { min: 30, max: 30 }, "legacy Together is preserved without increasing its share");
  const consented = fixture(source, { "wfsim-compute-consent": JSON.stringify({ v: 3, on: true }) });
  equal(consented.computeRange("together"), { min: 30, max: 30 }, "existing consent keeps the old default until its owner edits it");
  const corrupt = fixture(source, { "wfsim-compute-ranges": "broken", "wfsim-compute": "broken" });
  equal(corrupt.computeRange("solo"), { min: 0, max: 50 }, "unreadable settings recover");
  defaults.setComputeRange("solo", "min", 73);
  equal(defaults.computeRange("solo"), { min: 73, max: 73 }, "raising the minimum moves the maximum");
  defaults.setComputeRange("solo", "max", 27);
  equal(defaults.computeRange("solo"), { min: 27, max: 27 }, "lowering the maximum moves the minimum");
  defaults.setComputeRange("solo", "min", 3);
  equal(defaults.computeRange("solo"), { min: 3, max: 27 }, "one-percentage-point endpoints persist");
  assert.equal(defaults.setComputeRange("solo", "max", ""), false, "an empty edit is not zero");
  const restored = fixture(source, Object.fromEntries(defaults.storage));
  equal(restored.computeRange("solo"), { min: 3, max: 27 }, "a reload preserves the range");

  const f = fixture(source);
  let cores = 1;
  const ranges = { solo: { min: 100, max: 100 }, together: { min: 100, max: 100 } };
  const active = { solo: true, together: true };
  const budget = new f.ComputeBudget({ cores: () => cores, range: kind => ranges[kind], active: kind => active[kind],
    now: f.now, wake: f.wake, clear: f.clear });
  equal(budget.shares(), { solo: 50, together: 50 }, "100–100 on both sides allocates 50/50");
  ranges.together = { min: 50, max: 50 };
  assert(Math.abs(budget.shares().solo - 200 / 3) < 1e-10, "100/50 requests allocate 2:1");
  ranges.solo = { min: 0, max: 50 }; ranges.together = { min: 0, max: 50 };
  budget.pressure(250);
  assert.equal(budget.target("solo"), 25, "observed pressure lowers an adaptive target");
  budget.pressure(500);
  assert.equal(budget.target("solo"), 0, "a zero minimum permits a complete pause");
  budget.pressure(0);
  assert.equal(budget.target("solo"), 1, "recovery is gradual at one-percentage-point resolution");
  budget.pressure(0, 1);
  assert.equal(budget.target("solo"), 50, "elapsed hidden-tab recovery cannot leave a zero minimum stalled forever");
  ranges.solo = { min: 50, max: 100 }; budget.headroom = 0;
  assert.equal(budget.target("solo"), 50, "50–100 keeps its minimum request under pressure");
  ranges.solo = { min: 100, max: 100 };
  assert.equal(budget.target("solo"), 100, "a fixed range ignores adaptive pressure");

  active.solo = false; active.together = false;
  ranges.solo = { min: 0, max: 0 }; ranges.together = { min: 0, max: 0 };
  budget.changed();
  let ran = 0, cancelled = false;
  const paused = budget.run("solo", () => { ran++; return "resumed"; });
  const stopped = budget.run("together", () => { ran++; }, () => !cancelled);
  await f.advance(500);
  assert.equal(ran, 0, "zero upper bounds run no work");
  cancelled = true; await f.advance(100);
  assert.equal((await stopped).cancelled, true, "cancel settles while the budget is zero");
  ranges.solo = { min: 100, max: 100 }; budget.changed(); await f.advance(1);
  assert.equal(await paused, "resumed", "raising the limit resumes the same queued calculation");
  assert.equal(ran, 1, "a resumed piece executes exactly once");

  const one = fixture(source);
  const fractional = new one.ComputeBudget({ cores: () => 1, range: () => ({ min: 1, max: 1 }),
    now: one.now, wake: one.wake, clear: one.clear });
  let finished = 0, flying = 0, peak = 0;
  const pieces = Array.from({ length: 20 }, (_, index) => fractional.run("solo", () => new Promise(resolve => {
    peak = Math.max(peak, ++flying);
    one.wake(() => { flying--; finished++; resolve(index); }, 10);
  })));
  await one.advance(1_000);
  assert.equal(finished, 1, "1% cannot run the whole queue in its initial burst");
  await one.advance(19_000);
  equal(await Promise.all(pieces), Array.from({ length: 20 }, (_, index) => index), "throttling preserves every piece and its result");
  assert.equal(peak, 1, "a one-core host never runs multiple pieces together");
  assert.equal(finished * 10 / 20_000, 0.01, "1% works on a single core through actual time slicing");
  const timing = fixture(source);
  timing.setComputeRange("solo", "max", 0);
  const timed = timing.computeRun("solo", () => new Promise(resolve => timing.wake(() => resolve({ measured: true }), 10)));
  await timing.advance(100);
  timing.setComputeRange("solo", "max", 50);
  await timing.advance(20);
  const measured = await timed;
  assert.equal(timing.computeElapsed(measured, 0), 10, "budget waits do not become reported compute time");
  assert.equal(JSON.stringify(measured), '{"measured":true}', "timing cannot alter an engine response on the wire");

  const concurrent = fixture(source), counts = { solo: 0, together: 0 }, starts = [];
  const fair = new concurrent.ComputeBudget({ cores: () => 1, range: () => ({ min: 100, max: 100 }),
    now: concurrent.now, wake: concurrent.wake, clear: concurrent.clear });
  const jobs = ["solo", "together"].flatMap(kind => Array.from({ length: 20 }, () => fair.run(kind, () => new Promise(resolve => {
    starts.push([kind, concurrent.now()]);
    concurrent.wake(() => { counts[kind]++; resolve(); }, 10);
  }))));
  await concurrent.advance(500); await Promise.all(jobs);
  equal(counts, { solo: 20, together: 20 }, "both 100–100 consumers finish through the same budget");
  assert(starts.slice(0, 10).some(([kind]) => kind === "together"), "a second consumer is not starved behind the first queue");
  const reports = readFileSync(new URL("../web/src/static/app/93-community-calculations.js", import.meta.url), "utf8");
  const cancelledReport = vm.createContext({ AbortController, performance, communityLanes: () => 1,
    makeLane: () => ({ send: async () => ({ ok: false, cancelled: true }), abandon() {}, worker: { terminate() {} } }) });
  vm.runInContext(reports.slice(0, reports.indexOf("async function communityOptimizationReport"))
    + "\nglobalThis.run = communitySimulationReport;", cancelledReport);
  await assert.rejects(cancelledReport.run({ runs: 1 }, new AbortController().signal),
    error => !error.engineRefused, "a budget pause is never reported as an engine refusal");
  console.log("compute ranges: migration, integer inputs, adaptive targets, zero/resume/cancel, fractional CPU time and proportional sharing pass");
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) await checkComputeRanges();
