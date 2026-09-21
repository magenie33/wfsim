// ---- Auto-save ---------------
// A preset MIRRORS the editor: every edit debounces straight into the
// active preset's stored state, so there is no save button and no
// unsaved-changes dot. Branch before experimenting with "+ new" (empty)
// or the ⧉ duplicate — the preset you leave behind keeps its content.
//
// `presetApplying` guards the other direction: loading a preset (or a
// weapon switch) re-renders everything, and those renders must NOT be
// mistaken for user edits and written back — a cross-weapon apply drops
// unknown ids, which auto-save would otherwise make permanent.
let presetApplying = 0;
function whileApplying(fn) {
  presetApplying++;
  try {
    fn();
  } finally {
    presetApplying--;
    // Renders during the apply queue their own debounced saves — drop
    // them, or the applied (possibly pruned) state writes itself back.
    dropSave("builds");
    dropSave("search");
  }
}

let presetSaveTimer = null;
/// THE SAVES WAITING ON A DEBOUNCE, by collection. A switch of document first
/// runs them (`flushPresetSaves`): `whileApplying` clears the timers so an
/// apply cannot write itself back, and before this a switch inside the
/// debounce dropped the reader's last edit — or, on duplicate, wrote it into
/// the copy instead. A save deliberately dropped leaves the table too.
const pendingSaves = new Map();
function deferSave(key, fn, ms) {
  dropSave(key);
  const run = () => { pendingSaves.delete(key); fn(); };
  const t = setTimeout(run, ms);
  pendingSaves.set(key, { t, run });
  return t;
}
function dropSave(key) {
  const p = pendingSaves.get(key);
  if (p) { clearTimeout(p.t); pendingSaves.delete(key); }
}
function flushPresetSaves() {
  for (const [key, p] of [...pendingSaves]) { clearTimeout(p.t); pendingSaves.delete(key); p.run(); }
}
// …AND BEFORE THE PAGE GOES: a tab closed inside the debounce kept nothing.
window.addEventListener("pagehide", () => flushPresetSaves());
/// TWO STATES ARE THE SAME STATE WHATEVER ORDER THEIR KEYS CAME IN.
///
/// `JSON.stringify` is key-ORDER sensitive, and the order is whatever order the
/// producer happened to assign in. `snapshotOpt()` and `blankOpt()` build the
/// identical object and spell it `…modes, evos…` and `…evos, modes…`, so a
/// plain stringify comparison of the two is false for every input — which made
/// the "is this still untouched" guard below always answer "touched" and gave a
/// search preset to anyone who so much as opened the optimizer tab.
const canon = (v) => {
  if (Array.isArray(v)) return v.map(canon);
  if (v && typeof v === "object") {
    return Object.fromEntries(Object.keys(v).sort().map((k) => [k, canon(v[k])]));
  }
  return v;
};
const sameState = (a, b) => JSON.stringify(canon(a)) === JSON.stringify(canon(b));

/// THE LIVE BUILD AS IT STANDS WITH NOTHING OWNED, recorded at the two moments
/// a blank is applied — the boot of a weapon nobody has built for, and the
/// delete of the last build.
///
/// It exists because `markPresetDirty` is NOT only an edit: `refreshPanel`
/// calls it, and `refreshPanel` runs from `renderMods`, `restoreState` and
/// `applyWeapon`. So with birth-on-edit, a render was enough to create a build
/// — and deleting your last one grew it straight back, on the re-render that
/// the delete itself caused.
///
/// It cannot be `sameState(snapshotState(), blankBuildState())`, which was the
/// first attempt: `blankBuildState()` is SPARSE — `slots: []`, `evoSel: {}`, no
/// mode, no valence, every one of them meaning "the weapon's own default" for
/// `restoreState` to fill in — while `snapshotState()` is the dense realized
/// state. They describe the same build and never compare equal.
///
/// So the blank is realized ONCE, through the same path every other state goes
/// through, and remembered. Derived rather than declared: an axis added
/// tomorrow is in the snapshot on the day it is added, and nobody has to be
/// told about this variable.
let pristineBuild = null;
const notePristineBuild = () => { pristineBuild = JSON.stringify(canon(snapshotState())); };
const buildIsUntouched = () =>
  pristineBuild !== null && JSON.stringify(canon(snapshotState())) === pristineBuild;

