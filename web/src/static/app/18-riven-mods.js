// ---- Rivens as MODS ----------------------------------------------------
// A saved riven is equipment, so it belongs in the mod list — findable by its
// preset name ("riven 1"), by its generated name ("Visican"), or by any value
// printed on it.
//
// It is NOT put in `META.mod_pools`: a riven is the visitor's own item, built
// against this weapon's disposition, so it travels WITH the request. The
// engine adds it to the pool for that build only, which is why one shared
// pool can still serve everyone.
//
// An INCOMPLETE riven is deliberately allowed through. A card with no stats
// is a mod that does nothing, which is an ordinary thing for a build to
// contain and not worth refusing.
const RIVEN_PREFIX = "riven:";
/// A CARD'S IDENTITY, WHICH IS NOT ITS NAME.
///
/// A build references a riven as `riven:<id>`. While that id was the NAME, a
/// rename had to chase every saved build that pointed at the card, two cards
/// could not share a label, and a collision had to be settled by renaming
/// somebody's riven. A name is something a player edits; an identity is not.
///
/// Opaque and never shown. `Date.now()` orders them and the suffix separates
/// two made in the same millisecond — uniqueness is checked against the store
/// either way, because that is the only thing this has to be.
const newRivenId = (all) => {
  for (;;) {
    const id = "r" + Date.now().toString(36) + Math.random().toString(36).slice(2, 5);
    if (!all.some((p) => p.id === id)) return id;
  }
};
const isRivenId = (id) => typeof id === "string" && id.startsWith(RIVEN_PREFIX);
let rivenModCache = { key: null, list: [] };

// The saved rivens of the CURRENT weapon, shaped like mods so every list,
// picker and slot that already understands a mod understands these.
function rivenMods() {
  const w = $("weapon").value;
  const raw = loadPresetList(RIVENS);
  const key = w + "|" + JSON.stringify(raw) + "|" + JSON.stringify(rivenNames);
  if (rivenModCache.key === key) return rivenModCache.list;
  const list = raw.map((p) => {
    const st = p.state || {};
    const stats = (st.bonuses || st.positives || []).concat(st.malus || st.curse || []);
    const lines = (rivenNames[p.id] || {}).lines || [];
    const official = (rivenNames[p.id] || {}).name || "";
    return {
      id: RIVEN_PREFIX + p.id,
      // DE's own riven card — the game draws every riven the same, so one
      // image serves them all (`data/assets.yaml` mods.riven).
      image: META.riven_image || null,
      name: official ? `${p.name} · ${official}` : p.name,
      name_en: official,
      subtype: "Riven",
      riven: true,
      elemental: (st.bonuses || st.positives || []).some((b) => (rivenStat((b && b.id) || b) || {}).elemental),
      // A weapon takes ONE riven. Same family = mutually exclusive, the rule
      // the pool already has — so every list, slot and scope enforces it
      // without knowing what a riven is.
      family: "riven",
      rarity: "legendary",
      polarity: (st.polarity || "madurai").replace(/^./, (c) => c.toUpperCase()),
      drain: 2 + 2 * (st.rank ?? 8),
      max_rank: 8,
      exilus: false,
      // The VEILED riven card — DE's own image for every riven type
      // (`imageName` on the riven mod item), and the one on the CDN like
      // every other mod picture.
      image: "OmegaMod.png",
      // Every printed value is searchable, which is how you find the riven
      // with the crit damage on it without remembering what you called it.
      effects: lines.length ? lines : stats.filter((s) => s && s.id).map((s) => s.id.replace(/_/g, " ")),
      unmodeled_effects: (rivenNames[p.id] || {}).unmodeled || [],
      // THE AUCTIONS THIS CARD COMPETES WITH — built here because this is
      // where both halves are in scope: the roll, and the weapon it is for.
      // The cache key covers both, so it cannot outlive either.
      market_url: rivenMarketUrl(st, (META.weapons || []).find((x) => x.id === w)),
      __spec: st,
    };
  });
  rivenModCache = { key, list };
  return list;
}

