// THERE IS ONE FIGHT, AND EVERY MODULE SENDS IT.
//
// The THIRTY-THIRD check, and the page's half of AGENTS.md's hard rule. The
// server's half has been true since `parse_fight`: the fight is parsed once
// and the optimizer calls it, so the two cannot read different fields. The
// PAGE had no such half — it grew FIVE spellings of "the fight", and each one
// was a chance for a module to measure something the simulator never runs:
//
//   * Run Sim sent `fightPayload()`;
//   * the share card sent a RAW `sim` — no run count, so the server's own
//     default rather than the page's, and no `custom_enemies`, so the claim on
//     a card for a fight against a target you MADE was measured against a
//     target the server had never heard of;
//   * the quick calc resolved a scenario of its OWN, from a sticky persisted
//     pointer that outlived the weapon, the scenario and the session — build a
//     nine-body Ocucor fight, switch the simulator to it, and every mod was
//     still ranked under whichever scenario that popover was last left on;
//   * the optimizer's gain scan spread that scenario raw, without
//     `custom_enemies`;
//   * the optimizer itself sent the STORED shape of the fight rather than the
//     live one.
//
// Each was right when written and none of them was right by the end, which is
// the argument against having five: a fight gains a field — `custom_enemies`,
// a formation, an aim point — and it reaches whichever spellings somebody
// remembered.
//
// IT HOLDS NO LIST OF FIELDS, which is what makes it the strong half of the
// pair with `check_run_counts` (that one reads the box; this one reads the
// wire). The expected value is `theFight()` ITSELF, so every key it carries is
// asserted on every module's outgoing request — including keys nobody has
// invented yet. The only exemptions are the four things a CALLER legitimately
// owns rather than the fight: `runs` (the quick calc's own precision, the one
// axis that is deliberately decoupled), `seed` and `run_series` (a scan pairs
// its two builds run for run) and `replay` (Run Sim alone pays for one).
//
//   node scripts/check_one_fight.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, finish } = app;

const r = await evaluate(`(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const out = {};
  localStorage.clear();
  history.pushState({}, '', '/weapons/Ocucor/simulator'); route(); await sleep(3500);

  // A FIGHT OF YOUR OWN, and one that is DISTINCTIVE on every axis a module
  // could drop: a formation, an aim point of its own, a Warframe ability, a
  // buff setting, a level. A default fight would pass this check by
  // coincidence — two modules that both send nothing agree.
  document.querySelector('#preset-bar-simulator-scenarios .pchip.add').click();
  await sleep(1500);
  out.editable = typeof officialScenarioActive === 'function' && !officialScenarioActive();
  sim.level = 137;
  sim.duration = 6;
  for (let i = 0; i < 3; i++) arenaAddFoe(sim);
  sim.aim_at = [sim.target_at[0] + 1.5, sim.target_at[1]];
  sim.abilities = [{ id: 'roar', secs: null }];
  sim.ability_strength = 1.6;
  const anyBuff = (buffList[0] || {}).id;
  if (anyBuff) sim.buffs = { ...sim.buffs, [anyBuff]: { stacks: 3, locked: true } };
  markScenarioDirty();
  // The simulator's own count, small — this check runs REAL sims and cares
  // about what is in them, not how many.
  const rb = document.getElementById('sim-runs');
  if (rb) { rb.value = '2'; rb.dispatchEvent(new Event('change', { bubbles: true })); }
  await sleep(1200);

  const want = theFight();
  out.wantKeys = Object.keys(want).length;
  out.wantFormation = (want.formation || []).length;
  out.wantAim = !!want.aim_at;
  out.wantAbilities = (want.abilities || []).length;
  out.wantCustomKey = 'custom_enemies' in want;

  // THE WIRE. Every transport funnels through \`api\` — \`postJson\` delegates to
  // it and \`/api/optimize\` is dispatched inside it — so one interception sees
  // every module. The optimize call is STUBBED rather than forwarded: this
  // check is about what was sent, and a real search costs minutes.
  const seen = [];
  const realApi = api;
  window.api = async (p, b) => {
    if (p === '/api/simulate' || p === '/api/optimize' || p === '/api/pairings') seen.push({ p, b });
    if (p === '/api/optimize') return { ok: false, error: 'intercepted by check_one_fight' };
    return realApi(p, b);
  };
  // THE FLEET IS A SIXTH WAY TO SEND THE FIGHT, and this check
  // exists to say they all send the same one — so it watches that path too.
  // Run Sim and the share card both go through it now, and a sharded
  // simulation never touches api at all.
  const realFleet = window.simulateFleet;
  window.simulateFleet = async (b, onp) => {
    seen.push({ p: '/api/simulate', b });
    return realFleet(b, onp);
  };
  const take = () => { const n = seen.length; return () => seen.slice(n); };

  // ---- 1. THE SIMULATOR, which is the truth the other three obey ----------
  let mark = take();
  document.getElementById('run-sim').click();
  for (let i = 0; i < 60 && !mark().length; i++) await sleep(300);
  out.simSent = mark()[0] ? mark()[0].b : null;
  out.simReplay = out.simSent ? out.simSent.replay === true : null;
  for (let i = 0; i < 60 && document.getElementById('run-sim').disabled; i++) await sleep(300);

  // ---- 2. THE QUICK CALC --------------------------------------------------
  // Its lanes are Web Workers in the shipping build and bypass \`api\`, so they
  // are pointed back through it: the body is built at ONE site either way, and
  // routing it here is what makes it visible. Cancelled as soon as the first
  // request is out — the generation counter is how a scan stands down.
  gainPool = [{ call: (p, b) => api(p, b) }];
  gainPrefs = { ...gainPrefs, on: true, runs: 30 };
  mark = take();
  scanGains({ kind: 'mods', idx: 0 }, null);
  for (let i = 0; i < 60 && !mark().length; i++) await sleep(200);
  gainGen++;
  out.gainSent = mark()[0] ? mark()[0].b : null;
  await sleep(500);

  // ---- 3. THE OPTIMIZER'S GAIN SCAN --------------------------------------
  document.querySelector('[data-tab="optimizer"]')?.click(); await sleep(800);
  mark = take();
  try { scanOptGains(null); } catch (_) {}
  for (let i = 0; i < 60 && !mark().length; i++) await sleep(200);
  optGainGen++;
  out.optGainSent = mark()[0] ? mark()[0].b : null;
  out.optGainPath = mark()[0] ? mark()[0].p : null;
  await sleep(500);

  // ---- 4. THE OPTIMIZER ITSELF -------------------------------------------
  mark = take();
  try { await runOptimize(); } catch (_) {}
  for (let i = 0; i < 40 && !mark().length; i++) await sleep(200);
  out.optSent = mark()[0] ? mark()[0].b : null;
  out.optPath = mark()[0] ? mark()[0].p : null;

  // ---- 5. THE SHARE CARD'S OWN MEASUREMENT -------------------------------
  // It reuses the build's stored result when that result was measured under
  // THIS fight, which Run Sim above just made true — so the cache is dropped
  // to force the measurement this check is about.
  const bp = loadPresetList(BUILDS);
  const bi = bp.findIndex((x) => x.name === activePreset);
  if (bi >= 0) { delete bp[bi].lastResult; storePresetList(BUILDS, bp); }
  await sleep(400);
  mark = take();
  try { await resultForShare(); } catch (_) {}
  out.shareSent = mark()[0] ? mark()[0].b : null;

  window.api = realApi;
  window.simulateFleet = realFleet;
  // Read the expectation AFTER the run too: nothing above may have edited the
  // fight, which is the other half of "the count is the scan's, not the
  // scenario's".
  out.unchanged = JSON.stringify(theFight()) === JSON.stringify(want);
  out.want = want;
  return out;
})()`);

