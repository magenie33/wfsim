// A SCENARIO EDIT REACHES THE QUICK CALC IMMEDIATELY.
//
// The gain scan is measured under a scenario, so when the scenario changes the
// numbers on screen are answers to a question nobody is asking any more. The
// cache key must not name the scenario's fields ONE BY ONE: such a list
// drifts, and a missing `buffs` means raising a buff's starting stacks changes
// what the scan would measure without changing the key, leaving a stale ranking
// on screen looking current.
//
// Asserts the two halves separately, because they fail separately: the key
// must MOVE when the fight changes, and something must then RE-RUN.
//
//   node scripts/check_gain_freshness.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep, send } = app;

const r = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  localStorage.clear();
  history.pushState({},'','/weapons/Torid'); route(); await sleep(3500);
  // The default scenario is the official one and it cannot be edited, so this
  // check — which is entirely about a scenario EDIT reaching the quick calc —
  // takes an editable copy first. Same flow a player follows.
  if (typeof officialScenarioActive === 'function' && officialScenarioActive()) {
    copyActiveScenario(); await sleep(1200);
  }

  const out = {};

  // Every scenario field must reach the key, so walk a representative set of
  // them rather than the one that broke — the point is that NOTHING is left out.
  const keyNow = () => gainKey();
  const moved = async (label, mutate) => {
    const before = keyNow();
    mutate();
    markScenarioDirty();
    await sleep(700);            // the scenario auto-save debounce, then the refresh
    return { label, changed: keyNow() !== before };
  };

  out.buffs = await moved('buffs', () => {
    const id = (buffList[0] || {}).id || 'test_buff';
    sim.buffs = { ...sim.buffs, [id]: { stacks: 7, locked: false } };
  });
  out.metric = await moved('metric', () => { sim.metric = sim.metric === 'dps' ? 'kpm' : 'dps'; });
  out.level  = await moved('level',  () => { sim.level = sim.level === 100 ? 200 : 100; });
  out.dur    = await moved('duration', () => { sim.duration = sim.duration === 60 ? 90 : 60; });

  // ...and a field nobody has invented yet: the key is DERIVED from the
  // scenario payload, so an unknown one counts too.
  out.future = await moved('a field added later', () => { sim.some_future_knob = 42; });

  // The second half: after the edit something must actually re-run, so the
  // scan's own key catches up with the fight instead of sitting stale.
  gainPrefs = { ...gainPrefs, on: true };
  await sleep(200);
  openPicker(0, slotEl(0));   // (slotIdx, anchor) — the order the app uses
  for (let i = 0; i < 60 && (gainScan.running || gainScan.key === null); i++) await sleep(500);
  out.scanned = gainScan.key !== null;
  out.freshBefore = gainScan.key === gainKey();
  const keyBefore = gainScan.key;
  // Edit the fight and touch NOTHING else — no reopening, no clicking.
  sim.level = sim.level === 500 ? 600 : 500;
  markScenarioDirty();
  for (let i = 0; i < 60; i++) { await sleep(500); if (!gainScan.running && gainScan.key === gainKey()) break; }
  out.freshAgain = gainScan.key === gainKey();
  // ...and it is a NEW scan under the NEW fight, not the old one still sitting
  // there because the key never noticed. This is the assertion that fails if
  // the key goes back to naming scenario fields by hand.
  out.rescanned = gainScan.key !== keyBefore;

  // AN EDIT MID-SCAN DOES NOT WAIT FOR THE STALE SCAN.
  //
  // A scan is ~90 serial simulate calls. The old guard was "a scan is running,
  // so ignore this edit", so a change made midway was DROPPED: the stale scan
  // ran to the end under the config you had just left and only then did the
  // refresh restart it. Measured against the cost of a full scan, which is the
  // only honest reference — a fixed millisecond budget would pass on a fast
  // machine and fail on a slow one for no reason.
  const settle = async () => {
    for (let i = 0; i < 200 && (gainScan.running || gainScan.key !== gainKey()); i++) await sleep(100);
    return !gainScan.running && gainScan.key === gainKey();
  };
  await settle();

  // THE SCAN RUNS WIDE. ~80 candidates x 10 runs is ~800 engagements, and they
  // all went down ONE worker until 2026-08-03 while the optimizer next door ran
  // a fleet — which is why a rich build made the quick calc feel like a search.
  const lanes = await gainLanes();
  out.laneCount = lanes.length;

  // ...and it is INTERRUPTIBLE. Forced down a single lane so the scan is slow
  // enough to be caught mid-flight on purpose — a timing race against an
  // 800 ms scan proves nothing on a fast machine.
  gainPool = [lanes[0]];
  try {
    // A RICH build, because that is what makes a scan slow enough to interrupt
    // — and slow is the case that matters. An empty build's fight is over in
    // microseconds; seven mods means multishot, crit, status and elements all
    // live, and one engagement generates several times the procs and DoT ticks.
    ['serration','split_chamber','point_strike','vital_sense','hellfire','infected_clip','stormbringer']
      .forEach((m, i) => { slots[i] = { ...(slots[i] || {}), mod: m, rank: null }; });
    await sleep(100);
    // On the MOD axis — ~80 candidates, and one lane makes each one slow
    // enough that "midway" is a real place rather than a race.
    openPicker(0, slotEl(0));
    await settle();
    const axis = { kind: 'mods', idx: 0 };
    out.modAxisTotal = gainScan.total;

    const p1 = scanGains(axis, () => {});          // measuring fight A
    for (let i = 0; i < 400 && !(gainScan.running && gainScan.done > 0); i++) await sleep(25);
    out.caughtMidScan = gainScan.running && gainScan.done > 0;
    out.abandonedAt = gainScan.done;
    out.staleTotal = gainScan.total;
    // WHAT A LIVE SCAN IS FOR, which is want. The key is stamped on COMPLETION
    // — that is what stops a scan that died half way being filed as answered —
    // so mid-flight it is null for the old scan and null for the new one, and
    // comparing it here asked whether nothing had changed into nothing.
    const staleWant = gainScan.want;

    // The fight moves. Straight into the LIVE fight, which is the only place
    // one is edited: theFight() reads sim and nothing else, so the key moves
    // on the assignment with no auto-save to wait out. A preset write is not a
    // fight edit and must not behave like one.
    sim.level = 900;
    out.stillRunningAtCut = gainScan.running;
    const t0 = Date.now();
    const p2 = scanGains(axis, () => {});          // ...and B takes over at once
    out.supersedeMs = Date.now() - t0;
    out.supersededKey = gainScan.want !== staleWant;
    await Promise.all([p1, p2]);
    // B finished as itself: a superseded A that kept writing would push its done
    // past B's total and scribble A's answers into B's table.
    out.overrun = gainScan.done > gainScan.total;
    out.finishedClean = gainScan.done === gainScan.total;

    // NO PING-PONG. Two axes can ask at once — a mod picker open over a ranked
    // list, or a refresh reaching both — and if the newest request always won
    // they would cancel each other forever and neither would ever finish. Only
    // a stale FIGHT supersedes; a different axis waits its turn.
    const p3 = scanGains({ kind: 'mods', idx: 0 }, () => {});
    for (let i = 0; i < 400 && !(gainScan.running && gainScan.done > 0); i++) await sleep(25);
    const heldAxis = JSON.stringify(gainScan.axis);
    ensureGains({ kind: 'evo', idx: 0 }, () => {});
    await sleep(50);
    out.axisHeld = JSON.stringify(gainScan.axis) === heldAxis && gainScan.running === true;
    out.heldAxis = heldAxis;
    await p3;
  } finally {
    gainPool = lanes;
  }
  await settle();

  // The wording: the block says what the numbers in it MEAN.
  const hint = document.querySelector('#sim-buffs')?.previousElementSibling?.querySelector('.sim-hint');
  out.hint = (hint?.textContent || '').trim();
  // ---- A SWITCH RE-ASKS, not just repaints ------------------------------
  //
  // An EDIT has re-run the scan since this check existed. A SWITCH is the
  // other way a fight moves and it never did: it is a REPLACEMENT rather than
  // an edit, so it goes nowhere near the auto-save debounce that triggers the
  // re-run. The box was redrawn under the new fight's name while every chip
  // beside it still answered the old fight's question.
  //
  // ON THE EVOLUTION AXIS, THROUGH AN OPEN LIST. Every ranked axis scans on
  // OPEN (openRanked), so an open list is the only thing a fight change can
  // make stale — and it is exactly the
  // path refreshGains takes. Driving it by opening the tier the way a reader
  // does is therefore the same assertion on the surface that now carries it.
  {
    const settleScan = async () => {
      for (let i = 0; i < 120; i++) {
        await sleep(250);
        if (!gainScan.running && gainScan.key === gainKey()) return true;
      }
      return false;
    };
    // TWO FIGHTS THAT DIFFER ENOUGH TO MOVE A NUMBER. A switch between two
    // identical fights would pass this by coincidence.
    sim.level = 90; markScenarioDirty(); await sleep(900);
    const chips = () => [...document.querySelectorAll('#preset-bar-simulator-scenarios .pchip')]
      .filter((c) => !c.classList.contains('add') && !c.classList.contains('imp'));
    copyActiveScenario(); await sleep(1500);
    sim.level = 950; markScenarioDirty(); await sleep(900);
    // OPEN A TIER'S LIST, which is what starts a scan now.
    const evoBtn = () => document.querySelector('[data-slot="dd-evo-1"]');
    openRanked('dd-evo-1', evoBtn()); await sleep(300);
    out.switchScanned = await settleScan();
    const beforeKey = gainScan.key;
    // THE SCAN'S OWN BASELINE, which is this build measured under this fight
    // and the one number that MUST move when the fight does. A per-candidate
    // gain is the wrong probe: a perk worth nothing under both fights is worth
    // nothing under both fights, which is true and is no evidence that
    // anything was re-measured.
    out.switchHadValue = Object.keys(gainScan.by).length > 0 && gainScan.base > 0;
    const beforeVal = gainScan.base;

    // THE SWITCH ITSELF — the chip, the way a reader does it. Nothing else is
    // touched afterwards: no picker reopened, no tab clicked, no edit made.
    const other = chips().find((c) => !c.classList.contains('sel'));
    out.switchFound = !!other;
    if (other) other.click();
    await sleep(400);
    out.switchMovedTheFight = gainKey() !== beforeKey;
    out.switchRescanned = await settleScan();
    out.switchNewKey = gainScan.key !== beforeKey;
    const afterVal = gainScan.base;
    out.switchValueMoved = beforeVal > 0 && afterVal > 0 && beforeVal !== afterVal;
    out.switchVals = [beforeVal, afterVal];
  }

  return out;
})()`);

for (const k of ["buffs", "metric", "level", "dur", "future"]) {
  check(`a change to ${r[k].label} moves the scan key`, r[k].changed === true);
}
check("a scan ran at all", r.scanned === true);
check("its key matched the fight it measured", r.freshBefore === true);
// WITHOUT REOPENING ANYTHING — the picker is still the one opened above, and
// the edit alone has to make it re-ask. It was a REPAINT for the mod picker
// until 2026-08-24 and this assertion passed anyway, because refreshGains
// ended in renderEvo() and the EVOLUTION scan made the key catch up while the
// open mod picker still showed the old fight answers.
check("editing the fight re-runs it without reopening anything", r.freshAgain === true);
check("and what is on screen was measured under the NEW fight", r.rescanned === true);
check(`the scan runs over ${r.laneCount} lanes, not one`, r.laneCount > 1, String(r.laneCount));
check("a scan was caught mid-flight to interrupt", r.caughtMidScan === true);
check("an edit mid-scan supersedes it instead of queueing behind it",
  r.supersededKey === true, `took ${r.supersedeMs} ms`);
check("...the scan it interrupted was genuinely still going", r.stillRunningAtCut === true);
check("...and it gave way early rather than running to its end",
  r.abandonedAt < r.staleTotal, `abandoned at ${r.abandonedAt} of ${r.staleTotal}`);
check("...leaving the new scan's own tally intact", r.overrun === false && r.finishedClean === true,
  `done ${r.abandonedAt}, overrun ${r.overrun}`);
// A SWITCH IS THE OTHER WAY A FIGHT MOVES, and it never re-ran until
// 2026-08-17 — the box was redrawn under the new name while the chips beside it
// still answered the old fight.
check("a scenario was scanned before the switch",
  r.switchScanned === true && r.switchHadValue === true);
check("...and there was another scenario to switch to", r.switchFound === true);
check("switching the fight moves what the quick calc would measure",
  r.switchMovedTheFight === true);
check("...and it RE-RUNS on its own, with nothing reopened or clicked",
  r.switchRescanned === true && r.switchNewKey === true);
check("...against a fight that really is a different fight",
  r.switchValueMoved === true, `baseline ${JSON.stringify(r.switchVals)}`);

check("a different axis does not preempt a running scan", r.axisHeld === true,
  `axis was ${r.heldAxis}`);
check("the buff block says the stacks are the START", /START|开战/.test(r.hint), JSON.stringify(r.hint));

await app.finish("a scenario edit reaches the quick calc immediately");