// The generated name and printed lines per saved riven, filled in by asking
// the engine — the page never computes a riven value itself.
let rivenNames = {};
async function refreshRivenNames() {
  const ps = loadPresetList(RIVENS);
  const out = {};
  for (const p of ps) {
    try {
      const r = await api("/api/riven", { weapon: $("weapon").value, ...(p.state || {}) });
      out[p.id] = {
        name: r.name || "",
        lines: (r.stats || []).map((s) => s.text),
        // …AND WHICH OF THEM PAY NOTHING. A riven's stats are DE's rather than
        // ours, so a card can carry one this engine does not model — a melee
        // card's Finisher Damage — and it gets the same "partly modelled" line
        // a mod with a dead clause gets, naming the line that is dead.
        unmodeled: (r.stats || []).filter((s) => !s.modeled).map((s) => s.text),
      };
    } catch (_) { /* a name is a nicety; the riven still equips */ }
  }
  rivenNames = out;
  rivenModCache = { key: null, list: [] };
  // The lists that show a riven's printed values may already have rendered
  // with only its stat ids — this arrives afterwards, so they get redrawn.
  //
  // THE BUILD'S OWN SLOTS ARE ONE OF THEM. An equipped riven prints its roll in
  // its slot like any other card, and that text comes from here — so a slot
  // drawn before this returned showed the fallback, four bare stat NAMES with
  // no values on them, and nothing ever came back to correct it. It was
  // invisible while the slot printed nothing at all.
  if ($("mod-slots")) renderMods();
  if (typeof renderOptModList === "function" && $("opt-mods") && !$("opt-block").hidden) renderOptModList();
  if ($("riven-all") && !$("riven-block").hidden) renderRivenAll();
  if ($("mod-popover") && !$("mod-popover").hidden && rivenPickerSlot != null) renderMenu(rivenPickerSlot, $("mod-search").value || "");
}
// Which slot the builder's mod picker is open for, so an async refresh can
// redraw it without reopening it.
let rivenPickerSlot = null;

/// Everything equippable on this weapon: the pool, plus this weapon's rivens.
const poolWithRivens = () => currentPool.concat(rivenMods());

/// The mod ids a set of EVOLUTIONS takes off the weapon.
///
/// An equip rule is asked of every firing mode a weapon has, and installing the
/// Incarnon form adds one — so Dual Toxocyst wears Semi-Pistol Cannonade until
/// tier 1 goes in and cannot after ("Weapons with an Incarnon mode must have
/// Semi-Auto trigger type for both firing modes in order to equip this mod",
/// wiki Semi-Pistol_Cannonade; user, 2026-08-04). The RULE is the engine's
/// (`pool_for_build`); `evo_forbids` is its answer per evolution, so this is a
/// lookup rather than a second implementation of it — the last time the client
/// re-derived a pool rule it went stale the same week (see `applyWeaponInner`).
const forbiddenByEvos = (sel) => {
  const map = (weaponInfo($("weapon").value) || {}).evo_forbids || {};
  const out = new Set();
  for (const id of Object.values(sel || evoSel)) for (const m of map[id] || []) out.add(m);
  return out;
};
/// What THIS BUILD can equip: the weapon's pool minus what its evolutions cost
/// it. The optimizer keeps asking `poolWithRivens()` — evolutions are a search
/// DIMENSION there, so a mod one variant refuses is still in scope for the ones
/// that do not, and which is which is decided per candidate by the engine.
const buildPool = () => {
  const no = forbiddenByEvos();
  return poolWithRivens().filter((m) => !no.has(m.id));
};
/// WHICH SLOT A MOD MAY SIT IN, asked by the picker and the agent door alike.
/// The exilus slot takes what `exilusPool()` says, the question the optimizer's
/// exilus scope asks. The stance rule runs BOTH ways: a stance is legal in the
/// stance slot and nowhere else, where an exilus mod may also sit in a main one.
const modFitsSlot = (m, i) => (i !== EXILUS || !!m.exilus) && ((i === STANCE) === !!m.stance);
/// What may go in the EXILUS slot. Both modules ask this one function: the
/// builder used `poolWithRivens()` and the optimizer `currentPool`, which
/// agreed only because no riven is exilus-eligible — a coincidence, not a
/// rule, and the sort that stops being true without anyone noticing.
const exilusPool = () => poolWithRivens().filter((m) => m.exilus);
/// THE STANCE SLOT'S OWN POOL. A stance is legal there and nowhere else, which
/// is the whole reason the slot costs no wire field — and it is why this filter
/// runs in BOTH directions: the stance slot takes only stances, and the eight
/// main slots take everything BUT.
const stancePool = () => poolWithRivens().filter((m) => m.stance);

