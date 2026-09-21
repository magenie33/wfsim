// ---- UNDO — Ctrl+Z across every preset collection ----------------------
//
// Presets AUTO-SAVE, which is exactly what makes a slip expensive: a cleared
// tier, a deleted preset, a mis-aimed import is written the instant it
// happens and there is no save button to not press. Every
// collection — builds, scenarios, searches, rivens — writes through
// `storePresetList`, so that one call is where a "before" can be taken, and
// one stack covers the whole page rather than four.
//
// What is remembered is the whole COLLECTION plus which preset was active,
// not a field-level diff: a delete and an edit are then the same kind of
// event, and undoing either is a plain write-back.
const UNDO_LIMIT = 60;
// Two consecutive EDITS to the same collection within this window are ONE
// step — the auto-save fires per settled edit, and a Ctrl+Z that walked back
// through every keystroke would not be an undo, it would be a rewind. Adding,
// deleting, renaming or importing a preset never coalesces, however fast it
// follows: those are the slips worth one Ctrl+Z each, and folding a delete
// into the edit before it would undo both or neither.
const UNDO_COALESCE_MS = 900;
let undoStack = [], redoStack = [], undoSuspended = false;

const presetSnapshotOf = (d, w) => ({
  domain: d, weapon: w,
  list: localStorage.getItem(presetListKey(d, w)),
  active: localStorage.getItem(presetActiveKey(d, w)),
  at: Date.now(),
});

function recordUndo(d, w, next) {
  if (undoSuspended) return;
  const before = presetSnapshotOf(d, w);
  if (before.list === null) return;          // nothing existed to go back to
  // A no-op write is not a step. Switching weapons re-applies and re-saves
  // every collection, and a stack full of those would make the first Ctrl+Z
  // do nothing visible.
  if (before.list === JSON.stringify(next)) return;
  const names = (l) => (l || []).map((p) => p.name).join("\u0000");
  let structural = true;
  try { structural = names(JSON.parse(before.list)) !== names(next); } catch (_) {}
  const top = undoStack[undoStack.length - 1];
  // Same collection, still mid-gesture, same presets: keep the OLDER "before".
  if (!structural && top && top.domain === d && top.weapon === w
      && before.at - top.at < UNDO_COALESCE_MS) {
    top.at = before.at;
    return;
  }
  undoStack.push(before);
  if (undoStack.length > UNDO_LIMIT) undoStack.shift();
  redoStack = [];   // a new edit forks the timeline
}

// Each domain's "make the page show this again". The bars already own these
// three operations; this is the same trio the preset bar is built from.
function presetDoc(d) {
  if (d === BUILDS) return {
    setActive: (n) => { activePreset = n; },
    apply: (st) => restoreState(st, presetWeapon()),
    rerender: renderPresetBar,
  };
  if (d === SCENARIOS) return {
    setActive: (n) => { activeScenario = n; },
    apply: applyScenario,
    rerender: renderScenarioBar,
  };
  if (d === OPT_DOMAIN) return {
    setActive: (n) => { activeOptPreset = n; },
    apply: applyOptPreset,
    rerender: renderOptPresetBars,
  };
  if (d === WF_BUILDS) return {
    setActive: (n) => { wfActive = n; },
    apply: wfApply,
    rerender: renderWfPresetBar,
  };
  if (d === COMP_BUILDS) return {
    setActive: (n) => { compActive = n; },
    apply: compApply,
    rerender: renderCompPresetBar,
  };
  if (d === OPS) return {
    setActive: (n) => { opActive = n; },
    apply: opApply,
    rerender: renderOpPresetBar,
  };
  if (d === RIVENS) return {
    setActive: (n) => { activeRiven = n; },
    // An undo can land on a collection that is now empty (the last riven
    // deleted) — renderRivens is the one that decides between the list and
    // the editor, so it does the applying too.
    apply: () => { riven = null; },
    rerender: () => { renderRivens(); pruneDanglingRivens(); },
  };
  return null;
}

