// THE VALENCE BONUS — an adversary weapon's bonus element, on screen and in the
// number.
//
// The TWENTY-SEVENTH check, and the first about a build axis that is a property
// of the COPY a player owns rather than of the model. A Kuva Lich hands out
// 25–60% of base damage as one of seven elements, so two Kuva Nukors are two
// different weapons and neither is "the" Kuva Nukor (owner, 2026-08-13:
// "kuva武器有个初属性。类似evo得多一块建立").
//
// It is checked on the NUMBER rather than on the control, for the reason every
// axis here is: a dropdown that stores a value nobody reads looks exactly like
// one that works. And on the ARITHMETIC rather than on a direction, because the
// wiki states the rule exactly — "ranging from 25-60% of the weapon's base
// damage … applies as weapon base damage" — so 21 Radiation plus a 60% Toxin
// progenitor is 33.6, and nothing else.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Kuva_Nukor'); route(); await sleep(3500);

  const spec = (weaponInfo('kuva_nukor') || {}).valence || null;
  const block = document.getElementById('element-block');
  const shown = !!block && !block.hidden;

  // The panel's own base, which is where a base-damage add has to land.
  const panel = async () => {
    const s = await api('/api/simulate', { ...buildPayload(), ...fightPayload(sim), runs: 2 });
    return { base: (s.panel || {}).modified_base, dmg: (s.panel || {}).damage };
  };
  // WHAT IT OPENS ON, before anything is touched: every copy of an adversary
  // weapon comes out of its Lich carrying a bonus, so a fresh page must not
  // show a weapon nobody has.
  const opened = JSON.parse(JSON.stringify(valence));
  const openedBase = (await panel()).base;

  // …and the weapon's own printed panel, which "none" is the only way to see.
  valence.element = ''; renderValence(); refreshPanel(); await sleep(1200);
  const bare = await panel();

  // Pick TOXIN at the ceiling: it is not the weapon's own element, so it has to
  // arrive beside the Radiation rather than merge into it.
  valence.element = 'toxin'; valence.bonus = 0.60;
  renderValence(); refreshPanel(); await sleep(1500);
  const toxin = await panel();

  // …and RADIATION at the ceiling, which the weapon already deals: it must
  // MERGE, so the total is the same 33.6 with one element and not two.
  valence.element = 'radiation';
  renderValence(); refreshPanel(); await sleep(1500);
  const rad = await panel();

  // IT SAVES WITH THE BUILD, not the fight: a valence is a statement about one
  // weapon, and the fight is shared across the roster.
  const buildDoc = snapshotState();
  const inFight = JSON.stringify(snapshotScenario()).includes('valence');

  // …AND NOTHING CROSSES BETWEEN WEAPONS. Open an ordinary weapon and the axis
  // is gone, along with the choice.
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(2500);
  const otherShown = !!document.getElementById('element-block')
    && !document.getElementById('element-block').hidden;
  const otherValence = JSON.parse(JSON.stringify(valence));

  return { spec, shown, opened, openedBase, bare, toxin, rad,
           savedElement: (buildDoc.valence || {}).element,
           savedBonus: (buildDoc.valence || {}).bonus,
           inFight, otherShown, otherValence };
})()`);

check("the weapon declares which elements its Lich can roll",
  r.spec && r.spec.elements.length === 7 && r.spec.min === 0.25 && r.spec.max === 0.6,
  JSON.stringify(r.spec));
// PUNCTURE AND SLASH ARE NOT PROGENITOR ELEMENTS. A list that merely had seven
// entries would pass a length check and be wrong.
check("...the seven the wiki names, and not one more",
  r.spec && !r.spec.elements.includes("puncture") && !r.spec.elements.includes("slash")
    && ["impact", "heat", "cold", "electricity", "toxin", "magnetic", "radiation"]
      .every((e) => r.spec.elements.includes(e)),
  JSON.stringify(r.spec && r.spec.elements));
check("the block is drawn for an adversary weapon", r.shown === true, String(r.shown));
// 60% IMPACT, the owner's own default: no copy of this weapon exists without a
// bonus, so a fresh page opens on one. 21 x 1.60 = 33.6.
check("it opens on 60% Impact, not on a weapon nobody has",
  r.opened.element === "impact" && r.opened.bonus === 0.6
    && Math.abs(r.openedBase - 33.6) < 1e-6,
  JSON.stringify({ ...r.opened, base: r.openedBase }));
// 21 Radiation is the infobox's own number, and "none" is the only way to it.
check("...and `none` still shows the weapon's printed panel",
  Math.abs(r.bare.base - 21) < 1e-6, String(r.bare.base));
// THE ARITHMETIC, exactly: 21 + 21 × 0.60 = 33.6, as BASE damage.
check("a 60% Toxin progenitor is +12.6 Toxin beside the Radiation",
  Math.abs(r.toxin.base - 33.6) < 1e-6, `${r.bare.base} -> ${r.toxin.base}`);
// …AND THE MERGE, which is the half a "new element" implementation gets wrong.
check("...and a Radiation one merges into the 21 it already deals",
  Math.abs(r.rad.base - 33.6) < 1e-6, String(r.rad.base));
check("it saves with the BUILD, not the fight",
  r.savedElement === "radiation" && r.savedBonus === 0.6 && r.inFight === false,
  JSON.stringify({ element: r.savedElement, bonus: r.savedBonus, inFight: r.inFight }));
check("an ordinary weapon has no such axis, and inherits no choice",
  r.otherShown === false && r.otherValence.element === "",
  JSON.stringify({ shown: r.otherShown, carried: r.otherValence }));

// …AND THE QUICK CALC RANKS IT, the same way it ranks a tier of evolutions
// (owner, 2026-08-13: "融合属性 这个也是要参与快速计算的，和evo是一样的"). It is
// the axis a scan is worth the most on: a progenitor element is a whole element
// entering the hierarchy, so which one wins depends on the mods around it and on
// the target — not a question anyone answers by reading cards.
const gain = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Kuva_Nukor'); route(); await sleep(3500);
  for (let i = 0; i < 60 && !(gainScan.axis && gainScan.axis.kind === 'valence' && !gainScan.running); i++) {
    await sleep(500);
  }
  // …and the BUILDER's step numbers follow the blocks this weapon actually has.
  // THE BUILDER's blocks only. The config page also holds the Sim, Rivens,
  // Enemies and Optimizer tabs, which have their own numbering — a sweep over
  // the shared class is exactly the bug this renumbering had on its first pass
  // (it made the Rivens editor step 5 of building a gun).
  const steps = BUILDER_BLOCKS
    .map(id => document.getElementById(id))
    .filter(b => b && !b.hidden)
    .map(b => b.id + ':' + ((b.querySelector('.bh .n') || {}).textContent || ''));
  // …and the chips are ON SCREEN, in the same component every other ranked
  // axis uses. A scan whose answers never reach a pick is a scan nobody reads.
  const chips = [...document.querySelectorAll('#element-cfg .evopick .gainchip')].length;
  const picks = [...document.querySelectorAll('#element-cfg .evopick')].length;
  return { kind: gainScan.axis && gainScan.axis.kind,
           ranked: Object.keys(gainScan.by || {}).sort(),
           cur: valence.element,
           chips, picks,
           steps };
})()`);

