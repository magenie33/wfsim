// A WARFRAME BUFF IS THE FIGHT'S, AND IT REACHES THE NUMBER.
//
// The NINETEENTH check, and the first for a family that belongs to neither the
// build nor the weapon: Roar, Eclipse, Nourish and the four elemental augments
// are things done TO this weapon for a while (`data/abilities/`). That
// ownership is the whole design, and every claim below is a way it could
// silently stop being true:
//
//   · the section DRAWS, in both languages, with DE's own ability names —
//     these are transcribed (战吼, 黯然失色), and a phrase-substituted card
//     would read as a translation nobody wrote;
//   · the value on the card FOLLOWS Ability Strength, because a page that
//     shows a static +50% while the sim runs +100% is worse than showing
//     nothing;
//   · ticking one MOVES THE SIM — asserted against a real /api/simulate in the
//     shipping wasm build, not against the state object;
//   · two of a FAMILY do not stack and the page SAYS which one lost. This is
//     the rule the owner asked for by name (2026-08-08: "同时选了 roar 和
//     roar（helminth），那就选择生效当前最强的") and it is the one a player
//     cannot verify by eye — the difference between +50% and +80% is a number
//     you have to be told;
//   · the OPTIMIZER shows the same buffs, read-only, because it runs the
//     simulator's fight and a search scored under a different Roar is scored
//     under a fight nobody can reproduce;
//   · and the BOARD carries none — the negative control, and the reason a
//     board row is still a statement about the weapon.
//
//   node scripts/check_wf_buffs.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, send, sleep, BASE } = app;