// Write a remembered collection back and make the page reflect it.
function restorePresetSnapshot(s) {
  undoSuspended = true;
  try {
    if (s.list === null) localStorage.removeItem(presetListKey(s.domain, s.weapon));
    else localStorage.setItem(presetListKey(s.domain, s.weapon), s.list);
    if (s.active === null) localStorage.removeItem(presetActiveKey(s.domain, s.weapon));
    else localStorage.setItem(presetActiveKey(s.domain, s.weapon), s.active);
    // A step taken on ANOTHER weapon is restored in storage but not applied:
    // yanking the editor to a weapon the user has since left would be a
    // second surprise on top of the one being undone.
    if (s.weapon !== undoOwner(s.domain)) return;
    const doc = presetDoc(s.domain);
    const list = JSON.parse(s.list || "[]");
    // WHAT NAMES AN ENTRY is its name in every collection but one: a riven's
    // identity is its own id, because its name is a label a player edits.
    const key = (p) => (s.domain === RIVENS ? p && p.id : p && p.name) || "";
    const active = list.find((p) => key(p) === s.active) || list[0];
    if (!doc || !active) return;
    doc.setActive(key(active));
    whileApplying(() => doc.apply(active.state));   // a restore is not an edit
    doc.rerender();
  } finally {
    undoSuspended = false;
  }
}

// The stack is ONE timeline, but a button lives on ONE collection, so it acts
// on that collection's most recent step: undo has to be visible, not only a
// shortcut. Clicking ↶ on the riven toolbar undoing a
// scenario edit would be the shortcut's behaviour wearing a button's clothes.
// Ctrl+Z stays global — that is what a global key should mean.
//
// Filtering is safe because every entry is a whole snapshot of ONE collection:
// restoring an older one for a domain does not depend on what other domains
// did in between.
const lastIn = (stack, d, w) => {
  for (let i = stack.length - 1; i >= 0; i--) {
    if (stack[i].domain === d && stack[i].weapon === w) return i;
  }
  return -1;
};
/// WHOSE COLLECTION A DOMAIN IS: a weapon's, or the open Warframe's.
const undoOwner = (d) => (d === WF_BUILDS ? (wf ? wf.frame : "")
  : d === COMP_BUILDS ? (comp ? comp.companion : "") : presetWeapon());
const canUndoIn = (d) => lastIn(undoStack, d, undoOwner(d)) >= 0;
const canRedoIn = (d) => lastIn(redoStack, d, undoOwner(d)) >= 0;

function stepIn(from, to, d, label) {
  const w = undoOwner(d);
  const i = lastIn(from, d, w);
  if (i < 0) return;
  const step = from.splice(i, 1)[0];
  to.push(presetSnapshotOf(d, w));
  restorePresetSnapshot(step);
  presetToast(`${tr(label)} · ${tr(PRESET_LABELS[d] || d)}`);
}
const undoIn = (d) => stepIn(undoStack, redoStack, d, "undone");
const redoIn = (d) => stepIn(redoStack, undoStack, d, "redone");

// The pair of buttons, for any bar that wants to show them.
const undoButtons = (d) =>
  `<span class="pundo">` +
  `<button class="pop pundo-u" ${canUndoIn(d) ? "" : "disabled"} title="${escHtml(tr("undo (Ctrl+Z)"))}">↶</button>` +
  `<button class="pop pundo-r" ${canRedoIn(d) ? "" : "disabled"} title="${escHtml(tr("redo (Ctrl+Shift+Z)"))}">↷</button>` +
  `</span>`;

// Wire them wherever they were drawn.
function wireUndoButtons(host, d) {
  const u = host.querySelector(".pundo-u"), r = host.querySelector(".pundo-r");
  if (u) u.onclick = (e) => { e.stopPropagation(); undoIn(d); };
  if (r) r.onclick = (e) => { e.stopPropagation(); redoIn(d); };
}

function undoPreset() {
  const step = undoStack.pop();
  if (!step) return presetToast(tr("nothing to undo"));
  redoStack.push(presetSnapshotOf(step.domain, step.weapon));
  restorePresetSnapshot(step);
  presetToast(`${tr("undone")} · ${tr(PRESET_LABELS[step.domain] || step.domain)}`);
}

function redoPreset() {
  const step = redoStack.pop();
  if (!step) return presetToast(tr("nothing to redo"));
  undoStack.push(presetSnapshotOf(step.domain, step.weapon));
  restorePresetSnapshot(step);
  presetToast(`${tr("redone")} · ${tr(PRESET_LABELS[step.domain] || step.domain)}`);
}

const PRESET_LABELS = {
  "builder-builds": "Builds",
  "simulator-scenarios": "Scenarios",
  optimizer: "Searches",
  rivens: "Rivens",
  warframes: "Builds",
  operators: "Operator builds",
};

// Inline feedback, never a native dialog (those are blocked in the owner's
// browser). It has to say WHICH collection moved: the shortcut is global and
// the slip may have been two tabs ago.
let toastTimer = null;
function presetToast(msg) {
  let el = $("toast");
  if (!el) {
    el = document.createElement("div");
    el.id = "toast";
    document.body.appendChild(el);
  }
  el.textContent = msg;
  el.classList.add("on");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove("on"), 2200);
}

