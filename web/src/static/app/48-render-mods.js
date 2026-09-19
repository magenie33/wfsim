// ---- render mods ----
// WHICH POLARITIES WE SHIP ART FOR. `imgTag` hides an image that fails to load,
// so a polarity with no icon draws NOTHING — which is exactly what an empty
// slot draws, and the reader cannot tell "unpolarised" from "polarised in a
// colour this page has no picture of".
//
// The Vinquibus is why it matters: it is "Innate one Madurai and one Aura
// polarities", the wiki hosts no Aura_Pol.svg, and an invisible icon would put
// the page back to claiming the slot is free when it is a +25% penalty. A named letter says more than a missing picture.
const POL_ART = new Set(["Madurai", "Naramon", "Vazarin", "Zenurik", "Unairu", "Penjaga", "Umbra", "Omni"]);
// ITS OWN, because `cap` is a LOCAL helper inside two other functions and using
// it here threw `cap is not defined` on every render (caught by
// check_debuff_coverage, 2026-08-21).
const polCap = (s) => String(s || "").replace(/^./, (c) => c.toUpperCase());
const polGlyph = (pol) => POL_ART.has(polCap(pol))
  ? imgTag(POL(polCap(pol)), "pol")
  : `<span class="pol pol-txt">${escHtml(polCap(pol).slice(0, 1))}</span>`;

function polBtn(pol, i) {
  const t = pol ? `${polCap(pol)} — ${tr("change polarity")}` : tr("change polarity");
  return `<button class="pol-btn" data-i="${i}" title="${escHtml(t)}">${pol ? polGlyph(pol) : '<span class="nopol">◇</span>'}</button>`;
}
function renderMods() {
  // The quick-calc bar sits above this block and is measured against the same
  // build, so it redraws with it.
  // A FIXED STANCE IS SEATED BEFORE ANYTHING READS THE SLOTS, so the capacity
  // line counts its grant and a build restored without it holds it anyway.
  const fixedStance = fixedStanceOf($("weapon").value);
  if (fixedStance && slots[STANCE]) {
    slots[STANCE].mod = fixedStance;
    slots[STANCE].pol = stancePolOf($("weapon").value);
    slots[STANCE].rank = null;
  }
  if (typeof renderQuickCalc === "function") renderQuickCalc();
  // ...and so does the mode control: equipping a Cannonade is what takes the
  // cycle away, so the reason it is greyed changes with the slots.
  if (typeof renderMode === "function") renderMode();
  if (typeof renderAssembly === "function") renderAssembly();
  // ...and so does what the BOARD would say about it. The panel's "this one is
  // not sent" is the door's own answer now, so it has to be re-asked when the
  // build it is about changes.
  if (typeof refreshBoardDoor === "function") refreshBoardDoor();
  const used = capacityUsed();
  const cap = builderCap();
  const capEl = $("capacity");
  capEl.textContent = `${used} / ${cap}`;
  capEl.classList.toggle("over", used > cap);
  const f = formaCount();
  $("forma").textContent = [`${f.regular} Forma`, f.umbra ? `${f.umbra} Umbra` : null, f.omni ? `${f.omni} Omni` : null]
    .filter(Boolean).join(" · ");

  const box = $("mod-slots");
  box.innerHTML = "";
  for (let i = 0; i < 8; i++) box.appendChild(buildSlot(i));

  // Exilus: a REAL slot (utility mods only, drain counts) — absent on sentinels.
  // A sentinel weapon has no exilus slot, so it shows no exilus block —
  // label included. Standing a placeholder where the slot would be says
  // "something is missing here"; the truth is that nothing belongs there.
  const hasExilus = weaponAxes().hasExilus;
  show("exilus-block", hasExilus);
  const ex = $("exilus");
  ex.innerHTML = "";
  if (hasExilus) ex.appendChild(buildSlot(EXILUS));

  // THE STANCE SLOT, on a weapon that has stances to put in it. Asked of the
  // POOL rather than of the slot — `hasStance` is "this weapon's pool holds a
  // stance", which is the only honest test: a melee class whose stances nobody
  // has transcribed yet would otherwise show an empty slot with nothing to
  // offer, and that reads as a broken picker rather than as missing data.
  const hasStance = weaponAxes().hasStance;
  show("stance-block", hasStance);
  const st = $("stance");
  st.innerHTML = "";
  if (hasStance) st.appendChild(buildSlot(STANCE));
  renderBuilderFormaPlan();
  refreshPanel();
}