// SIX, not seven: the element the build is already on is the BASE run, and a
// candidate that is the current choice would be measuring nothing.
check("the quick calc ranks the valence axis",
  gain.kind === "valence" && gain.ranked.length === 6 && !gain.ranked.includes(gain.cur),
  `${gain.kind} ranked ${gain.ranked.length}: ${gain.ranked.join(",")} (on ${gain.cur})`);
// THE SAME CHIP COMPONENT the mod and arcane lists use, on the pick itself —
// eight picks (None plus the seven elements) and a chip on each ranked one.
check("...with the gain on the pick, not in a tooltip",
  gain.picks === 8 && gain.chips === 6,
  `${gain.picks} picks, ${gain.chips} chips`);
// THE STEP NUMBERS ARE DERIVED, not written into the markup (owner: "不应该写死
// 的。应该取决于当前的武器的模块个数"). This weapon has no evolutions, so its
// Valence block is step 4 — there is no 5 with nothing at 4.
check("...and the builder numbers its steps from the blocks it actually has",
  gain.steps.join(" ") === "mode-block:1 mod-block:2 arcane-block:3 element-block:4",
  gain.steps.join(" "));

// …AND IT IS THE OPTIMIZER'S DIMENSION, the other half of "just like an evo".
// Pinning one element brings every ranked row back in it; pooling two doubles
// the candidate count and each row carries the element it was scored with.
const opt = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Kuva_Nukor/optimizer'); route(); await sleep(4000);
  const sect = document.getElementById('opt-valence-sect');
  const shown = !!sect && !sect.hidden;
  const rows = [...document.querySelectorAll('#opt-valence .opt')].length;

  const run = async (marks) => {
    const body = {
      weapon: 'kuva_nukor',
      mods: { hornet_strike: 'search', barrel_diffusion: 'search', lethal_torrent: 'search',
              primed_pistol_gambit: 'search', pathogen_rounds: 'search' },
      build_size: 2, build_min: 2,
      arcanes: {}, evolutions: {}, modes: {}, exilus: {},
      valence: marks,
      valence_element: 'impact', valence_bonus: 0.6,
      ...fightPayload(snapshotScenario()),
      duration: 8, runs: 2, final_runs: 2, finalists: 3, threads: 1, buffs: {},
    };
    const r = await postJson('/api/optimize', body);
    let s = r;
    for (let i = 0; i < 400 && (!s || !s.done); i++) {
      await sleep(300);
      s = await postJson('/api/optimize/status', {});
    }
    const res = (s && s.result && s.result.results) || (s && s.results) || [];
    return { n: (s && (s.candidates || (s.result||{}).candidates)) || 0,
             els: [...new Set(res.map(x => x.valence))] };
  };
  const pinned = await run({ toxin: 'fixed' });
  const pooled = await run({ toxin: 'search', heat: 'search' });
  return { shown, rows, pinned, pooled };
})()`);

check("the optimizer offers the same seven elements", opt.shown === true && opt.rows === 7,
  `${opt.rows} rows, shown ${opt.shown}`);
check("...pinning one brings every ranked row back in it",
  opt.pinned.els.length === 1 && opt.pinned.els[0] === "toxin",
  JSON.stringify(opt.pinned.els));
check("...and pooling two doubles the candidate count",
  opt.pooled.n === opt.pinned.n * 2,
  `${opt.pinned.n} -> ${opt.pooled.n}`);

await app.finish("an adversary weapon's Valence bonus reaches its damage");