check("the run starts in an editable fight of its own", r.editable === true);
check("...with a formation, an aim point, an ability and a buff on it",
  r.wantFormation === 3 && r.wantAim === true && r.wantAbilities === 1 && r.wantCustomKey === true,
  JSON.stringify({ formation: r.wantFormation, aim: r.wantAim, abilities: r.wantAbilities }));

// A CALLER OWNS FOUR THINGS. Everything else in the fight is the fight's.
const OWNED = new Set(["runs", "seed", "run_series", "replay"]);
const eq = (a, b) => JSON.stringify(a) === JSON.stringify(b);

const sameFight = (label, body) => {
  if (!body) { check(label, false, "nothing was sent"); return; }
  const bad = Object.keys(r.want)
    .filter((k) => !OWNED.has(k))
    .filter((k) => !eq(body[k], r.want[k]))
    .map((k) => `${k}: sent ${JSON.stringify(body[k])}, fight has ${JSON.stringify(r.want[k])}`);
  check(label, bad.length === 0, bad.slice(0, 4).join(" | "));
};

sameFight("the SIMULATOR sends the fight", r.simSent);
check("...and it is the only one that pays for a replay", r.simReplay === true);
sameFight("the QUICK CALC sends the SAME fight, field for field", r.gainSent);
sameFight("the OPTIMIZER'S GAIN SCAN sends the same fight", r.optGainSent);
sameFight("the OPTIMIZER sends the same fight", r.optSent);
sameFight("the SHARE CARD'S measurement is made in the same fight", r.shareSent);

// THE ONE DELIBERATE DIVERGENCE, asserted so it stays deliberate: the quick
// calc's precision is the reader's, and it must not be the simulator's and must
// not edit the fight.
check("...the quick calc's run count is its OWN, and reaches the wire",
  r.gainSent && r.gainSent.runs === 30 && r.simSent && r.simSent.runs !== 30,
  JSON.stringify({ scan: r.gainSent && r.gainSent.runs, sim: r.simSent && r.simSent.runs }));
check("...and pairs its two builds run for run", r.gainSent && r.gainSent.run_series === true);
check("...while the fight itself is untouched by any of them", r.unchanged === true);

await finish("one fight, and every module sends it");
