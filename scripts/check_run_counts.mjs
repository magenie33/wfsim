// HOW HARD YOU MEASURE IS A NUMBER SOMEONE CAN SET, in all three modules.
//
// A run count used to be three different kinds of thing: the simulator's own
// setting, a constant the quick calc kept to itself, and a rule the optimizer
// obeyed without a control. Two of those became settings (owner, 2026-08-11)
// and the third moved, so this walks all three and asserts the number a reader
// picks is the number the request carries.
//
//   the SIMULATOR   defaults to 1000 — the official rulers' count, so a first
//                   number is comparable with the board without touching a box
//   the QUICK CALC  takes its own, floored at 10, because a chip is meant to be
//                   cheap; the floor is where a status mod stops being a coin
//                   flip (M24)
//   the OPTIMIZER   takes its own for the final round, and BLANK means the
//                   fight's — the precision the replay will use
//
// The last one is the reason this exists as a check rather than a comment. A
// blank box that silently means something is exactly the kind of state that
// reads as broken and works, or reads as fine and sends 0.
//
//   node scripts/check_run_counts.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep } = app;

const r = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  localStorage.clear();
  history.pushState({},'','/weapons/Verglas_Prime'); route(); await sleep(3000);
  const out = {};

  // ---- the SIMULATOR's count is DECOUPLED from the fight ---------------
  // It is a preference, not a scenario field (owner, 2026-08-13): 100 by
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

  const runsBox = document.getElementById('opt-runs');
  out.optOnScreen = !!runsBox;
  out.optBlank = runsBox ? runsBox.value : 'MISSING';
  // BLANK = the fight's own count, whatever it happens to be.
  out.sentBlank = (await sendOnce()).final_runs;
  // …and it FOLLOWS the fight rather than having copied it once.
  setSimRuns(137);
  out.sentFollows = (await sendOnce()).final_runs;
  setSimRuns(100);
  // A number of its own overrides it, and does not move the fight.
  await set('opt-runs', 60);
  out.sentOwn = (await sendOnce()).final_runs;
  out.simStillDefault = simRuns();
  // It is a SEARCH setting, so it survives a preset round trip.
  const snap = snapshotOpt();
  optRun.runs = 0; applyOptState(snap); await sleep(200);
  out.restored = [optRun.runs, document.getElementById('opt-runs').value];
  // …and back to blank means back to the fight's.
  await set('opt-runs', 0);
  out.sentBackToFight = (await sendOnce()).final_runs;
  return out;
})()`);

// HOW HARD YOU MEASURE IS NOT PART OF THE FIGHT (owner, 2026-08-13: "计算次数应
// 该是个和scenario解耦的选项…这样更干净"). Two claims, and the second is the one
// that makes it true rather than merely defaulted: a scenario cannot carry a run
// count at all, so opening an official ruler — whose yaml says 1,000 — cannot
// silently make the page slow, and saving a fight cannot record a precision.
check("the simulator runs at 100 by default", r.liveDefault === 100, `${r.liveDefault}`);
check("...and a scenario cannot carry a run count at all",
  r.simDefault === undefined && r.inSnapshot === false,
  `default ${r.simDefault}, in snapshot ${r.inSnapshot}`);

check("the quick calc starts at its floor of 10", r.qcBefore === 10, `${r.qcBefore}`);
check("...takes the count it is given", r.qc250 === 250, `${r.qc250}`);
check("...without touching the fight", r.simUntouched === 100, `${r.simUntouched}`);
check("...and a count change invalidates the scan's key", r.qcKeyMoved === true);
check("...a number under the floor is raised, not obeyed", r.qcFloored === 10, `${r.qcFloored}`);
check("the box is on screen with the floor declared", r.qcOnScreen === true);
check("...and typing in it reaches the scan", r.qcFromScreen === 40, `${r.qcFromScreen}`);
check("...a rejected number snaps back to what was taken", r.qcSnapBack === "10", r.qcSnapBack);

check("the optimizer offers a final-round count", r.optOnScreen === true);
check("...blank by default, meaning the fight's", r.optBlank === "", `"${r.optBlank}"`);
check("...and blank SENDS the page's", r.sentBlank === 100, `${r.sentBlank}`);
check("...following it rather than a copy of it", r.sentFollows === 137, `${r.sentFollows}`);
check("...its own number overrides", r.sentOwn === 60, `${r.sentOwn}`);
check("...and does not edit the page's", r.simStillDefault === 100, `${r.simStillDefault}`);
check("...it survives a search-preset round trip", String(r.restored) === "60,60", String(r.restored));
check("...and clearing it returns to the page's", r.sentBackToFight === 100, `${r.sentBackToFight}`);

await app.finish("how hard you measure is a number someone can set, in all three modules");