/// THE WEAPON'S BUILD AXES — one description, read by BOTH modules.
///
/// The builder fills these slots and the optimizer searches them, so they are
/// one question asked twice, and stating it once makes a new weapon a ONE-PLACE
/// change: a sentinel that seats no arcane, an Arch-Gun that seats two, a pool
/// with no exilus mod in it.
///
/// AN AXIS IS SHOWN IFF IT HAS OPTIONS — one rule in place of three category
/// guesses (`!sentinel`, `arcane_slots >= 1`, `uses_evo2`) standing in for "is
/// there anything to choose here", each remembered twice.
// INFINITE AMMO — on by DEFAULT for every weapon, because the sim models no
// ammo PICKUPS: a finite reserve is the pessimistic half of a mechanic we only
// half have. A weapon whose reserve is infinite IN GAME cannot be switched off
// it (every sentinel prints "Ammo Max: infinity / Ammo Type: None"), which
// shows as a TICKED, DISABLED box rather than a hidden one.
// WHERE the fight happens, when that changes the weapon. An Arch-Gun on the
// ground and in Archwing is the SAME weapon — same damage, same mod pool, same
// riven — and only its sustain differs (reload 2.50 vs 4.50, a finite 400-round
// reserve vs a regenerating magazine), so it is a scenario axis rather than a
// second entry. Shown only where there is a choice.
const deployField = (w, state) => {
  const opts = w.deployments || [];
  if (opts.length < 2) return "";
  const cur = opts.includes(state.deployment) ? state.deployment : opts[0];
  return `<label title="${escHtml(tr("where the weapon is fired — it changes reload and ammo, nothing else"))}">${escHtml(tr("Environment"))} ` +
    ddButton("dd-deployment", {
      value: cur,
      dataK: "deployment",
      // The data keys are lowercase; the LABEL is the wiki's own column head.
      items: opts.map((o) => ({ value: o, label: tr(o[0].toUpperCase() + o.slice(1)) })),
    }) + `</label>`;
};

// IS THE TARGET ITS ELITE VARIANT? Offered only where one EXISTS, because
// there is no such unit otherwise — the engine rejects the combination rather
// than quietly simulating an ordinary enemy under an elite label (a Thrax's
// overguard is innate, not Eximus-granted), so a control here would be a
// promise the fight cannot keep.
//
// DEFAULT ON wherever it exists. The Eximus is what a
// Steel Path player actually meets, and it is not a cosmetic difference: it
// adds health and puts a pool of Overguard in front of it, so a build measured
// on the ordinary unit is measured on a fight nobody has.
//
// `sim.eximus` is NULL until you say otherwise, and null means "this unit's
// answer" — which is what makes the default follow the TARGET rather than
// stick to whatever the last target happened to be.
const eximusOn = (en) => sim.eximus ?? !!(en && en.can_be_eximus);
const eximusField = (en) => {
  if (!en || !en.can_be_eximus) return "";
  return `<label class="check" title="${escHtml(tr("the elite variant: more health, and a pool of Overguard in front of it"))}"><input type="checkbox" data-k="eximus" ${eximusOn(en) ? "checked" : ""}> ${escHtml(tr("Eximus"))}</label>`;
};

/// A UNIT THAT DIES TWICE — offered only where there is a second half to ask
/// about, which today is a Thrax and nothing else.
///
/// OFF by default, and that is the honest default rather than a convenience:
/// every number this app has published is the physical form alone.
const spectralField = (en) => {
  if (!en || !en.has_spectral_form) return "";
  return `<label class="check" title="${escHtml(tr("destroying the physical form leaves a spectre with 40% of its health that only the Operator can damage. Ticked, the kill — and every on-kill buff and ammo drop with it — waits for that spectre, so no weapon can finish this fight"))}"><input type="checkbox" data-k="spectral_form"${sim.spectral_form ? " checked" : ""}> ${escHtml(tr("Spectral form"))}</label>`;
};

