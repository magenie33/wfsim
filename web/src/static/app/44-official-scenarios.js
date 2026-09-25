// ---- THE OFFICIAL SCENARIOS -------------------------------------------
//
// `data/benchmarks/*.yaml`, served in META. They sit in the scenario bar
// beside the player's own and behave like presets in every way but three:
// nothing stores them (so they cannot drift from one machine to another),
// nothing edits them, and they appear on EVERY weapon — which is the whole
// point, since a number only means something against a ruler someone else can
// pick up. Wanting a variant is what ⧉ is for: it copies the official one into
// an ordinary, editable scenario of your own.
const builtinScenarios = () => (META.benchmarks || []).map((b) => ({
  // TRANSLATED for display, like everything else on the page. Its identity is
  // the `builtin` id below, never this string — see `scenarioNamed`.
  name: tr(b.name),
  // The id, not just a flag: a board row records WHICH ruler it was measured
  // under, and a retired version has to be recognisable as retired.
  builtin: b.id,
  savedAt: 0,
  state: b.scenario,
}));
// The list every reader sees: official first, then the player's own.
const scenarioList = () => builtinScenarios().concat(loadPresetList(SCENARIOS));
// By NAME or by official ID. A benchmark's display name is translated, so the
// name alone cannot be its identity: switching language would orphan the
// pointer that says which scenario is open. The id is what gets stored.
const scenarioNamed = (n) => scenarioList().find((p) => p.name === n || p.builtin === n);
const scenarioKey = (n) => (scenarioNamed(n) || {}).builtin || n;
/// Is the fight on screen the official one? Then nothing may write to it.
const officialScenarioActive = () => !!(scenarioNamed(activeScenario) || {}).builtin;

// The scenario bar's config, as a FACTORY rather than a local — two callers
// need it now: the bar itself, and the "edit a copy of this fight" button in
// the official note, which performs the identical copy. A second hand-written
// config there is how the two come to copy different things.
function scenarioBarCfg() {
  return {
    domain: SCENARIOS,
    label: tr("Scenarios"),
    noun: "scenario",
    load: scenarioList,
    // An official scenario is not stored, so it can never be written back —
    // this is the line that makes "read-only" a property of the DATA rather
    // than of the buttons drawn over it.
    store: (ps) => storePresetList(SCENARIOS, ps.filter((p) => !p.builtin)),
    readonly: (p) => !!p.builtin,
    active: () => activeScenario,
    setActive: (n) => { activeScenario = n; localStorage.setItem(presetActiveKey(SCENARIOS), scenarioKey(n)); },
    snapshot: snapshotScenario,
    apply: applyScenario,
    blank: snapshotScenario,
    // Owning nothing, the fight is an official ruler: there is no blank to show.
    pinned: true,
    rerender: scenariosChanged,
  };
}

/// "Give me an editable copy of the fight I am looking at" — the one
/// implementation, shared by the chip's ⧉ and the note's button.
const copyActiveScenario = () => copyActivePreset(scenarioBarCfg());

function renderScenarioBar() {
  const scenariosCfg = scenarioBarCfg();
  // ONE QUESTION: WHICH RULER. An official scenario is one per ruler, so the
  // other three axes would be controls answering nothing.
  renderBenchmarkBarIn($("bench-bar-simulator-scenarios"), { ...scenariosCfg, axes: ["ruler"], benchLabel: tr("Benchmark scenarios"), benchHint: tr("the official rulers — the same fight on every weapon") });
  renderPresetBarIn($("preset-bar-simulator-scenarios"), scenariosCfg);
}

