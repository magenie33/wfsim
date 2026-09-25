let META = null;
// 9 × { mod:id|null, pol:string|null, rank:int|null } — POSITIONAL.
// Indices 0–7 are the regular slots; index 8 is the EXILUS slot (utility mods
// only; drain counts toward capacity like any slot; absent on sentinels).
const EXILUS = 8;
// …AND THE STANCE SLOT, index 9, on a MELEE weapon and nowhere else.
//
// IT NEEDS NO FIELD OF ITS OWN ON THE WIRE, which is what made it cheap: a
// stance mod is legal in the stance slot and NOWHERE else, so a flat mod list
// can say which entry is the stance by looking at it. That is exactly what the
// exilus slot could not do — an exilus-eligible mod is legal in a main slot too
// — which is why THAT one travels in a field of its own (AGENTS.md,
// 2026-08-25) and this one rides `mods` with the rest.
const STANCE = 9;
let slots = [];
// 9 × innate polarity name|null — index 8 is the EXILUS slot, which HAS one on
// most weapons (wiki "Exilus Polarity") — never "exilus is never innate",
// which the loader would then slice off to match.
let innate = [];
// ONE ENTRY PER ARCANE POOL the weapon seats, in the weapon's own pool
// order. Almost always a single entry; an Arch-Gun seats two — one Primary
// and one Secondary (wiki Arch-Gun) — and a sentinel seats none.
//
// A weapon never seats two of the SAME pool: slot i draws from pool i, so
// two Primary arcanes is not a build the page can express.
let arcanes = ["none"];
let arcaneRanks = [null];   // null → max rank (mirrors mod slot ranks)
// Pad or trim to the weapon's pool count. Storage is migrated to the list
// shape on load (`migrateArcaneShape`), so a bare value only ever arrives
// from something hand-written — and it is read the obvious way rather than
// silently dropped.
const asArcaneList = (v, n) => {
  const a = Array.isArray(v) ? v.slice() : v == null ? [] : [v];
  while (a.length < n) a.push(undefined);
  return a.slice(0, n);
};
// Per-tier evolution selection {tier: id|null}; null = EMPTY (nothing
// installed at that tier). Tier 1 is the Incarnon Form unlock: empty there
// means no transformation, so the panel falls back to the base form.
// Overwritten by META.defaults on init.
let evoSel = { 1: null, 2: null, 3: null, 4: null };
// HOW THIS BUILD IS PLAYED — part of the BUILD, not of the fight.
//
// "Torid, played through its cycle" is the thing a board ranks, and the entry
// it ranks is `weapon + mode + mods + evolutions + arcanes` — mode is inside
// that list, not beside it. A build preset is exactly what gets submitted and
// shared, so a mode kept anywhere else would have to be fetched from the fight
// at submission time, which is the coupling the board just shed.
//
// It is NOT "installed" like a mod — you own one Torid and play it both ways,
// switching mid-engagement for free. What it is part of is the SUBJECT of a
// measurement, which is what a build is.
let mode = "base";

/// A MODULAR WEAPON'S PARTS — the Grip and the Loader it was built with.
///
/// Part of the BUILD for the same reason a valence is: a Kitgun has no
/// published stat line, so the parts ARE its numbers, and two assemblies of one
/// chamber are two different weapons in every figure a card shows.
///
/// `null` means "this weapon has none", which is every weapon but a Kitgun. The
/// SERVER falls back to a derived default for a request that names no assembly,
/// so this can never hand parts to something that does not take them — and
/// `defaultAssembly` seeds it from `/api/meta` rather than guessing, because a
/// page that picked a different default would draw a build that is not the one
/// being simulated.
///
/// THE CHAMBER IS NOT IN HERE. It is the WEAPON — one mastery track, one riven,
/// one wiki page — and the slot is the weapon too, since it decides the mod
/// pool. So what is left to choose is exactly these two.
let assembly = null;