/// HOW MANY PEOPLE THE TARGET WAS BROUGHT FOR — offered only where the unit's
/// health reads it, which today is a Demolisher and nothing else.
///
/// ONE PLAYER STILL FIRES. This is a HARDER TARGET, not three more guns: the
/// arena has one shooter and always has, so what four means here is the
/// Demolisher a full squad walks into, measured against your weapon alone. The
/// title says so, because a control called "squad" invites the other reading.
///
/// THE LADDER IS THE UNIT'S. `squad_health_bonus` comes off `/api/meta` rather
/// than being written here — a second copy of +0/+50/+100/+200 would be a
/// second answer the day DE changes it — and it is also what decides whether
/// this control exists at all.
const squadField = (en) => {
  const ladder = (en && en.squad_health_bonus) || [];
  if (ladder.length < 2) return "";
  const opts = ladder
    .map((bonus, i) => {
      const n = i + 1;
      const label = bonus > 0
        ? `${n} (+${Math.round(bonus * 100)}% ${tr("health")})`
        : `${n}`;
      return `<option value="${n}"${squadOf() === n ? " selected" : ""}>${escHtml(label)}</option>`;
    })
    .join("");
  return `<label title="${escHtml(tr("the target a squad of this size meets — it is a fatter enemy, not more guns: one weapon still fires, and what is ranked is how much of it yours takes off"))}">${
    escHtml(tr("Squad"))} <select data-k="squad_size" data-num>${opts}</select></label>`;
};
/// SOLO UNLESS SAID, which is what every fight in this app has always been.
const squadOf = () => Number(sim.squad_size) || 1;

/// WHAT THIS WEAPON SETTLES THIS FIELD TO, AND WHY — or null when the choice is
/// the reader's.
///
/// THE RULE IS THE ENGINE'S. `engine::build::scenario::forced_for`
/// decides and `/api/meta` states the consequence per weapon; this reads it.
/// Re-deriving the three rules here from weapon flags is two implementations
/// of one rule, and a forced field looks identical whoever forced it, so they
/// drift without anything going red.
///
/// It still has THREE states and not two, which is what the flags could not say: "there is nothing to run out of" and "there is, and you
/// cannot refill it" force the SAME box to OPPOSITE values. Reading one as the
/// other left the only adjustable weapon being the one weapon the game gives no
/// way to adjust.
const settledAxis = (w, id) => {
  const f = (w || {}).settled || {};
  const hit = f[id];
  return hit ? { value: hit[0], why: hit[1], overridable: !!hit[2] } : null;
};

/// **A FIGHT'S OWN HOUSE RULES, PER WEAPON CLASS**.
///
/// A scenario is ONE DOCUMENT that any weapon can be tested against, so it
/// carries the rules for classes it is not currently pointed at — which is what
/// makes "in my fight, Arch-Guns have infinite ammo" a thing you can write on a
/// Burston's page and have apply the next time you open the Larkspur.
///
/// OVERRIDES SIT BEHIND LEGALITY and the engine draws that line, never this
/// file: `META.class_rules.overridable` is the (class, axis) pairs a scenario
/// may argue with, which is exactly where the capability's absence is OUR
/// stand-in rather than the game's rule. So a control is offered for
/// Arch-Gun ammo and no control exists anywhere for a Sentinel's headshots.
const overridablePairs = () => ((META && META.class_rules) || {}).overridable || [];
const isOverridable = (cls, id) => overridablePairs().some((p) => p[0] === cls && p[1] === id);
const classRuleOf = (cls, id) => ((sim.class_rules || {})[cls] || {})[id];
/// Read a class-rule control and store what it says. Returns whether it WAS
/// one, so a generic `[data-k]` handler can delegate and return early.
///
/// A RULE THAT AGREES WITH THE DEFAULT IS NOT A RULE. `data-crdef` carries the
/// value the capability would have given, and ticking back to it CLEARS the key
/// rather than storing the same answer twice — the absent-key mechanism a
/// Warframe override already uses, for the same reason: a scenario's stored
/// rules should say what it actually decided.
const applyClassRule = (el) => {
  const cr = el.dataset.cr;
  if (!cr) return false;
  const [cls, id] = cr.split("|");
  writeClassRule(cls, id, el.type === "checkbox" ? el.checked : Number(el.value));
  return true;
};
/// The value the CAPABILITY gives a class on this axis, read off any weapon of
/// that class — what a cleared rule falls back to.
const classRuleDefault = (cls, id) => {
  const f = settledAxis((META.weapons || []).find((x) => x.weapon_class === cls), id);
  return f ? f.value : false;
};
/// A class rule as a control or the door states it: stored only where it
/// differs from the default, and `null` clears it.
const writeClassRule = (cls, id, v) =>
  setClassRule(cls, id, v === null || v === classRuleDefault(cls, id) ? undefined : v);