// Ctrl/Cmd+Z, and Ctrl+Shift+Z / Ctrl+Y to come back. Never while a text
// field has focus: there the browser's own undo is the one being asked for,
// and stealing it would make renaming a preset a trap.
document.addEventListener("keydown", (e) => {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
  const k = e.key.toLowerCase();
  if (k !== "z" && k !== "y") return;
  // The event's own target first: that is where a real keystroke lands, and
  // it is true even when the page itself does not hold focus.
  const el = (e.target && e.target.nodeType === 1 ? e.target : null) || document.activeElement;
  if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
  e.preventDefault();
  if (k === "y" || e.shiftKey) redoPreset(); else undoPreset();
});


// One-time move of the pre-module storage keys.
(function migratePresetKeys() {
  try {
    // Targets are the WEAPON-LESS keys of the previous scheme; migrateWeaponScope()
    // below moves those onto a weapon once META has told us which one.
    const moves = {
      "wfsim-presets": "wfsim-presets-builder-builds",
      "wfsim-active-preset": "wfsim-preset-active-builder-builds",
      "wfsim-opt-mods-presets": "wfsim-presets-optimizer-mods",
      "wfsim-opt-arc-presets": "wfsim-presets-optimizer-arcanes",
      "wfsim-opt-evo-presets": "wfsim-presets-optimizer-evolutions",
    };
    Object.entries(moves).forEach(([from, to]) => {
      const v = localStorage.getItem(from);
      if (v !== null && localStorage.getItem(to) === null) localStorage.setItem(to, v);
      localStorage.removeItem(from);
    });
    const oldActives = JSON.parse(localStorage.getItem("wfsim-opt-active") || "null");
    if (oldActives) {
      Object.entries({ mods: "optimizer-mods", arcs: "optimizer-arcanes", evos: "optimizer-evolutions" }).forEach(([k, d]) => {
        const to = "wfsim-preset-active-" + d;
        if (oldActives[k] && localStorage.getItem(to) === null) localStorage.setItem(to, oldActives[k]);
      });
      localStorage.removeItem("wfsim-opt-active");
    }
  } catch (_) {}
})();

// One-time rewrite of the EVOLUTION ids that dropped their ad-hoc
// abbreviations for full weapon names. Saved builds store the per-tier selection and the
// optimizer's scope stores its option sets, so both collections carry ids
// that would otherwise silently stop resolving.
(function migrateEvolutionIds() {
  try {
    const fix = (id) => (typeof id === "string"
      ? id.replace(/^dt_/, "dual_toxocyst_").replace(/^lae_/, "laetum_")
      : id);
    const rewriteKeys = (obj) => {
      if (!obj || typeof obj !== "object") return obj;
      const out = {};
      for (const [k, v] of Object.entries(obj)) out[fix(k)] = v;
      return out;
    };
    // Build presets: state.evoSel = { tier: id|null }. The domain names
    // are literals here on purpose: this block runs BEFORE the BUILDS
    // constant is initialised, and a temporal-dead-zone throw would be
    // swallowed by the catch, silently skipping the migration.
    const buildsDomain = "builder-builds";
    const builds = loadPresetList(buildsDomain);
    let touched = false;
    builds.forEach((p) => {
      const sel = p.state && p.state.evoSel;
      if (!sel) return;
      Object.keys(sel).forEach((t) => {
        const nv = fix(sel[t]);
        if (nv !== sel[t]) { sel[t] = nv; touched = true; }
      });
    });
    if (touched) storePresetList(buildsDomain, builds);
    // Optimizer evolution presets: state.evos = { tier: { id: mark } }
    const evoDomain = "optimizer-evolutions";
    const evos = loadPresetList(evoDomain);
    let touched2 = false;
    evos.forEach((p) => {
      const tiers = p.state && p.state.evos;
      if (!tiers) return;
      Object.keys(tiers).forEach((t) => {
        const before = JSON.stringify(tiers[t]);
        tiers[t] = rewriteKeys(tiers[t]);
        if (JSON.stringify(tiers[t]) !== before) touched2 = true;
      });
    });
    if (touched2) storePresetList(evoDomain, evos);
  } catch (_) {}
})();

// The builder module's build presets — domain "builder-builds". A build
// preset captures the WHOLE configuration: weapon, mod slots (mod id +
// polarity + rank), arcane + rank, and the per-tier evolution selection.
const BUILDS = "builder-builds";

