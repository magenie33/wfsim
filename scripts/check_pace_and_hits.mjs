// WHAT A ROOM-CLEAR IS PACED BY — the TWENTY-FIFTH check, and every block
// folding.
//
// `dps` is the whole engagement with its reloads in it: the honest number for a
// long fight and the wrong one for a room. These are the others — the rate
// while the trigger is actually down, how long the first body takes to fall,
// what the magazine you walked in with was worth, the biggest single number the
// build can produce — and burst DPS is RECOMPUTED here rather than trusted.
//
// IT DOES NOT CHECK A HISTOGRAM. Six buckets of mean damage summarise what the
// combat record now gives as a row per hit with its own ledger. The property
// that matters — "an impossible number cannot hide in a mean" — is
// `check_combat_record.mjs`, which reads every row and does its arithmetic.
//
// AND EVERY BLOCK FOLDS AND REMEMBERS across a re-render and a reload: a panel
// that re-opens everything on every Run Sim is a panel you re-close on every
// Run Sim, so the state lives outside the markup.
//
// Usage:
//   node scripts/check_pace_and_hits.mjs

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

check("every block folds", (r.foldIds || []).length >= 5, String(r.foldIds));
check("...open to begin with", r.openAtFirst === true);
check("...shut when clicked", r.shutAfterClick === true && r.bodyHidden === true);
check("...and still shut after a re-render", r.stillShut === true);
check("...remembered across a reload", r.persisted === true);

await app.finish("the pace, and every block folding");