/// Write one, or clear it with `undefined`.
///
/// AN ABSENT KEY IS THE MECHANISM, exactly as a Warframe override's is: the
/// engine falls back to the capability when nothing is written, so a rule that
/// merely AGREES with the default is pruned rather than stored. That keeps a
/// scenario's diff about what it actually decided, and keeps the fingerprint a
/// board row stores from moving on a no-op tick.
const setClassRule = (cls, id, v) => {
  const rules = { ...(sim.class_rules || {}) };
  const col = { ...(rules[cls] || {}) };
  if (v === undefined) delete col[id]; else col[id] = v;
  if (Object.keys(col).length) rules[cls] = col; else delete rules[cls];
  if (Object.keys(rules).length) sim.class_rules = rules; else delete sim.class_rules;
  markScenarioDirty();
};
// A SENTINEL WEAPON IS ALWAYS AIMING — it just never aims
// at the HEAD, which is why its headshot default is 0 and every on-headshot
// trigger stays dead anyway. Ticked and disabled, the same shape as infinite
// ammo: the state is real, the control is honestly unavailable.
const aimField = (w, state) => {
  const f = settledAxis(w, "aiming");
  const forced = !!f;
  const on = forced ? !!f.value : state.aiming;
  const why = forced
    ? tr(f.why)
    : tr("mods that only work while aiming (Galvanized Crosshairs, Argon Scope, Sharpened Bullets…) grant nothing when this is off");
  return `<label class="check" title="${escHtml(why)}"><input type="checkbox" data-k="aiming"${on ? " checked" : ""}${forced ? " disabled" : ""}> ${escHtml(tr("Aiming"))}</label>`;
};
/// …AND ITS HEADSHOT RATE IS 0 AND NOT YOURS.
///
/// The VALUE was already right from both ends — `defaultHeadshotPct` opens a
/// sentinel at 0, and `parse_fight` forces 0 on one whatever the request
/// carries. The CONTROL was not: a bare number input, editable, so a reader
/// could type 100 on a Verglas, watch the page accept it, and get a run
/// computed at 0 with nothing on screen saying why. A column that is shown and
/// not applied looks exactly like one that works — the same fault
/// `check_custom_enemies` was written for, in another panel.
///
/// It is DISPLAY-ONLY, and that is deliberate: `sim.headshot_pct` belongs to a
/// scenario SHARED across the whole roster (`SHARED_DOMAINS`), so writing 0
/// into it here would rewrite the fight every time a sentinel was opened and
/// auto-save would store it — which is the exact trap `switchWeapon` documents
/// and avoids. So the field shows 0 and disables itself while the underlying
/// scenario keeps whatever the reader set for the rest of the roster.
const headshotField = (w, state) => {
  const f = settledAxis(w, "headshot_pct");
  const forced = !!f;
  const val = forced ? f.value : state.headshot_pct;
  const why = forced
    ? tr(f.why)
    : tr("a per-PELLET aim weight on the body the shot STRUCK, not a whole-spread promise — the landing spot is rolled for each pellet, and nothing a blast or a chain reaches can be a headshot");
  return `<label title="${escHtml(why)}">${escHtml(tr("Headshot %"))} <input type="number" data-k="headshot_pct" min="0" max="100" value="${val}"${forced ? " disabled" : ""}></label>`;
};
/// …AND THE AMMO BOX IS THE ONE FIELD A FIGHT MAY ARGUE WITH.
///
/// Settled OFF on a ground Arch-Gun because it cannot be resupplied — but that
/// is OUR pessimistic stand-in for ammo pickups the sim has no entities for,
/// not a rule of the game, so a scenario is allowed to say otherwise. When it
/// is, the box stays LIVE and writes this fight's rule for the class rather
/// than the scenario's own field: the setting is about Arch-Guns, so it is
/// stored against Arch-Guns and applies to every one of them.
const ammoField = (w, state) => {
  const f = settledAxis(w, "infinite_ammo");
  const cls = w && w.weapon_class;
  const ruled = f && f.overridable && cls;
  const rule = ruled ? classRuleOf(cls, "infinite_ammo") : undefined;
  const forced = !!f && !ruled;
  const on = ruled
    ? (rule === undefined ? !!f.value : !!rule)
    : (f ? !!f.value : state.infinite_ammo !== false);
  const why = ruled
    ? tr(f.why) + " — " + tr("this fight can rule otherwise for the whole class, and that is what this box writes")
    : f
      ? tr(f.why)
      : tr("on = ammo pickups keep the reserve topped up, which the sim has no entities for; off = it runs dry. The magazine and its reloads apply either way");
  const attr = ruled
    ? `data-cr="${escHtml(cls)}|infinite_ammo" data-crdef="${!!f.value}"`
    : `data-k="infinite_ammo"`;
  return `<label class="check${ruled && rule !== undefined ? " ruled" : ""}" title="${escHtml(why)}"><input type="checkbox" ${attr}${on ? " checked" : ""}${forced ? " disabled" : ""}> ${escHtml(tr("Infinite ammo"))}</label>`;
};

