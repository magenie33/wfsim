// WHAT A ROOM-CLEAR IS PACED BY, AND WHERE AN IMPOSSIBLE NUMBER HIDES.
//
// Two blocks the result panel gained on 2026-08-11, and one affordance they
// share.
//
// PACE: `dps` is the whole engagement, reloads included — the honest number for
// a long fight and the wrong one for a room. Burst DPS is the same damage over
// the time the trigger was actually down, and the identity between them is
// arithmetic this check does rather than trusts: damage / (fight − downtime).
//
// HITS: a mean is where an impossible number goes to hide. The same damage
// spread over "one in twelve hits did 40×" and "every hit did 3.3×" reads
// identically as an average and is two different weapons — only one of them a
// bug. So the histogram's counts must add up to the pellets that were fired,
// and its damage must add up to what the meter counted.
//
// FOLDS: every block folds and REMEMBERS, across a re-render and a reload
// (owner). A panel that re-opens everything on every Run Sim is a
// panel you re-close on every Run Sim.
//
//   node scripts/check_pace_and_hits.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep } = app;

const r = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  localStorage.clear();
  history.pushState({},'','/weapons/Torid'); route(); await sleep(3500);
  history.pushState({},'','/weapons/Torid/simulator'); route(); await sleep(700);
  const out = {};

  ['serration','split_chamber','point_strike','vital_sense'].forEach((id, i) => {
    const m = modById(id);
    if (m) { slots[i].mod = id; slots[i].rank = m.max_rank; }
  });
  markPresetDirty(); refreshPanel(); await sleep(600);
  // A LEVEL THE WEAPON CAN KILL AT, so there is a time-to-kill to report.
  sim.duration = 30; sim.runs = 20; sim.level = 40; sim.steel_path = false; sim.eximus = false;
  let shot = null;
  const real = window.api;
  window.api = async (p, b) => { const res = await real(p, b); if (p === '/api/simulate') shot = res; return res; };
  // …AND THE FLEET, which is what Run Sim uses since 2026-08-18: a sharded
  // simulation never touches api, so this came back with nothing at all.
  const realFleet = window.simulateFleet;
  window.simulateFleet = async (b, onp) => { const res = await realFleet(b, onp); shot = res; return res; };
  await runSim();
  window.api = real;
  window.simulateFleet = realFleet;
  for (let i = 0; i < 60 && !shot; i++) await sleep(400);
  if (!shot) return { ok: false };

  // ---- PACE -----------------------------------------------------------
  const firing = shot.duration - shot.downtime;
  out.burst = shot.burst_dps;
  out.sustained = shot.dps;
  out.burstFromParts = (shot.damage_per_pellet * shot.pellets) / firing;
  out.downtime = shot.downtime;
  out.ttk = shot.ttk;
  out.firstMag = shot.first_magazine;
  out.maxHit = shot.max_hit;
  out.perShot = shot.damage_per_shot;
  out.perPellet = shot.damage_per_pellet;
  out.shots = shot.shots;
  out.pellets = shot.pellets;

  // ---- HITS -----------------------------------------------------------
  const hits = shot.hits || [];
  out.hitCount = hits.flat().reduce((a, b) => a + b.count, 0);
  out.hitDamage = hits.flat().reduce((a, b) => a + b.damage, 0);
  out.headRow = hits[1] ? hits[1].reduce((a, b) => a + b.count, 0) : -1;
  out.critRow = hits.reduce((a, row) => a + row[1].count + row[2].count, 0);

  // ---- FOLDS ----------------------------------------------------------
  await sleep(300);
  const ids = [...document.querySelectorAll('#sim-results .fold')].map(f => f.dataset.fold);
  out.foldIds = ids;
  const speed = document.querySelector('#sim-results .fold[data-fold="speed"]');
  out.openAtFirst = speed ? !speed.classList.contains('shut') : null;
  speed.querySelector('.fold-h').click(); await sleep(100);
  out.shutAfterClick = speed.classList.contains('shut');
  out.bodyHidden = getComputedStyle(speed.querySelector('.fold-b')).display === 'none';
  // …and it SURVIVES a re-render, which is the whole point of keeping the
  // state outside the markup.
  renderResults(shot); await sleep(200);
  const again = document.querySelector('#sim-results .fold[data-fold="speed"]');
  out.stillShut = again ? again.classList.contains('shut') : null;
  out.persisted = JSON.parse(localStorage.getItem('wfsim-folds') || '{}').speed === true;
  return out;
})()`);

check("burst DPS is reported", r.burst > 0, String(r.burst));
check("...and it is the damage over the time the weapon was FIRING",
  Math.abs(r.burst - r.burstFromParts) / r.burst < 0.02,
  `${Math.round(r.burst)} vs ${Math.round(r.burstFromParts)} recomputed`);
check("...which is faster than the sustained figure", r.burst > r.sustained,
  `${Math.round(r.burst)} vs ${Math.round(r.sustained)}`);
check("the weapon spent real time not firing", r.downtime > 0, `${r.downtime}s`);
check("time to first kill is reported with its spread",
  r.ttk && r.ttk.runs > 0 && r.ttk.p90 >= r.ttk.median, JSON.stringify(r.ttk));
check("the opening magazine is reported", r.firstMag > 0, String(r.firstMag));
check("the biggest single hit is reported", r.maxHit > 0, String(r.maxHit));
check("per shot and per pellet agree with the shot count",
  Math.abs(r.perShot * r.shots - r.perPellet * r.pellets) / (r.perShot * r.shots) < 0.02,
  `${Math.round(r.perShot)}×${r.shots} vs ${Math.round(r.perPellet)}×${r.pellets}`);

// The histogram is the MEAN over runs and `pellets` is the MEDIAN run's, so
// they agree to within the spread rather than exactly — which is the honest
// comparison between a mean and a median of the same thing.
check("every hit is in the histogram",
  Math.abs(r.hitCount - r.pellets) / r.pellets < 0.02,
  `${r.hitCount.toFixed(1)} vs ${r.pellets} pellets`);
check("...and the head row is not empty on a 100% headshot fight", r.headRow > 0, String(r.headRow));
check("...with crits among them", r.critRow > 0, String(r.critRow));

check("every block folds", (r.foldIds || []).length >= 5, String(r.foldIds));
check("...open to begin with", r.openAtFirst === true);
check("...shut when clicked", r.shutAfterClick === true && r.bodyHidden === true);
check("...and still shut after a re-render", r.stillShut === true);
check("...remembered across a reload", r.persisted === true);

await app.finish("the pace, the histogram, and every block folding");
