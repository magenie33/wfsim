// THE ACCOUNT OF ONE HIT HAS TO MULTIPLY OUT.
//
// Every other number this sim reports is an aggregate, and an aggregate hides
// an error inside an average: a factor applied twice, or in the wrong bracket,
// moves a mean by a few per cent and reads as "this build is good". The account
// is the one output that can be FALSIFIED (owner, 2026-08-11) — and it
// is only falsifiable if the product of its own lines is the number it
// claims.
//
// So this check does the arithmetic the reader would do. If a factor is ever
// applied in the engine and not listed here, or listed and not applied, the
// product stops matching and this fails — which is the whole reason the account
// is written at the one site where every factor exists at once, rather than
// reconstructed afterwards.
//
//   node scripts/check_hit_account.mjs
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

  // A build with something in every bracket, so the account has more than ones
  // in it: base damage, crit, an element, and a faction the target answers to.
  const want = ['serration','split_chamber','point_strike','vital_sense','hellfire'];
  want.forEach((id, i) => {
    const m = modById(id);
    if (m) { slots[i].mod = id; slots[i].rank = m.max_rank; }
  });
  markPresetDirty(); refreshPanel(); await sleep(600);
  sim.duration = 20; sim.runs = 12; sim.level = 60; sim.steel_path = false;
  let shot = null;
  const real = window.api;
  window.api = async (p, b) => { const res = await real(p, b); if (p === '/api/simulate') shot = res; return res; };
  // …AND OFF THE FLEET, which is what Run Sim uses since 2026-08-18: a sharded
  // simulation never touches api at all, so an interception of it alone came
  // back empty and this file reported "no account" for a run that had one.
  const realFleet = window.simulateFleet;
  window.simulateFleet = async (b, onp) => { const res = await realFleet(b, onp); shot = res; return res; };
  await runSim();
  window.api = real;
  window.simulateFleet = realFleet;
  for (let i = 0; i < 60 && !shot; i++) await sleep(400);
  const acc = (shot && shot.replay && shot.replay.accounts) || [];
  out.count = acc.length;
  out.sources = acc.map(a => a.source);
  out.rows = acc.map(a => {
    const product = a.steps.reduce((x, s) => x * s.mult, a.base);
    return {
      source: a.source,
      base: a.base,
      raw: a.raw,
      effective: a.effective,
      product,
      // The arithmetic the reader would do, as a relative error.
      err: a.raw > 0 ? Math.abs(product - a.raw) / a.raw : (product === 0 ? 0 : 1),
      steps: a.steps.map(s => s.label),
      tier: a.tier,
      t: a.t,
      mitigated: a.effective <= a.raw + 1e-9,
    };
  });
  // …and it is ON THE PAGE, not just in the payload.
  await sleep(300);
  out.drawn = document.querySelectorAll('.acct').length;
  out.hasHeading = [...document.querySelectorAll('h3')].some(h => /account of one hit|一发的账/.test(h.textContent));
  const first = document.querySelector('.acct-t');
  out.lines = first ? first.querySelectorAll('tr').length : 0;
  return out;
})()`);

check("the run recorded an account", r.count > 0, `${r.count}`);
check("...one per attack part", new Set(r.sources).size === r.sources.length, String(r.sources));

for (const row of r.rows || []) {
  check(`the ${row.source} account multiplies out`, row.err < 1e-9,
    `base ${row.base.toFixed(3)} × steps = ${row.product.toFixed(3)}, claimed ${row.raw.toFixed(3)}`);
  check(`...and the ${row.source} target took no more than was dealt`, row.mitigated === true,
    `${row.effective} vs ${row.raw}`);
}

check("the panel draws it", r.hasHeading === true);
check("...one card per account", r.drawn === r.count, `${r.drawn} cards for ${r.count} accounts`);
check("...with every factor on its own line", r.lines >= 6, `${r.lines} rows`);

await app.finish("the account of one hit multiplies out");
