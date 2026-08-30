// A MODULAR WEAPON IS ASSEMBLED, AND THE ASSEMBLY IS THE NUMBER.
//
// A Kitgun has no published stat line: every figure on the panel is composed
// from its Chamber, Grip and Loader. So the failure this exists for is a page
// that draws a perfectly good parts picker and simulates something else, with
// no printed stat line anywhere to contradict it — which is why every assertion
// is ON THE WIRE or on a real `/api/simulate` in the shipping wasm build.
//
// The sharp cases:
//
//   * two grips are two weapons, in the ANSWER and not only on the card;
//   * a build REMEMBERS its parts, through a preset and through a share link —
//     parts that reset to the default turn a saved build into a different
//     weapon with the same name;
//   * the block is HIDDEN on the other 134 weapons, the negative control.
//
// It also pins the two structural decisions, both invisible from outside and
// both reading as bugs if they regressed: the CHAMBER is stated rather than
// offered (it is the weapon), and only THIS slot's five grips are offered.
//
//   node scripts/check_assembly.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, finish } = app;

const KIT = "Tombfinger";
// The negative control. An ordinary secondary: same page, same controls, no
// parts — so every "is drawn" assertion below has something to fail against.
const PLAIN = "Lex";

const open = (weapon) => `
  history.pushState({}, '', '/weapons/${weapon}'); route();
  await new Promise(r => setTimeout(r, 4000));`;

// ---- the block draws, and only where it should ------------------------------

const drawn = await evaluate(`(async () => {
  ${open(KIT)}
  const block = document.getElementById('assembly-block');
  const row = document.getElementById('assembly-row');
  const id = $('weapon').value;
  // OPENING A LIST IS WHAT STARTS THE MEASUREMENT (openRanked) —
  // the same moment the mod and arcane pickers have always used. The parts are
  // ONE axis, so opening either list measures both.
  row.querySelector('.slot.axis .dots').click();
  await new Promise(r => setTimeout(r, 300));
  document.querySelector('#slot-menu [data-a="swap"]').click();
  for (let i = 0; i < 60; i++) {
    await new Promise(r => setTimeout(r, 500));
    if (gainScan.axis && gainScan.axis.kind === 'assembly'
        && !gainScan.running && gainScan.key === gainKey()) break;
  }
  closePopovers();
  return {
    shown: !!block && !block.hidden,
    id,
    slot: weaponInfo(id).slot,
    sibling: slotSibling(id),
    controls: [...row.querySelectorAll('[id^="dd-"]')].map(d => d.id),
    // THE PARTS ARE DROPDOWNS THAT CARRY THEIR GAINS — the mod picker's shape,
    // which is what a Kitgun's twenty loaders need and what four evolution
    // chips did not. ddReg is the component own registry, so this reads what
    // the list WILL draw rather than re-deriving it.
    parts: ['grip', 'loader'].filter(p => ddReg.has('dd-' + p)),
    picks: ['grip', 'loader'].flatMap(p =>
      (ddReg.get('dd-' + p) || { items: [] }).items.map(i => p + ':' + i.value)),
    selected: ['grip', 'loader']
      .filter(p => ddReg.has('dd-' + p))
      .map(p => p + ':' + ddReg.get('dd-' + p).value),
    // A gain chip, or the "…" that says one is still being measured, on every
    // row of the list.
    //
    // THE INSTALLED PART IS THE BASELINE and carries none, which is not a gap:
    // there is no gain in swapping a part for itself. So the claim is about the
    // options that are CANDIDATES, and a check counting every chip would fail
    // on a working page by exactly the number of parts.
    //
    // READ OFF THE RENDERED LIST, not the registry: a gain chip is computed
    // when the list DRAWS, because the scan that fills it starts
    // when the list is OPENED — baking it in at registration is what kept a
    // freshly opened list showing none at all. ddRender is the function the
    // popover itself calls.
    unranked: ['grip', 'loader'].flatMap(p => {
      const cfg = ddReg.get('dd-' + p) || { items: [], value: null };
      ddRender('dd-' + p);
      return [...document.querySelectorAll('#dd-menu .opt')]
        .filter(el => el.dataset.v !== String(cfg.value) && !el.querySelector('.gainchip'))
        .map(el => p + ':' + el.dataset.v);
    }),
    // …AND THE ORDER IS THE RANKING. Best first, by the picker's own rule.
    order: ['grip', 'loader'].flatMap(p =>
      (ddReg.get('dd-' + p) || { items: [] }).items.map(i => p + ':' + i.value)),
    fixed: [...row.querySelectorAll('.fixed-val')].map(e => e.textContent.trim()),
    // The grips on offer, as the engine states them for THIS entry...
    grips: (assemblySpec(id).grips || []).map(g => g.id),
    // ...and every grip the game has, so "only this slot's" is a real claim.
    allGrips: [...new Set(META.weapons.filter(w => w.assembly)
      .flatMap(w => w.assembly.grips.map(g => g.id)))].sort(),
    text: row.textContent,
  };
})()`);

