// THE VALENCE BONUS — an adversary weapon's bonus element, on screen and in the
// number.
//
// The TWENTY-SEVENTH check, and the first about a build axis that is a property
// of the COPY a player owns rather than of the model. A Kuva Lich hands out
// 25–60% of base damage as one of seven elements, so two Kuva Nukors are two
// different weapons and neither is "the" Kuva Nukor (owner,
// 2026-08-13).
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

  // NO WAY TO EMPTY IT. Every copy comes out of a Lich with an element, so
  // the pick row offers no "none" — the weapon's printed panel is the wiki
  // infobox's figure and not a build anyone can play.
  const offered = [...document.querySelectorAll('#element-cfg .evopick')]
    .map(c => c.dataset.vel);
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
// …AND THERE IS NO WAY BACK TO A WEAPON NOBODY HAS (owner, 2026-08-14). Seven
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

// …AND THE QUICK CALC RANKS IT, the same way it ranks a tier of evolutions
// (owner, 2026-08-13). It is the axis a scan is worth the most on: a
// progenitor element is a whole element entering the hierarchy, so which one
// wins depends on the mods around it and on the target — not a question anyone
// answers by reading cards.
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
  gain.picks === 7 && gain.chips === 6,
  `${gain.picks} picks, ${gain.chips} chips`);
// THE STEP NUMBERS ARE DERIVED, not written into the markup (owner). This
// weapon has no evolutions, so its Valence block is step 4 — there is no 5
// with nothing at 4.
check("...and the builder numbers its steps from the blocks it actually has",
  gain.steps.join(" ") === "mode-block:1 mod-block:2 arcane-block:3 element-block:4",
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

  // A FULL EIGHT, which is the case that used to read red: it fits 80 and
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
// refused on every run since they arrived, while the page said "sent" (owner,
// 2026-08-14: he had tested several on his phone and none appeared).
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
check("...with the reason ON SCREEN rather than in a workflow log",
  /Valence/.test(door.bad.panel) && /榜单不会收|would not take/.test(door.bad.panel),
  door.bad.panel.replace(/\s+/g, " ").slice(0, 160));

await app.finish("an adversary weapon's Valence bonus reaches its damage");