for (const lang of ["en", "zh"]) {
  await send("Page.navigate", { url: BASE });
  await sleep(lang === "en" ? 12000 : 4000);
  // A CLEAN MACHINE EACH PASS. The second language would otherwise inherit the
  // first one's copied scenario — with Roar already ticked — and "ticking a
  // buff moves the sim" would compare a buffed run against a buffed run.
  await evaluate(`localStorage.clear(); localStorage.setItem('wfsim-lang', ${JSON.stringify(lang)})`);
  await send("Page.navigate", { url: BASE });
  await sleep(12000);

  const r = await evaluate(`(async () => {
    const sleep = ms => new Promise(r => setTimeout(r, ms));
    const out = { lang: ${JSON.stringify(lang)} };
    history.pushState({}, '', '/weapons/Torid'); route(); await sleep(3800);
    // THE FIGHT ARRIVES READ-ONLY: the default scenario is the OFFICIAL ruler,
    // and a ruler casts nothing. Take an editable copy first — the same flow a
    // player follows, and the reason the ruler assertion at the bottom means
    // something.
    if (officialScenarioActive()) { copyActiveScenario(); await sleep(1500); }

    // 1. THE SECTION IS DRAWN, one card per ability in the data.
    const cards = () => [...document.querySelectorAll('#sim-wfbuffs .wfb')];
    out.catalogue = (META.abilities || []).length;
    out.cards = cards().length;
    out.names = cards().map(c => (c.querySelector('.wfb-n') || {}).textContent || '');
    out.values = cards().map(c => (c.querySelector('.wfb-v') || {}).textContent || '');
    out.effects = cards().map(c => (c.querySelector('.wfb-e') || {}).textContent || '');
    // ITS OWN BLOCK, not a section of the fight (owner, 2026-08-08).
    out.ownBlock = !!document.querySelector('#wfbuff-block #sim-wfbuffs');
    out.insideFight = !!document.querySelector('#sim-block #sim-wfbuffs');
    out.early = ((document.querySelector('#sim-wfbuffs .wfb-early') || {}).textContent || '').trim();

    // 2. THE VALUE FOLLOWS STRENGTH. Roar is +50% at 100% and +100% at 200%.
    const roarText = () => {
      const i = (META.abilities || []).findIndex(a => a.id === 'roar');
      return (cards()[i].querySelector('.wfb-v') || {}).textContent || '';
    };
    out.roarAt100 = roarText();
    const str = document.getElementById('sim-wfbuffs-str');
    str.value = '200'; str.dispatchEvent(new Event('change')); await sleep(300);
    out.roarAt200 = roarText();
    str.value = '100'; str.dispatchEvent(new Event('change')); await sleep(300);

    // 3. TICKING ONE MOVES THE SIM. Same build, same seed, one buff.
    const body = () => ({ ...buildPayload(), ...sim, buffs: {},
      enemy: 'corrupted_heavy_gunner', level: 500, steel_path: true,
      duration: 20, runs: 20, seed: 11 });
    const dpsOf = async () => {
      const x = await api('/api/simulate', body());
      return x && x.ok !== false ? Math.round(x.dps || 0) : -1;
    };
    out.dpsPlain = await dpsOf();
    const tick = async (id, on) => {
      const i = (META.abilities || []).findIndex(a => a.id === id);
      const box = cards()[i].querySelector('[data-wf]');
      if (box.checked !== on) { box.click(); await sleep(250); }
    };
    await tick('roar', true);
    out.sent = JSON.parse(JSON.stringify(sim.abilities || []));
    out.dpsRoar = await dpsOf();

    // 4. TWO OF A FAMILY: only the stronger runs, and the page says so.
    await tick('roar_helminth', true);
    out.dpsBoth = await dpsOf();
    out.deadCards = cards().filter(c => c.classList.contains('dead')).length;
    out.deadWhy = ((cards().find(c => c.classList.contains('dead')) || {})
      .querySelector ? (cards().find(c => c.classList.contains('dead'))
        .querySelector('.wfb-dead') || {}).textContent || '' : '').trim();
    // …and the WEAKER one is the dead one.
    out.deadIsHelminth = cards().some((c, i) =>
      c.classList.contains('dead') && (META.abilities || [])[i].id === 'roar_helminth');

    // 4b. THE QUICK CALC MEASURES UNDER THEM. It reads the scenario, and a
    //     Warframe buff is part of the scenario (owner, 2026-08-08) — so this
    //     needs no plumbing of its own and that is exactly why it is asserted:
    //     gainKey is DERIVED from the fight the scan will run, so a field
    //     nobody had invented when it was written still reaches it.
    // …after the scenario's own auto-save, which is what the quick calc
    // resolves for the ACTIVE preset. Waiting for it here is not papering over
    // a lag: the scan is keyed on the fight it will run, and the fight is not
    // saved yet at the instant a checkbox changes.
    await sleep(1500);
    out.gainScenario = JSON.parse(JSON.stringify(gainScenario().scenario.abilities || []));
    out.gainStrength = gainScenario().scenario.ability_strength;
    const keyWith = gainKey();
    await tick('roar', false); await tick('roar_helminth', false);
    await sleep(1500);
    out.gainKeyMoved = gainKey() !== keyWith;
    out.gainScenarioOff = (gainScenario().scenario.abilities || []).length;
    await tick('roar', true);
    // …and a RULER does not inherit them. A benchmark yaml names only what it
    // has an opinion about, so a ruler spread over the live fight would rank
    // every slot under the ruler's enemy AND your Roar.
    const ruler = (scenarioList().find(x => x.builtin) || {});
    const prev = gainPrefs.scenario;
    gainPrefs.scenario = presetId(ruler);
    out.rulerGain = (gainScenario().scenario.abilities || []).length;
    gainPrefs.scenario = prev;

    // 4c. IT IS THE SIMULATOR'S BLOCK, AND ONLY THE SIMULATOR'S. A Warframe
    //     buff is not part of the weapon — the builder answers "what is this
    //     gun" and a Roar belongs to no gun (owner, 2026-08-09). Checked by
    //     GEOMETRY, not by the class list: hiding is a CSS id list, which is
    //     exactly the kind of thing a new block silently falls out of.
    const seen = (id) => {
      const e = document.getElementById(id);
      if (!e) return null;
      return getComputedStyle(e).display !== 'none' && !e.hidden;
    };
    out.blockByTab = {};
    for (const [name, suffix] of [['builder', ''], ['simulator', '/simulator'],
                                  ['optimizer', '/optimizer']]) {
      history.pushState({}, '', '/weapons/Torid' + suffix); route(); await sleep(2200);
      out.blockByTab[name] = seen('wfbuff-block');
    }
    history.pushState({}, '', '/weapons/Torid/simulator'); route(); await sleep(2200);

    // 5. THE OPTIMIZER SHOWS THE SAME FIGHT, read-only.
    history.pushState({}, '', '/weapons/Torid/optimize'); route(); await sleep(3000);
    const oc = [...document.querySelectorAll('#opt-wfbuffs .wfb')];
    out.optCards = oc.length;
    out.optChecked = oc.filter(c => c.querySelector('[data-wf]').checked).length;
    out.optEditable = oc.filter(c => !c.querySelector('[data-wf]').disabled).length;

    // 6. THE BOARD CARRIES NONE — the negative control.
    history.pushState({}, '', '/benchmark'); route(); await sleep(3000);
    const rulers = (META.benchmarks || []).map(b => b.scenario || {});
    out.rulerAbilities = rulers.reduce(
      (n, s) => n + ((s.abilities || []).length) + (s.ability_strength ? 1 : 0), 0);
    out.rulers = rulers.length;
    return out;
  })()`);

  const cjk = /[一-鿿]/;
  check(`[${lang}] every ability in the data has a card`,
    r.cards === r.catalogue && r.catalogue >= 10,
    `${r.cards} cards for ${r.catalogue} abilities`);
  // DE'S OWN NAMES. In Chinese every one of them is a transcribed string
  // (战吼, 黯然失色, 电击奇兵) — in English every one is DE's English.
  check(`[${lang}] …named the way DE names them`,
    r.names.every((n) => n.length > 1 && cjk.test(n) === (lang === "zh")),
    JSON.stringify(r.names.slice(0, 3)));
  // THE NUMBER IS ON THE CARD, at the strength you set — and separately, the
  // BUCKET it lands in, because two buffs both reading "+50%" are worth
  // different amounts on a DoT weapon.
  check(`[${lang}] …each showing the value at the current strength`,
    r.values.every((v) => /^\+\d/.test(v)), JSON.stringify(r.values.slice(0, 3)));
  check(`[${lang}] …and where it lands, in the display language`,
    r.effects.every((e) => e.length > 8 && cjk.test(e) === (lang === "zh")),
    JSON.stringify(r.effects[0] || "").slice(0, 100));
  // ITS OWN BLOCK. It is not a step of the fight — nothing about it is the
  // enemy, the measurement or the run.
  check(`[${lang}] the buffs are their own block, not a section of the fight`,
    r.ownBlock === true && r.insideFight === false,
    `own=${r.ownBlock} insideFight=${r.insideFight}`);
  // EARLY ACCESS IS ON THE PAGE, not only in a yaml comment: this block moves
  // onto the Warframe later and a player is entitled to know that now.
  check(`[${lang}] …and the block admits it is early access`,
    r.early.length > 10 && cjk.test(r.early) === (lang === "zh"),
    r.early.slice(0, 70));
  check(`[${lang}] the value follows Ability Strength`,
    r.roarAt100.includes("50%") && r.roarAt200.includes("100%"),
    `${r.roarAt100.slice(0, 40)} -> ${r.roarAt200.slice(0, 40)}`);
  // THE ONLY CLAIM THAT MATTERS: it reaches the number, in the shipping build.
  check(`[${lang}] ticking a buff moves the SIM`,
    r.dpsPlain > 0 && r.dpsRoar > r.dpsPlain * 1.2,
    `${r.dpsPlain} -> ${r.dpsRoar}`);
  // WHOLE FIGHT, ALWAYS, for now (owner, 2026-08-08: "目前就只能全程吧") —
  // `secs: null` is that, and the engine's per-buff end time is still there
  // under it for the day Ability Duration supplies one.
  check(`[${lang}] …sent as the fight's own field, running the whole fight`,
    Array.isArray(r.sent) && r.sent.length === 1 && r.sent[0].id === "roar"
      && r.sent[0].secs === null,
    JSON.stringify(r.sent));
  // TWO ROARS ARE ONE ROAR. Adding them would be +80% against +50%, which is
  // a 20% error nobody would spot in a DPS number.
  check(`[${lang}] two buffs of a family do not stack`,
    Math.abs(r.dpsBoth - r.dpsRoar) < Math.max(2, r.dpsRoar * 0.005),
    `${r.dpsRoar} alone vs ${r.dpsBoth} with both`);
  check(`[${lang}] …and the page says WHICH one lost`,
    r.deadCards === 1 && r.deadIsHelminth === true && r.deadWhy.length > 6
      && cjk.test(r.deadWhy) === (lang === "zh"),
    `${r.deadCards} superseded · ${r.deadWhy.slice(0, 50)}`);
  // THE QUICK CALC READS THE SCENARIO, and this is part of the scenario.
  // BOTH PICKS TRAVEL, and that is right: settling a family is the ENGINE's job
  // (`abilities_data::resolve`), so the payload carries what you ticked and the
  // sim carries what runs. A page that filtered here would be a second
  // implementation of the rule.
  check(`[${lang}] the quick calc measures under the buffs`,
    r.gainScenario.length === 2
      && r.gainScenario.every((a) => a.id.startsWith("roar") && a.secs === null)
      && r.gainStrength === 1,
    JSON.stringify(r.gainScenario));
  check(`[${lang}] …and its cache key follows them`,
    r.gainKeyMoved === true && r.gainScenarioOff === 0,
    `moved=${r.gainKeyMoved}, off=${r.gainScenarioOff}`);
  // A RULER IS THE SAME FIGHT FOR EVERYONE OR IT IS NOT A RULER.
  check(`[${lang}] …but an official ruler inherits none of them`,
    r.rulerGain === 0, `${r.rulerGain} on the ruler`);
  // ONE ticked: the quick-calc pass above unticked the pair and put Roar back.
  // A ROAR BELONGS TO NO GUN. The builder is where you answer what the weapon
  // IS; this is something done to it for a while, so it lives with the fight.
  check(`[${lang}] the buff block is the SIMULATOR's alone`,
    r.blockByTab.simulator === true && r.blockByTab.builder === false
      && r.blockByTab.optimizer === false,
    JSON.stringify(r.blockByTab));
  check(`[${lang}] the optimizer shows the same buffs`,
    r.optCards === r.catalogue && r.optChecked === 1,
    `${r.optCards} cards, ${r.optChecked} ticked`);
  check(`[${lang}] …and cannot edit them`, r.optEditable === 0,
    `${r.optEditable} editable`);
  // THE CONTROL. A ruler that cast Roar would make its board a statement about
  // Rhino.
  check(`[${lang}] no ruler carries a Warframe buff`,
    r.rulers >= 1 && r.rulerAbilities === 0,
    `${r.rulerAbilities} across ${r.rulers} rulers`);
}

await app.finish("a Warframe buff is the fight's, and it reaches the number");