/// WHAT A BUILD CONSISTS OF, declared once.
///
/// Five things produce a build state — the live page, "+ new", a board row, a
/// share link, an optimizer result — and `restoreState` fills a MISSING axis
/// with the weapon's own default, which is right for a blank build and for a
/// preset written before an axis existed. That is also why forgetting an axis
/// is invisible: a producer that meant "the default" and one that never heard
/// of the axis hand over the same object.
///
/// The same bug arrived FOUR times — `mode` missing from the board submission,
/// `valence` from the worker's table, both from the share tuple, `valence` from
/// the optimizer's "+ add", which a player measured at 22.34 KPM against 17.44.
/// So every producer NAMES every axis, `undefined` stays a legal value meaning
/// "the weapon's default", and adding a row here breaks all five at once.
/// …and the LIST itself is not declared here. `engine::board::builds::BUILD_AXES` is
/// the one place a build's axes are named, and it arrives in `/api/meta`; this
/// table only says which of THIS page's state keys carry each of them, because
/// the spellings are the page's own and renaming them would migrate every
/// stored preset. `scripts/check_build_axes.mjs` asserts the two agree, so an
/// axis added in Rust cannot stay invisible here.
const BUILD_STATE_KEYS = [
  { axis: "mods", keys: ["slots"] },
  { axis: "evolutions", keys: ["evoSel"] },
  { axis: "arcanes", keys: ["arcane"] },
  { axis: "arcane_ranks", keys: ["arcaneRank"] },
  { axis: "mode", keys: ["mode"] },
  { axis: "valence", keys: ["valence"] },
  { axis: "assembly", keys: ["assembly"] },
  // A RIVEN IS A MOD: its id sits in a slot like any other, and the item
  // itself lives in its own collection rather than inside a build. So the axis
  // is covered, by `slots`, and saying so is the point — an axis with no entry
  // at all is the thing the check is looking for.
  { axis: "rivens", keys: ["slots"] },
  // WHO HOLDS IT: `null` for the unbuilt Prototype, `{frame}` for a frame with
  // no build, `{frame, preset}` for a link to one of its saved builds.
  { axis: "wielder", keys: ["wielder"] },
];
const BUILD_AXES = [...new Set(BUILD_STATE_KEYS.flatMap((a) => a.keys))];

/// A build state, from a producer that has to account for every axis.
///
/// It THROWS rather than filling in, because there is no data that can reach
/// it: a missing axis is a producer somebody wrote without the table in front
/// of them, so it fails on the first click of the session that introduced it
/// (`scripts/check_opt_to_build.mjs` is the one that forces it).
function buildState(weapon, axes) {
  const missing = BUILD_AXES.filter((k) => !(k in axes));
  if (missing.length) {
    throw new Error(`build state is missing ${missing.join(", ")} — see BUILD_AXES`);
  }
  return { weapon, ...axes };
}

function snapshotState() {
  return buildState($("weapon").value, {
    evoSel: { ...evoSel },
    arcane: arcanes,
    arcaneRank: arcaneRanks,
    slots: slots.map((s) => ({ mod: s.mod, pol: s.pol, rank: s.rank })),
    mode,
    // The VALENCE, for the same reason `mode` is here: it is part of what this
    // build IS, and two builds of one weapon may differ only in it.
    valence: { ...valence },
    // THE PARTS, for the same reason: on a Kitgun they are the stat line, and
    // `null` on everything else, which is what "this weapon has none" means.
    assembly: assembly ? { ...assembly } : null,
    // AS WRITTEN, NOT AS RESOLVED: an unset link stays unset so it keeps
    // following the frame's first preset, and the DEFAULT is stored by name. The
    // unset Prototype is null, as it always was, so a preset written before it
    // was a frame reads as unchanged.
    wielder: (buildWielder.frame === PROTOTYPE_ID || isHostId(buildWielder.frame)) && !buildWielder.preset
      ? null : { ...buildWielder },
    // NO `sim` FIELD. A build carrying a snapshot of the fight is a snapshot
    // `restoreState` then applies, so picking a build silently rewrites the
    // scenario you are working in. The scenario is INDEPENDENT: nothing
    // outside `simulator-scenarios` writes it.
    //
    // Nothing is lost. "What this build was last measured under" was never
    // this field's job — `lastResult.key` is that record, it lives outside
    // `state`, and it is what makes a stale result show as stale.
  });
}