// The current build as a request payload — shared by /api/panel and
// /api/simulate so the stats panel and the sim always agree on the loadout.
// Mods keep slot order (elements are position-sensitive).
//
// It carries the TENNO as well as the weapon, because half of what a build is
// worth is a question about the player: a mod gated `invisible` pays or
// does not, and Primary Bulwark is worth +500% or nothing depending on the
// frame's armor. A panel resolving against the NEUTRAL player while the sim
// resolves against the fight's offers a buff card the sim never runs, and
// hides a contribution the sim is paying. One player, both
// answers. The scenario fields that describe the
// PLAYER rather than the fight.
// NO `frame` AND NO `shards`: the wielder is the BUILD's (`buildWielder`), and
// what the fight keeps is what others hand it — the squad's auras, its own stat
// bonuses — and the ticked overrides.
const TENNO_KEYS = ["aiming", "invisible", "airborne", "overshields", "channeling", "melee_equipped", "solo_weapon", "wf_health", "wf_shield", "wf_armor", "wf_energy", "wf_sprint", "extra_stats", "auras"];

// THE FIGHT'S OWN STAT BONUSES: what this weapon is handed by something that is
// not its build — a squad buff, a Warframe ability, an arcane on another weapon.
//
//. So they are not buffs: no trigger, no clock, no stack
// count. They join the same ADDITIVE buckets the mods feed, which is what
// makes them cheap to be right about — a scenario's +60% multishot and Split
// Chamber's +90% sum, exactly as
// two multishot mods would, and every lock still wins over them.
//
// NO ELEMENTS. An elemental mod is position-sensitive and enters a hierarchy,
// so "+90% Heat" is not a number, it is a place in an ordering.
const EXTRA_STAT_KEYS = [
  ["base_damage", "Base Damage"],
  ["multishot", "Multishot"],
  ["crit_chance", "Critical Chance"],
  ["crit_damage", "Critical Damage"],
  ["status_chance", "Status Chance"],
  ["status_damage", "Status Damage"],
  ["fire_rate", "Fire Rate"],
  ["reload_speed", "Reload Speed"],
  ["magazine", "Magazine Capacity"],
  // AMMO EFFICIENCY is the odd one and belongs here anyway: it is not a damage
  // bucket, it is a chance for a shot to cost NOTHING, and half a dozen sources
  // outside a build grant it. Worth zero under Infinite
  // ammo, which the Limits block above already says out loud.
  ["ammo_efficiency", "Ammo Efficiency"],
  ["status_duration", "Status Duration"],
  // NOT A BUCKET, and its hint says so: points added after mods, the layer an
  // ability's flat grant uses, so 25 is +25% on a 5% base and on a 30% one.
  ["flat_crit_chance", "Flat Critical Chance", "percentage points added after mods, never scaled by the base — permanent, no trigger and no clock"],
];

/// THE WARFRAME ROSTER, and what picking one means: it fills armor, max energy
/// and sprint speed at once. Sprint is the one that could not be set at all
/// before — there was no field for it — so every "With Sprint Speed 1.2 or
/// Higher" perk in the roster was unreachable from this page.
///
/// The numbers stay EDITABLE after a pick, because the roster is unmodded: a
/// built frame carries Steel Fiber and Primed Flow and this one does not, and
/// "With Energy Max Over 700" is a gate no frame can open at all (the highest
/// maxed pool in the game is 300). Replacing the fields with a dropdown would
/// have made that unaskable.
const frames = () => (META && META.frames) || [];
const frameOf = (id) => frames().find((f) => f.id === id);

// The fight's player, as request fields. Its own function because three
// callers need exactly this subset and a fourth will: it is the actor, not the
// scenario, and the enemy half of the scenario has no business travelling with
// a panel request.
function tennoPayload() {
  return Object.fromEntries(TENNO_KEYS.map((k) => [k, sim[k]]));
}

function buildPayload() {
  return {
    ...tennoPayload(),
    weapon: $("weapon").value,
    // WHO HOLDS IT — the linked Warframe build, as the Warframe module reads it.
    // Absent is the Prototype, or a locked weapon's own frame on the server.
    wielder: wielderPayload(),
    evolutions: Object.values(evoSel).filter(Boolean),
    // One per pool, in the weapon's pool order — the server reads either
    // this or a bare value, so an old saved build still means what it meant.
    arcane: arcanes,
    arcane_rank: arcaneRanks,
    mods: slots.filter((s) => s.mod).map(slotModId),
    // HOW IT IS PLAYED, from the BUILD. Riding in the scenario as `form` lets
    // the FIGHT decide how a weapon is fired, so the official ruler silently
    // plays every Incarnon weapon through its cycle and "never transmuting"
    // cannot be asked for.
    mode,
    // THE VALENCE, as two flat fields rather than an object: `base_for` reads
    // them off the request the same way it reads the deployment, and every
    // path that builds a weapon for a request goes through it.
    valence_element: valence.element,
    valence_bonus: valence.bonus,
    // THE PARTS, as an object, because they are one fact: `assembly_of` reads
    // the pair and repairs it part by part. Omitted entirely on a weapon that
    // has none, so the wire says nothing rather than saying `null`.
    ...(assembly ? { assembly: { ...assembly } } : {}),
    // A `riven:` id means nothing without the riven itself — it is the
    // visitor's item, not a pool entry, so it rides along with the request.
    rivens: rivenPayload(),
  };
}

