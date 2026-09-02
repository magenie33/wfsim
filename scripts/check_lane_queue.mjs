// A BUSY LANE IS NOT A DEAD LANE, and neither is the request waiting behind it.
//
// A worker runs its messages one at a time and the wasm call blocks the thread,
// so a request queued behind a long one hears nothing at all until the long one
// is done. The stall watchdog kept a clock PER REQUEST and killed the lane as
// soon as any clock ran out — so a quick-calc request parked behind a simulate
// shard took that shard down with it, while the worker was alive and reporting
// progress on the very job making it wait.
//
// That is both surfaces dying at once, under exactly the load worth having:
// Melee Influence on a 361-body formation, few lanes, a simulate and a scan
// wanting the same pool.
//
//   node scripts/check_lane_queue.mjs
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
  // THE FIGHT THIS IS ABOUT: Influence on the crowd, which is the most
  // expensive thing this engine does and the reason a lane is busy for long
  // enough to matter.
  ['primed_pressure_point', 'sacrificial_steel', 'organ_shatter',
   'voltaic_strike', 'shocking_touch', 'condition_overload']
    .forEach((id, i) => { slots[i] = { ...(slots[i] || {}), mod: id, rank: null }; });
  arcanes[0] = 'melee_influence';
  arcaneRanks[0] = 5;

  // A WINDOW SHORTER THAN THE LONG CALL, so the queued one is guaranteed to
  // outlast it. The product default is 45 s.
  LANE_WATCHDOG.loading = 3000;
  LANE_WATCHDOG.stall = 3000;

  const lane = laneAt(0);
  const body = { ...buildPayload(), ...theFight({ runs: 1400, seed: 3 }) };
  out.bodies = 1 + ((body.formation || []).length);

  // THE LONG ONE — asked WITH progress, the way a simulate asks, so the worker
  // is audibly alive throughout.
  let beats = 0;
  const slow = lane.call('/api/simulate', body, () => { beats += 1; });
  // …AND THE ONE BEHIND IT, which hears nothing until the first is answered.
  await sleep(150);
  const queued = lane.call('/api/meta', {});

  // Outlive the window several times over.
  const t0 = Date.now();
  const [a, b] = await Promise.all([slow, queued]);
  out.tookMs = Date.now() - t0;
  out.beats = beats;
  out.laneDead = lane.dead;
  out.slowOk = !!(a && a.ok !== false && !a.worker_dead && !a.cancelled);
  out.queuedOk = !!(b && !b.worker_dead && !b.cancelled);
  out.slowWhy = a && (a.error || (a.worker_dead ? 'worker_dead' : a.cancelled ? 'cancelled' : ''));
  out.queuedWhy = b && (b.error || (b.worker_dead ? 'worker_dead' : b.cancelled ? 'cancelled' : ''));
  return out;
})()`);

check(
  "the fight really is the crowd",
  r.bodies === 361,
  `bodies ${r.bodies}`,
);
check(
  "the long call outlived the stall window several times over",
  r.tookMs > 6000,
  `took ${r.tookMs} ms against a 3000 ms window`,
);
check(
  "…and the worker was audibly alive the whole time",
  r.beats > 0,
  `progress messages ${r.beats}`,
);
check(
  "the lane is NOT killed for having something queued behind that",
  r.laneDead === false,
  `dead ${r.laneDead}`,
);
check(
  "…the long call is answered",
  r.slowOk === true,
  `why ${JSON.stringify(r.slowWhy)}`,
);
check(
  "…and so is the one that waited",
  r.queuedOk === true,
  `why ${JSON.stringify(r.queuedWhy)}`,
);

process.exit(0);