/// Create any BOARD riven this state wears that the reader does not have yet.
///
/// Returns nothing and writes at most once: the riven's name is derived from
/// its shape, so a row taken twice reuses the first copy rather than stacking
/// `… 2`, `… 3` behind it — which is the whole difference between this and the
/// share link's import, where two links legitimately carry two different
/// people's rivens that happen to share a name.
function materialiseBoardRivens(st) {
  const want = (st.slots || [])
    .map((s) => s && s.mod)
    .filter((id) => isRivenId(id) && boardRivenDefs[id]);
  if (!want.length) return;
  const ps = loadPresetList(RIVENS);
  let added = 0;
  for (const id of want) {
    // THE BOARD ROW'S RIVEN KEEPS THE IDENTITY THE BUILD ALREADY NAMES. Its id
    // is derived from the SHAPE (`boardRivenName`), so it is deterministic —
    // which is what makes taking the same row twice reuse the first copy, and
    // what lets the slot the build arrived with go on resolving.
    const key = id.slice(RIVEN_PREFIX.length);
    const state = boardRivenState(boardRivenDefs[id]);
    const had = ps.find((p) => p.id === key);
    // A COPY WHOSE ROLLS ARE NOT THE ROW'S IS CORRECTED, not reused: a name
    // that once dropped the corner filed a deep roll under the god roll's id.
    const rolls = (x) => JSON.stringify([...((x && x.bonuses) || []), (x && x.malus) || null]
      .map((b) => (b ? [b.id, b.roll] : null)));
    if (had && rolls(had.state) === rolls(state)) continue;
    if (had) had.state = { ...had.state, bonuses: state.bonuses, malus: state.malus };
    else ps.push({ id: key, name: key, savedAt: Date.now(), state });
    added++;
  }
  if (!added) return;
  storePresetList(RIVENS, ps);
  refreshRivenNames();
}

// Apply a saved state. `weapon` is the weapon it belongs to — pass it and the
// payload's own `st.weapon` is IGNORED.
//
// That parameter is the whole anti-crossing design. A preset's owner is already
// decided by its storage key (`wfsim-presets-<weapon>-<domain>`), so carrying a
// weapon inside the payload too gave the same fact two homes — and every
// crossing bug was those two disagreeing: a payload saying `laetum` under Dual
// Toxocyst's key dragged the editor (and the URL, below) to Laetum. Repairing
// the stored data only chased the symptom; not READING it makes crossing
// structurally impossible, whatever a payload happens to contain.
//
// Only the language-switch stash omits the argument: it restores the whole page
// including which weapon was open, so there the payload IS the authority.
function restoreState(st, weapon) {
  const w = weapon || (st && st.weapon);
  if (!st || !weaponInfo(w)) return;
  $("weapon").value = w;
  // Keep the route honest when the restore changes weapon (the stash case);
  // for a preset `w` is already the current weapon, so this is a no-op.
  // …EXCEPT ON A KITGUN, whose two slots are one page: an address naming the
  // sibling is this entry's address too. Rewriting it is how a board landing
  // late (`ensureWeaponBoard` re-runs `initPresets`) walked the Slot control off
  // `/weapons/<Chamber>`.
  const here = routeWeaponId();
  const siblingsPage = here && here !== w && slotSibling(here) === w;
  if (!document.querySelector(".config-page").hidden && !siblingsPage) {
    history.replaceState(null, "", weaponModPath(w));
  }
  applyWeapon(w, null); // resets pool/innate/visibility
  // A BOARD ROW'S RIVEN IS CREATED HERE, and only here — the moment a build
  // that wears one is actually taken. Without it the loop below would drop the
  // slot as "an id gone from the pool" and hand back a seven-mod build that
  // scores nothing like the row it came from.
  //
  // IDEMPOTENT: the name is derived from the shape, so taking the same row
  // twice finds the riven it made the first time.
  materialiseBoardRivens(st);
  (st.slots || []).forEach((s, i) => {
    if (i >= slots.length) return;
    slots[i].mod = s.mod && modById(s.mod) ? s.mod : null; // drop ids gone from the pool
    slots[i].pol = s.pol ?? null;
    slots[i].rank = s.rank ?? null;
  });
  evoSel = { 1: null, 2: null, 3: null, 4: null, ...(st.evoSel || {}) };
  // ONTO A DEFAULT, never onto whatever the last build was playing. A preset
  // written before this field existed is played the way the arsenal plays it,
  // which is exactly what the board's own migration does with a mode-less
  // submission — and what keeps a builtin board build showing the mode it was
  // MEASURED in rather than the one you happened to be in.
  mode = defaultMode(w, st.mode);
  // ONTO A DEFAULT, never onto the last build's — a valence is a statement
  // about one weapon and nothing crosses between them. `defaultValence` also
  // drops an element this weapon's spec does not offer, which is what a preset
  // copied across weapons carries.
  valence = defaultValence(w, st.valence);
  // ONTO THIS WEAPON'S DEFAULT, never onto the last build's parts — a grip
  // belongs to one slot and a preset copied across weapons carries the other
  // one's. `defaultAssembly` also drops a part this entry cannot take, which is
  // the same repair the server does on the wire.
  assembly = defaultAssembly(w, st.assembly);
  // ONTO THIS WEAPON'S HANDS: a frame that cannot hold it is replaced, and a
  // preset written before the wielder existed is held by the default.
  buildWielder = defaultWielder(w, st.wielder);
  // THE RANKS FOLLOW THE ARCANE, not the index — see `seatArcanes`. Reading
  // them positionally would put a Kitgun arcane's rank on the ordinary seat
  // the moment one of them moved.
  const seat = seatArcanes(w, st.arcane);
  arcanes = seat.ids;
  const rawRanks = Array.isArray(st.arcaneRank) ? st.arcaneRank
    : st.arcaneRank == null ? [] : [st.arcaneRank];
  arcaneRanks = seat.from.map((src) => (src < 0 ? null : rawRanks[src] ?? null));
  // The scenario is NOT restored: it belongs to `simulator-scenarios` and a
  // build has no opinion about it. An old preset may still carry `st.sim`;
  // it is ignored rather than migrated, because reading it back is the exact
  // behaviour this removed.
  // A BOARD ROW IS A BUILD, NOT A LAYOUT: it carries mods and no polarities,
  // so restoring it verbatim shows a legal build as an impossible one — full
  // drain, 91/60 in red. The cheapest legal layout is planned on the spot.
  //
  // IT LIVES HERE because TWO callers restore a build — the build bar, and
  // `initPresets` on boot — so a plan living in the bar's `apply()` alone
  // shows the wrong Forma on a page whose active build is a benchmark build,
  // until you click something. Both
  // callers set the active preset BEFORE restoring, which is what makes
  // the question answerable here.
  //
  // Nothing of yours is at risk: a benchmark build is read-only and has no
  // hand-set polarity to overwrite.
  if (officialBuildActive()) autoForma({ alone: true }).then((r) => { if (r) renderMods(); });
  renderAssembly();
  renderWielder(); renderWeaponName();
  renderMods(); renderArcanes(); renderEvo(); renderMode(); renderValence(); renderSim(); refreshPanel();
  renderStoredSimResult(); // the simulator shows THIS preset's last test
}