/// THE INVERSE OF `buildPayload` — a request back into builder state, and the
/// ONLY translation left between the two.
///
/// It exists so that nothing else has to translate. An optimizer row carries
/// `replay`, a complete simulate request the SERVER authored from the very
/// candidate it scored, and "+ add" applies it through here — instead of the
/// page re-deriving a build from a description of one, which is what dropped an
/// axis four times. What makes this pair safe where a hand-written producer was
/// not is that it is a PAIR: `buildPayload(stateFromBuild(p)) === p` is a
/// property one check can assert over a build with every axis set, and it fails
/// for any axis either side forgets. Counting fields cannot say that.
///
/// `exilusId` is the one thing a payload cannot state. The wire carries ONE
/// flat list of mod ids — which is what the engine resolves, and the number
/// never depends on which slot they sat in — so the ninth slot is a matter of
/// LAYOUT (its polarity, the Forma plan) and the caller that knows says so. An
/// optimizer row does and a share link does not, so the link takes the order
/// the list is in. A BOARD ROW DOES: it carries `exilus` as its own field, and
/// `boardEntries` is where that is read back into the slot.
function stateFromBuild(p, weapon, exilusId) {
  const w = weaponInfo(weapon) || {};
  const ids = (p.mods || []).filter(Boolean);
  // THE STANCE IS TOLD APART BY LOOKING AT IT, which is the whole reason it
  // needs no field of its own on the wire — where the exilus id has to be
  // PASSED IN, because an exilus-eligible mod is legal in a main slot and the
  // list alone cannot say which entry came out of which slot.
  const stanceId = ids.find((id) => (modById(splitRank(id)[0]) || {}).stance);
  const main = ids.filter((id) => id !== exilusId && id !== stanceId);
  const sl = Array.from({ length: 10 }, () => ({ mod: null, pol: null, rank: null }));
  main.slice(0, 8).forEach((id, i) => { sl[i].mod = id; });
  if (exilusId && exilusId !== "none" && ids.includes(exilusId)) {
    sl[EXILUS].mod = exilusId;
  } else if (main.length > 8) {
    sl[EXILUS].mod = main[8];
  }
  if (stanceId) sl[STANCE].mod = stanceId;
  // A RANK IS THE ONE THE ID NAMES, and the card's ceiling when it names none.
  sl.forEach((s) => {
    const [card, r] = splitRank(s.mod);
    const m = card && modById(card);
    if (m) { s.mod = card; s.rank = r ?? m.max_rank; }
  });
  const evo = { 1: null, 2: null, 3: null, 4: null };
  (p.evolutions || []).forEach((id) => {
    const t = (w.evolutions || []).find((tt) => tt.options.some((o) => o.id === id));
    if (t) evo[t.tier] = id;
  });
  const nPools = arcanePools(weapon).length;
  return buildState(weapon, {
    // No wielder travels with a board row or a link: the weapon's own default.
    wielder: null,
    slots: sl,
    evoSel: evo,
    arcane: asArcaneList(p.arcane, nPools).map((x) => x || "none"),
    arcaneRank: asArcaneList(p.arcane_rank, nPools).map((x) => x ?? null),
    // BOTH HALVES OR NEITHER. `restoreState` cleans them against the weapon
    // being opened, so a payload naming an element this spec does not offer
    // lands on the default rather than on a weapon nobody has.
    // `null` WHEN THE REQUEST CARRIES NONE, meaning "the weapon's own default"
    // — which is what an omitting request meant and what the server itself
    // used. It was `undefined`, on the same reasoning and one step short of it:
    // `defaultMode`/`defaultValence`/`defaultAssembly` all read `null` as that
    // instruction, and `undefined` is DELETED BY `JSON.stringify` on the way
    // into a preset. So a producer that carefully NAMED the axis handed over an
    // object that had lost it, which is precisely what `BUILD_AXES` exists to
    // catch — and did (check_valence, 2026-08-24, on an optimizer winner's
    // saved build).
    mode: p.mode || null,
    valence: p.valence_element
      ? { element: p.valence_element, bonus: p.valence_bonus }
      : null,
    assembly: p.assembly || null,
  });
}

