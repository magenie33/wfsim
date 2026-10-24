let META = null;
// SET BY `init` RIGHT BEFORE ITS OWN `route()`. The account check answers
// before the engine does, and a signed-in reader on /login is sent home from
// it — a page drawn then reads a null `META`. Until this is set, `route` waits.
let bootRouted = false;
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
// Per-tier evolution selection {tier: id}, every tier filled
// (`defaultEvolutions`): a finished Genesis has no empty tier.
let evoSel = {};
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
/// THE FIGHT EVERY SCENARIO IS APPLIED ONTO — `headlessDefaultScenario`, the
/// one the bot's riven gain is frozen under too.
function defaultScenario() { return headlessDefaultScenario(META, CONTACT_M, METRIC_FALLBACK.id); }

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
/// THE FIGHT'S ONE EMPOWERMENT, in words: Steel Path or a mission's Empowered
/// Enemies, never both (webapi `empowerment_from`); "" when it has none. `e`
/// is a scenario's `empowered` id or a result's `{ id, squad_size }`.
function empowermentLabel(steelPath, e, short) {
  const id = e && typeof e === "object" ? e.id : e;
  if (id === "archimedea") return tr("Archimedea");
  if (id === "icebind") return tr("Icebind");
  return steelPath || id === "steel_path" ? (short ? "SP" : tr("Steel Path")) : "";
}
/// …AND WHICH ONE A SCENARIO HOLDS, as the Empowerment control names it.
const empowermentOf = (s) => (s.empowered === "archimedea" || s.empowered === "icebind" ? s.empowered
  : s.steel_path || s.empowered === "steel_path" ? "steel_path" : "none");

let sim = { enemy: "thrax_centurion", level: 9999, steel_path: true, eximus: null,
  guardian_aura: false, ancient_protector_aura: false,
  headshot_pct: 100, aiming: true, scope_zoom: 0,
  // THE FIGHT'S GEOMETRY: where the two of them stand, in metres. CONTACT by
  // default (0.5 m, twice a body's radius) — the closest they can be, which is
  // what point blank means once a body has a size, and what both official
  // rulers pin. Dragged on the arena scene; see `mountArena`.
  player_at: [0, 0.5 - 0.5],
  target_at: [0, 0.5],
  invisible: false, airborne: false, sliding: false, aim_gliding: false, overshields: false, channeling: false,
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
  ability_strength: null, abilities: [], apl: [], extra_stats: {}, auras: [], shards: [] };
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
let opt = { starts: [], limits: null };
// THE START OPEN IN THE BUILDER, or null — `79-start-edit.js`. Declared here,
// early, because the autosave it suspends can run during boot.
let startEdit = null;
let optSeeded = false;
// HOW THE SEARCH RUNS. `finalists` is the whole of it, and it IS the search's:
// how many builds survive to the last round is a decision about this search
// and about nothing else, so it rides the search preset.
//
// The optimizer tab is TWO HALVES and the page now draws them as two BOXES: the search preset owns everything in the first, the
// simulator owns everything in the second and it is read-only here.
//
// THE FINAL ROUND IS THE SIMULATOR'S, runs included: the server measures each
// contender through `simulate_json` with the fight's own `runs` (`simRuns()`),
// so a row's number is what simulating its build answers.
// `threads` LEFT ON 2026-08-29. How much of this machine the page may
// use is ONE setting and it lives in the topbar beside the language and the
// theme (`compute-select`, a share of the reported cores); a per-search
// override of a global preference is two controls for one fact, which is the
// arena's own rule in another module. `poolSize()` is the whole answer now.
// An older preset may still carry `threads` and `runs`; both are ignored on
// load, and the auto-save drops them the first time the scope is touched.
const OPT_RUN_DEFAULTS = { finalists: 10, candidate_runs: 10 };
let optRun = { ...OPT_RUN_DEFAULTS };
let pickerSlot = 0;
// Mod-picker sort/filter prefs — persisted across slots, presets and weapons.
let pickerPrefs = { sort: "gain", dir: "desc", pol: null };
try { const s = JSON.parse(localStorage.getItem("wfsim-picker")); if (s) pickerPrefs = { ...pickerPrefs, ...s }; } catch (_) {}
const savePickerPrefs = () => localStorage.setItem("wfsim-picker", JSON.stringify(pickerPrefs));