function fillSelect(id, items) {
  const el = $(id); el.innerHTML = "";
  for (const it of items) { const o = document.createElement("option"); o.value = it.id; o.textContent = it.name; el.appendChild(o); }
}
let currentPool = [];
const weaponInfo = (id) => META.weapons.find((w) => w.id === id) || META.weapons[0];
/// …AND WHETHER THERE IS ONE. `weaponInfo` FALLS BACK to the first weapon on
/// the roster, so it always answers and can never be the test: an id nobody
/// has resolves to whatever sorts first, silently. Anything validating an id
/// it was handed — the agent door, a stored link into another weapon's presets
/// — asks this instead.
const weaponExists = (id) => !!id && META.weapons.some((w) => w.id === id);
// Evolutions moved into each weapon's meta entry (they are per transform
// group); this reads the CURRENT weapon's tiers.
/// THE TIERS OF A WEAPON — **the one it is ASKED about**, and only the LIVE one
/// when nobody says.
///
/// It took no argument at all and read `#weapon` every time, which made
/// `weaponAxes(id)` able to disagree with itself: every other axis in that
/// object is derived from the `id` it was handed, and `evolutions` was derived
/// from whatever the select happened to say. So `weaponAxes(A)` could return
/// A's mods, A's arcanes and B's evolution tiers, and the caller has no way to
/// see it — `show("evo-block", AX.evolutions.length > 0)` would then draw the
/// block for a weapon with none.
///
/// That is precisely the shape of `check_parity`'s intermittent "the builder
/// shows an evolution block for a weapon with none" — four times over eight
/// runs, never the same weapon twice, always a weapon with NO evolutions. It
/// is not proven to be the cause and the diagnostic that would prove it is in
/// place; what IS certain is that this could produce it, and that a function
/// reading global state instead of what it was asked about is wrong whether or
/// not it is the bug of the day.
const weaponEvos = (weaponId) =>
  (weaponInfo(weaponId || $("weapon").value) || {}).evolutions || [];
// THE TIER LADDER, in one place. Tier N is choosable only once N-1 is filled:
// a tier-2 perk with no tier 1 is not a weaker build, it is not a build. The
// builder greys the later rows out; the gain scan has to obey the same rule or
// it measures — and recommends — evolutions nobody can select.
const evoOpenTo = () => {
  let n = 0;
  for (const t of weaponEvos()) { if (!evoSel[t.tier]) break; n = t.tier; }
  return n + 1; // the deepest tier that may be chosen
};
const modById = (id) => poolWithRivens().find((m) => m.id === id);
// A SECTION THAT IS NOT THERE IS NOT A DEAD APP.
//
// This threw on a null element, and the throw is upstream of everything:
// `init` -> `applyWeapon` -> `renderOpt` -> here, so ONE missing id put
// "WFSim could not start" on the whole page. Reported from an iPhone with the
// stack ending at this line — `#opt-modes-sect` sits at byte 42,939 of a
// 52,687-byte document, which is exactly the tail a truncated mobile response
// loses.
//
// LOUD, NOT SILENT. Swallowing it would hide a real markup fault, so the
// missing id is named in the console and the app carries on: a page with one
// section unstyled is worth more than a page that will not open.
const show = (id, on) => {
  const el = $(id);
  if (!el) {
    console.warn(`show(${id}): no such element — section skipped`);
    return;
  }
  if (on) el.removeAttribute("hidden");
  else el.setAttribute("hidden", "");
};
// Where (other than exceptIdx) this mod is currently slotted, or -1.
const placedAt = (id, exceptIdx) => slots.findIndex((s, i) => i !== exceptIdx && s.mod === id);
// A CARD BELOW ITS MAX RANK IS AN ID OF ITS OWN on the wire, `<card>@<rank>`
// (`engine::data::mods::RANK_MARK`), so every list of mod ids carries its ranks.
// A slot keeps the card and the rank apart, and these are the crossings.
const splitRank = (id) => {
  const m = typeof id === "string" && !isRivenId(id) ? /^(.+)@(\d+)$/.exec(id) : null;
  return m ? [m[1], Number(m[2])] : [id, null];
};
const rankedId = (card, rank) => {
  const m = card && modById(card);
  return m && !m.riven && rank != null && rank < m.max_rank ? `${card}@${rank}` : card;
};
const slotModId = (s) => (s && s.mod ? rankedId(s.mod, s.rank) : null);
/// The cards the quick calc and the search try at EVERY rank — `every_rank` in
/// /api/meta, unless the reader replaced it in the quick calc's settings.
const everyRank = () => gainPrefs.everyRank || (META && META.every_rank) || { mods: [], arcanes: [] };
/// `m` at each rank below its max, as list rows: `id` is the ranked id, `card`
/// the card's, `rank` the rank it seats at and `drain` what it costs there.
const lowerRanks = (m) => (!m.riven && everyRank().mods.includes(m.id)
  ? Array.from({ length: m.max_rank }, (_, r) =>
    ({ ...m, id: `${m.id}@${r}`, card: m.id, rank: r, drain: modDrain(m, r) }))
  : []);