check("a modular weapon draws a parts block", drawn.shown, JSON.stringify(drawn));
check("...with a list for the grip and one for the loader",
  drawn.parts.includes("grip") && drawn.parts.includes("loader"),
  JSON.stringify(drawn.parts));
// ONE SELECTED PER PART, because a pick row that highlights nothing (or two
// things) is a control that cannot say what the weapon currently is.
check("...each showing exactly which part is installed",
  drawn.selected.length === 2
    && drawn.selected.some(x => x.startsWith("grip:"))
    && drawn.selected.some(x => x.startsWith("loader:")),
  JSON.stringify(drawn.selected));
// A PART IS RANKED LIKE EVERY OTHER AXIS. A grip is worth
// a different amount on every build and no card states it, which is the same
// argument the valence row and the evolution tiers are built on — so the parts
// carry quick-calc chips rather than a dropdown that shuts over its own hints.
check("...and every option that is a candidate is ranked, not just described",
  drawn.unranked.length === 0,
  `${drawn.unranked.length} of ${drawn.picks.length} unranked: ${drawn.unranked}`);
// TWENTY LOADERS IS WHY THIS IS A LIST AND NOT A ROW OF CHIPS. Asserted so the shape cannot quietly go back to one that only
// works for an axis with four options.
check("...and the loader list is the long one, so it is searchable",
  drawn.picks.filter(x => x.startsWith("loader:")).length >= 20,
  JSON.stringify(drawn.picks.filter(x => x.startsWith("loader:")).length));
// THE CHAMBER IS NOT DRAWN AT ALL: the chamber IS the weapon, whose name is
// already at the top of the page, so a read-only row would say the same word
// twice.
check("...and the chamber is not drawn at all",
  !drawn.fixed.includes("Tombfinger") && !drawn.controls.includes("dd-chamber"),
  JSON.stringify({ fixed: drawn.fixed, controls: drawn.controls }));
// ONLY THIS SLOT'S GRIPS. A grip decides primary or secondary, so the other
// five compose into the sibling entry and into nothing here — and the ENGINE
// says which, because a page that re-derived the rule would go stale the first
// time a chamber arrives that exists in one slot only.
check("...and only this slot's grips are offered",
  drawn.grips.length === 5 && drawn.allGrips.length === 10
    && drawn.grips.every((g) => drawn.allGrips.includes(g)),
  `${drawn.grips.length} of ${drawn.allGrips.length}: ${drawn.grips}`);
// …AND THE ROW OFFERS EXACTLY THOSE, so the scan and the picker cannot rank
// different sets.
check("...and the row offers exactly those five",
  drawn.grips.every((g) => drawn.picks.includes("grip:" + g))
    && drawn.picks.filter(x => x.startsWith("grip:")).length === drawn.grips.length,
  JSON.stringify(drawn.picks));
// EACH OPTION CARRIES ITS OWN NUMBERS. Twenty names say nothing about the
// decision, which is the Mode control's own lesson.
check("...and each part says what it is worth",
  /\d/.test(drawn.text), drawn.text.slice(0, 120));

// ---- ONE PAGE, TWO SLOTS ----------------------------------------------------

// A Kitgun is ONE weapon and TWO roster entries, so `/weapons/Tombfinger`
// resolves to one of them and the Slot control has to reach the other WITHOUT
// changing the address — otherwise the two are two pages that happen to share a
// name, and the second is unreachable. Switching is switching WEAPONS in every
// other sense, which is what keeps each slot's build its own.
check("...and it offers the other slot", !!drawn.sibling, JSON.stringify(drawn));

