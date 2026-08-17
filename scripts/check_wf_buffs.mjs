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
// the rule the owner asked for by name (2026-08-08) and it is the one a
// player cannot verify by eye — the difference between +50% and +80% is a
// number you have to be told; · the OPTIMIZER shows the same buffs,
// read-only, because it runs the
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
    // A SECTION OF THE FIGHT, and WRAPPED so it still reads as its own thing
    // (owner, 2026-08-09). It saves with the scenario, travels with it
    // across weapons and is what the optimizer reads off the fight, so
    // inside is where it belongs.
    out.insideFight = !!document.querySelector('#sim-block #wfbuff-block #sim-wfbuffs');
    out.wrapped = !!document.querySelector('#wfbuff-block.sim-panel');
    // …and LAST, right above the run: it is the final input before you press it.
    const panel = document.getElementById('wfbuff-block');
    const runh = [...document.querySelectorAll('#sim-block .sim-h')].pop();
    out.beforeRun = !!(panel && runh &&
      (panel.compareDocumentPosition(runh) & Node.DOCUMENT_POSITION_FOLLOWING));
    out.early = ((document.querySelector('#sim-wfbuffs .wfb-early') || {}).textContent || '').trim();

    // 2. THE VALUE FOLLOWS STRENGTH. Roar is +50% at 100% and +100% at 200%.
    const roarText = () => {
      const i = (META.abilities || []).findIndex(a => a.id === 'roar');
      return (cards()[i].querySelector('.wfb-v') || {}).textContent || '';
    };
    out.roarAt100 = roarText();
    // …AND THE ONE THE KNOB DOES NOT MOVE. Energized Munitions' 75% ammo
    // efficiency carries no Ability Strength icon on its wiki row, so a card
    // that scaled it would promise 225% at a 300% frame — free shooting, off a
    // buff the game gives flat.
    const emText = () => {
      const i = (META.abilities || []).findIndex(a => a.id === 'energized_munitions');
      return i < 0 ? '' : (cards()[i].querySelector('.wfb-v') || {}).textContent || '';
    };
    out.emAt100 = emText();
    const str = document.getElementById('sim-wfbuffs-str');
    str.value = '200'; str.dispatchEvent(new Event('change')); await sleep(300);
    out.roarAt200 = roarText();
    out.emAt200 = emText();
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

    // 4a. A CHOSEN ELEMENT, where the ability offers one. Resupply's gear
    //     wheel is ten choices, and the card has to let you make it — the value
    //     line names the element, so a picker that did not reach the payload
    //     would print one thing and run another.
    const idxOf = (id) => (META.abilities || []).findIndex(a => a.id === id);
    const sel = () => cards()[idxOf('resupply')].querySelector('[data-wfel]');
    out.selectable = (META.abilities || []).filter(a => (a.elements || []).length).map(a => a.id);
    out.noPickerWhenFixed = !cards()[idxOf('xatas_whisper')].querySelector('[data-wfel]');
    await tick('resupply', true);
    out.pickerOptions = sel() ? [...sel().options].length : 0;
    out.pickedDefault = (sim.abilities.find(a => a.id === 'resupply') || {}).element;
    sel().value = 'corrosive'; sel().dispatchEvent(new Event('change')); await sleep(400);
    out.pickedAfter = (sim.abilities.find(a => a.id === 'resupply') || {}).element;
    out.valueLine = (cards()[idxOf('resupply')].querySelector('.wfb-v') || {}).textContent || '';
    await tick('resupply', false);

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
    //
    // THROUGH THE REAL PATH, because there is no second pointer to fake it
    // with any more: the quick calc measures the fight you are IN, so this
    // SWITCHES to the ruler and asks what is on the fight then (2026-08-17).
    // Stronger than what it replaced — it now also proves that switching
    // scenarios drops the abilities rather than only that a reader pointed
    // elsewhere would not have seen them.
    const ruler = (scenarioList().find(x => x.builtin) || {});
    const cfg = scenarioBarCfg();
    const prevId = activeScenario;
    const prevState = snapshotScenario();
    cfg.setActive(presetId(ruler)); cfg.apply(ruler.state); await sleep(600);
    out.rulerGain = (theFight().abilities || []).length;
    cfg.setActive(prevId); cfg.apply(prevState); await sleep(600);

    // 4c. IT IS THE SIMULATOR'S BLOCK, AND ONLY THE SIMULATOR'S. A Warframe
    //     buff is not part of the weapon — the builder answers "what is this
    //     gun" and a Roar belongs to no gun (owner, 2026-08-09). Checked by
    //     GEOMETRY, not by the class list: hiding is a CSS id list, which is
    //     exactly the kind of thing a new block silently falls out of.
    // BY GEOMETRY, not by the element's own style. It is nested inside
    // #sim-block now, and a child of a hidden parent still reports
    // display: block — offsetParent is what actually answers "is this on
    // the screen", and it was the difference between a real check and one that
    // passed on every tab.
    const seen = (id) => {
      const e = document.getElementById(id);
      if (!e) return null;
      return !!e.offsetParent && !e.hidden;
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

    // 7. AN EXTRA HIT IS NOT A MULTIPLIER, and the page has to survive that.
    //    Xata's Whisper is the first buff that adds a damage INSTANCE rather
    //    than scaling one, so it has two surfaces the other seven do not: a
    //    row of its own in the damage meter, and a card that admits what it
    //    leaves out. Both are asserted here rather than in the engine, because
    //    the engine already proves the arithmetic and neither of these is
    //    arithmetic.
    history.pushState({}, '', '/weapons/Torid/simulator'); route(); await sleep(3000);
    await tick('roar', false);
    await tick('xatas_whisper', true);
    await sleep(400);
    const xi = (META.abilities || []).findIndex(a => a.id === 'xatas_whisper');
    const xcard = cards()[xi];
    out.xhValue = (xcard.querySelector('.wfb-v') || {}).textContent || '';
    // THE ADMISSIONS, in the chips every other family uses: ⊘ for the Bullet
    // Attractor it cannot value, ⚑ for the Blast interaction that is DE's
    // bug — and the TITLE is where the sentence lives, so that is what is read.
    out.xhChips = [...xcard.querySelectorAll('.wfb-u > span')]
      .map(s => (s.className || '') + '|' + (s.getAttribute('title') || ''));
    // …AND THE NEGATIVE CONTROL. A buff with nothing to admit shows no chips;
    // a check that only asserts presence passes on a page that shouts "not
    // modelled" at everything.
    const ri = (META.abilities || []).findIndex(a => a.id === 'roar');
    out.roarChips = cards()[ri].querySelectorAll('.wfb-u > span').length;

    // IT REACHES THE NUMBER, and it arrives as its OWN source. A second damage
    // instance folded into "direct" would credit the build for damage no mod on
    // it scaled — the same reason the field and the syndicate radial have rows.
    out.dpsWhisper = await dpsOf();
    const sim1 = await api('/api/simulate', body());
    const rows = (sim1 && sim1.damage_sources) || [];
    const xh = rows.find(x => x.source === 'extra hit');
    out.xhDamage = xh ? xh.dmg : 0;
    out.xhTypes = xh && xh.by_type ? xh.by_type.map(t => t.type) : [];
    out.xhLabel = tr('Extra hit (ability)');
    await tick('xatas_whisper', false);
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
  // INSIDE THE FIGHT, AND WRAPPED. Both halves matter: it belongs to the
  // scenario (it saves with it, travels with it, and the optimizer reads it off
  // it), and it is a layer nothing else in that panel is, so it keeps an edge
  // of its own.
  check(`[${lang}] the buffs are a wrapped section OF the fight`,
    r.insideFight === true && r.wrapped === true,
    `inside=${r.insideFight} wrapped=${r.wrapped}`);
  check(`[${lang}] …and sit last, right above the run`, r.beforeRun === true);
  // EARLY ACCESS IS ON THE PAGE, not only in a yaml comment: this block moves
  // onto the Warframe later and a player is entitled to know that now.
  check(`[${lang}] …and the block admits it is early access`,
    r.early.length > 10 && cjk.test(r.early) === (lang === "zh"),
    r.early.slice(0, 70));
  check(`[${lang}] the value follows Ability Strength`,
    r.roarAt100.includes("50%") && r.roarAt200.includes("100%"),
    `${r.roarAt100.slice(0, 40)} -> ${r.roarAt200.slice(0, 40)}`);
  // …AND THE ONE IT DOES NOT. The negative control for the same knob, and it
  // has to be a DIFFERENT ability rather than a second reading of Roar: the
  // claim is that the page knows which buffs the stat governs, not that it can
  // multiply.
  check(`[${lang}] …and a flat one does not — Energized Munitions stays 75%`,
    r.emAt100.includes("75%") && r.emAt200.includes("75%"),
    `${r.emAt100.slice(0, 40)} -> ${r.emAt200.slice(0, 40)}`);
  // THE ONLY CLAIM THAT MATTERS: it reaches the number, in the shipping build.
  check(`[${lang}] ticking a buff moves the SIM`,
    r.dpsPlain > 0 && r.dpsRoar > r.dpsPlain * 1.2,
    `${r.dpsPlain} -> ${r.dpsRoar}`);
  // WHOLE FIGHT, ALWAYS, for now (owner, 2026-08-08) — `secs: null` is
  // that, and the engine's per-buff end time is still there under it for
  // the day Ability Duration supplies one.
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
  // A CHOSEN ELEMENT. The data says which abilities offer one and the card has
  // to obey it in both directions — a picker where there is a choice, none
  // where the ability fixes its element.
  check(`[${lang}] an ability with a choice of element offers one`,
    (r.selectable || []).includes("resupply") && r.pickerOptions === 10,
    `${JSON.stringify(r.selectable)} · ${r.pickerOptions} options`);
  check(`[${lang}] …and one with a fixed element does not`, r.noPickerWhenFixed === true);
  check(`[${lang}] …the choice reaches the fight`,
    r.pickedDefault === "heat" && r.pickedAfter === "corrosive",
    `${r.pickedDefault} -> ${r.pickedAfter}`);
  // …AND THE CARD PRINTS WHAT IT WILL RUN. The value line names the element, so
  // a picker that moved the payload and not the label would lie quietly.
  check(`[${lang}] …and the card's value line names it`,
    /25/.test(r.valueLine) && cjk.test(r.valueLine) === (lang === "zh"),
    r.valueLine);
  // …IN THE DISPLAY LANGUAGE, and capitalised in English. The helper is handed
  // a yaml token here and a server-cased name elsewhere; echoing the token put
  // a lowercase "void" on the English card while the Chinese one read 虚空.
  check(`[${lang}] …properly, not as the raw data token`,
    r.values.every((v) => !/ [a-z]/.test(v)), JSON.stringify(r.values));
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

  // THE EXTRA HIT — the fourth effect kind, and the first that is an INSTANCE.
  // Its value label names the element it lands as, because "+26%" alone says
  // nothing about a payload that is entirely Void.
  check(`[${lang}] an extra hit names the element it lands as`,
    /^\+26/.test(r.xhValue) && r.xhValue.length > 4,
    r.xhValue);
  check(`[${lang}] …and it reaches the sim`,
    r.dpsWhisper > r.dpsPlain * 1.05, `${r.dpsPlain} -> ${r.dpsWhisper}`);
  // ITS OWN ROW IN THE METER. Folded into "direct" it would look like the build
  // got better; it is an ability, and it goes when the ability does.
  check(`[${lang}] …reported as its own damage source, as Void`,
    r.xhDamage > 0 && r.xhTypes.map((t) => t.toLowerCase()).includes("void"),
    `${Math.round(r.xhDamage)} as ${JSON.stringify(r.xhTypes)}`);
  check(`[${lang}] …under a label in the display language`,
    r.xhLabel.length > 4 && cjk.test(r.xhLabel) === (lang === "zh"), r.xhLabel);
  // WHAT IT DOES NOT DO IS ON THE CARD, in both families of admission: the
  // Bullet Attractor this sim has nothing to point at, and the Blast
  // interaction that is DE's own bug and can be hotfixed away.
  check(`[${lang}] …and the card admits its gaps and its live bug`,
    r.xhChips.some((c) => c.startsWith("unmodeled")) &&
      r.xhChips.some((c) => c.includes("livebug")),
    JSON.stringify(r.xhChips.map((c) => c.split("|")[0])));
  check(`[${lang}] …in the display language, sentences included`,
    r.xhChips.every((c) => cjk.test(c.split("|")[1] || "") === (lang === "zh")),
    (r.xhChips[0] || "").slice(0, 90));
  // THE CONTROL: a buff with nothing to admit admits nothing.
  check(`[${lang}] …while a buff with nothing to admit shows no chips`,
    r.roarChips === 0, `${r.roarChips} on Roar`);
}

await app.finish("a Warframe buff is the fight's, and it reaches the number");