/// SEATING A MOD IS ONE DECISION, and it is made here — the picker, the agent
/// door and every check take the same one. Written inside a menu's click
/// handler instead, "what happens when this mod is already in another slot"
/// becomes a property of that menu rather than of the build.
///
/// A mod already seated elsewhere is EXCHANGED with this slot, polarities stay
/// with their slots, a mod arrives at `rank`, else at its maximum, and a null
/// id empties the slot.
/// Redrawing is the caller's: a batch seats several and repaints once.
function equipMod(i, id, rank) {
  if (id === null) { slots[i].mod = null; return; }
  if (slots[i].mod === id) { if (rank != null) slots[i].rank = rank; return; }
  const at = placedAt(id, i);
  if (at >= 0) {
    const a = slots[i], b = slots[at];
    [a.mod, b.mod] = [b.mod, a.mod];
    [a.rank, b.rank] = [b.rank, a.rank];
    if (rank != null) a.rank = rank;
  } else {
    slots[i].mod = id;
    slots[i].rank = rank ?? modById(id).max_rank;
  }
}

/// A BARE WEAPON: no mods, and every slot back to the polarity the weapon
/// itself came with — a polarity a reader changed is theirs, not the build's.
function clearMods() {
  slots.forEach((s, i) => { s.mod = null; s.pol = innate[i]; });
  renderMods();
}

// Switching weapons rebuilds every weapon-scoped view. It runs as an
// APPLY (auto-save stays out of the reset/reseed churn — the optimizer
// scope resets here, and saving that would wipe the scope presets); the
// build preset then follows the new weapon via the explicit
// markPresetDirty() below, which no-ops when this runs inside a preset
// load (restoreState already holds the applying guard).
function applyWeapon(id, presetMods) {
  whileApplying(() => applyWeaponInner(id, presetMods));
  markPresetDirty();
}

// A user-driven weapon CHANGE, as opposed to applyWeapon's "rebuild the
// editor for this weapon". Presets are per weapon, so the bar must reload
// from the new weapon's own storage — initPresets() restores its active
// preset (creating "build 1" the first time). The optimizer's groups
// re-bootstrap on their own: applyWeaponInner clears optSeeded, and
// renderOpt re-runs bootstrapOptPresets against the new scope.
// NOT called from restoreState: loading a preset must not re-enter this.
function switchWeapon(id) {
  flushPresetSaves();
  $("weapon").value = id;
  ensureWeaponBoard(id);
  applyWeapon(id, null);
  initPresets();
}