const switched = await evaluate(`(async () => {
  ${open(KIT)}
  const before = { id: $('weapon').value, slot: weaponInfo($('weapon').value).slot,
                   grips: assemblySpec($('weapon').value).grips.map(g => g.id) };
  const path = location.pathname;
  switchWeapon(slotSibling(before.id));
  renderAssembly();
  await new Promise(r => setTimeout(r, 600));
  const id = $('weapon').value;
  return {
    before, after: { id, slot: weaponInfo(id).slot,
                     grips: assemblySpec(id).grips.map(g => g.id) },
    path, pathAfter: location.pathname,
    // The parts reset to the new entry's own default: the sibling's five grips
    // are not legal here, so carrying one over would be a weapon nobody has.
    assembly: { ...assembly },
    sent: buildPayload().weapon,
  };
})()`);
check("picking the other slot moves to the other entry",
  switched.after.id !== switched.before.id
    && switched.after.slot !== switched.before.slot
    && switched.sent === switched.after.id,
  JSON.stringify(switched));
check("...without changing the address, because it is one weapon",
  switched.pathAfter === switched.path,
  `${switched.path} -> ${switched.pathAfter}`);
check("...and the grips on offer are the new slot's",
  switched.after.grips.every((g) => !switched.before.grips.includes(g))
    && switched.after.grips.includes(switched.assembly.grip),
  JSON.stringify(switched));

const plain = await evaluate(`(async () => {
  ${open(PLAIN)}
  const block = document.getElementById('assembly-block');
  return { hidden: !block || block.hidden, spec: !!(weaponInfo('lex').assembly) };
})()`);
check("a weapon with no parts draws no parts block", plain.hidden, JSON.stringify(plain));
check("...and the engine says it has none", !plain.spec, JSON.stringify(plain));

// ---- the KITGUN ARCANES, and who may seat them -----------------------------

// Eight arcanes go on a modular weapon and nothing else. No CLASS can say so —
// a secondary Tombfinger is a `pistol` exactly like the Lex above — so the gate
// is a weapon TRAIT, and the ENGINE answers it per weapon (`/api/meta.arcanes`)
// rather than the page re-deriving a rule. It offered all eight on a Lex until
// that landed.
//
// They also fit BOTH SEATS, which no other arcane does: a Kitgun is one weapon
// with a roster entry per slot, so a primary Tombfinger has a PRIMARY seat and
// a secondary one a SECONDARY seat, and the same arcane goes in either. Ids are
// globally unique across slots, so it is one record declaring two seats.
const arcanes = await evaluate(`(async () => {
  const seen = {};
  for (const w of ['${PLAIN}', '${KIT}']) {
    history.pushState({}, '', '/weapons/' + w); route();
    await new Promise(r => setTimeout(r, 4000));
    const id = $('weapon').value;
    const seats = weaponInfo(id).arcane_pools || [];
    const isKit = (a) => /^(pax_|residual_)/.test(a.id);
    seen[w] = {
      id,
      seats,
      total: arcanePool(0).length,
      kit: arcanePool(0).filter(isKit).map(a => a.id),
      // The OTHER seat, where a Kitgun arcane must not appear at all.
      ordinary: seats.length > 1 ? arcanePool(1).filter(isKit).map(a => a.id) : [],
      ordinaryTotal: seats.length > 1 ? arcanePool(1).length : 0,
    };
  }
  return seen;
})()`);
check("the eight Kitgun arcanes are offered on a Kitgun",
  arcanes[KIT].kit.length === 8, JSON.stringify(arcanes[KIT]));
// THE NEGATIVE CONTROL, and the bug this was written for: a page filtering on
// class alone offers every one of them on an ordinary pistol.
check("...and on nothing else",
  arcanes[PLAIN].kit.length === 0, JSON.stringify(arcanes[PLAIN]));
// A SEAT OF THEIR OWN, AND IT IS NOT THE WEAPON'S. The
// wiki puts it as an "as well" rather than an "instead" — "These can be
// installed simultaneously with Secondary/Primary arcanes" (`Kitgun` §Kitgun
// Arcanes) — so filing them under the weapon's slot made the page ask the
// reader to choose between a Pax Charge and a Primary Merciless, which the
// game never does.
check("...in a seat of their own, which comes first",
  arcanes[KIT].seats.length === 2 && arcanes[KIT].seats[0] === "kitgun",
  JSON.stringify(arcanes[KIT].seats));
