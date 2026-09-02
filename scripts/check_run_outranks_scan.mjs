// A PERSON'S SIMULATE DOES NOT WAIT FOR A RANKING NOBODY ASKED FOR OUT LOUD.
//
// Stage 2 of docs/WASM.md §"one executor". The quick calc and the simulator
// both take the whole pool, so without a priority between them they interleave
// by luck — and the reader who just pressed Run waits behind eighty candidates
// on exactly the fights where waiting is worst.
//
// A person's Run holds priority for its duration; background work yields
// between pieces, never mid-piece, so the foreground waits for one piece (a
// quarter second by construction) rather than for the scan. Nothing is
// cancelled and nothing is recomputed to make room — the scan finishes after.
//
//   node scripts/check_run_outranks_scan.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, send, sleep, BASE } = app;

await evaluate("localStorage.clear(); localStorage.setItem('wfsim-lang', 'en')");
await send("Page.navigate", { url: `${BASE}/weapons/Praedos?bench=group_clear` });
await sleep(15000);

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const out = {};
  // THE FIGHT THIS IS FOR: Influence across the crowd, where a scan is long
  // enough that waiting behind it is the whole complaint.
  ['primed_pressure_point', 'sacrificial_steel', 'organ_shatter',
   'voltaic_strike', 'shocking_touch', 'condition_overload']
    .forEach((id, i) => { slots[i] = { ...(slots[i] || {}), mod: id, rank: null }; });
  arcanes[0] = 'melee_influence';
  arcaneRanks[0] = 5;
  const anchor = () => document.querySelector('.slot') || document.body;
  await lanes();

  // Warm the ENGINE, not just the module: the first real fight pays for wasm
  // tiering up and would make the two timings below incomparable.
  await simulateFleet({ ...buildPayload(), ...theFight({ runs: 60, seed: 4 }) });

  const runSimTimed = async () => {
    const t = performance.now();
    await runSim();
    return performance.now() - t;
  };

  // WHAT IT COSTS WITH THE MACHINE TO ITSELF.
  out.aloneMs = Math.round(await runSimTimed());

  // …AND WITH A SCAN ALREADY RUNNING. Started and left running: it is the
  // obstacle, not the subject.
  gainPrefs = { ...gainPrefs, on: true };
  openPicker(6, anchor());
  for (let i = 0; i < 40 && !(gainScan.running && gainScan.done > 0); i++) await sleep(100);
  out.scanWasRunning = gainScan.running && gainScan.done > 0;
  out.scanDoneAtStart = gainScan.done;
  out.busyMs = Math.round(await runSimTimed());
  out.scanStillRunning = gainScan.running;

  // …AND THE SCAN STILL FINISHES.
  for (let i = 0; i < 200; i++) {
    if (!gainScan.running && gainScan.key !== null) break;
    await sleep(500);
  }
  out.scanSettled = !gainScan.running && gainScan.key !== null;
  out.scanRanked = Object.keys(gainScan.by || {}).length;
  out.scanWanted = gainScan.ids ? gainScan.ids.size : 0;
  return out;
})()`);

console.log(`      run alone ${r.aloneMs} ms · run during a scan ${r.busyMs} ms · `
  + `scan had ${r.scanDoneAtStart} candidates done when it started`);

check(
  "a scan really was running when Run was pressed",
  r.scanWasRunning === true,
  `running with ${r.scanDoneAtStart} done`,
);
// THE BOUND IS ONE BACKGROUND PIECE, not a ratio. Background work yields
// BETWEEN pieces, and a piece is sized to about a quarter second, so the extra
// a person waits is that and not the scan. Measured here: 58 ms with the
// priority, 573 ms without it.
check(
  "the simulate answers at about its own speed, not the scan's",
  r.busyMs - r.aloneMs < 400,
  `alone ${r.aloneMs} ms, during a scan ${r.busyMs} ms, so it waited ${r.busyMs - r.aloneMs} ms extra`,
);
check(
  "…and nothing was cancelled to make room for it",
  r.scanStillRunning === true,
  `scan still running after the simulate ${r.scanStillRunning}`,
);
check(
  "…and the scan finishes afterwards, complete",
  r.scanSettled === true && r.scanRanked >= r.scanWanted && r.scanWanted > 0,
  `settled ${r.scanSettled}, ranked ${r.scanRanked}/${r.scanWanted}`,
);

process.exit(0);
