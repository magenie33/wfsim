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

process.exit(0);