/// AN ADVERSARY WEAPON'S VALENCE BONUS — the element it came out of a Lich with
/// and how big the roll was. Part of the BUILD, because it is a property of the
/// COPY a player owns rather than of the model: two Kuva Nukors are two
/// different weapons and neither is "the" Kuva Nukor.
///
/// `element: ""` means none chosen, which is what an ordinary weapon means too
/// — the server ignores both fields on a weapon with no valence spec, so this
/// can never hand a bonus to something that does not have one.
let valence = { element: "", bonus: 0 };
// A FRESH scenario, built from the server's defaults and from nothing else.
//
// This is what a weapon that has never been opened gets. NOT
// `snapshotScenario()` — the live fight, which at that moment still belongs to
// the weapon you just left — or opening a new weapon inherits the previous
// one's level, duration, Tenno and buffs, and saves them as that weapon's
// "scenario 1". Two weapons' fights are ALLOWED to look alike; they are never
// allowed to be the same object or to be born from each other.
//
// Rebuilt FIELD BY FIELD, because a field missing here is a field that
// silently becomes `undefined` — which is how `infinite_ammo` once vanished
// from state while the declaration below set it. The server owns every
// default; this copies them.
/// THE ONE METRIC LITERAL ON THIS PAGE, and it exists because a DEFAULT has to
/// be nameable before `/api/meta` has been fetched — a scenario constant is
/// evaluated while the script is, when there is no table to resolve against.
/// Everything after boot goes through `metricOf`, which reads the engine's own
/// table and falls back to this.
const METRIC_FALLBACK =
  { id: "kpm", field: "score", per_minute: true, label: "KPM", hint: "kills per minute" };
function defaultScenario() {
  const d = META.defaults || {};
  return {
    enemy: d.enemy, level: d.level, steel_path: d.steel_path,
    // A GUARDIAN EXIMUS'S AURA over the target. OFF, and that is the honest
    // default rather than a convenience: every number this app has published
    // assumes nobody is shielding it.
    guardian_aura: d.guardian_aura === true,
    // …and the pool an Ancient Protector holds in front of it. OFF, same
    // reason.
    ancient_protector_aura: d.ancient_protector_aura === true,
    // NULL means "whatever this unit is by default", which the server resolves
    // to its `can_be_eximus`. Only an explicit choice is stored, so switching
    // targets keeps giving you the elite unit wherever one exists rather than
    // carrying a decision made about a different enemy.
    eximus: d.eximus ?? null,
    // HOW MANY THE TARGET WAS BROUGHT FOR. One unless a scenario says
    // otherwise — every fight this app has ever run is one player against one
    // enemy, and this changes the ENEMY, never the number of guns.
    squad_size: d.squad_size || 1,
    headshot_pct: d.headshot_pct, aiming: d.aiming !== false,
    // WHERE THE TWO OF THEM STAND, metres. Contact by default — as close as
    // two bodies can be, which is what point blank means once they have a size.
    player_at: [...(d.player_at || [0, 0])],
    target_at: [...(d.target_at || [0, CONTACT_M])],
    // THE REST OF THE FORMATION — every body that is not the one being aimed
    // at. Each is wholly its own: a place, and optionally
    // its own unit and level. What it omits it takes from the target above, so
    // nine identical enemies are nine positions and nothing else.
    formation: (Array.isArray(d.formation) ? d.formation : []).map((f) => ({
      at: [...(f.at || [0, 0])],
      enemy: f.enemy || "",
      level: f.level ?? null,
    })),
    // WHO ELSE IS FIRING — see `28-fight-roster.js`. Empty is the fight this
    // app has always run, one gun against the formation, and it is what every
    // official ruler means: a ruler never mentions this, so switching to one
    // empties the roster rather than carrying the last fight's squad into it.
    also_acting: (Array.isArray(d.also_acting) ? d.also_acting : [])
      .map((x) => ({ weapon: x.weapon, preset: x.preset || "" })),
    // WHERE THE WEAPON IS POINTED. `null` means "at the target", which is the
    // fight this app has always run and what keeps every stored scenario
    // meaning what it meant. Drag the marker and it becomes a place of its own.
    aim_at: d.aim_at ? [...d.aim_at] : null,
    invisible: !!d.invisible, airborne: !!d.airborne, overshields: !!d.overshields,
    channeling: !!d.channeling, melee_equipped: d.melee_equipped !== false,
    solo_weapon: !!d.solo_weapon,
    // KEPT WHOLE, unknown names included: an older page must not strip a
    // newer fight's terms.
    buff_triggers_off: [...(d.buff_triggers_off || [])],
    frame: d.frame || "",
    // ABSENT MEANS THE FLOOR. Only a key that is really there is an override,
    // so these are copied only when the saved scenario had them.
    ...(d.wf_health === undefined ? {} : { wf_health: d.wf_health }),
    ...(d.wf_armor === undefined ? {} : { wf_armor: d.wf_armor }),
    ...(d.wf_energy === undefined ? {} : { wf_energy: d.wf_energy }),
    ...(d.wf_sprint === undefined ? {} : { wf_sprint: d.wf_sprint }),
    infinite_ammo: d.infinite_ammo !== false, metric: d.metric || METRIC_FALLBACK.id,
    // NO `form`: how the weapon is played belongs to the build.
    duration: d.duration, buffs: {},
    // WARFRAME ABILITY BUFFS. A fraction, not a percent — 1 is 100% Ability
    // Strength — because that is what the server multiplies by, and a scenario
    // that stored a percent would need a converter nobody would remember.
    // `abilities` is what is ticked: `{id, secs}`, and `secs: null` is the
    // whole fight. Empty by default, which is what makes the untouched
    // scenario the same fight it has always been.
    ability_strength: 1, abilities: [],
    // THE FIGHT'S OWN STAT BONUSES — see `EXTRA_STAT_KEYS`. Empty is a fight
    // that hands this weapon nothing it did not earn, which is every ruler.
    extra_stats: { ...(d.extra_stats || {}) },
    // WHAT THE WARFRAME BRINGS. Both are the frame's rather than the weapon's,
    // so they travel with the fight exactly as `abilities` does — and empty is
    // a squad running no aura and a frame with bare sockets, which is every
    // ruler and every scenario stored before they existed.
    auras: (d.auras || []).map((a) => ({ id: a.id, count: a.count || 1 })),
    shards: (d.shards || []).map((x) => ({
      shard: x.shard, effect: x.effect, tauforged: !!x.tauforged,
    })),
  };
}