function applyWeaponInner(id, presetMods) {
  const w = weaponInfo(id);
  buffList = []; // rebuilt from the next /api/panel response for this build
  // HOW THIS WEAPON IS PLAYED, reset to its own default. "base" is a legal id
  // on almost every weapon, so without this an Incarnon weapon opened after a
  // plain one would keep the plain one's mode and quietly skip the cycle that
  // is supposed to be its default — the same rule that makes a new weapon get
  // `defaultScenario()` rather than the last one's fight. `restoreState`
  // overwrites this immediately when a preset is being applied.
  mode = defaultMode(id, null);
  // NOTHING CROSSES BETWEEN WEAPONS: a valence is a statement about one of
  // them, so opening another starts with none.
  valence = defaultValence(id, null);
  // …AND SO ARE ITS PARTS. A grip belongs to one slot, so the sibling entry's
  // five are not even legal here; `defaultAssembly` returns null on a weapon
  // that takes none, which is what clears them on the way to an ordinary gun.
  assembly = defaultAssembly(id, null);
  // THE FIGHT DOES NOT MOVE. A scenario is shared across the roster now
  // (`SHARED_DOMAINS`), so switching weapons keeps the fight you are measuring
  // under — which is the entire point of being able to compare two guns.
  //
  // Its one weapon-shaped knob is headshot %, and it is left alone for the same
  // reason the official rulers leave it alone: the ruler pins 100 and applies
  // to the whole roster, and the SERVER forces 0 on a weapon that cannot
  // headshot (`parse_fight`, sentinel). Resetting it here would have rewritten
  // the shared scenario every time you changed weapon, and auto-save would have
  // stored that.
  sim.__weapon = id;
  // A weapon's search is its own: `renderOpt` seeds the default starts.
  opt = { starts: [], limits: null }; optSeeded = false;
  optLast = null;
  // ...and how it RUNS, for the same reason the scenario resets: a weapon that
  // has never been searched must not inherit the last weapon's finalists or
  // thread count into the "search 1" it is about to be given.
  optRun = { ...OPT_RUN_DEFAULTS };
  // Last weapon's ranking is not this weapon's. Nothing else clears it — the
  // tabs are CSS-hidden rather than re-rendered, so it simply stayed on screen
  // under the new weapon's name.
  if ($("opt-results")) $("opt-results").innerHTML = "";
  // A weapon's pool is the UNION of the pools it draws from: `primary` mods
  // fit any primary weapon, `rifle` is the class pool. One flat list per
  // weapon was right only while every rifle-class weapon was a launcher.
  rivenModCache = { key: null, list: [] };
  refreshRivenNames(); // this weapon's rivens, named by the engine
  // The SERVER decides which mods this weapon can equip (`pool_for_weapon`)
  // and sends the id list; the class tables are only where the mod objects
  // live. A JS re-implementation of the same rules goes stale the moment the
  // engine learns a new one — Amalgam mods are not equippable on a sentinel
  // weapon and ammo mods do nothing on an infinite reserve, and neither
  // reaches the builder or the optimizer while the pool
  // was computed twice.
  const byId = new Map(
    Object.values(META.mod_pools || {}).flat().map((m) => [m.id, m]),
  );
  currentPool = (w.mods || []).map((id) => byId.get(id)).filter(Boolean);
  // NINE, not eight. The server sends 8 main slots plus the EXILUS slot's own
  // innate polarity (`innate_slots_for`, which has appended it since the
  // 2026-07-28 wiki cross-check), and this line sliced that ninth entry off
  // and padded a null over it. So every weapon with an exilus polarity — Boar
  // Prime's Madurai, the Naramon on Torid / Cernos Prime / Dual Toxocyst /
  // Laetum — showed an unpolarized exilus slot: the mod in it paid full drain
  // instead of half, and the Forma plan charged for a polarity the weapon
  // comes with.
  innate = (w.innate_polarities || []).slice(0, 9);
  while (innate.length < 9) innate.push(null);
  // TEN, not nine — and the tenth is the STANCE slot, which has a polarity of
  // its own on almost every melee (the Magistar's is Vazarin). Left off, the
  // slot drew blank and the grant read as a mismatch: the same slice-and-pad
  // that hid the exilus polarity, one slot further along.
  innate[STANCE] = w.stance_polarity || null;

  // A weapon with no asset yet must show NOTHING, not a broken-image box:
  // src="" still resolves (to the page) and renders as a failed load.
  const wimg = IMG(w.image);
  $("w-img").hidden = !wimg;
  if (wimg) $("w-img").src = wimg;
  // The weapon name links to its wiki page too (display suffixes like
  // " (sentinel)" are ours, not part of the page name).
  renderWeaponName();
  // Subtype (e.g. "Dual Pistols") + form tags; the mod-eligibility group
  // (mod_class) drives the picker's pool but isn't shown as a tag.
  // Name the POOL, the way the wiki does ("Rifle Mods" / "Pistol Mods"): the
  // eligibility group is what actually decides which mods equip, and a bare
  // "Mods" heading leaves the visitor guessing which pool a launcher draws
  // from. Falls back to the plain word if a class ever has no label.
  const POOL_NAME = { rifle: "Rifle Mods", pistol: "Pistol Mods", primary: "Primary Mods" };
  // Names the pools it actually draws, widest first — "Primary + Rifle Mods".
  const poolName = (w.mod_pools || [w.mod_class])
    .map((p) => (POOL_NAME[p] || p).replace(/ Mods$/, "")).join(" + ") + " Mods";
  $("mod-block-h").textContent = tr(poolName || "Mods");

  // WHAT THIS WEAPON DOES BEYOND ITS STATS. Generated engine-side from the data
  // that implements each passive, so the line and the simulation cannot say
  // different things. Empty for most weapons; the strip hides itself then.
  //
  // It lives here, in the weapon strip, because a passive is part of the
  // WEAPON — not of the build, not of the fight. Gotva Prime's crit set and
  // Dual Toxocyst's Frenzy are most of what those weapons are, and until
  // 2026-08-05 the page never mentioned either, so both read as ordinary guns.
  const wp = $("w-passives");
  if (wp) {
    const lines = w.passives || [];
    wp.hidden = !lines.length;
    wp.innerHTML = lines.map((x) => `<div>${escHtml(tr(x))}</div>`).join("");
  }
  $("w-tags").innerHTML = [w.subtype, w.uses_evo2 ? "Incarnon" : null, w.sentinel ? "Sentinel" : null]
    .filter(Boolean).map((t) => `<span class="tag">${t}</span>`).join("");

  const AX = weaponAxes(w.id);
  show("arcane-block", AX.arcanes.length > 0);
  show("evo-block", AX.evolutions.length > 0);
  // WHICH WEAPON THE BLOCKS ON SCREEN ARE FOR, stamped where the blocks are
  // decided. `#weapon`'s value is set SYNCHRONOUSLY by the switch while these
  // are drawn after it, so there is a window in which the page names one
  // weapon and shows another's axes — and a reader of the DOM has no way to
  // tell. It is one line and it makes that window observable: `check_parity`
  // waits on it rather than on the value plus a guess, which is where its
  // "builder shows an evolution block for a weapon with none" flake came from
  // (three times over six runs, always a weapon following one WITH evolutions
  // — 2026-08-24).
  document.body.dataset.panelFor = w.id;
  // THE VALENCE AXIS exists only where the weapon has one — an adversary
  // weapon. Same "no choice, no control" rule every other axis follows.
  show("element-block", !!valenceSpec(w.id));
  // …AND THE NUMBERS FOLLOW THE BLOCKS THAT ARE ACTUALLY THERE. They were
  // written into the markup, so a weapon with no evolutions numbered its
  // Valence block 5 with no 4 above it. Derived from the
  // DOM rather than from a table of weapons, so a block added later — or a
  // weapon built in the app one day — is numbered without anyone maintaining a
  // list.
  //
  // Non-numeric badges (Σ, ≡, ▶) are the ones that are not steps, and they keep
  // whatever they say.
  renumberBlocks();
  // An Arch-Gun's two slots are NOT interchangeable, so the line names the
  // pools rather than counting them: "primary + secondary", not "2 slots".
  $("arcane-sub").textContent = w.sentinel
    ? tr("sentinels cannot equip arcanes")
    : (w.arcane_pools || []).map((p) => tr(SLOT_LABEL[p] || p)).join(" + ");
  // The previous weapon's arcanes may not fit this one, slot by slot.
  arcanes = arcanesFor(w.id, arcanes);
  arcaneRanks = asArcaneList(arcaneRanks, arcanes.length).map((x) => x ?? null);

  slots = Array.from({ length: 10 }, (_, i) => ({ mod: null, pol: innate[i], rank: null }));
  (presetMods || []).filter((m) => modById(m)).slice(0, 8).forEach((m, i) => { slots[i].mod = m; slots[i].rank = modById(m).max_rank; });
  // A sensible default for a preset's mods: its cheapest layout. Nothing to
  // plan for an empty build, whose layout is the one the weapon was born with.
  if (slots.some((x) => x.mod)) autoForma({ alone: true }).then((r) => { if (r) renderMods(); });

  renderAssembly();
  renderMods(); renderArcanes(); renderEvo(); renderMode(); renderValence(); renderSim(); renderOpt();
}

