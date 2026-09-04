// THE VALENCE BONUS — an adversary weapon's bonus element, on screen and in the
// number.
//
// The TWENTY-SEVENTH check, and the first about a build axis that is a property
// of the COPY a player owns rather than of the model. A Kuva Lich hands out
// 25–60% of base damage as one of seven elements, so two Kuva Nukors are two
// different weapons and neither is "the" Kuva Nukor.
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
    const s = await api('/api/simulate', { ...buildPayload(), ...theFight(), runs: 2 });
    return { base: (s.panel || {}).modified_base, dmg: (s.panel || {}).damage };
  };
  // WHAT IT OPENS ON, before anything is touched: every copy of an adversary
  // weapon comes out of its Lich carrying a bonus, so a fresh page must not
  // show a weapon nobody has.
  const opened = JSON.parse(JSON.stringify(valence));
  const openedBase = (await panel()).base;

  // NO WAY TO EMPTY IT. Every copy comes out of a Lich with an element, so
  // the list offers no "none" — the weapon's printed panel is the wiki
  // infobox's figure and not a build anyone can play.
  //
  // Read off the dropdown's own registry, which is what the list WILL draw.
  // The DOM half is asserted further down, by opening it.
  const offered = (ddReg.get('dd-valence') || { items: [] }).items.map(i => i.value);
  // …and the roll is always live, since there is always an element under it.
  const rollDisabled = !!(document.getElementById('valence-bonus') || {}).disabled;

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

  return { spec, shown, opened, openedBase, offered, rollDisabled, toxin, rad,
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
// …AND THERE IS NO WAY BACK TO A WEAPON NOBODY HAS. Seven
// picks, all of them elements, and the roll never greys out.
check("...and the element is mandatory: seven picks, no `none`",
  r.offered.length === 7 && r.offered.every(Boolean) && !r.rollDisabled,
  JSON.stringify(r.offered) + ` roll disabled: ${r.rollDisabled}`);
// THE ARITHMETIC, exactly: 21 + 21 × 0.60 = 33.6, as BASE damage. 21 is the
// weapon's own printed Radiation, which the infobox states.
check("a 60% Toxin progenitor is +12.6 Toxin beside the Radiation",
  Math.abs(r.toxin.base - 33.6) < 1e-6, `21 -> ${r.toxin.base}`);
// …AND THE MERGE, which is the half a "new element" implementation gets wrong.
check("...and a Radiation one merges into the 21 it already deals",
  Math.abs(r.rad.base - 33.6) < 1e-6, String(r.rad.base));
check("it saves with the BUILD, not the fight",
  r.savedElement === "radiation" && r.savedBonus === 0.6 && r.inFight === false,
  JSON.stringify({ element: r.savedElement, bonus: r.savedBonus, inFight: r.inFight }));
check("an ordinary weapon has no such axis, and inherits no choice",
  r.otherShown === false && r.otherValence.element === "",
  JSON.stringify({ shown: r.otherShown, carried: r.otherValence }));

// …AND THE QUICK CALC RANKS IT, the same way it ranks a tier of evolutions. It is the axis a scan is worth the most on: a
// progenitor element is a whole element entering the hierarchy, so which one
// wins depends on the mods around it and on the target — not a question anyone
// answers by reading cards.
const gain = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Kuva_Nukor'); route(); await sleep(3500);
  // OPENING THE LIST IS WHAT STARTS THE SCAN (openRanked) — the
  // same moment the mod and arcane pickers use. It must not run
  // from the block's render, which is why this simply waited.
  document.querySelector('#element-cfg .slot.axis .dots').click();
  await sleep(300);
  document.querySelector('#slot-menu [data-a="swap"]').click();
  // …AND THE KEY, not just the axis: a scan from an earlier block can still be
  // sitting there, finished and stale, which satisfies 'valence and not
  // running' on the first iteration and reads zero chips.
  for (let i = 0; i < 60 && !(gainScan.axis && gainScan.axis.kind === 'valence'
        && !gainScan.running && gainScan.key === gainKey()); i++) {
    await sleep(500);
  }
  // …and the BUILDER's step numbers follow the blocks this weapon actually has.
  // THE BUILDER's blocks only. The config page also holds the Sim, Rivens,
  // Enemies and Optimizer tabs, which have their own numbering — a sweep over
  // the shared class is exactly the bug this renumbering had on its first pass
  // (it made the Rivens editor step 5 of building a gun).
  const steps = builderBlocks()
    .filter(b => !b.hidden)
    .map(b => b.id + ':' + ((b.querySelector('.bh .n') || {}).textContent || ''));
  // …and the chips REACH THE SCREEN, in the same component every other ranked
  // axis uses. A scan whose answers never reach a pick is a scan nobody reads.
  //
  // OPENED FOR REAL, and BY THE PATH A READER TAKES — done above, since it is
  // also what starts the measurement. The registry says what the list would
  // draw; the claim here is that somebody can get to it and read the numbers.
  ddRender('dd-valence', '');
  const chips = document.querySelectorAll('#dd-menu .opt .gainchip').length;
  const picks = document.querySelectorAll('#dd-menu .opt').length;
  // …AND THE AXIS HAS NO REMOVE, because it can never be empty — one menu item
  // is the whole of the difference from an evolution tier.
  const removable = !!document.querySelector('#slot-menu [data-a="remove"]');
  closePopovers();
  return { kind: gainScan.axis && gainScan.axis.kind,
           ranked: Object.keys(gainScan.by || {}).sort(),
           cur: valence.element,
           chips, picks, removable,
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
  gain.picks === 7 && gain.chips === 6,
  `${gain.picks} picks, ${gain.chips} chips`);
// AND NO WAY TO EMPTY IT, said by the menu rather than by a missing option:
// every quantifiable axis wears the same card, and whether it can be empty is
// one menu item — an evolution tier has Remove and this does not.
check("...and the card offers no Remove, because there is no empty state",
  gain.removable === false, String(gain.removable));
// THE STEP NUMBERS ARE DERIVED, not written into the markup. This
// weapon has no evolutions, so its Valence block is step 4 — there is no 5
// with nothing at 4.
//
// …AND A READ-OUT IS NOT A STEP. The Stats panel is a builder block and keeps
// its `Σ`: the numbering walks `builderSteps()`, whose rule is that a step's
// badge is a NUMBER. That rule was in the doc from the start and lived in a
// hand LIST that simply left the read-outs out, so the first version of the
// derived query renumbered `Σ` to "5" — caught here.
check("...and the builder numbers its steps from the blocks it actually has",
  gain.steps.join(" ") === "mode-block:1 mod-block:2 arcane-block:3 element-block:4 stats-block:Σ",
  gain.steps.join(" "));

// …AND THE RANKING DOES NOT FLIP WHEN YOU MOVE.
//
// Reported twice: "with Magnetic picked it says Heat is better; with Heat
// picked it says Magnetic is better." That reading has one mathematical
// statement — a gain measured A -> B and the gain measured B -> A must INVERT,
// so (1 + gAB)(1 + gBA) = 1 — and nothing weaker is worth asserting: two chips
// can both look plausible and still describe a ranking that has no order.
//
// Measured on the pair the report named, whose gap is far outside the noise at
// this run count, so the check tests the arithmetic and not the dice.
const flip = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Kuva_Nukor'); route(); await sleep(3500);
  const scan = async (el) => {
    valence.element = el; renderValence(); refreshPanel(); await sleep(1000);
    // OPEN THE LIST, which is what asks for the measurement.
    const card = document.querySelector('#element-cfg .slot.axis');
    if (card) openRanked('dd-valence', card);
    for (let i = 0; i < 90; i++) {
      await sleep(400);
      if (gainScan.axis && gainScan.axis.kind === 'valence'
          && !gainScan.running && gainScan.key === gainKey()) break;
    }
    const by = {};
    Object.entries(gainScan.by || {}).forEach(([k, v]) => { by[k] = v.pct; });
    return { base: gainScan.base, by };
  };
  const a = await scan('heat');
  const b = await scan('magnetic');
  return { heatBase: a.base, magBase: b.base,
           heatToMag: a.by.magnetic, magToHeat: b.by.heat };
})()`);

const product = (1 + flip.heatToMag) * (1 + flip.magToHeat);
check("the ranking does not flip when you move along the axis",
  Math.abs(product - 1) < 0.02,
  `heat->magnetic ${(flip.heatToMag * 100).toFixed(1)}%, magnetic->heat ${(flip.magToHeat * 100).toFixed(1)}% (product ${product.toFixed(4)}, 1.0 is consistent)`);
// …and the direction is the one the BASE numbers say, so the chips agree with
// the headline they are derived from.
check("...and it agrees with the two builds' own scores",
  flip.heatBase > flip.magBase && flip.heatToMag < 0 && flip.magToHeat > 0,
  `heat ${flip.heatBase.toFixed(4)} vs magnetic ${flip.magBase.toFixed(4)}`);

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
      ...theFight(),
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

// …AND THE WINNER IS STILL THAT WEAPON WHEN IT BECOMES A BUILD.
//
// The row above carried the element all along; "+ add" dropped it, so a search
// won on Magnetic became a build fired on Impact — `defaultValence` opens on
// the spec's FIRST element, and a producer that omits an axis is
// indistinguishable from one that means "the default". A player measured the
// consequence: 22.34 KPM on the ranking against 17.44 for the same eight cards
// re-run in the simulator, which reads as the optimizer lying about its own
// number.
//
// So this asserts the PROPERTY rather than the field, twice over. Structurally:
// the state a row becomes NAMES every axis in `BUILD_AXES`, which is the one
// declaration of what a build is — the same cross-file shape
// `check_board_submit` uses, and the reason a fifth axis cannot be added to
// four of the five producers. And on the NUMBER: the build re-runs at the
// element it was scored on and not at the one it would have defaulted to, at a
// FIXED SEED so the two are exactly equal or exactly not.
const added = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Kuva_Nukor/optimizer'); route(); await sleep(4000);

  // MAGNETIC, which is not what a fresh page opens on — that is the whole
  // case. A row scored on the default element would pass this check without
  // the axis travelling at all.
  const opened = defaultValence('kuva_nukor', null).element;
  const body = {
    weapon: 'kuva_nukor',
    mods: { hornet_strike: 'search', barrel_diffusion: 'search',
            lethal_torrent: 'search', pathogen_rounds: 'search' },
    build_size: 2, build_min: 2,
    arcanes: {}, evolutions: {}, modes: {}, exilus: {},
    valence: { magnetic: 'fixed' },
    valence_element: opened, valence_bonus: 0.6,
    ...theFight(),
    duration: 8, runs: 2, final_runs: 2, finalists: 3, threads: 1, buffs: {},
  };
  const r0 = await postJson('/api/optimize', body);
  let s = r0;
  for (let i = 0; i < 400 && (!s || !s.done); i++) {
    await sleep(300);
    s = await postJson('/api/optimize/status', {});
  }
  const res = (s && s.result) || s;
  const row = ((res || {}).results || [])[0];
  if (!row) return { err: 'no ranking' };

  // Through the real button's own path, so what is asserted is what a click
  // produces and not a second copy of it.
  addResult(row);
  const ps = loadPresetList(BUILDS);
  const st = (ps[ps.length - 1] || {}).state || {};
  const missing = BUILD_AXES.filter((k) => !(k in st));

  // …ON SCREEN, which is a different claim from "in the object": open the
  // preset the way a player does and read the live axis.
  history.pushState({}, '', '/weapons/Kuva_Nukor'); route(); await sleep(2500);
  pickPreset(buildBarCfg(), presetId(ps[ps.length - 1])); await sleep(1200);
  const live = { element: valence.element, bonus: valence.bonus };

  // …AND IN THE NUMBER. Same fight, same fixed seed, three requests: the build
  // as it now stands, the build with the row's element forced, and the build on
  // the element it would have fallen back to. The first two are the same run to
  // the bit; the third is a different weapon.
  const fight = { ...theFight(), duration: 20, runs: 6, seed: 11 };
  const at = async (el) => {
    const q = { ...buildPayload(), ...fight };
    if (el) { q.valence_element = el; }
    const out = await api('/api/simulate', q);
    return out && out.ok ? Math.round((out.dps || 0) * 1000) / 1000 : null;
  };
  return { err: null, opened, rowElement: row.valence, rowMods: row.mods,
           missing, stateValence: st.valence, live,
           asAdded: await at(null), asScored: await at(row.valence),
           asDefaulted: await at(opened) };
})()`);