/// THE AMMO ECONOMY — what the bodies leave, and how far it is collected from.
///
/// All three sit beside Infinite ammo because none of them decides anything
/// while that box is ticked: a pack pays into a reserve the fight is ignoring.
const pickupFields = (state) => {
  const infinite = state.infinite_ammo !== false;
  const dim = infinite ? " dim" : "";
  const drops = state.ammo_drops !== false;
  const reach = state.pickup_range_m;
  return `<label class="check${dim}" title="${escHtml(tr("a killed body drops ammo as it does in game — 45% solo, and an Eximus always leaves one. It pays only into a reserve the fight is spending"))}"><input type="checkbox" data-k="ammo_drops"${drops ? " checked" : ""}> ${escHtml(tr("Ammo drops"))}</label>`
    + `<label class="${dim.trim()}" title="${escHtml(tr("how far a pack is collected from, in metres, measured from the body that dropped it. Empty = any distance, which is this arena's default because the Tenno never walks. In game it is 3 m on foot and 13.5 m with a maxed Vacuum or Fetch"))}">${escHtml(tr("Pickup range (m)"))} <input type="number" data-k="pickup_range_m" min="0" max="1000" step="0.5" value="${reach === undefined || reach === null ? "" : escHtml(String(reach))}" placeholder="∞"></label>`
    + `<label class="check${dim}" title="${escHtml(tr("an open-world fight: every drop rate is higher (60% solo against 45%)"))}"><input type="checkbox" data-k="landscape"${state.landscape ? " checked" : ""}> ${escHtml(tr("Open world"))}</label>`;
};

function weaponAxes(weaponId) {
  const w = weaponInfo(weaponId || $("weapon").value) || {};
  const exilus = w.sentinel ? [] : exilusPool();
  return {
    mods: poolWithRivens(),
    exilus,
    hasExilus: exilus.length > 0,
    // …AND THE STANCE SLOT, asked of the POOL rather than of the weapon's
    // class: "this weapon's pool holds a stance" is the only honest test, since
    // a melee class whose stances nobody has transcribed yet would otherwise
    // show an empty slot with nothing to offer — which reads as a broken picker
    // rather than as missing data.
    stance: stancePool(),
    hasStance: stancePool().length > 0,
    // One entry per arcane pool, in the weapon's own pool order.
    arcanes: (w.arcane_pools || []).map((pool, i) => ({ pool, options: arcanePool(i) })),
    // One entry per evolution tier — OF THE WEAPON THIS OBJECT IS ABOUT. It
    // read the live `#weapon` until 2026-08-24, which let this one axis
    // describe a different weapon from the other four.
    evolutions: weaponEvos(w.id),
    // HOW THE WEAPON IS PLAYED — an axis like the rest, because it is one: the
    // builder picks a value and the optimizer searches the set, and the board
    // ranks weapon x mode. Only the modes a fight can HOLD are offered; the
    // engine derives that from whether entering the form costs a gauge you
    // have to earn ("always Incarnon" is not a playstyle, it is a few seconds
    // at a time), so nothing here is a list anyone maintains.
    //
    // A ONE-MODE WEAPON GETS AN EMPTY AXIS, by the same rule every other axis
    // follows: an axis is shown iff it has options, and "base" alone is not a
    // choice anybody has.
    modes: (w.modes || []).length > 1 ? w.modes : [],
  };
}

/// The mode a build plays in: the asked-for one where this weapon offers it,
/// else however the arsenal plays it — the cycle where there is one to run.
///
/// One resolver, so "no mode named" means the same thing in the builder, in a
/// share link and in a board submission.
function defaultMode(weaponId, want) {
  const ms = (weaponInfo(weaponId) || {}).modes || ["base"];
  if (want && ms.includes(want)) return want;
  return ms.includes("cycle") ? "cycle" : (ms[0] || "base");
}

/// The rivens a request must carry for its `riven:` ids to mean anything.
///
/// `id` is what the item id is built from; `name` rides along because it is
/// what a share link written before ids carried, and the server still reads it
/// when there is no id.
const rivenPayload = () =>
  loadPresetList(RIVENS).map((p) => ({ id: p.id, name: p.name, spec: p.state || {} }));