// Kill score PER MINUTE. The score is whole kills plus the fraction of the
// current target's pool already drained, so it grows with the engagement —
// dividing by the clock is what makes two runs of different length
// comparable, exactly as DPS does for damage.
const kpm = (score, duration) => (duration > 0 ? ((score || 0) * 60) / duration : 0);

/// WHAT A SCENARIO IS JUDGED BY — resolved against the table `/api/meta`
/// publishes (`engine::rules::metrics`), never asked as "is it dps".
///
/// THAT QUESTION IS THE FAILURE MODE. `metric === "dps" ? … : KPM` reads a
/// third metric as kills per minute — silently, in the units of a different
/// question — and it was written eight times across this file. A metric added
/// to the engine's table reaches the Measure control, the headline's unit, the
/// gain scan's label and the board strip without any of them naming it.
///
/// AN UNKNOWN ID FALLS BACK TO THE DEFAULT rather than to nothing, because this
/// runs while drawing: `parse_fight` refuses one at the door, so anything that
/// reaches here is a metric this build knows or a scenario that cannot run.
const metricOf = (id) => {
  const all = META.metrics || [];
  return all.find((m) => m.id === (id || META.metric_default))
    || all.find((m) => m.id === META.metric_default)
    || METRIC_FALLBACK;
};
/// The run's number IN THAT METRIC. `score` off the wire is kill PROGRESS over
/// the whole engagement, so a per-minute metric turns it into a rate; a metric
/// that is already a rate reads its own field and is left alone.
const metricValue = (m, r) => {
  const raw = (r || {})[m.field] || 0;
  return m.per_minute ? kpm(raw, (r || {}).duration) : raw;
};
/// Its unit, translated. The label is the engine's; the translation is ours.
const metricLabel = (m) => tr(m.label);

const escHtml = (s) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));

// Decimal formatting with a SIGNIFICANCE floor: two
// decimals normally, but a very small number grows decimals until it
// carries 2 significant digits — 0.0085 stays 0.0085 instead of
// collapsing to 0.01. Used for every score/percentage in the results.
// EXACT ZERO keeps the plain "0.00" — only a nonzero-but-tiny value grows
// decimals (log10(0) is -Infinity, so zero must short-circuit first).
const sig2 = (x, min = 2) => {
  const v = Number(x);
  if (!Number.isFinite(v) || v === 0) return (0).toFixed(min);
  const a = Math.abs(v);
  const need = a >= 1 ? min : Math.max(min, 1 - Math.floor(Math.log10(a)));
  return v.toFixed(need);
};
const pct2 = (x) => sig2((Number(x) || 0) * 100) + "%";