check("...and nothing else is in it",
  arcanes[KIT].total === 8 && arcanes[KIT].kit.length === 8,
  JSON.stringify({ total: arcanes[KIT].total, kit: arcanes[KIT].kit.length }));
// BOTH DIRECTIONS. A check asserting only that the Kitgun seat holds them
// passes just as well on a page that ALSO leaves them in the ordinary one,
// which is the state this replaced.
check("...and the ordinary seat is still the ordinary seat",
  arcanes[KIT].ordinary.length === 0 && arcanes[KIT].ordinaryTotal > 8,
  JSON.stringify({ leaked: arcanes[KIT].ordinary, total: arcanes[KIT].ordinaryTotal }));
// THE NEGATIVE CONTROL on the seat itself: an ordinary pistol gains no seat.
check("...and an ordinary weapon has one seat, not two",
  arcanes[PLAIN].seats.length === 1 && arcanes[PLAIN].seats[0] !== "kitgun",
  JSON.stringify(arcanes[PLAIN].seats));

// ---- the assembly reaches the wire, and moves the answer --------------------

// TWO GRIPS ARE TWO WEAPONS. The lightest and the heaviest differ three to five
// times in base damage on every chamber, so a picker that is wired at all
// cannot answer the same twice. It runs a REAL simulate rather than reading the
// card, because a card is drawn from the state the picker just wrote and would
// agree with itself either way.
//
// NO DIRECTION IS ASSERTED, and that is not caution — it is the mechanic. On a
// CHARGE chamber the grip sets the charge time as well as the damage, and the
// two pull opposite ways: a primary Tombfinger on Brash is 38 damage every
// 0.5 s and on Tremor 116 every 1.4 s, so the LIGHTEST grip wins on DPS by a
// third. The wiki says as much ("grips with higher damage output will increase
// the charge time"), and a check demanding "heavier is better" would have been
// asserting the opposite of the weapon.
const fired = await evaluate(`(async () => {
  ${open(KIT)}
  const id = $('weapon').value;
  const spec = assemblySpec(id);
  // The LIGHTEST and the HEAVIEST grip this entry takes, DERIVED rather than
  // named — so the assertion holds whichever slot the route landed on and
  // whichever chamber is being tested.
  const by = [...spec.grips].sort(
    (a, b) => spec.grip_stats[a.id].damage - spec.grip_stats[b.id].damage);
  const light = by[0].id, heavy = by[by.length - 1].id;
  const out = {};
  for (const grip of [light, heavy]) {
    assembly = { ...assembly, grip };
    const p = buildPayload();
    out[grip] = {
      // Read DEFENSIVELY: a payload that stopped carrying the axis at all is
      // the failure this is looking for, and it should be reported by name
      // rather than crash the check on a property read of undefined.
      sent: (p.assembly || {}).grip || null,
      dps: (await api('/api/simulate', { ...p, ...theFight(), runs: 40, seed: 11 })).dps,
    };
  }
  return { light, heavy, sent: [out[light].sent, out[heavy].sent],
           dps: [out[light].dps, out[heavy].dps],
           damage: [spec.grip_stats[light].damage, spec.grip_stats[heavy].damage] };
})()`);

check("the grip the page shows is the grip it sends",
  fired.sent[0] === fired.light && fired.sent[1] === fired.heavy,
  JSON.stringify(fired));
const spread = Math.max(...fired.dps) / Math.min(...fired.dps);
check("...and two grips are two different weapons in the answer",
  spread > 1.2,
  `${fired.light} ${fired.damage[0]} dmg -> ${fired.dps[0]} dps; ` +
  `${fired.heavy} ${fired.damage[1]} -> ${fired.dps[1]} (x${spread.toFixed(2)})`);

