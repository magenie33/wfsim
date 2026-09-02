// THE QUICK CALC SURVIVES LOSING ITS WORKERS, AND SAYS THAT IT DID.
//
// The report, from the owner and from players: the quick calc stops producing
// numbers and stays stopped — a reload does not help. Three faults compounded,
// and each on its own was permanent:
//
//   * `laneAt` returned a DEAD lane rather than replacing it, and `freeLane`
//     fell back to `laneAt(0)`, so once every worker had died every later call
//     went to a corpse. A reload rebuilt the same pool the same way, because
//     the lane count is a stored preference.
//   * `laneAsk` recognised `cancelled` and not `worker_dead`, so a worker whose
//     module never loaded returned a failure the scan read as an empty
//     measurement: the counter advanced and the chip never appeared.
//   * the key was stamped when a scan STARTED, so a scan that died half way
//     left the page believing that fight was already answered, and no later
//     request re-asked.
//
// ASSERTED BY KILLING THE POOL, which is the only honest way to test a recovery
// path: every lane is abandoned, and the calculator has to produce numbers
// anyway.
//
//   node scripts/check_calc_recovers.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, send, sleep, BASE } = app;

await evaluate(`localStorage.clear(); localStorage.setItem('wfsim-lang', 'en')`);
await send("Page.navigate", { url: BASE });
await sleep(12000);

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  history.pushState({}, '', '/weapons/Braton'); route(); await sleep(3500);
  const out = {};

  // A DEAD SLOT IS REPLACED. The unit of the whole fix: everything else follows
  // from a pool that can come back.
  const first = laneAt(0);
  first.abandon();
  out.wasDead = first.dead;
  out.replaced = laneAt(0) !== first && !laneAt(0).dead;

  // …AND freeLane NEVER HANDS BACK A CORPSE, even with the whole pool gone.
  for (const l of pool) if (l) l.abandon();
  out.allDead = pool.filter(Boolean).every((l) => l.dead);
  out.freshFromFree = !freeLane().dead;

  return out;
})()`);

check("a lane that died is replaced, not handed back", r.wasDead && r.replaced,
  `dead ${r.wasDead}, replaced ${r.replaced}`);
check("…and with every lane dead, freeLane still returns a live one",
  r.allDead && r.freshFromFree, `allDead ${r.allDead}, fresh ${r.freshFromFree}`);

// THE SCAN ITSELF, across a pool killed under it. What has to be true is not
// that some particular scan survives — a scan superseded by a newer one exits
// silently ON PURPOSE, and that is most of them — but that the calculator
// eventually produces a COMPLETE answer again. Completion is `gainScan.key`
// being stamped, which now happens only on a scan that measured everything.
const s = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const out = {};
  resetPool();
  gainScan = { key: null, want: null, running: false, base: 0, floor: 0, by: {},
    done: 0, total: 0, note: '', metric: '', failed: false, lanesLost: 0 };
  gainPrefs = { ...gainPrefs, on: true };
  // A SCAN IS ASKED FOR BY A LIST, not by the page. Opening a mod picker is
  // what makes the quick calc rank anything: refreshGains alone repaints and
  // asks nobody, which on a weapon with no evolutions is a scan that never
  // starts.
  openPicker(0, document.querySelector('.slot') || document.body);
  // KILLED WHILE IT DEPENDS ON THEM, which is the failure being reproduced.
  await sleep(400);
  let killed = 0;
  for (const l of pool) if (l && !l.dead) { l.abandon(); killed++; }
  out.killed = killed;
  out.sawStatus = false;
  for (let i = 0; i < 90; i++) {
    if (!document.getElementById('calc-status').hidden) out.sawStatus = true;
    if (gainScan.key !== null) break;
    await sleep(500);
  }
  out.recovered = gainScan.key !== null;
  out.ranked = Object.keys(gainScan.by).length;
  out.running = gainScan.running;
  out.note = String(gainScan.note || '');
  return out;
})()`);

check(
  "the pool really was killed under the scan",
  s.killed > 0,
  `killed ${s.killed}`,
);
check(
  "…and the calculator recovers to a COMPLETE answer on its own",
  s.recovered === true,
  `key stamped ${s.recovered}, ranked ${s.ranked}, running ${s.running}, note ${JSON.stringify(s.note)}`,
);
check(
  "…which is a real ranking, not an empty one filed as done",
  s.ranked > 0,
  `ranked ${s.ranked}`,
);
check(
  "the status surface was on screen while it worked",
  s.sawStatus === true,
  `seen ${s.sawStatus}`,
);

