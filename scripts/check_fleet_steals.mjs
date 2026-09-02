// A SIMULATION DOES NOT WAIT FOR THE ONE LANE THAT IS BUSY.
//
// The runs used to be split `runs / lanes` once, before anything was known. A
// lane that was slow — throttled, or holding somebody else's work — then held
// the whole answer, and there was nothing to hand its share to. On the fight
// this matters for (Melee Influence across the 361-body formation, ~29 ms a
// run) that is the difference between an answer and a stall.
//
// Now the runs are a queue and a lane takes the next piece when free, sized
// from a one-run probe. This asserts the property that buys: with one lane
// occupied by a long job, the simulation finishes on the others rather than
// waiting for it.
//
// IT BITES, and it caught the first attempt at the fix. A lane that claimed a
// piece BEFORE knowing it was free took a run out of the pool that nobody else
// could take, so the simulation waited for the occupied lane exactly as the
// static split had: 4514 ms against a 4579 ms blocker, where an unobstructed
// run of the same fight is 654 ms. A lane now waits to be free before it
// claims.
//
//   node scripts/check_fleet_steals.mjs
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
  // THE FIGHT THIS IS FOR: Influence on the crowd, the most expensive thing
  // this engine does.
  ['primed_pressure_point', 'sacrificial_steel', 'organ_shatter',
   'voltaic_strike', 'shocking_touch', 'condition_overload']
    .forEach((id, i) => { slots[i] = { ...(slots[i] || {}), mod: id, rank: null }; });
  arcanes[0] = 'melee_influence';
  arcaneRanks[0] = 5;

  const ls = await lanes();
  out.lanes = ls.length;
  const fight = (runs) => ({ ...buildPayload(), ...theFight({ runs, seed: 11 }) });
  out.bodies = 1 + ((fight(1).formation || []).length);

  // Warm every lane, so the first timing is not paying for module loads.
  await Promise.all(ls.map((l) => l.call('/api/meta', {})));

  const timed = async (runs) => {
    const t = performance.now();
    const r = await simulateFleet(fight(runs));
    return { ms: performance.now() - t, ok: !!(r && r.ok), score: r && r.score };
  };

  const RUNS = 140;
  // WARM THE ENGINE, NOT JUST THE MODULE. A meta call loads the worker and
  // touches none of the fight, so the first real simulation pays for wasm
  // tiering up on a hot numeric loop — measured at about 4x, which is enough to
  // make the two timings below incomparable and the comparison meaningless.
  await simulateFleet(fight(RUNS));
  const clear = await timed(RUNS);
  out.clearMs = Math.round(clear.ms);
  out.clearOk = clear.ok;

  // NOW OCCUPY ONE LANE with a job long enough that waiting for it would show.
  // Fired and not awaited: it is the obstacle, not the subject.
  const t0 = performance.now();
  const blocker = ls[0].call('/api/simulate', fight(RUNS * 4));
  await sleep(60);
  const busy = await timed(RUNS);
  out.busyMs = Math.round(busy.ms);
  out.busyOk = busy.ok;
  out.sameAnswer = clear.score === busy.score;
  await blocker;
  out.blockerMs = Math.round(performance.now() - t0);
  return out;
})()`);

console.log(`      ${r.lanes} lanes · ${r.bodies} bodies · clear ${r.clearMs} ms · `
  + `with one lane occupied ${r.busyMs} ms · blocker ${r.blockerMs} ms`);
check("the pool has lanes to steal between", r.lanes > 2, `lanes ${r.lanes}`);
check("the fight really is the crowd", r.bodies === 361, `bodies ${r.bodies}`);
check(
  "both simulations answered",
  r.clearOk === true && r.busyOk === true,
  `clear ${r.clearOk}, busy ${r.busyOk}`,
);
check(
  "…with the same answer, busy lane or not",
  r.sameAnswer === true,
  `same ${r.sameAnswer}`,
);
check(
  "the blocker really was the long pole",
  r.blockerMs > r.clearMs * 2,
  `blocker ${r.blockerMs} ms against a clear run of ${r.clearMs} ms`,
);
// THE PROPERTY. A static split waits for the occupied lane, so it cannot finish
// before the blocker does. Stealing finishes on the others.
check(
  "…and the simulation did NOT wait for the busy lane",
  r.busyMs < r.blockerMs * 0.8,
  `busy run ${r.busyMs} ms, blocker ${r.blockerMs} ms, clear ${r.clearMs} ms`,
);

process.exit(0);
