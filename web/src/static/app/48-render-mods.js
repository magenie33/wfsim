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

  // Exilus: a REAL slot (utility mods only, drain counts) — absent on an
  // Arch-Gun or a sentinel weapon, which show no exilus block —
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
const TENNO_KEYS = ["aiming", "scope_zoom", "invisible", "airborne", "sliding", "aim_gliding", "overshields", "channeling", "melee_equipped", "solo_weapon", "wf_health", "wf_shield", "wf_armor", "wf_energy", "wf_sprint", "extra_stats", "auras"];

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
  // bucket, it divides what a shot costs (the fight caps the combined stat at
  // 75%), and half a dozen sources outside a build grant it. Worth zero under
  // Infinite ammo, which the Limits block above already says out loud.
  ["ammo_efficiency", "Ammo Efficiency"],
  ["status_duration", "Status Duration"],
  // NOT A BUCKET, and its hint says so: points added after mods, the layer an
  // ability's flat grant uses, so 25 is +25% on a 5% base and on a 30% one.
  ["flat_crit_chance", "Flat Critical Chance", "percentage points added after mods, never scaled by the base — permanent, no trigger and no clock"],
  // THREE THAT ARE NOT PERCENTAGES, so each states its unit (the fourth
  // column): a multiplier added after every relative bonus, Arcane
  // Crepuscular's layer, and flat base damage on the weapon's own — once into
  // the base Condition Overload reads and once outside it.
  ["final_crit_damage", "Final Critical Damage", "a critical multiplier added after every relative bonus, as Arcane Crepuscular's is — 1.6 turns a modded 3.0x into 4.6x", "x"],
  ["flat_base_damage_co", "Flat Base Damage (Condition Overload reads it)", "damage added to the weapon's own base, which Condition Overload's bonus then counts — permanent, no trigger and no clock", "flat"],
  ["flat_base_damage", "Flat Base Damage (Condition Overload ignores it)", "damage added to the weapon's own base and left out of what Condition Overload counts — permanent, no trigger and no clock", "flat"],
];
/// WHAT UNIT AN EXTRA STAT IS TYPED IN: `pct` (the default, stored as a
/// fraction), `x` (a multiplier) or `flat` (a plain number), both stored as typed.
const extraStatUnit = (k) => (EXTRA_STAT_KEYS.find(([key]) => key === k) || [])[3] || "pct";

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

/// ONE SEAT'S HALF OF A REQUEST, from a BUILD STATE rather than from the
/// editor — the weapon, the mods, the evolutions, the arcanes and who is
/// holding it.
///
/// A FUNCTION OF A STATE BECAUSE A FIGHT HOLDS n OF THEM. The live editor is
/// one seat; a preset picked in the fight's roster is another, and it has no
/// editor to read its globals off. Two producers of this object would be two
/// answers to "what is this build", and it is `buildState`'s axes that say
/// which fields there are — so a new axis reaches both seats or neither.
function seatPayload(st) {
  // The wire itself is `headlessSeat`'s, which a surface with no page shares;
  // WHO HOLDS IT is the one field only a page can resolve — the linked
  // Warframe build, and absent is the Prototype or a locked weapon's own frame.
  return headlessSeat(META, st, wielderPayloadOf(st.wielder || { frame: PROTOTYPE_ID, preset: "" }));
}

/// THE OPEN BUILD, as a whole request — this seat plus what the READER brings.
///
/// The Tenno fields and the riven definitions are the reader's, not a seat's:
/// every seat in one fight is the same player at the same rank carrying the
/// same items, so they are stated once for the request and `seat_from` on the
/// server reads them from it.
function buildPayload() {
  return {
    ...tennoPayload(),
    ...seatPayload(snapshotState()),
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
  // The axes are `headlessStateAxes`'s, shared with every surface with no page.
  // No wielder travels with a board row or a link: the weapon's own default.
  return buildState(weapon, headlessStateAxes(META, p, weapon, exilusId));
}