function markPresetDirty() {
  if (presetApplying) return;
  presetSaveTimer = deferSave("builds", () => {
    if (presetApplying) return;
    // An official build is not written — same rule as the official scenario,
    // and enforced in the same place. Auto-save is what would otherwise make
    // read-only a suggestion.
    if (officialBuildActive()) return;
    const ps = loadPresetList(BUILDS);
    // A PRESET IS BORN ON THE FIRST EDIT. Nothing is
    // auto-created when a weapon is opened, so this is where "a build you own"
    // begins to exist — the moment you change something.
    //
    // THE TRIGGER IS THE EXISTING DIRTY MARK AND NOT A LIST OF WHAT COUNTS.
    // Every control that edits a build already calls `markPresetDirty`, so an
    // axis added tomorrow creates a preset on the day it is added, by nobody.
    // A hand-kept list of "meaningful" edits would have to be maintained, and
    // this repo has the receipts for what happens to those — `BUILD_AXES`
    // exists because four separate hand lists each went stale once. The cost is
    // that a stray click creates a build, and that build is one row you can see
    // and delete; a missed edit is one you cannot see at all.
    if (!activePreset) {
      // Reaching here is not evidence of an edit — see `pristineBuild`.
      if (buildIsUntouched()) return;
      const name = newPresetName(ps);
      ps.push({ name, savedAt: Date.now(), state: snapshotState() });
      storePresetList(BUILDS, ps);
      activePreset = name;
      localStorage.setItem(presetActiveKey(BUILDS), name);
      renderPresetBar();
      return;
    }
    const at = ps.findIndex((p) => p.name === activePreset);
    if (at < 0) return;
    ps[at] = { ...ps[at], savedAt: Date.now(), state: snapshotState() };
    storePresetList(BUILDS, ps);
  }, 400);
}

let scenarioSaveTimer = null;
let gainRefreshTimer = null;
// The scenario's own auto-save. Same contract as the build's — the editor IS
// the preset — but a different collection, because a build is tested against
// several fights and each of them is worth keeping.
/// CHANGING THE FIGHT IS ONE SEQUENCE — write, mark both collections dirty,
/// redraw — and a caller that performs three of the four leaves a page that
/// disagrees with what is stored. The enemy's own rule rides along, because it
/// is a property of the FIGHT rather than of any one menu.
function setScenarioFields(patch) {
  writeScenarioFields(patch);
  markPresetDirty(); markScenarioDirty();
  renderSim();
}

/// WRITING A FIGHT FIELD IS ONE DECISION, shared by every field control and
/// the agent door — a copy per caller is how a door-set Tenno field once left
/// the panel resolving against the old player. A null DELETES the key: an
/// absent key is what the server reads as "unset", a null is not.
function writeScenarioFields(patch) {
  for (const [k, v] of Object.entries(patch)) {
    if (v === null) delete sim[k]; else sim[k] = v;
  }
  // An explicit "yes, Eximus" cannot follow you onto a unit that has no Eximus
  // variant — the fight would be refused. Dropping it back to null hands the
  // new target its OWN default, which is the elite one wherever there is one.
  // An explicit "no" is legal everywhere and is kept.
  if ("enemy" in patch && sim.eximus === true) {
    const picked = allEnemies().find((e) => e.id === sim.enemy);
    if (!(picked && picked.can_be_eximus)) sim.eximus = null;
  }
  // A TENNO field changes what the BUILD is worth, not just what the fight
  // looks like — the panel resolves against the player, so it is asked again.
  if (Object.keys(patch).some((k) => TENNO_KEYS.includes(k))) refreshPanel();
}

function markScenarioDirty() {
  if (presetApplying) return;
  scenarioSaveTimer = deferSave("scenarios", () => {
    if (!activeScenario || presetApplying) return;
    // NO BIRTH-ON-EDIT HERE, deliberately. The fight always resolves to
    // something — an official ruler when you own nothing — and a ruler is
    // PINNED, so there is no edit to be the first one. A scenario of your own
    // starts at "+ new", which is the only way to leave a ruler at all.
    // THE OFFICIAL SCENARIO IS NOT WRITTEN. Auto-save is what would otherwise
    // make "read-only" a lie: every edit debounces into the active preset, so
    // a disabled control is a suggestion and this line is the rule. It is also
    // why the official one is not in localStorage at all — there is nothing
    // here to write into even if this check were removed.
    if (officialScenarioActive()) return;
    const ps = loadPresetList(SCENARIOS);
    const at = ps.findIndex((p) => presetId(p) === activeScenario);
    if (at < 0) return;
    ps[at] = { ...ps[at], savedAt: Date.now(), state: snapshotScenario() };
    storePresetList(SCENARIOS, ps);
  }, 400);
  // ...and the QUICK CALC, which measures under a scenario and therefore goes
  // stale when one is edited. It waits for the same debounce because the scan
  // resolves the SAVED preset over `sim` — refreshing before the write would
  // re-measure the old fight. `ensureGains` re-checks the key, so an edit the
  // chosen scenario does not use costs nothing.
  clearTimeout(gainRefreshTimer);
  gainRefreshTimer = setTimeout(refreshGains, 450);
}