// Sim scenario + per-buff config. Seeded from META.defaults in init().
// `buffs` maps buff id -> { stacks, locked } (section 2); the buff SET comes
// from /api/panel and syncs as the build changes.
// THE TENNO's fields (`aiming`, `invisible`, `airborne`, `wf_armor`,
// `wf_energy`) describe the fight's other actor — who is holding the weapon
// and what they are doing. Every `condition:` on a mod card is a question
// about them, and the arcanes that scale off a Warframe read the two stats.
// They live flat on `sim` like every other scenario field; the engine is
// where they become a Tenno (`data/tenno/default.yaml` + these overrides).
// `aiming` defaults TRUE because that is what the sim silently assumed before
// the knob existed, so no stored preset changes meaning.
let sim = { enemy: "thrax_centurion", level: 9999, steel_path: true, eximus: null,
  guardian_aura: false, ancient_protector_aura: false,
  headshot_pct: 100, aiming: true,
  // THE FIGHT'S GEOMETRY: where the two of them stand, in metres. CONTACT by
  // default (0.5 m, twice a body's radius) — the closest they can be, which is
  // what point blank means once a body has a size, and what both official
  // rulers pin. Dragged on the arena scene; see `mountArena`.
  player_at: [0, 0.5 - 0.5],
  target_at: [0, 0.5],
  invisible: false, airborne: false, overshields: false, channeling: false,
  // DRAWN by default — what "With Melee Weapon Equipped" asks, and what every
  // ruler runs. Quick-melee is the OTHER answer and the reason it is a knob.
  melee_equipped: true,
  // THE LOADOUT, not what the wielder is doing: false = carrying a full one,
  // which is the fight the board is scored under and what every clause about
  // the other slots has always been answered with.
  solo_weapon: false,
  // WHICH TRIGGERS FIRE NO BUFF HERE — the events still happen and
  // still score. Empty is the engagement this app has always run.
  // See `buffTriggersField` (Limits).
  buff_triggers_off: [],
  frame: "",
  // NO `form`, AND NO `mode`. How the weapon is played is part of the BUILD;
  // a fight that carried it could decide how the weapon was fired, which is
  // what let a ruler pin an Incarnon weapon at its cycle:
  // the official scenarios no longer carry a mode, and a custom one must not
  // either.
  // 180 s: the same length the official rulers run, so a player's first
  // comparison against the board is not a puzzle. Only the
  // DEFAULT — a saved scenario carries its own duration and keeps it.
  infinite_ammo: true, metric: METRIC_FALLBACK.id, duration: 180, buffs: {},
  ability_strength: 1, abilities: [], extra_stats: {}, auras: [], shards: [] };