// A LOADER IS NOT COSMETIC EITHER: it sets the magazine and the reload and adds
// three deltas that may be negative. Killstream is +14% crit chance on a `low`
// magazine, Flutterfire -8% and +14% status on the same class. Asserted on the
// answer, so a loader that reaches the card and not the fight fails here.
const loaders = await evaluate(`(async () => {
  ${open(KIT)}
  const out = {};
  for (const loader of ['killstream', 'flutterfire']) {
    assembly = { ...assembly, loader };
    const p = buildPayload();
    out[loader] = { sent: (p.assembly || {}).loader || null,
      dps: (await api('/api/simulate', { ...p, ...theFight(), runs: 40, seed: 11 })).dps };
  }
  return out;
})()`);
check("the loader is sent too",
  loaders.killstream.sent === "killstream" && loaders.flutterfire.sent === "flutterfire",
  JSON.stringify(loaders));
check("...and it moves the answer",
  Math.abs(loaders.killstream.dps - loaders.flutterfire.dps) > 1e-6,
  JSON.stringify(loaders));

// ---- a build REMEMBERS its parts --------------------------------------------

const remembered = await evaluate(`(async () => {
  ${open(KIT)}
  assembly = { grip: 'haymaker', loader: 'killstream' };
  markPresetDirty();
  await new Promise(r => setTimeout(r, 1200));   // the auto-save debounce
  const saved = snapshotState().assembly;
  // Move away and back, the way switching presets does.
  assembly = { grip: 'gibber', loader: 'zip' };
  const st = loadPresetList(BUILDS).find(x => x.name === activePreset).state;
  restoreState(st, 'tombfinger_secondary');
  return { saved, after: { ...assembly }, sent: buildPayload().assembly || null };
})()`);
check("a build remembers the parts it was built with",
  remembered.saved && remembered.saved.grip === "haymaker"
    && remembered.saved.loader === "killstream",
  JSON.stringify(remembered));
check("...and restoring it puts them back",
  remembered.after.grip === "haymaker" && remembered.after.loader === "killstream"
    && remembered.sent && remembered.sent.grip === "haymaker",
  JSON.stringify(remembered));

// A SHARE LINK CARRIES THEM. The tuple omits what the recipient would derive
// anyway, so this uses a NON-default pair: a link that dropped them would land
// on the default and claim a number for a build it does not carry, which is
// exactly what happened to `mode` and `valence`.
const shared = await evaluate(`(async () => {
  ${open(KIT)}
  assembly = { grip: 'haymaker', loader: 'killstream' };
  const url = await shareUrl(false);
  const back = await decodeShare(new URL(url).searchParams.get('b'));
  return { assembly: back.assembly, dflt: defaultAssembly($('weapon').value, null) };
})()`);
check("a share link carries a build's parts",
  shared.assembly && shared.assembly.grip === "haymaker"
    && shared.assembly.loader === "killstream",
  JSON.stringify(shared));
check("...and they are not merely the default coming back",
  shared.dflt.grip !== "haymaker" || shared.dflt.loader !== "killstream",
  JSON.stringify(shared.dflt));

// ---- a part this weapon cannot take is REPAIRED, not obeyed -----------------

// A stale link naming a grip from the sibling slot must land on a real weapon.
// Repaired PART BY PART, so a correctly named loader survives — and the
// difference is visible, since a discarded loader moves the magazine, the
// reload and all three of crit, crit damage and status.
const repaired = await evaluate(`(async () => {
  ${open(KIT)}
  const id = $('weapon').value;
  const mine = assemblySpec(id).default.grip;
  // A grip from the SIBLING slot: a real grip, on the wrong entry.
  const theirs = assemblySpec(slotSibling(id)).grips[0].id;
  const fight = { weapon: id, mods: [], runs: 40, seed: 11, ...theFight() };
  const good  = await api('/api/simulate', { ...fight, assembly: { grip: mine,   loader: 'killstream' } });
  const stale = await api('/api/simulate', { ...fight, assembly: { grip: theirs, loader: 'killstream' } });
  const none  = await api('/api/simulate', { ...fight, assembly: { grip: mine,   loader: 'nope' } });
  return { mine, theirs, good: good.dps, stale: stale.dps, none: none.dps };
})()`);
check("a grip from the other slot is repaired and the loader kept",
  Math.abs(repaired.stale - repaired.good) < 1e-6,
  JSON.stringify(repaired));
check("...and a loader that does not exist falls back without erroring",
  repaired.none > 0 && Math.abs(repaired.none - repaired.good) > 1e-6,
  JSON.stringify(repaired));

await finish("a modular weapon is assembled, and the assembly is the number");