/// A SCORE, SPELLED THE WAY THE BOARD SPELLS IT — `data::boards::format_score`,
/// transcribed. Four significant figures with four decimals as the floor, which
/// on anything above 1 is simply four decimals.
///
/// ONE RULE FOR ONE NUMBER. The page rounded a result to two decimals while the
/// ranking published four, so a reader comparing their own run against the row
/// it is about to become was reading two different spellings of the same
/// measurement — and a disagreement of 0.001 looked like agreement.
const fmtScore = (x) => {
  const v = Number(x);
  if (!Number.isFinite(v) || v === 0) return (0).toFixed(4);
  const mag = Math.floor(Math.log10(Math.abs(v)));
  return v.toFixed(Math.min(12, Math.max(4, 3 - mag)));
};

// One-time move of the weapon-LESS preset lists onto a weapon. They were
// written before presets were scoped, so they belong to whatever the user
// was last looking at — which we cannot know. They go to the weapon that
// is active when the migration runs (the one being restored on this load),
// and everything else starts empty.
// The optimizer's three scope domains merged into one on 2026-08-02; the old
// names stay listed so a browser arriving from before the weapon-scope move
// still lands its data where migrateOptPresets can find it.
const PRESET_DOMAINS = ["builder-builds", "optimizer", "optimizer-mods",
  "optimizer-arcanes", "optimizer-evolutions", "simulator-scenarios"];
// The SIMULATOR's collection: one saved fight — enemy, technique, measurement.
// Its own domain because a build is tested against SEVERAL of them, and
// because the marginal-gain scan needs to name which one it ran under. The
// build preset keeps carrying its own `sim` (a build remembers what it was
// last tested with); this is the reusable library beside it.
const SCENARIOS = "simulator-scenarios";
let activeScenario = "";
function migratePresetsToWeaponScope() {
  const w = presetWeapon();
  if (!w) return;
  PRESET_DOMAINS.forEach((d) => {
    // A SHARED DOMAIN IS ALREADY AT ITS FINAL KEY. This migration's whole job is
    // to move a weapon-LESS key under a weapon, which is precisely what the
    // scenario list must not have done to it — it would file the shared list
    // under whichever weapon happened to be open and take it away from every
    // other one, on every load.
    if (isSharedDomain(d)) return;
    [["wfsim-presets-" + d, presetListKey(d, w)],
     ["wfsim-preset-active-" + d, presetActiveKey(d, w)]].forEach(([from, to]) => {
      const v = localStorage.getItem(from);
      if (v === null) return;
      // RESCOPE while moving. The legacy list was global, so its entries carry
      // whatever weapon was current when each was saved — filing them verbatim
      // under `w` leaves a preset that yanks the editor to another weapon the
      // moment it loads. Only the preset LIST needs this; the active-name key
      // is a bare string.
      let out = v;
      if (from.startsWith("wfsim-presets-")) {
        try {
          out = JSON.stringify(JSON.parse(v).map((p) =>
            p && p.state ? { ...p, state: { ...p.state, weapon: w } } : p));
        } catch (_) { /* unparseable: move it as-is, initPresets repairs it */ }
      }
      if (localStorage.getItem(to) === null) localStorage.setItem(to, out);
      localStorage.removeItem(from);
    });
  });
}

// The ACTIVE preset — the one the editor is editing. Never null after
// init: the page always has ≥1 preset and is always editing one (user's
// mental model, 2026-07-28: "if no presets exist, the current state IS
// preset 1"). Restored across reloads.
let activePreset = null;

// A stored buff config outlives the RULE it was written under. A stacking buff
// opens EARNED at zero, and `syncBuffConfig` only seeds an id it has never
// seen — so a scenario saved under an open-at-full-stacks rule keeps the old
// numbers, and the current rule looks broken on exactly the builds that have
// been tested most.
//
// One-time, and it drops the whole map rather than guessing which entries were
// deliberate: `{stacks: 3}` on a 3-stack buff is what "never touched" and
// "chose the maximum" both look like, and there is no third field to tell them
// apart. Re-seeding from the server's defaults is the answer the rule change
// promised; a wrong guess about intent is not.
(function migrateBuffDefaults() {
  const FLAG = "wfsim-buff-defaults-earned";
  try {
    if (localStorage.getItem(FLAG)) return;
    Object.keys(localStorage)
      .filter((k) => /^wfsim-presets-.*-(simulator-scenarios|builder-builds)$/.test(k))
      .forEach((k) => {
        const list = JSON.parse(localStorage.getItem(k));
        if (!Array.isArray(list)) return;
        let hit = false;
        list.forEach((p) => {
          if (p.state && p.state.buffs && Object.keys(p.state.buffs).length) {
            p.state.buffs = {}; hit = true;
          }
          // Builds no longer carry a fight at all — drop the dead copy while
          // we are here rather than leaving it to be misread later.
          if (p.state && p.state.sim) { delete p.state.sim; hit = true; }
        });
        if (hit) localStorage.setItem(k, JSON.stringify(list));
      });
    localStorage.setItem(FLAG, "1");
  } catch (_) { /* a browser with no storage has nothing to migrate */ }
})();

