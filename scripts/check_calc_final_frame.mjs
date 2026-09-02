// THE SCAN'S LAST FRAME IS DRAWN, and the queue behind it gets its turn.
//
// Reported from the live site on a mature build: replacing one card left the
// ranking permanently one short — "66/67" — and opening another slot then
// produced nothing. The books were perfect every time; the SCREEN was not.
//
// Ticks are throttled to 250 ms, so a scan whose last candidate lands inside
// that window draws its final frame one answer short, and `gainStop` repainted
// the page's own surfaces but never the LIST that asked. The same missing frame
// is where a queued request takes its turn, so the axis waiting behind the scan
// was never started either — which is what "then it hangs again" was.
//
//   node scripts/check_calc_final_frame.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, send, sleep, BASE } = app;

await evaluate("localStorage.clear(); localStorage.setItem('wfsim-lang', 'en')");
await send("Page.navigate", { url: BASE });
await sleep(12000);

// A MATURE BUILD, because that is where it was reported and where it bites: a
// full build makes every candidate cost the same, so the last one lands in the
// same throttle window as the rest.
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  history.pushState({}, '', '/weapons/Ballistica_Prime'); route(); await sleep(4000);
  const out = {};
  const fill = ['hornet_strike', 'galvanized_diffusion', 'galvanized_shot',
    'lethal_torrent', 'primed_convulsion', 'primed_heated_charge', 'galvanized_crosshairs'];
  fill.forEach((id, i) => { if (slots[i + 1]) slots[i + 1].mod = id; });
  out.filled = slots.filter((s) => s.mod).length;

  openPicker(0, document.querySelector('.slot') || document.body);
  for (let i = 0; i < 160; i++) {
    if (!gainScan.running && gainScan.done > 0) break;
    await sleep(500);
  }
  await sleep(1200);            // past the throttle, so a late frame would land

  out.done = gainScan.done;
  out.total = gainScan.total;
  out.ranked = Object.keys(gainScan.by || {}).length;
  out.cands = gainScan.ids ? gainScan.ids.size : 0;
  out.stamped = gainScan.key !== null;

  // WHAT THE SCREEN SAYS, which is the whole of this check.
  const strip = document.querySelector('#mod-menu .scan-strip');
  out.stripText = strip ? strip.textContent.replace(/\\s+/g, ' ').trim() : '';
  out.stillRanking = /\\d+\\s*\\/\\s*\\d+/.test(out.stripText);

  // NO CANDIDATE ROW IS LEFT LOOKING UNMEASURED.
  const menu = document.getElementById('mod-menu');
  out.waiting = menu ? menu.querySelectorAll('.gainchip.pending, .gainwait').length : -1;
  return out;
})()`);

check(
  "the build really is a mature one",
  r.filled >= 7,
  `filled ${r.filled}`,
);
check(
  "the scan finished and its books balance",
  r.stamped === true && r.ranked === r.cands && r.cands > 0,
  `stamped ${r.stamped}, ranked ${r.ranked} of ${r.cands}, done ${r.done}/${r.total}`,
);
check(
  "…and the list is NOT left showing a count one short",
  r.stillRanking === false,
  `strip reads ${JSON.stringify(r.stripText)} with every answer present`,
);

// THE QUEUE GETS ITS TURN. A repaint-driven request asked for while a scan is
// running is parked, and its turn is taken in the frame this check is about.
const q = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const out = {};
  gainScan = { key: null, want: null, running: false, base: 0, floor: 0, by: {},
    done: 0, total: 0, note: '', metric: '', failed: false, lanesLost: 0 };
  openPicker(1, document.querySelector('.slot') || document.body);
  // SHORT ON PURPOSE: a single-target scan over this pool is done in about a
  // third of a second, so a wait long enough to be comfortable is a wait long
  // enough for there to be nothing left to park behind.
  await sleep(40);
  out.wasRunning = gainScan.running;
  // Parked, not preempting: this is the repaint's path, so no user flag.
  ensureGains({ kind: 'mods', idx: 2 }, () => {});
  out.parked = gainPending !== null;
  for (let i = 0; i < 160; i++) {
    if (gainPending === null && !gainScan.running
        && JSON.stringify(gainScan.axis) === JSON.stringify({ kind: 'mods', idx: 2 })) break;
    await sleep(500);
  }
  out.drained = gainPending === null;
  out.ranAxis = JSON.stringify(gainScan.axis || null);
  return out;
})()`);

check(
  "a repaint's request really was parked behind the running scan",
  q.wasRunning === true && q.parked === true,
  `running ${q.wasRunning}, parked ${q.parked}`,
);
check(
  "…and it gets its turn once that scan stops",
  q.drained === true && q.ranAxis === JSON.stringify({ kind: "mods", idx: 2 }),
  `drained ${q.drained}, ran ${q.ranAxis}`,
);

process.exit(0);
