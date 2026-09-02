// A LONG SCAN SAYS HOW LONG IT HAS LEFT, and the number is worth reading.
//
// Stage 3 of docs/WASM.md §"one executor". The per-run cost in this product
// spans 1.1 ms to 29 ms, so "this will be a moment" and "this will be a minute"
// are the same screen — and the second one, unannounced, is what a reader calls
// stuck.
//
// FROM THE RATE THE SCAN IS ACTUALLY GOING, not from the baseline: the baseline
// is the first real fight of the page and pays for wasm tiering up on a hot
// numeric loop, so an estimate built on it over-predicts about fivefold
// (measured: 12.5 s predicted against 2.4 s). Throughput already contains the
// lane count, the machine, the fight and whatever else the browser is doing.
//
//   node scripts/check_calc_eta.mjs
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
  // THE SHAPE THAT NEEDS TELLING: Influence on the crowd, a share of the
  // machine a phone would have, and a run count that makes it minutes rather
  // than seconds. All three are things a reader really sets.
  ['primed_pressure_point', 'sacrificial_steel', 'organ_shatter',
   'voltaic_strike', 'shocking_touch', 'condition_overload']
    .forEach((id, i) => { slots[i] = { ...(slots[i] || {}), mod: id, rank: null }; });
  arcanes[0] = 'melee_influence';
  arcaneRanks[0] = 5;
  setComputePct(10);
  gainPrefs = { ...gainPrefs, on: true, runs: 60 };
  resetPool();

  const t0 = Date.now();
  openPicker(6, document.querySelector('.slot') || document.body);

  let guess = null, guessAt = null;
  out.sawRow = false;
  for (let i = 0; i < 400; i++) {
    await sleep(500);
    // THE FIRST ESTIMATE WORTH QUOTING, kept with the moment it was made so it
    // can be judged against what actually happened after it.
    if (guess === null && gainScan.etaMs && gainScan.done >= 5) {
      guess = gainScan.etaMs;
      guessAt = Date.now();
      out.guessAtDone = gainScan.done;
      out.guessOf = gainScan.total;
    }
    const box = document.getElementById('calc-status');
    if (box && !box.hidden && /left/.test(box.innerText)) out.sawRow = true;
    if (!gainScan.running && gainScan.key !== null) break;
  }
  out.settled = !gainScan.running && gainScan.key !== null;
  out.lanes = poolSize();
  out.totalMs = Date.now() - t0;
  out.predictedMs = guess;
  out.actualAfterGuessMs = guessAt ? Date.now() - guessAt : null;
  // …AND IT IS GONE WHEN THERE IS NOTHING LEFT TO WAIT FOR.
  const box = document.getElementById('calc-status');
  out.rowAfter = box ? /left/.test(box.innerText) : false;
  return out;
})()`);

console.log(`      ${r.lanes} lanes · scan ${Math.round(r.totalMs / 1000)}s · at `
  + `${r.guessAtDone}/${r.guessOf} it predicted ${Math.round(r.predictedMs / 1000)}s, `
  + `actually ${Math.round(r.actualAfterGuessMs / 1000)}s`);

check(
  "the scan was long enough to be worth an estimate",
  r.totalMs > 8000,
  `took ${r.totalMs} ms`,
);
check("…and it finished", r.settled === true, `settled ${r.settled}`);
check(
  "it made an estimate",
  r.predictedMs > 0,
  `predicted ${r.predictedMs}`,
);
// WITHIN A FACTOR OF TWO, both ways. A reader is deciding whether to wait, so
// the number has to be honest about the ORDER — a tighter claim than that would
// be one the machine cannot keep while a browser schedules other tabs.
check(
  "…and it was right to within a factor of two",
  r.predictedMs > r.actualAfterGuessMs / 2 && r.predictedMs < r.actualAfterGuessMs * 2,
  `predicted ${r.predictedMs} ms, actual ${r.actualAfterGuessMs} ms`,
);
check(
  "the reader was shown it",
  r.sawRow === true,
  `row seen ${r.sawRow}`,
);
check(
  "…and it is gone once there is nothing left to wait for",
  r.rowAfter === false,
  `row still there ${r.rowAfter}`,
);

process.exit(0);
