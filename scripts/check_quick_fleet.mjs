// THE QUICK DESCENT ACROSS THE FLEET answers the same whatever cores score it
// (08-checkpoint-api.js `quickFleet`): one worker, three, and a count that
// changes every round give the same build, the same number and the same work,
// and so do workers that rest between calls (a share below 100%).
// Two volunteers' answers agree only to the bit (worker/appraise.js), so a
// search that depended on its slicing would never be confirmed. A second
// background search starts on the workers the first handed back.
//   node scripts/check_quick_fleet.mjs        (WFSIM_BASE=<origin> for a dev server)
import { openApp } from "./cdp.mjs";

// A FROZEN SURVEY QUESTION, cut short: its fight shorter and fewer runs and starts.
const REQUEST = {"weapon":"furis","rivens":[{"id":"rmv0r8x1z402","name":"riven 1","spec":{"bonuses":[{"id":"cold","roll":1.1},{"id":"critical_chance","roll":1.1}],"malus":null,"rank":8,"polarity":"madurai","shape":"2","drafts":{"2":{"bonuses":[{"id":"cold","roll":1.1},{"id":"critical_chance","roll":1.1}],"malus":null},"3":{"bonuses":[{"id":null,"roll":1},{"id":null,"roll":1},{"id":null,"roll":1}],"malus":null},"3+1":{"bonuses":[{"id":null,"roll":1},{"id":null,"roll":1},{"id":null,"roll":1}],"malus":{"id":null,"roll":1}},"2+1":{"bonuses":[{"id":null,"roll":1},{"id":null,"roll":1}],"malus":{"id":null,"roll":1}}}}}],"every_rank":{"arcanes":[],"mods":["hunter_track","continuous_misery","lingering_torment","perpetual_agony","augur_seeker","lasting_sting"]},"mode":"cycle","valence_element":"","valence_bonus":0,"stance":"","enemy":"thrax_centurion","level":9999,"steel_path":true,"guardian_aura":false,"ancient_protector_aura":false,"eximus":null,"squad_size":1,"headshot_pct":100,"aiming":true,"player_at":[0,0],"target_at":[0,0.5],"formation":[],"also_acting":[],"aim_at":null,"invisible":false,"airborne":false,"sliding":false,"aim_gliding":false,"overshields":false,"channeling":false,"melee_equipped":true,"solo_weapon":false,"buff_triggers_off":[],"frame":"","infinite_ammo":true,"metric":"kpm","duration":20,"buffs":{},"ability_strength":null,"abilities":[],"apl":[],"extra_stats":{},"auras":[],"shards":[],"seed":12648430,"spectral_form":false,"wf_energy_pct":1,"__weapon":"furis","runs":4,"custom_enemies":[],"finalists":1,"strategy":"quick","starts":[{"slots":["riven:rmv0r8x1z402","scorch",null,null,null,null,null,null,null,null],"evolutions":["furis_evo1_incarnon_form","furis_haven_foray","furis_practiced_grip","furis_headcracker"],"arcane":["none"],"arcane_rank":[null],"mode":null,"valence_element":null,"fixed":[{"kind":"mods","idx":0}]},{"slots":["riven:rmv0r8x1z402","frostbite",null,null,null,null,null,null,null,null],"evolutions":["furis_evo1_incarnon_form","furis_haven_foray","furis_practiced_grip","furis_headcracker"],"arcane":["none"],"arcane_rank":[null],"mode":null,"valence_element":null,"fixed":[{"kind":"mods","idx":0}]}],"candidate_runs":2,"limits":{"exclude":{"mods":[],"arcanes":[],"evolutions":[],"modes":[],"valence":[]},"mods":8,"exilus":true,"arcane_seats":[true]}};

const app = await openApp({ boot: 13000, base: process.env.WFSIM_BASE });
const { evaluate, check } = app;
const r = await evaluate(`(async () => {
  const body = ${JSON.stringify(REQUEST)};
  const run = async (n, opts) => {
    const job = quickFleet(body, n, async () => {}, opts);
    while (!job.result) await new Promise((ok) => setTimeout(ok, 200));
    const best = (job.result.results || [])[0] || {};
    return { ok: job.result.ok !== false, error: job.result.error || null, work: job.result.work,
      kill: best.kill_progress, mods: JSON.stringify(best.mods), arcanes: JSON.stringify(best.arcanes), workers: job.workers.length };
  };
  const one = await run(1);
  const three = await run(3, { keep: true });
  const kept = fleetKept.length;
  let flip = 0;
  const changing = await run(() => (flip++ % 2 ? 4 : 1), { keep: true });
  const keptAfter = fleetKept.length;
  const rested = await run(2, { rest: (ms) => Math.min(20, ms) });
  return { one, three, kept, changing, keptAfter, rested };
})()`, 600000);

const same = (a, b) => a.ok && b.ok && a.work === b.work && a.kill === b.kill && a.mods === b.mods && a.arcanes === b.arcanes;
check("the search finds a build", r.one.ok && r.one.mods && r.one.mods !== "[]" && r.one.work > 0, JSON.stringify(r.one));
check("three workers give the same build, number and work as one", same(r.one, r.three), JSON.stringify([r.one, r.three]));
check("...and a count that changes every round gives them too", same(r.one, r.changing), JSON.stringify([r.one, r.changing]));
check("...and so do workers resting between calls, as a share below 100% has them", same(r.one, r.rested), JSON.stringify([r.one, r.rested]));
check("a background search hands its workers back for the next one", r.kept === 3 && r.keptAfter >= 3, JSON.stringify(r));
await app.finish("a quick search answers the same on any number of cores, and a volunteer's next one starts warm");