function initPresets() {
  migratePresetsToWeaponScope();
  let ps = loadPresetList(BUILDS);
  // (The old 300-run migration lived here. It rewrote `state.sim.runs` inside
  // BUILD presets — a field nothing reads any more, because a build no longer
  // carries a copy of the fight.)
  // NOTHING IS AUTO-CREATED. A blank "build 1" per weapon opened leaves a
  // reader who browsed forty weapons owning forty builds they never made,
  // which stops being harmless the moment one page lists everything you own:
  // the page's whole answer becomes "everything", the same as no answer.
  //
  // THE RULE IT LOOKS LIKE IT BREAKS, AND DOES NOT: "the modules always have a
  // state" is about the LIVE state, which still always exists — with nothing
  // stored the builder opens on `blankBuildState()` and the simulator on an
  // official ruler, a BUILTIN that was never in this list. What is optional is
  // the SAVED entry, the contract customs have always had. A preset appears on
  // the first EDIT instead (`markPresetDirty`), so the collection means "what
  // you have worked on".
  //
  // What the blank seed was FOR is not lost: a weapon opened for the first time
  // must be a bare weapon rather than the live state of the weapon you just
  // left, and `blankBuildState()` is still what is applied below.
  const sc = loadPresetList(SCENARIOS);
  // Resolved against the JOINT list, so the official scenario can be the one
  // you left open — it is a scenario like any other to everything downstream.
  //
  // THE OFFICIAL ONE IS THE DEFAULT. This costs nothing and
  // corrects something that was quietly misleading: `defaultScenario()` is
  // ALREADY the benchmark's fight, field for field — same enemy, level 9999,
  // Steel Path, 180 s, 1000 runs, kpm, aiming, infinite ammo. "scenario 1" was
  // the ruler wearing a private name, so its number could not be compared with
  // anyone's and could not reach the board, for no reason a player could see.
  //
  // Landing on the official one instead means a first number is a COMPARABLE
  // number. Nobody's results move — it is the same fight.
  const lastSc = localStorage.getItem(presetActiveKey(SCENARIOS));
  // THE ID, like every other write to this pointer. Storing a builtin's NAME
  // here left the boot state spelled differently from every later selection —
  // and `presetId(p) === activeScenario` is false against a name, so readers
  // that resolve by id fell through to the first ruler until you touched the
  // bar once.
  activeScenario =
    presetId(scenarioNamed(lastSc))
    || presetId(builtinScenarios()[0])
    || presetId(sc[0]);
  localStorage.setItem(presetActiveKey(SCENARIOS), activeScenario);

  const here = presetWeapon();
  const last = resolveBoardActive(localStorage.getItem(presetActiveKey(BUILDS)));
  activePreset = presetId(buildNamed(last)) || presetId(ps[0]);
  localStorage.setItem(presetActiveKey(BUILDS), activePreset);
  noteBoardActive(activePreset);
  // Applied under THIS weapon, never the payload's — a preset filed here
  // belongs here by definition.
  whileApplying(() => {
    // NO PRESET IS A REAL STATE, so this is the one place that says what it
    // means: a bare weapon. `blankBuildState()` was already the answer — it
    // was simply being written to disk first.
    restoreState((buildNamed(activePreset) || {}).state || blankBuildState(), here);
    if (!activePreset) notePristineBuild();
    // THE FIGHT, from its own collection — never from inside the build
    // preset, which is how picking a build comes to change the scenario. It
    // comes from the active `simulator-scenarios` entry, and this is the only
    // place the live scenario is seeded on load or on a weapon switch.
    // The scenario cannot actually reach the fallback — an official ruler is a
    // BUILTIN and there is always one — but it is written the same way so that
    // the two collections have one rule between them rather than one each.
    applyScenario((scenarioNamed(activeScenario) || {}).state || defaultScenario());
  });
  renderPresetBar();
  lockOfficialBuild();
}