// A WORKER THAT NEITHER ANSWERS NOR FAILS — the fourth fault, and the only one
// that produced no error at all. `abandon` settles its waiters and `onerror`
// settles its waiters; a worker the browser reclaims mid-fight does neither, so
// the scan awaited an answer that could not come, `running` stayed true, and
// `ensureGains` refused that same list for the life of the page.
//
// WEDGED BY DROPPING postMessage, which is what silence looks like from here:
// the worker is alive, un-abandoned, and will never reply. Only the first two
// are wedged, so the pool has somewhere to recover TO.
const w = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const out = {};
  LANE_WATCHDOG.loading = 1500;
  LANE_WATCHDOG.stall = 1500;
  const Real = window.Worker;
  let made = 0;
  out.dropped = 0;
  window.Worker = function (u) {
    const ww = new Real(u);
    made += 1;
    if (made <= 2) ww.postMessage = () => { out.dropped += 1; };
    return ww;
  };
  resetPool();
  gainScan = { key: null, want: null, running: false, base: 0, floor: 0, by: {},
    done: 0, total: 0, note: '', metric: '', failed: false, lanesLost: 0 };
  gainPrefs = { ...gainPrefs, on: true };
  openPicker(0, document.querySelector('.slot') || document.body);
  for (let i = 0; i < 90; i++) {
    if (gainScan.key !== null) break;
    await sleep(500);
  }
  window.Worker = Real;
  out.recovered = gainScan.key !== null;
  out.ranked = Object.keys(gainScan.by).length;
  out.running = gainScan.running;
  out.wedgedLanes = pool.filter((l) => l && l.dead).length;
  return out;
})()`);

check(
  "a wedged worker really did swallow its requests",
  w.dropped > 0,
  `dropped ${w.dropped}`,
);
check(
  "…and a lane that goes SILENT is given up on rather than waited on for ever",
  w.recovered === true && w.running === false,
  `key stamped ${w.recovered}, running ${w.running}, ranked ${w.ranked}`,
);
check(
  "…producing a real ranking on the workers that were left",
  w.ranked > 0,
  `ranked ${w.ranked}`,
);

// THE MOVE THAT ALWAYS WORKS. Everything above is the calculator recovering on
// its own; this is the reader's guarantee for the case nobody predicted, and a
// guarantee with no check behind it is a claim. Asserted from the worst state
// there is: a scan latched running against a pool that will never answer.
const f = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const out = {};
  LANE_WATCHDOG.loading = 1500;
  LANE_WATCHDOG.stall = 1500;
  // WEDGED BY HAND into the exact shape the fixes above are for: a scan that
  // believes it is running, over a fight the page still wants.
  resetPool();
  gainPrefs = { ...gainPrefs, on: true };
  gainAxis = { kind: 'mods', idx: 0 };
  gainScan = { key: null, want: gainKey(), axis: { kind: 'mods', idx: 0 },
    running: true, base: 0, floor: 0, by: {}, done: 3, total: 40, phase: '',
    note: '', metric: '', failed: false, lanesLost: 0, ids: new Set() };
  gainPending = { axis: { kind: 'mods', idx: 3 }, repaint: () => {} };
  renderCalcStatus();
  // THE SURFACE IS REACHABLE AT ALL, which is half of the guarantee.
  out.boxShown = !document.getElementById('calc-status').hidden;
  const tab = document.getElementById('cs-tab');
  if (tab && !document.getElementById('cs-reset')) tab.click();
  const btn = document.getElementById('cs-reset');
  out.buttonThere = !!btn;
  if (btn) btn.click();
  out.clearedPending = gainPending === null;
  await sleep(500);
  // …AND THE CALCULATOR ANSWERS AGAIN AFTERWARDS.
  openPicker(0, document.querySelector('.slot') || document.body);
  for (let i = 0; i < 80; i++) {
    if (gainScan.key !== null) break;
    await sleep(500);
  }
  out.answersAgain = gainScan.key !== null;
  out.ranked = Object.keys(gainScan.by).length;
  return out;
})()`);

check(
  "the status surface is reachable even with nothing to report",
  f.boxShown === true && f.buttonThere === true,
  `box ${f.boxShown}, button ${f.buttonThere}`,
);
check(
  "…and rebuilding strands the queued request rather than carrying it over",
  f.clearedPending === true,
  `pending cleared ${f.clearedPending}`,
);
check(
  "…and the calculator produces a complete answer after the rebuild",
  f.answersAgain === true && f.ranked > 0,
  `key stamped ${f.answersAgain}, ranked ${f.ranked}`,
);

// THE SWITCH IS THE OTHER WAY OUT. It is the first thing anyone does to
// something that looks stuck, so off has to MEAN off — a scan that kept its
// workers and its right to write would resume into the state it was stuck in.
const s2 = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const out = {};
  // THE WINDOWS BACK TO NORMAL. The wedge cases above lower them to make a
  // silent worker cheap to prove; a real scan measured under 1.5 s of patience
  // is killed on its own baseline.
  LANE_WATCHDOG.loading = 90000;
  LANE_WATCHDOG.stall = 45000;
  const box = document.getElementById('gp-on');
  out.switchThere = !!box;
  if (!box) return out;
  // LATCHED, and with something queued behind it.
  gainAxis = { kind: 'mods', idx: 0 };
  gainScan = { key: null, want: gainKey(), axis: { kind: 'mods', idx: 0 },
    running: true, base: 0, floor: 0, by: {}, done: 5, total: 60, phase: '',
    note: '', metric: '', failed: false, lanesLost: 0, ids: new Set() };
  gainPending = { axis: { kind: 'mods', idx: 4 }, repaint: () => {} };
  laneAt(0);
  out.hadWorkers = pool.filter(Boolean).length > 0;
  box.checked = false;
  box.dispatchEvent(new Event('change'));
  await sleep(200);
  out.stopped = gainScan.running === false;
  out.pendingCleared = gainPending === null;
  out.workersDropped = pool.filter(Boolean).length === 0;
  return out;
})()`);

check(
  "the quick-calc switch is on the page to be found",
  s2.switchThere === true,
  `found ${s2.switchThere}`,
);
check(
  "…switching it OFF stops the scan, drops the workers and strands the queue",
  s2.stopped === true && s2.pendingCleared === true && s2.workersDropped === true,
  `stopped ${s2.stopped}, pending cleared ${s2.pendingCleared}, workers dropped ${s2.workersDropped} (had ${s2.hadWorkers})`,
);
process.exit(0);