check("an optimizer winner becomes a build that states every axis",
  added.err === null && added.missing.length === 0,
  added.err || `missing: ${JSON.stringify(added.missing)}`);
// The case itself: the row was won on an element the page was not holding.
check("...and the row it came from was scored on a NON-default element",
  added.rowElement === "magnetic" && added.opened !== "magnetic",
  `row ${added.rowElement}, page opened on ${added.opened}`);
check("...so the build carries the element the row was scored with",
  added.stateValence && added.stateValence.element === added.rowElement
    && added.live.element === added.rowElement && added.live.bonus === 0.6,
  JSON.stringify({ state: added.stateValence, live: added.live }));
// THE FALSIFIABLE HALF, at a fixed seed: equal to the run it claims, and not
// equal to the run it would otherwise become. 22.34 against 17.44 is the gap.
check("...and it re-runs as the weapon it was scored on, not the default one",
  added.asAdded !== null && added.asAdded === added.asScored
    && added.asAdded !== added.asDefaulted,
  `added ${added.asAdded} · as scored (${added.rowElement}) ${added.asScored} · as defaulted (${added.opened}) ${added.asDefaulted}`);

// …AND THE THREE THINGS THAT FOLLOW FROM "IT IS PART OF THE BUILD".
//
// A build axis is not one because a variable holds it. It is one because every
// surface that claims to state the build states it, and because a link that
// claims to reproduce the build reproduces it. Both were missing while the
// number above was already right: the simulator's read-only card listed mode,
// mods, arcane and evolutions and not this, and the share tuple read neither
// this nor the mode — so a shared Kuva Nukor reopened on the default element.
//
// The CAPACITY is here rather than in a check of its own because it is the
// same weapon family and the same fact about it: an adversary weapon ranks to
// 40, two rank per Forma, five polarizations, so 80 capacity — and the client
// carried a literal 60, which showed a legal build in red.
const build = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Kuva_Nukor'); route(); await sleep(3500);

  const w = weaponInfo('kuva_nukor');
  const cap = document.getElementById('capacity').textContent;

  // A FULL EIGHT, which is the case most easily read as red: it fits 80 and
  // never fitted 60.
  const eight = ['hornet_strike', 'barrel_diffusion', 'lethal_torrent',
                 'primed_pistol_gambit', 'primed_target_cracker',
                 'pathogen_rounds', 'primed_heated_charge', 'galvanized_shot'];
  eight.forEach((m, i) => { slots[i].mod = m; slots[i].rank = modById(m).max_rank; });
  autoForma(); renderMods(); await sleep(400);
  const full = { text: document.getElementById('capacity').textContent,
                 over: document.getElementById('capacity').classList.contains('over'),
                 forma: document.getElementById('forma').textContent,
                 count: formaCount() };

  // The SIMULATOR's card, which is the page's claim about what is being run.
  valence.element = 'heat'; valence.bonus = 0.45;
  history.pushState({}, '', '/weapons/Kuva_Nukor/simulator'); route(); await sleep(1500);
  const card = document.getElementById('sim-build-info').textContent;

  // A SHARE LINK, opened by reading it back the way a recipient's browser
  // does. Also carries the MODE, which was dropped by the same tuple.
  const url = await shareUrl();
  const code = new URL(url).searchParams.get(SHARE_PARAM);
  const back = await decodeShare(code);

  // …and an ORDINARY weapon is still 60, so the number is the weapon's and not
  // a new constant.
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(2500);
  const plain = { cap: document.getElementById('capacity').textContent,
                  formaMin: formaMin('torid'), capacity: capOf('torid') };

  return { metaCap: w.capacity, metaForma: w.forma_min, metaRank: w.max_rank,
           cap, full, card, back, plain };
})()`);

// THE LADDER, as the server states it: rank 40, five Forma, 80.
check("an adversary weapon reports its own rank ceiling and capacity",
  build.metaRank === 40 && build.metaForma === 5 && build.metaCap === 80,
  JSON.stringify({ max_rank: build.metaRank, forma_min: build.metaForma, capacity: build.metaCap }));
check("...and the builder counts against 80, not 60",
  / \/ 80$/.test(build.cap.trim()), build.cap);
// The bug this replaces: eight maxed mods on this weapon rendered red.
check("...so a full eight-mod build is not shown as impossible",
  build.full.over === false && / \/ 80$/.test(build.full.text.trim()),
  `${build.full.text} (over: ${build.full.over})`);
// FIVE, and the auto plan is what spends them: the 80 above assumes rank 40,
// and rank 40 is those five polarizations. A plan that fits in two and stops
// has spent the capacity of a weapon it did not build.
check("...and Auto spends the five polarizations the rank 40 costs",
  build.full.count.regular >= 5 && /5 Forma/.test(build.full.forma),
  `${build.full.forma} (${JSON.stringify(build.full.count)})`);
check("an ordinary weapon is untouched: 60, no mastery Forma",
  build.plain.capacity === 60 && build.plain.formaMin === 0 && / \/ 60$/.test(build.plain.cap.trim()),
  JSON.stringify(build.plain));

// THE CARD. Element AND percentage — 45% Heat and 60% Heat are not the same
// weapon, so naming the element alone would still be half the fact.
check("the simulator's build card names the valence it is testing",
  /Heat|火焰/.test(build.card) && /45%/.test(build.card),
  build.card.replace(/\s+/g, " ").slice(0, 160));

// THE LINK. Read back through the app's own decoder, because that is what a
// recipient runs.
check("a share link carries the valence",
  build.back && build.back.valence && build.back.valence.element === "heat"
    && Math.abs(build.back.valence.bonus - 0.45) < 1e-9,
  JSON.stringify(build.back && build.back.valence));
// …and the other axis the same tuple was dropping. The Kuva Nukor has one
// mode, so `mode` is derivable here and correctly absent — which is the
// assertion: omitted means "the weapon's default", never "lost".
check("...and a mode the recipient cannot derive would travel with it",
  build.back && build.back.mode === undefined,
  String(build.back && build.back.mode));

// …AND THE BOARD'S DOOR IS ASKED BEFORE IT IS KNOCKED ON.
//
// A submission is written to a store that accepts anything and judged an HOUR
// later by the scoring job, which prints its reason into a workflow log. So a
// build the board will never take looked exactly like one it took — forever.
// Three Kuva Nukor submissions sat there carrying no progenitor element,
// refused on every run since they arrived, while the page said "sent" — a
// submission that never appears on any board.
//
// The endpoint is `validate_for_board` ITSELF, so this asserts the app gets the
// board's answer rather than a second opinion that agrees today.
const door = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Kuva_Nukor'); route(); await sleep(3500);
  const eight = ['hornet_strike', 'barrel_diffusion', 'lethal_torrent',
                 'primed_pistol_gambit', 'primed_target_cracker',
                 'pathogen_rounds', 'primed_heated_charge', 'galvanized_shot'];
  // Its own slot's pool, through the app's own helper — an arcane list filtered
  // by hand is a second copy of a rule this page already owns.
  const arc = (arcanePool(0) || [])[0];
  const body = (valence) => ({ benchmark: 'single_target', weapon: 'kuva_nukor',
    mode: 'base', mods: eight, evolutions: [], arcanes: [arc ? arc.id : 'none'], valence });
  const withEl = await api('/api/board/check', body('impact'));
  const without = await api('/api/board/check', body(''));
  const short = await api('/api/board/check',
    { ...body('impact'), mods: eight.slice(0, 6) });
  // …and the panel, driven through the real submit path with the store made
  // unreachable: a refusal must never reach the fetch at all.
  const sent = [];
  const realFetch = window.fetch;
  window.fetch = (u, o) => { sent.push(String(u)); return realFetch(u, o); };
  setBoardConsent('yes');
  const sc = builtinScenarios().find(s => s.builtin === 'single_target');
  pickPreset(scenarioBarCfg(), presetId(sc)); await sleep(1200);
  eight.forEach((m, i) => { slots[i].mod = m; slots[i].rank = modById(m).max_rank; });
  arcanes = [arc ? arc.id : 'none'];
  autoForma(); renderMods(); renderArcanes(); await sleep(600);
  // A build the board WILL take, with the store answering nothing useful.
  sent.length = 0;
  await offerBoardSubmit();
  const good = { hitStore: sent.some(u => u.indexOf('board/submit') >= 0),
                 panel: (document.getElementById('board-consent') || {}).textContent || '' };
  // …and one it will not: the element is gone, which is the case that stranded.
  valence.element = '';
  sent.length = 0;
  await offerBoardSubmit();
  const bad = { hitStore: sent.some(u => u.indexOf('board/submit') >= 0),
                panel: (document.getElementById('board-consent') || {}).textContent || '' };
  window.fetch = realFetch;
  return { withEl, without, short, good, bad };
})()`);