// The current build's configurable buffs (from the last /api/panel response).
let buffList = [];
// Damage-meter rows the player has expanded into their per-type split, kept
// across runs so a simulate does not re-collapse them.
const simMeterOpen = new Set();
// Optimizer scope, 8 + 1 slots in TWO blocks: `mods` (id -> "search"|"fixed")
// scopes the MAIN 8 slots — exilus-flagged mods may sit here too, all 9
// slots accept them (game rule); `exilus` (same states, exilus-eligible mods
// only) scopes the +1 exilus slot — "search" = a slot option next to "leave
// empty", "fixed" = pin it (max one). Plus the arcane set and per-tier
// evolution option sets. Enemy + buffs are shared with the Sim panel
// (`sim`). Seeded from the current build on weapon change.
let opt = { starts: [] };
// THE START OPEN IN THE BUILDER, or null — `79-start-edit.js`. Declared here,
// early, because the autosave it suspends can run during boot.
let startEdit = null;
let optSeeded = false;
// HOW THE SEARCH RUNS. `finalists` is the whole of it, and it IS the search's:
// how many builds survive to the last round is a decision about this search
// and about nothing else, so it rides the search preset.
//
// The optimizer tab is TWO HALVES and the page now draws them as two BOXES: the search preset owns everything in the first, the
// simulator owns everything in the second and it is read-only here. What sits
// outside both boxes is in neither preset — which is exactly one thing.
//
// THE FINAL ROUND'S RUN COUNT IS THAT THING. A run count is not what to
// search and it is not the fight either — `sim.runs` does not exist, because
// "how hard do I want to measure right now" is a fact about the person (see
// `SIM_RUNS_KEY`). So it is a PREFERENCE with a key of its own, typed rather
// than defaulted, saved by no preset and pinned by no ruler.
//
// The cost is stated rather than hidden: the two counts can differ, so a winner
// may be crowned at a precision the replay will not use. The ranking reports it
// — each row is re-run through `/api/simulate` and marked `≠` when the two
// disagree by more than 4σ.
const OPT_RUNS_KEY = "wfsim-opt-final-runs";
const finalRuns = () => {
  const v = Math.round(Number(localStorage.getItem(OPT_RUNS_KEY)));
  return Number.isFinite(v) && v >= 1 && v <= 20000 ? v : SIM_RUNS_DEFAULT;
};
const setFinalRuns = (n) => {
  const v = Math.max(1, Math.min(20000, Math.round(Number(n)) || SIM_RUNS_DEFAULT));
  localStorage.setItem(OPT_RUNS_KEY, String(v));
};
// `threads` LEFT ON 2026-08-29. How much of this machine the page may
// use is ONE setting and it lives in the topbar beside the language and the
// theme (`compute-select`, a share of the reported cores); a per-search
// override of a global preference is two controls for one fact, which is the
// arena's own rule in another module. `poolSize()` is the whole answer now.
// An older preset may still carry `threads` and `runs`; both are ignored on
// load, and the auto-save drops them the first time the scope is touched.
// `swap_width` is how many positions the descent may change at once once
// single changes stop paying — how HARD to search, like `finalists`.
const OPT_RUN_DEFAULTS = { finalists: 10, swap_width: 1, candidate_runs: 10 };
let optRun = { ...OPT_RUN_DEFAULTS };
let pickerSlot = 0;
// Mod-picker sort/filter prefs — persisted across slots, presets and weapons.
let pickerPrefs = { sort: "gain", dir: "desc", pol: null };
try { const s = JSON.parse(localStorage.getItem("wfsim-picker")); if (s) pickerPrefs = { ...pickerPrefs, ...s }; } catch (_) {}
const savePickerPrefs = () => localStorage.setItem("wfsim-picker", JSON.stringify(pickerPrefs));

