// HOW HARD YOU MEASURE IS A NUMBER SOMEONE CAN SET, in all three modules.
//
// A run count is three different kinds of thing — the simulator's own setting,
// a floor the quick calc keeps to itself, and the optimizer's final round — so
// this walks all three and asserts the number a reader picks is the number the
// request carries.
//
//   the SIMULATOR   defaults to 1000, the official rulers' count, so a first
//                   number is comparable with the board without touching a box
//   the QUICK CALC  takes its own, floored at 10 — where a status mod stops
//                   being a coin flip (M24)
//   the OPTIMIZER   takes its own for the final round, TYPED, saved by no
//                   preset and pinned by no ruler
//
// The last one is why this is a check rather than a comment: a blank box
// meaning "the fight's own count" is one control with two readings, which reads
// as broken and works, or reads as fine and sends 0. It is a preference, in
// neither half of the tab and in neither preset, and this asserts all three.
//
//   node scripts/check_run_counts.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep } = app;

const r = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  localStorage.clear();
  history.pushState({},'','/weapons/Verglas_Prime'); route(); await sleep(3000);
  const out = {};

  // ---- the SIMULATOR's count is DECOUPLED from the fight ---------------
  // It is a preference, not a scenario field: 100 by
  // default, and a scenario cannot carry one at all — which is what stops the
  // rulers' 1,000 from arriving as a local setting the moment you open one.
  out.simDefault = defaultScenario().runs;   // undefined: not a scenario field
  out.liveDefault = simRuns();
  out.inSnapshot = "runs" in snapshotScenario();

  // ---- the QUICK CALC takes its own, floored ---------------------------
  out.qcBefore = gainScenario().scenario.runs;
  const qcKeyBefore = gainKey();
  gainPrefs = { ...gainPrefs, runs: 250 };
  out.qc250 = gainScenario().scenario.runs;
  // …and the FIGHT it measures under is untouched: a scan's precision is the
  // scan's, not an edit to the scenario.
  out.simUntouched = simRuns();
  // The scan's cache key is DERIVED from the fight it will run, so a count
  // change invalidates it without anything being told to.
  out.qcKeyMoved = gainKey() !== qcKeyBefore;
  // The floor bites on a paste as well as on the box.
  gainPrefs = { ...gainPrefs, runs: 1 };
  out.qcFloored = gainScenario().scenario.runs;

  // …AND IT REACHES THE SERVER, which is the assertion this check was missing.
  //
  // Everything above reads gainScenario().scenario.runs — an intermediate,
  // and it was RIGHT for months while the box did nothing: the scan wrote its
  // count into that object and then spread the page's own fight payload over
  // the top, so every scan ran at the simulator's count and every chip's
  // tooltip quoted a number that had never been sent. A
  // count is only decoupled if the REQUEST says so, so this intercepts the
  // request.
  gainPrefs = { ...gainPrefs, runs: 30 };
  const seen = [];
  const realApi = api;
  window.api = async (p, b) => { if (p === '/api/simulate') seen.push(b); return realApi(p, b); };
  try { await scanGains({ kind: 'mods', idx: 0 }, null); } catch (_) {}
  window.api = realApi;
  out.qcPosted = seen.length ? seen[0].runs : null;
  out.qcPostedIsNotTheSim = seen.length ? seen[0].runs !== simRuns() : null;
  gainPrefs = { ...gainPrefs, runs: 10 };

  // The control is on screen and says what unit it is in.
  document.querySelector('[data-tab="builder"]')?.click(); await sleep(300);
  renderQuickCalc(); await sleep(100);
  const box = document.getElementById('gp-runs');
  out.qcOnScreen = !!box && box.min === '10';
  if (box) {
    box.value = '40'; box.dispatchEvent(new Event('change',{bubbles:true})); await sleep(200);
    out.qcFromScreen = gainScenario().scenario.runs;
    box.value = '2'; box.dispatchEvent(new Event('change',{bubbles:true})); await sleep(200);
    out.qcSnapBack = box.value;   // the box shows what was actually taken
  }

  // ---- the OPTIMIZER's final round -------------------------------------
  document.querySelector('[data-tab="optimizer"]')?.click(); await sleep(600);
  const ids=['serration','split_chamber','point_strike','vital_sense'];
  ids.forEach(i=>{opt.mods[i]='search';});
  updateOptEstimate(); renderOptMods(); await sleep(300);

  const sendOnce = async () => {
    const seen=[]; const real=window.api;
    window.api = async (p,b) => { seen.push([p,b]); throw new Error('stop'); };
    try { await runOptimize(); } catch {}
    window.api = real;
    return (seen.find(([p])=>p==='/api/optimize')||[])[1]||{};
  };
  const set = async (el,v) => { const n=document.getElementById(el); n.value=String(v); n.dispatchEvent(new Event('input',{bubbles:true})); await sleep(200); };

  // IT IS A PREFERENCE, TYPED, AND IN NEITHER PRESET. Riding the search preset
  // with a BLANK box meaning "the fight's own count" is one control with two
  // readings, and the wrong home for both: a run count is not what to search,
  // and the fight carries none.
  const setC = async (el,v) => { const n=document.getElementById(el); n.value=String(v); n.dispatchEvent(new Event('change',{bubbles:true})); await sleep(200); };
  const runsBox = document.getElementById('opt-runs');
  out.optOnScreen = !!runsBox;
  // FILLED, never blank — the reader can always say what the last round used.
  out.optShown = runsBox ? runsBox.value : 'MISSING';
  out.sentDefault = (await sendOnce()).final_runs;
  // IN NEITHER BOX. The two halves are the two presets; this is outside both,
  // which is the whole claim the page is making by drawing it there.
  out.optRunsOutside = !!runsBox
    && !runsBox.closest('#opt-plan') && !runsBox.closest('#opt-fight-half');
  // A number of its own reaches the request, and does not move the simulator's.
  await setC('opt-runs', 60);
  out.sentOwn = (await sendOnce()).final_runs;
  out.simStillDefault = simRuns();
  // NOT SAVED BY THE SEARCH PRESET, which is the half a round-trip test would
  // have got backwards before today: the snapshot must not carry it at all,
  // and restoring a scope taken while it read something else must leave it
  // exactly where the reader put it.
  const snap = snapshotOpt();
  out.notInPreset = !('runs' in snap) && !('threads' in snap);
  await setC('opt-runs', 250);
  applyOptState(snap); await sleep(200);
  out.survivesPreset = [finalRuns(), document.getElementById('opt-runs').value];
  await setC('opt-runs', 100);
  // …and the CPU-thread box is gone: how much of the machine the page may use
  // is the topbar's one setting.
  out.threadsBox = !!document.getElementById('opt-threads');
  out.sentThreads = 'threads' in (await sendOnce());

  // ---- A LONG SIM SAYS HOW FAR IT HAS GOT -------------------------------
  //
  // The run count is unbounded and so is the cost per run: a single-target
  // fight is about a millisecond, a 361-body one is ~28, so the rulers' 1000
  // runs is half a minute. It runs on a WORKER, so the page was never frozen —
  // but a button reading Simulating... for half a minute is reported as a
  // hang, and it should be.
  {
    const ruler = scenarioList().find((p) => presetId(p) === 'group_clear');
    if (ruler) {
      const cfg = scenarioBarCfg();
      cfg.setActive('group_clear');
      cfg.apply(ruler.state);
      renderSim();
      await sleep(1500);
      // ENOUGH RUNS THAT THERE IS A WAIT TO REPORT. The weapon this check
      // opens with is a SENTINEL one — cheap even against 361 bodies — so a
      // couple of hundred runs is a second and the remaining time is under
      // one at every sample point, where it is deliberately hidden ("about 0s
      // left" is noise). 600 buys about three seconds of wait, which is the
      // thing being tested.
      // A FLEET MAKES THIS FAST, so the count has to grow with it: eight
      // workers over a sentinel weapon finish 600 runs before a 100 ms sampler
      // sees anything worth reporting.
      setSimRuns(4000);
      out.progBodies = 1 + (sim.formation || []).length;
      const seen = [];
      document.getElementById('run-sim').click();
      for (let i = 0; i < 600; i++) {
        await sleep(100);
        const n = document.getElementById('sim-prog-n');
        const b = document.getElementById('sim-prog');
        if (n && n.textContent) seen.push([n.textContent, b ? b.style.width : '']);
        if (!document.getElementById('run-sim').disabled) break;
      }
      const uniq = [...new Set(seen.map((x) => x.join('|')))];
      out.progSteps = uniq.length;
      out.progFirst = uniq[0] || '';
      out.progLast = uniq[uniq.length - 1] || '';
      // THE COUNT, not just a proportion — a reader can act on 412 / 1000.
      // NO REGEX HERE: a backslash inside this template literal is eaten
      // before the page ever sees it, so /^\d+/ arrives as /^d+/ and the
      // string stops parsing. Plain string work has no such trap.
      out.progCounts = uniq.every((u) => {
        const words = u.split('|')[0].trim().split(' ');
        return words[1] === '/' && words[2] === '4000';
      });
      out.progAll = uniq.slice(0, 6);
      // …AND HOW MUCH LONGER, which is the number they actually want.
      out.progEta = uniq.some((u) => /left|还剩/.test(u));
      // A BAR THAT MOVES: the widths are non-decreasing and reach the end.
      const w = uniq.map((u) => parseFloat(u.split('|')[1]) || 0);
      out.progRises = w.every((v, i) => i === 0 || v >= w[i - 1]);
      // …TO THE END, or as near as the last paint got: the loop stops the
      // moment the button re-enables, which can be the same tick as the 100%.
      out.progEnds = w[w.length - 1] >= 90;
    }
  }

  // ---- …AND IT CAN BE STOPPED ------------------------------------------
  //
  // The wait is unbounded, so a reader who realises the fight is too big must
  // not have to reload the page. There is no yield point inside a wasm call to
  // check a flag at, so the worker is TERMINATED — instant, and costs nothing
  // to recover from since a sim carries no state between calls.
  {
    const ruler = scenarioList().find((p) => presetId(p) === 'group_clear');
    if (ruler) {
      setSimRuns(4000);
      document.getElementById('run-sim').click();
      await sleep(2000);
      out.stopOffered = !!document.getElementById('sim-stop');
      out.stopMidRun = (document.getElementById('sim-prog-n') || {}).textContent || '';
      const t0 = Date.now();
      document.getElementById('sim-stop').click();
      // MEASURED AT THE CLICK, not after a sleep: terminating a worker is
      // synchronous, so what is being timed is the call and not the wait.
      out.stopMs = Date.now() - t0;
      await sleep(700);
      out.stopSaid = (document.querySelector('#sim-results .placeholder') || {}).textContent || '';
      out.stopFreed = !document.getElementById('run-sim').disabled;
      // A CANCEL IS NOT A FAILURE, and it must not leave the page broken: the
      // next run builds a fresh worker and answers.
      setSimRuns(5);
      document.getElementById('run-sim').click();
      for (let i = 0; i < 300; i++) {
        await sleep(200);
        if (!document.getElementById('run-sim').disabled) break;
      }
      out.stopRecovered = !!document.querySelector('#sim-results table, #sim-results .fold');

      // ---- A FLEET ANSWERS WHAT ONE WORKER ANSWERS ---------------------
      //
      // The runs of a simulation are independent given their index, so the
      // page shards them across a worker fleet and merges in Rust. That is
      // worth nothing if the answer depends on how many workers happened to be
      // free, so this asserts the two paths agree — and it asserts it on the
      // WIRE, where the engine's own test cannot reach: a shard crosses the
      // wasm boundary as JSON, and a JSON number in JavaScript is a double, so
      // the run's 64-bit RNG state came back ROUNDED and the merge replayed a
      // fight that never happened. Every mean matched; only the
      // median run's figures moved.
      const body = { ...buildPayload(), ...theFight({ runs: 40 }) };
      // THE PAGE'S ONE POOL. The simulator had a fleet of its own until
      // 2026-08-18, beside the quick calc's lanes and the single rpc worker;
      // they are one pool now and this reads it by its new name.
      out.fleetLanes = lanes().length;
      const fleet = await simulateFleet(body, () => {});
      const solo = await api('/api/simulate', body);
      out.fleetDiff = ['score', 'score_mean', 'dps', 'burst_dps', 'max_hit', 'procs', 'kills_std']
        .map((k) => [k, fleet[k], solo[k]])
        .filter(([, a, b]) => Math.abs(a - b) > Math.abs(b) * 1e-9 + 1e-9)
        .map(([k, a, b]) => k + ': ' + a + ' vs ' + b);
      out.fleetRuns = [fleet.runs, solo.runs];
    }
  }

  return out;
})()`);

// HOW HARD YOU MEASURE IS NOT PART OF THE FIGHT. Two
// claims, and the second is the one that makes it true rather than merely
// defaulted: a scenario cannot carry a run count at all, so opening an
// official ruler — whose yaml says 1,000 — cannot silently make the page slow,
// and saving a fight cannot record a precision.
check("the simulator runs at 100 by default", r.liveDefault === 100, `${r.liveDefault}`);
check("...and a scenario cannot carry a run count at all",
  r.simDefault === undefined && r.inSnapshot === false,
  `default ${r.simDefault}, in snapshot ${r.inSnapshot}`);

check("the quick calc starts at its floor of 10", r.qcBefore === 10, `${r.qcBefore}`);
check("...takes the count it is given", r.qc250 === 250, `${r.qc250}`);
check("...without touching the fight", r.simUntouched === 100, `${r.simUntouched}`);
check("...and a count change invalidates the scan's key", r.qcKeyMoved === true);
check("...a number under the floor is raised, not obeyed", r.qcFloored === 10, `${r.qcFloored}`);
// THE ONE THAT BITES. Every assertion above passed throughout the months the
// box was inert, because they all read the object rather than the request.
check("...and the count the scan sends IS the scan's, not the simulator's",
  r.qcPosted === 30 && r.qcPostedIsNotTheSim === true,
  `posted ${r.qcPosted}`);
check("the box is on screen with the floor declared", r.qcOnScreen === true);
check("...and typing in it reaches the scan", r.qcFromScreen === 40, `${r.qcFromScreen}`);
check("...a rejected number snaps back to what was taken", r.qcSnapBack === "10", r.qcSnapBack);

check("the optimizer offers a final-round count", r.optOnScreen === true);
check("...with a number in it, never blank", r.optShown === "100", `"${r.optShown}"`);
check("...and that number is what it SENDS", r.sentDefault === 100, `${r.sentDefault}`);
check("...drawn outside both halves, because it is in neither preset",
  r.optRunsOutside === true);
check("...its own number reaches the request", r.sentOwn === 60, `${r.sentOwn}`);
check("...and does not edit the simulator's", r.simStillDefault === 100, `${r.simStillDefault}`);
check("...the search preset does not carry it", r.notInPreset === true);
check("...so restoring a scope leaves it where the reader put it",
  String(r.survivesPreset) === "250,250", String(r.survivesPreset));
check("...and CPU threads is gone, the topbar owning that question",
  r.threadsBox === false && r.sentThreads === false,
  `box ${r.threadsBox}, sent ${r.sentThreads}`);

// AND A LONG ONE SAYS HOW FAR IT HAS GOT.
check(`a ${r.progBodies}-body fight reports its progress`,
  r.progSteps > 3, `${r.progSteps} distinct readings`);
check("...as a COUNT, which is a number a reader can act on",
  r.progCounts === true, `${r.progFirst} -> ${r.progLast}`);
check("...and as a time remaining", r.progEta === true, JSON.stringify(r.progAll));
check("...with a bar that only ever rises, and reaches the end",
  r.progRises === true && r.progEnds === true, r.progLast);
// AND IT CAN BE STOPPED — the wait is unbounded, so a reader who realises the
// fight is too big must not have to reload the page.
check("a running sim offers a stop", r.stopOffered === true, r.stopMidRun);
check("...which takes effect at once", r.stopMs < 700, `${r.stopMs} ms`);
check("...says nothing was measured rather than reporting a failure",
  /nothing was measured|没有测出/.test(r.stopSaid), r.stopSaid);
check("...frees the button", r.stopFreed === true);
check("...and the next run still answers", r.stopRecovered === true);
// A FLEET ANSWERS WHAT ONE WORKER ANSWERS.
check(`the sim shards across ${r.fleetLanes} workers`, r.fleetLanes > 1, String(r.fleetLanes));
check("...and every one of the runs is still run once",
  String(r.fleetRuns[0]) === String(r.fleetRuns[1]), JSON.stringify(r.fleetRuns));
check("...for the same answer, on the wire and not just in Rust",
  r.fleetDiff.length === 0, r.fleetDiff.join(" · "));

await app.finish("how hard you measure is a number someone can set, in all three modules");