check("the app can ask whether the board would take a build",
  door.withEl && door.withEl.ok === true && door.withEl.accepted === true,
  JSON.stringify(door.withEl));
check("...and it is refused for the reason the SCORER gives",
  door.without && door.without.accepted === false && /Valence/.test(door.without.reason || ""),
  JSON.stringify(door.without));
// A second rule, so the endpoint is the whole door rather than one clause.
check("...for every rule of the ruler's, not only that one",
  door.short && door.short.accepted === false && /main slots|8/.test(door.short.reason || ""),
  JSON.stringify(door.short));
check("a build the board would take still reaches the store",
  door.good.hitStore === true, JSON.stringify(door.good.panel.slice(0, 80)));
// THE ONE THAT COST THREE SUBMISSIONS: refused, and it never leaves.
check("...and one it would refuse never leaves the page",
  door.bad.hitStore === false, `store called: ${door.bad.hitStore}`);
// …AND NOBODY IS REFUSED FOR NOT HAVING POLARIZED YET.
//
// A submission carries NO polarities — the server plans the cheapest layout
// itself — so the door is asked about a build measured FULLY FORMA'D. Judging
// the live layout instead would refuse exactly the people who had not got round
// to it, which is the shape of every bug on this path. The verdict is the
// ENGINE'S: `boardVerdict` is what the panel reads, so this asks the same
// question the reader's screen does.
const forma = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Kuva_Nukor'); route(); await sleep(3500);
  // THE ELEMENT BACK ON, because the block before this one took it off to be
  // refused and routing to the address already on screen re-enters nothing.
  // This block is about CAPACITY, and a build refused for its valence would
  // pass a capacity assertion by accident.
  valence = defaultValence('kuva_nukor', null); renderValence();
  const sc = builtinScenarios().find(s => s.builtin === 'single_target');
  pickPreset(scenarioBarCfg(), presetId(sc)); await sleep(1000);
  const eight = ['hornet_strike', 'barrel_diffusion', 'lethal_torrent',
                 'primed_pistol_gambit', 'primed_target_cracker',
                 'pathogen_rounds', 'primed_heated_charge', 'galvanized_shot'];
  eight.forEach((m, i) => { slots[i].mod = m; slots[i].rank = modById(m).max_rank; });
  arcanes = [(arcanePool(0) || [{ id: 'none' }])[0].id];
  // NOT A POLARITY IN SIGHT, which is where a player starts.
  slots.forEach(s => { s.pol = null; });
  renderMods(); renderArcanes(); await sleep(400);
  const bare = { raw: capacityUsed(), door: await boardVerdict(boardPayload()) };
  autoForma(); renderMods(); await sleep(400);
  const planned = { raw: capacityUsed(), door: await boardVerdict(boardPayload()) };

  // …and the positive control: eight mods that cannot fit however much Forma
  // you own. The Burston Prime's priciest eight are 64 against a 60 cap, which
  // is one of the rows sitting refused on the live board.
  history.pushState({}, '', '/weapons/Burston_Prime'); route(); await sleep(2500);
  pickPreset(scenarioBarCfg(), presetId(sc)); await sleep(1000);
  const fams = [];
  const heavy = currentPool.filter(m => !m.exilus).sort((a, b) => b.drain - a.drain)
    .filter(m => { if (!m.family) return true;
                   if (fams.indexOf(m.family) >= 0) return false;
                   fams.push(m.family); return true; })
    .slice(0, 8);
  heavy.forEach((m, i) => { slots[i].mod = m.id; slots[i].rank = m.max_rank; });
  arcanes = [(arcanePool(0) || [{ id: 'none' }])[0].id];
  autoForma(); renderMods(); renderArcanes(); await sleep(400);
  return { bare, planned, over: await boardVerdict(boardPayload()),
           cap: capOf('burston_prime') };
})()`);

// The raw layout is far over 80 and the door does not care: it is asked about
// the build, and a player who has not polarized is not turned away for it.
check("an unpolarized build is not refused for capacity",
  forma.bare.raw > 80 && forma.bare.door.accepted === true,
  `raw ${forma.bare.raw}, door ${JSON.stringify(forma.bare.door)}`);
check("...and the auto plan does not change that answer",
  forma.planned.door.accepted === true, JSON.stringify(forma.planned.door));
// …and a build that genuinely cannot exist IS named, in the engine's own terms.
check("a build over capacity even fully forma'd is named before the run",
  forma.over.accepted === false && /capacity|容量/.test(forma.over.reason || ""),
  `cap ${forma.cap} · ${JSON.stringify(forma.over)}`);

// The consent box carries ONE sentence for every refusal the door gives, so
// the claim here is that the box holds the board's own reason AND says the
// build is going nowhere — not which of the page's two phrasings it reached
// for.
check("...with the reason ON SCREEN rather than in a workflow log",
  /Valence/.test(door.bad.panel)
    && /不会提交|榜单不会收|is not sent|would not take/.test(door.bad.panel),
  door.bad.panel.replace(/\s+/g, " ").slice(0, 160));

await app.finish("an adversary weapon's Valence bonus reaches its damage");
