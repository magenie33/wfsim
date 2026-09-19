// ---- CUSTOM ENEMIES: a target you MADE ---------------------------------
//
// A CUSTOM in AGENTS.md's sense: a thing you made that the OTHER modules
// consume — an entry in the scenario's target list, which is what makes it
// reach the simulator and the optimizer with no code of its own in either. NOT
// WEAPON-SCOPED, unlike a riven: a target is no more a statement about one
// weapon than a fight is, so this collection is SHARED across the roster
// (`SHARED_DOMAINS`).
//
// THE ID IS THE NAME, and renaming repoints whatever names it — a custom exists
// only on the machine that made it, so its id comes from something the player
// typed rather than a table nobody can see.
const ENEMIES = "enemies";
const enemyId = (name) => "custom:" + name;
let activeEnemy = null;
let enemyDoc = null;

const DAMAGE_TYPES = ["impact", "puncture", "slash", "heat", "cold", "electricity",
  "toxin", "blast", "corrosive", "gas", "magnetic", "radiation", "viral", "void", "true"];
const SCALING_FACTIONS = ["grineer", "corpus", "infested", "corrupted", "unaffiliated"];

// The blank target: a plain humanoid with nothing unusual about it. Every
// number here is one the player is expected to replace — it is a starting
// point, not a claim about anything in game, which is what `synthetic` says.
const blankEnemy = () => ({
  synthetic: true,
  faction: "grineer",
  scaling_faction: "grineer",
  can_be_eximus: false,
  // A DIFFERENT MECHANIC from taking no damage of a type — see the engine's
  // `status_immunities`. This one moves the PROC DISTRIBUTION: an immune type
  // leaves the denominator and the rest renormalize onto the roll.
  status_immunities: [],
  stats: { base_level: 1, health: 1000, shield: 0, armor: 0, overguard: 0, affinity: 0 },
  // null = take the faction's own column. The moment a player writes one
  // value, the whole column becomes theirs — see `renderEnemyForm`.
  damage_modifiers: null,
  body_parts: [
    { name: "body", multiplier: 1, is_head: false, crit_bonus: false },
    { name: "head", multiplier: 3, is_head: true, crit_bonus: true },
  ],
});

const activeEnemyName = () => {
  if (activeEnemy === null) activeEnemy = localStorage.getItem(presetActiveKey(ENEMIES));
  return activeEnemy;
};
const snapshotEnemy = () => JSON.parse(JSON.stringify(enemyDoc));

/// Every custom target as an ENGINE enemy spec — the same type a published unit
/// has, which is why nothing downstream learns that an enemy can be homemade.
function customEnemySpecs() {
  return loadPresetList(ENEMIES).map((p) => ({
    ...blankEnemy(), ...(p.state || {}),
    id: enemyId(p.name),
    name: p.name,
  }));
}

/// …and as TARGET CARDS, in the shape `/api/meta` publishes, so the picker, the
/// card and the optimizer's read-only view need no branch for them.
function customEnemyCards() {
  const cols = Object.fromEntries((META.factions || []).map((f) => [f.id, f.modifiers]));
  return customEnemySpecs().map((e) => ({
    id: e.id,
    name: e.name,
    custom: true,
    synthetic: true,
    image: null,
    base_level: e.stats.base_level,
    can_be_eximus: !!e.can_be_eximus,
    faction: e.faction || "unknown",
    scaling: e.scaling_faction,
    health: e.stats.health,
    shield: e.stats.shield,
    armor: e.stats.armor,
    overguard: e.stats.overguard,
    unmodeled: [],
    status_immunities: e.status_immunities || [],
    type_modifiers: e.damage_modifiers
      ? Object.entries(e.damage_modifiers).filter(([, v]) => v !== 1)
          .map(([type, mult]) => ({ type, mult }))
      : (cols[e.faction] || []),
    parts: e.body_parts.map((b) => ({ name: b.name, multiplier: b.multiplier, is_head: b.is_head })),
  }));
}

/// THE TARGET LIST, published plus made. One function, because every reader of
/// `META.enemies` is asking "what can this fight be against", and the answer
/// stopped being the roster the moment a player could add to it.
const allEnemies = () => (META.enemies || []).concat(customEnemyCards());
const enemyCard = (id) => allEnemies().find((e) => e.id === id);

/// The custom targets a request has to carry, given the fight it describes.
/// Empty for a published unit — the server has heard of those.
const customEnemiesFor = (id) => customEnemySpecs().filter((e) => e.id === id);

/// THE FIGHT AS A REQUEST BODY. Every path that sends a scenario goes through
/// here, so "a custom target travels with the fight that names it" is one rule
/// rather than one per endpoint — the same reason a riven rides in `rivens`.
