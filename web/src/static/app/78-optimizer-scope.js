// ---- Optimizer: scope (mods/arcanes/evolutions) → top-10 builds ---------
// Mod scope is a 3-state cycle (off / pool / required); the client estimates
// the candidate count (no cap — the server funnel culls large spaces).
function nChooseK(n, k) {
  if (k < 0 || k > n) return 0;
  k = Math.min(k, n - k);
  let r = 1;
  for (let i = 0; i < k; i++) r = (r * (n - i)) / (i + 1);
  return r;
}

// ---- scope mutex helpers -----------------------------------------------
// A SINGLE-SLOT group (the exilus slot, the arcane slot, one evolution tier)
// holds either "these are the options" or "it is this one" — never both. The
// two must not BLOCK each other, which in practice is asymmetric: any pool
// mark greys out every req, while a pin still lets you click pool.
//
// Blocking is the wrong answer to a question that has an obvious one. The
// marks are not in conflict, they are two ways of saying what the slot does,
// so the LAST click wins and the group is rewritten to mean it: req clears
// the pools, pool clears the pin. Nothing is refused and the scope never
// lies about itself — the same rule `clearFamMarks` already applies to
// families.
function setSingleSlotMark(map, id, want) {
  if ((map[id] || "off") === want) {   // clicking the ON seg turns it off
    delete map[id];
    return;
  }
  // req pins the slot: every other mark in the group goes, pools included.
  // pool opens it for search: a pin cannot survive that, but other pools can.
  Object.keys(map).forEach((o) => {
    if (o === id) return;
    if (want === "fixed" || map[o] === "fixed") delete map[o];
  });
  map[id] = want;
}

// Family exclusivity is a GAME rule across all 9 slots: req'ing a mod kills
// its family siblings everywhere (e.g. req Primed Pistol Gambit → plain
// Pistol Gambit can be neither pool nor req). The UI greys them out; setting
// a req actively clears conflicting marks so the scope never lies.
function famReqBy(m) {
  if (!m || !m.family) return null;
  const hit = (map) => Object.keys(map).find((id) => map[id] === "fixed" && id !== m.id && (modById(id) || {}).family === m.family);
  return hit(opt.mods) || hit(opt.exilus) || null;
}
function clearFamMarks(id) {
  const m = modById(id);
  if (!m || !m.family) return;
  [opt.mods, opt.exilus].forEach((map) => Object.keys(map).forEach((o) => {
    if (o !== id && (modById(o) || {}).family === m.family) delete map[o];
  }));
}
const reqCountMain = () => Object.values(opt.mods).filter((s) => s === "fixed").length;
// `none` IS NOT A PIN ON A MOD — it is this slot's range saying "searched
// empty" (see `slotRange`), so nothing may draw it as the pinned option.
const exilusPinned = () =>
  Object.keys(opt.exilus).find((id) => id !== "none" && opt.exilus[id] === "fixed") || null;
// The arcane pinned in a given POOL. A weapon with two slots has two
// independent pins — one Primary and one Secondary — so the question only
// means something with a pool attached.
const arcanePinnedIn = (pool) =>
  Object.keys(opt.arcanes).find(
    (id) => opt.arcanes[id] === "fixed" && arcaneSeats(arcaneById(id)).includes(pool),
  ) || null;
/// Options this seat contributes to the search.
///
/// IT READS THE SEAT'S RANGE, which is the same rule the server applies —
/// and it did not: this counted `marked + 1`, on the belief that the empty
/// choice is always reachable, while `parse_optimize` has dropped it beside
/// marked candidates since 2026-08-01. So the estimate over-counted by one per
/// seat on every scope with an arcane in it (found 2026-08-29, while making
/// the range a control).
const arcaneOptionsIn = (pool) => {
  if (arcanePinnedIn(pool)) return 1;
  const marked = Object.keys(opt.arcanes).filter(
    (id) => opt.arcanes[id] === "search" && arcaneSeats(arcaneById(id)).includes(pool),
  ).length;
  if (!marked) return 1;                                   // the empty seat, alone
  return marked + (opt.arcanes[arcaneEmptyId(pool)] === "fixed" ? -marked + 1
    : opt.arcanes[arcaneEmptyId(pool)] === "search" ? 1 : 0);
};
// `none` IS NOT A PIN ON AN OPTION. It is this tier's range saying "searched
// empty" (see `slotRange`), so a row must not draw itself as the pinned one.
const evoPinned = (tier) => {
  const m = opt.evos[tier] || {};
  return Object.keys(m).find((id) => id !== "none" && m[id] === "fixed") || null;
};
/// The REAL marks of a tier — its range's `none` is not one of them.
const evoRealMarks = (tier) => Object.keys(opt.evos[tier] || {}).filter((id) => id !== "none");

/// Does ANY set this tier produces install a rung here?
///
/// THE LADDER KEYS ON THE RANGE, NOT ON THE MARKS, and it has to: a tier set
/// to 0–0 keeps its candidates — going down and back up must not cost the
/// reader what they marked — so "it has marks" is no longer the same question
/// as "it fills the rung". Count the marks instead and a 0–0 tier opens the
/// one above it, whose every set `ladder_prefix` then truncates back: the
/// marks up there would price nothing and the scope would say otherwise.
/// 0–1 DOES open it — half of its sets carry the rung, and the other half
/// being truncated is the ladder working rather than a scope nobody can ask
/// for.
const evoFillsRung = (tier) =>
  evoRealMarks(tier).length > 0 && (opt.evos[tier] || {}).none !== "fixed";

// ---- HOW MANY OF AN AXIS'S SLOTS A CANDIDATE FILLS ----------------------
//
// EVERY AXIS IS THE SAME SHAPE: N slots, an option set, and a range saying how
// many of the slots a searched build must fill. The mods axis is 8 slots and a
// number 0–8; every other axis is ONE slot and 0–0, 0–1 or 1–1, drawn the same
// way because it is the same question.
//
// IT IS DERIVED FIRST AND ADJUSTED SECOND, which keeps it safe: the derived
// answer is what the scope did before this existed — nothing marked is 0–0, a
// mark is 1–1 — so no search grows unless somebody widens it on purpose. That
// matters most on the ARCANE seats, where an empty seat can only ever TIE the
// same build with the arcane in it, so it is ruled out as a DEFAULT and asking
// for it out loud is a different thing.
//
// THE EMPTY CHOICE IS A MARK LIKE ANY OTHER — `none` on the exilus slot and on
// an evolution tier, `none:<pool>` on an arcane seat, which names its seat
// because a weapon can hold two and the marks are one flat map. So the range
// is a VIEW over the option set rather than a second thing to store, and it
// travels in the search preset, the request and the round trip with no field
// of its own anywhere.
const arcaneEmptyId = (pool) => `none:${pool}`;

/// The range a single-slot axis is currently set to, read off its marks.
///
/// `locked` is a range that cannot be adjusted rather than one nobody has: a
/// pinned candidate settles the slot at 1–1, and a slot with no candidates at
/// all is 0–0 with nothing to widen to.
function slotRange(marks, emptyId, ids) {
  const real = [...ids].filter((id) => id !== emptyId && marks[id]);
  const realPinned = real.find((id) => marks[id] === "fixed") || null;
  const realPooled = real.filter((id) => marks[id] === "search");
  // A REAL PIN IS ASKED FIRST, so a stale empty mark cannot outrank it. On the
  // exilus slot and the evolution tiers `setSingleSlotMark` clears the empty
  // mark on the way to a pin, because they share one map; the arcane seats
  // clear it by hand (their group is per seat and excludes it).
  if (realPinned) return { lo: 1, hi: 1, locked: true, why: "pinned" };
  if (marks[emptyId] === "fixed") return { lo: 0, hi: 0, locked: false };
  if (!realPooled.length) return { lo: 0, hi: 0, locked: true, why: "empty" };
  return marks[emptyId] === "search" ? { lo: 0, hi: 1, locked: false } : { lo: 1, hi: 1, locked: false };
}

/// Write a range back onto the marks. The candidates are never touched: going
/// down to 0–0 and back up must not cost the reader what they marked.
function setSlotRange(marks, emptyId, lo, hi) {
  if (hi === 0) marks[emptyId] = "fixed";       // search it unfilled, marks kept
  else if (lo === 0) marks[emptyId] = "search"; // both
  else delete marks[emptyId];                   // always filled
}

/// A SINGLE SLOT'S RANGE on a search axis — the exilus slot, an arcane seat
/// (`key` its pool) or an evolution tier (`key` the tier): whether the search
/// may, must or must not leave it empty.
function setOptRange(axis, key, lo, hi) {
  if (axis === "exilus") {
    setSlotRange(opt.exilus, "none", lo, hi);
    renderOptMods(); renderOptExilus();
  } else if (axis === "arcanes") {
    setSlotRange(opt.arcanes, arcaneEmptyId(key), lo, hi);
    renderOptArcanes();
  } else {
    opt.evos[key] = opt.evos[key] || {};
    setSlotRange(opt.evos[key], "none", lo, hi);
    renderOptEvos();
  }
  updateOptEstimate();
}

/// The row itself. One renderer for four axes, so a change to how a range is
/// stated reaches all of them.
///
/// `max` is the slot count — 8 for the mods axis, 1 for every other — which is
/// the only thing that differs between them on screen.
function slotRangeHtml(key, { label, lo, hi, max = 1, locked = false, note = "" }) {
  const box = (which, v) =>
    `<input type="number" min="0" max="${max}" value="${v}" data-range="${key}" data-end="${which}"${
      locked ? " disabled" : ""}>`;
  return `<div class="oselrow slot-range${locked ? " locked" : ""}" data-range-row="${key}">
    <span class="osellbl">${escHtml(label)}</span>${box("lo", lo)}<span class="osdash">–</span>${box("hi", hi)}
    ${note ? `<span class="opt-size-eff">${escHtml(note)}</span>` : ""}
  </div>`;
}

/// …and its wiring, given the host that was just re-rendered. `onSet` takes a
/// NORMALISED pair: an end pushes the other rather than allowing lo > hi,
/// which is the rule the mods axis has had since the range landed.
function wireSlotRange(host, max, onSet) {
  host.querySelectorAll("input[data-range]").forEach((el) =>
    el.addEventListener("change", () => {
      const row = el.closest("[data-range-row]");
      const get = (w) => Number(row.querySelector(`input[data-end="${w}"]`).value) || 0;
      let lo = Math.max(0, Math.min(max, get("lo")));
      let hi = Math.max(0, Math.min(max, get("hi")));
      if (el.dataset.end === "lo" && lo > hi) hi = lo;
      if (el.dataset.end === "hi" && hi < lo) lo = hi;
      onSet(el.dataset.range, lo, hi);
    }));
}

/// WHICH BUILDER BLOCK EACH SCOPE SECTION IS THE BULK FORM OF.
///
/// The ONLY hand-written half of the alignment below, and it is a mapping
/// rather than an order: `stats-block` and `assembly-block` have no bulk form
/// (a search does not sum a build, and a kitgun's parts are the weapon), so
/// they are simply absent. Everything else — the sequence, the numbers, the
/// names — is read off the builder itself.
const OPT_SCOPE_OF = {
  "mode-block": "opt-modes-sect",
  "mod-block": "opt-mods-sect",
  "arcane-block": "opt-arcanes-sect",
  "evo-block": "opt-evos-sect",
  "element-block": "opt-valence-sect",
};

/// THE OPTIMIZER IS THE BUILDER, IN BULK.
///
/// Every axis here is a question the builder already asks — the difference is
/// that the builder binds a VALUE and the optimizer binds a SET. So the two
/// must read as one page: the same axes, in the same sequence, under the same
/// numbers and the same names. They did not. The optimizer opened on Mods and
/// put Mode fourth, called the builder's "Arcane" block "Arcanes" and its
/// "Evolution" block "Evolutions", and numbered nothing — three chances for a
/// reader to think the two tabs are about different things.
///
/// NOTHING DECLARES THAT ORDER TWICE. This walks the builder's own blocks in
/// DOM order and appends each axis's section as it meets one, stamping the
/// heading from that block's `.n` and `<h2>` — which `applyI18n` has already
/// translated, so the label is the builder's word in the reader's language
/// rather than a second string to keep in step. Reorder a builder block,
/// renumber one, rename one, and the optimizer follows with NO edit: the map
/// above is touched only when an axis is added or removed.
function orderOptScope() {
  const host = $("opt-scope");
  if (!host) return;
  document.querySelectorAll('section.block[data-module="builder"]').forEach((b) => {
    const sect = $(OPT_SCOPE_OF[b.id] || "");
    if (!sect) return;
    // appendChild MOVES it — walking the builder in order therefore leaves
    // this host in the builder's order, whatever it was before.
    host.appendChild(sect);
    const h = sect.querySelector(".axh");
    const n = b.querySelector(".bh .n"), h2 = b.querySelector(".bh h2");
    if (h && n && h2) h.textContent = `${n.textContent.trim()} · ${h2.textContent.trim()}`;
  });
}

/// THE FINAL ROUND'S RUN COUNT — the simulator's Runs control, said again.
///
/// Deliberately the same component in the same words, because it is the same
/// question: how hard do you want to measure, right now, on this machine. It
/// sits OUTSIDE both halves and is saved by neither preset (`OPT_RUNS_KEY`) —
/// the search preset says what to look through, the scenario says what the
/// fight is, and neither of them has ever had an opinion about precision.
///
/// It is TYPED rather than defaulted from the fight. A blank box meaning "the
/// fight's own count" is one control with two readings, which is how a reader
/// ends up unable to say what number the last round actually used.
function renderOptRuns() {
  const box = $("opt-runs-block");
  if (!box) return;
  box.innerHTML =
    `<label title="${escHtml(tr("how many simulations each finalist gets in the last round. Yours, not the search's and not the fight's: it is saved by no preset and pinned by no ruler. A smaller number searches faster and is worth re-measuring in the simulator"))}">${
      escHtml(tr("Final-round runs"))} <input type="number" id="opt-runs" min="1" max="20000" step="10" value="${finalRuns()}"></label>` +
    `<span class="sim-hint">${escHtml(tr("yours, in neither preset — the simulator's Runs is the same setting for the replay"))}</span>`;
  const el = $("opt-runs");
  el.addEventListener("change", () => {
    setFinalRuns(el.value);
    el.value = String(finalRuns());
    updateOptEstimate(); // the planned funnel quotes this number
  });
}

function renderOpt() {
  // Every weapon is optimizable: the scope is built from the weapon's OWN
  // pools (mod class, arcane slot, evolution tiers), so nothing here is
  // weapon-specific. META is the only prerequisite.
  show("opt-block", !!META);
  if (!META) return;
  // A scope for an axis the weapon does not have is a heading over nothing —
  // and worse, an invitation to configure a slot it cannot equip. The same
  // three facts the builder hides its blocks on.
  const w = weaponInfo($("weapon").value) || {};
  const AX = weaponAxes(w.id);
  // The fight's Warframe buffs, read-only. Painted here as well as from
  // `renderOptEnemy`, because arriving on this tab is its own moment: the
  // scenario may have gained a buff while you were in the simulator.
  renderWfBuffs("opt-wfbuffs", true);
  // A WEAPON WITH ONE WAY TO BE FIRED HAS NO AXIS HERE. The builder still
  // STATES its one mode (a fact about the weapon); a search over one option is
  // not a scope, so this section is simply absent — the same rule the exilus,
  // arcane and evolution sections follow.
  orderOptScope();
  renderOptRuns();
  show("opt-modes-sect", modeOpts(w).length > 0);
  show("opt-valence-sect", !!valenceSpec(w.id));
  show("opt-exilus-sect", AX.hasExilus);
  show("opt-arcanes-sect", AX.arcanes.length > 0);
  show("opt-evos-sect", AX.evolutions.length > 0);
  // Seed scope from the current build once: equipped mods = fixed.
  if (!optSeeded) {
    opt.mods = {}; opt.exilus = {};
    // Everything equipped seeds as REQ (pinned) — first-ever content for
    // the auto-created "search 1"; afterwards the ACTIVE preset is the
    // scope (document model) and immediately overwrite this seed.
    slots.slice(0, 8).forEach((s) => { if (s.mod) opt.mods[s.mod] = "fixed"; });
    if (slots[EXILUS].mod) opt.exilus[slots[EXILUS].mod] = "fixed";
    opt.arcanes = {};
    arcanes.filter((a) => a && a !== "none").forEach((a) => { opt.arcanes[a] = "fixed"; });
    opt.evos = {};
    Object.entries(evoSel).forEach(([t, id]) => { if (id) opt.evos[t] = { [id]: "fixed" }; });
    // The build's own mode seeds as REQ, like everything else equipped — so a
    // scope opened for the first time searches the weapon the way you are
    // holding it, not every way it can be held.
    opt.modes = modeOpts(w).length ? { [mode]: "fixed" } : {};
    // NOTHING CROSSES BETWEEN WEAPONS: the valence axis is seeded from the
    // build's own element, like the mode is.
    opt.valence = valenceSpec(w.id) ? { [valence.element]: "fixed" } : {};
    opt.starts = [];
    optSeeded = true;
    bootstrapOptPresets();
  }
  renderOptMods();
  renderOptPresetBars();
  renderOptModes();
  renderOptValence();
  renderOptExilus();
  renderOptArcanes();
  renderOptEvos();
  renderOptStarts();
  renderOptEnemy();
  updateOptEstimate();
  renderOptBuffs();
}

// The buffs across the WHOLE scope (union of every fixed/search mod + every
// searched arcane + every marked evolution option) — enumerated server-side;
// The SCENARIO's buffs, READ-ONLY — the optimizer reads the simulator the way
// the simulator reads the builder.
//
// It keeps none of its own. A scope-wide union with its own stack settings —
// because a candidate carries mods the current build does not — buys one real
// thing and costs a worse one: the two modules then score the same fight under
// different buffs, and "add this winner, then Run Sim" only matches
// because adding a winner secretly copied the search's config into your
// scenario. One fight, one buff config, and the disagreement cannot exist.
//
// The list is the WIDE one (`fetchAllBuffs`: every buff this weapon could
// produce, cached per weapon), because a search covers builds you are not
// holding — which is exactly what the scenario's "all potential buffs" view is
// for. A buff nobody set falls to its own default, which is now 0 for anything
// timed: a candidate is credited with a stack only if the fight says so.
async function renderOptBuffs() {
  const box = $("opt-buffs");
  if (!box) return;
  renderBuffCards(box, await fetchAllBuffs(), sim.buffs, null, { readonly: true });
}

// The mod scope: the SAME rich list as the mod picker (image, polarity icon,
// sort / polarity filter, effect lines) — the only difference is the rightmost
// control is pool/req instead of the drain. Plus a summary of selections and
// a "mods per build" size. `optPrefs` mirrors the picker's sort/filter.
function renderOptMods() {
  $("opt-size").value = opt.size;
  $("opt-min").value = opt.min;
  renderOptTools();
  renderOptModSel();
  renderOptModList();
}

// The optimizer does NOT own a scenario: it runs the
// SIMULATOR's, drawn here by the same renderer so the two cannot drift and a
// scenario preset switched on either tab is switched on both. What the search
// owns is its funnel — how many candidates survive each round — and that is
// the block below the buffs, not this one.
//
// No Runs/Measure section here: the funnel decides run counts round by round,
// so the engagement LENGTH is the only measurement input the search takes,
// and it sits beside the enemy.
function renderOptEnemy() {
  if (!$("opt-target")) return;
  renderWfBuffs("opt-wfbuffs", true);
  renderScenarioFields(
    { target: "opt-target", technique: "opt-technique", limits: "opt-limits",
      extra: "opt-extra", squad: "opt-squad" },
    { readonly: true },
  );
  // Which fight, and where it is edited. Not a preset bar: that bar can
  // rename, duplicate, delete and import, all of which are edits, and the
  // scenario collection is the SIMULATOR's to edit.
  const ref = $("opt-scenario-ref");
  if (ref) {
    const w = weaponInfo($("weapon").value) || {};
    ref.innerHTML =
      `<span class="plabel">${escHtml(tr("Scenario"))}</span>` +
      `<span class="pchip sel" title="${escHtml(tr("the scenario the simulator is set to"))}">${escHtml(presetLabel(scenarioNamed(activeScenario)) || tr("current"))}</span>` +
      `<a class="pchip" href="${weaponPath(w.id)}/simulator">${escHtml(tr("edit in the Simulator"))} →</a>`;
  }
}

// Exilus-slot scope (the +1 slot) — exilus-eligible mods with the same
// pool/req segs as the main list: pool = a slot option (empty always
// allowed), req = pin the slot (max one). The same mods may ALSO be marked
// in the main scope above — all 9 slots accept exilus mods; the search
// never equips one twice.
function renderOptExilus() {
  const pinned = exilusPinned();
  const hasPool = Object.values(opt.exilus).some((s) => s === "search");
  const row = (m) => {
    const st = opt.exilus[m.id] || "off";
    const fam = famReqBy(m);
    // Only a FAMILY conflict can kill a row: pool and req no longer block
    // each other — clicking one rewrites the group (setSingleSlotMark).
    const poolDead = !!fam;
    const reqDead = !!fam;
    const why = fam ? `excluded: ${(modById(fam) || { name: fam }).name} is required (same family)` : "";
    return modRow(m, {
      cls: `${st === "off" ? "" : st} ${fam ? "dis-soft" : ""}`,
      title: why,
      exilusChip: false,
      trailing: oseg(`data-m="${m.id}"`, st, {
        poolDead,
        reqDead,
        reqTitle: !reqDead && hasPool ? tr("req pins the slot — the pool marks give way") : "",
      }),
    });
  };
  const ax = weaponAxes().exilus;
  // …AND THE SLOT'S OWN RANGE, the same control the arcane seats and the
  // evolution tiers carry. This axis could always say 0–1 — `exilus_ids` has
  // taken a pooled `none` since it was written — and nothing on the page ever
  // said so, which is half of why the four axes looked like four rules.
  const r = slotRange(opt.exilus, "none", new Set([...ax.map((m) => m.id), "none"]));
  const note = r.locked
    ? (r.why === "pinned" ? tr("pinned — this slot is settled")
      : tr("nothing marked, so the slot is searched empty"))
    : r.hi === 0 ? tr("searched empty — the marks are kept")
      : r.lo === 0 ? tr("empty is searched beside them")
        : tr("every searched build fills it");
  $("opt-exilus").innerHTML = (ax.map(row).join("")
    || `<div class="opt dis">${escHtml(tr("no exilus mods in this pool"))}</div>`);
  const rangeHost = $("opt-exilus-range");
  if (rangeHost) {
    rangeHost.innerHTML = ax.length ? slotRangeHtml("exilus", {
      label: tr("This slot holds"), lo: r.lo, hi: r.hi, locked: r.locked, note,
    }) : "";
    wireSlotRange(rangeHost, 1, (_k, lo, hi) => setOptRange("exilus", null, lo, hi));
  }
  $("opt-exilus").querySelectorAll(".seg:not(.dis)").forEach((el) =>
    el.addEventListener("click", (e) => { e.stopPropagation(); markOptExilus(el.dataset.m, el.dataset.s); }));
}

/// The exilus slot's mark: one slot, so a pin clears the rest of the group.
function markOptExilus(id, want) {
  setSingleSlotMark(opt.exilus, id, want);
  if (opt.exilus[id] === "fixed") clearFamMarks(id);
  renderOptMods(); renderOptExilus(); updateOptEstimate();
}

// Arcane scope — the SAME rich rows as the arcane picker (image, name, effect
// lines), searchable, with an include toggle on the right. Marking nothing is
// what searches the empty slot, so there is no "None" row to mark.
/// THE MODE AXIS — how the weapon is played, as a search dimension.
///
/// Mode belongs to the BUILD, which is why the builder has a
/// control for it and the simulator does not. The optimizer is the third case
/// and it is neither: it binds a SET where the builder binds a value, exactly
/// as it does for mods, arcanes and evolutions. Before this the builder's own
/// Mode block was simply drawn on this tab, where it looked like a setting and
/// was not one — the request carried no mode at all, so picking the Phantasma's
/// charged mode here searched its base form and said nothing.
///
/// An UNSUSTAINABLE mode is still offered. `play_modes` marks the ones a board
/// may rank, which is a rule about the leaderboard rather than about what a
/// player may search — and the builder offers them all.
function renderOptModes() {
  const box = $("opt-modes");
  if (!box) return;
  const w = weaponInfo($("weapon").value) || {};
  const opts = modeOpts(w);
  // NEVER EMPTY, and asserted HERE rather than only where the scope is seeded:
  // a preset written before this axis existed carries no modes, a scope
  // imported from another weapon carries modes this one does not have, and
  // both arrive as "no mode at all" — which is not "search them all", it is a
  // question with no answer. The build's own mode is the answer, the same one
  // an empty scope seeds with.
  if (opts.length && !Object.keys(opt.modes).length) opt.modes = { [mode]: "fixed" };
  const marks = Object.keys(opt.modes).filter((id) => opts.some(([o]) => o === id));
  const pinned = marks.find((id) => opt.modes[id] === "fixed") || null;
  const hasPool = marks.some((id) => opt.modes[id] === "search");
  box.innerHTML = opts
    .map(([id, label, offReason]) => {
      const st = opt.modes[id] || "off";
      return `<div class="opt ${st === "off" ? "" : st} ${offReason ? "dis" : ""}">
        <div class="info"><div class="mn">${escHtml(label)}</div>${
          offReason ? `<div class="ef warn">⊘ ${escHtml(offReason)}</div>` : ""}</div>
        ${oseg(`data-m="${escHtml(id)}"`, st, {
          poolDead: !!offReason,
          reqDead: !!offReason,
          poolTitle: pinned && pinned !== id ? tr("pooling opens the slot — the pin gives way") : "",
          reqTitle: hasPool ? tr("req pins the slot — the pool marks give way") : "",
        })}
      </div>`;
    })
    .join("")
    // …AND ITS RANGE, READ-ONLY AT 1–1. A build is played
    // exactly one way, so this axis has no range to adjust — and saying so is
    // what makes the model complete rather than making this the axis the rule
    // forgot. The same row, the same words, disabled.
    + slotRangeHtml("mode", {
      label: tr("Every build is played"), lo: 1, hi: 1, locked: true,
      note: tr("one way — there is no build without a mode"),
    });
  box.querySelectorAll(".seg:not(.dis)").forEach((el) =>
    el.addEventListener("click", (e) => { e.stopPropagation(); markOptOneWay("modes", el.dataset.m, el.dataset.s); }));
}

/// THE MODE AND VALENCE MARKS — ONE GROUP each, because a build is played one
/// way and carries one element: `req` pins it and clears the pool, `pool` opens
/// it and gives way on the pin. NEVER EMPTY: a scope with no mode is not
/// "search them all", it is a question with no answer, so clearing the last
/// mark pins it instead.
function markOptOneWay(axis, id, want) {
  const was = opt[axis][id];
  if (want === "fixed") {
    opt[axis] = { [id]: "fixed" };
  } else if (was === "search") {
    delete opt[axis][id];
  } else {
    Object.keys(opt[axis]).forEach((k) => { if (opt[axis][k] === "fixed") delete opt[axis][k]; });
    opt[axis][id] = "search";
  }
  if (was === "fixed" && want === "fixed") delete opt[axis][id];
  if (!Object.keys(opt[axis]).length) opt[axis] = { [id]: "fixed" };
  if (axis === "modes") renderOptModes(); else renderOptValence();
  updateOptEstimate(); // the scope's auto-save
}

/// THE VALENCE AXIS, searched exactly like the mode: `pool` opens it, `req`
/// pins one, and the two marks behave as one group because a weapon has ONE
/// progenitor element.
///
/// Never empty for a weapon that has the axis — a scope with no element is not
/// "search them all", it is a question with no answer, and the server would
/// fall back to the request's single element without the screen saying so.
function renderOptValence() {
  const box = $("opt-valence");
  if (!box) return;
  const w = weaponInfo($("weapon").value) || {};
  const s = valenceSpec(w.id);
  if (!s) { box.innerHTML = ""; return; }
  if (!Object.keys(opt.valence || {}).length) {
    opt.valence = { [valence.element]: "fixed" };
  }
  const marks = Object.keys(opt.valence).filter((id) => s.elements.includes(id));
  const pinned = marks.find((id) => opt.valence[id] === "fixed") || null;
  const hasPool = marks.some((id) => opt.valence[id] === "search");
  box.innerHTML = s.elements
    .map((id) => {
      const st = opt.valence[id] || "off";
      return `<div class="opt ${st === "off" ? "" : st}">
        <div class="info"><div class="mn">${escHtml(DT(id))}</div></div>
        ${oseg(`data-m="${escHtml(id)}"`, st, {
          poolTitle: pinned && pinned !== id ? tr("pooling opens the slot — the pin gives way") : "",
          reqTitle: hasPool ? tr("req pins the slot — the pool marks give way") : "",
        })}
      </div>`;
    })
    .join("")
    // …and the same read-only 1–1 the mode axis carries: an adversary weapon
    // has exactly one progenitor element, always.
    + slotRangeHtml("valence", {
      label: tr("Every build carries"), lo: 1, hi: 1, locked: true,
      note: tr("one progenitor element — the weapon always has one"),
    });
  box.querySelectorAll(".seg").forEach((el) =>
    el.addEventListener("click", (e) => { e.stopPropagation(); markOptOneWay("valence", el.dataset.m, el.dataset.s); }));
}

function renderOptArcanes() {
  const q = ($("opt-arc-filter") && $("opt-arc-filter").value || "").trim().toLowerCase();
  const axes = weaponAxes().arcanes;
  const row = (a, pinned, hasPool) => {
    const st = opt.arcanes[a.id] || "off";
    // Neither mark blocks the other: clicking one rewrites the group
    // (setSingleSlotMark), so every seg here is live.
    const eff = effLines(cardLines(a, a.max_rank, effectsAt(a, a.max_rank)));
    return `<div class="opt ${a.rarity ? "rar-" + a.rarity : ""} ${st === "off" ? "" : st}">
      ${imgTag(IMG(a.image), "mod")}
      <div class="info"><div class="mn">${wl(a.name, wikiUrl(a.name_en || a.name))}${arcaneMarketLink(a)}${optGainChipFor(a.id)}</div>${eff}</div>
      ${oseg(`data-a="${a.id}"`, st, {
        poolTitle: pinned && pinned !== a.id ? tr("pooling opens the slot — the pin gives way") : "",
        reqTitle: hasPool ? tr("req pins the slot — the pool marks give way") : "",
      })}
    </div>`;
  };
  // ONE SECTION PER SLOT. An arcane belongs to exactly one pool, so the flat
  // `opt.arcanes` map already says which slot each mark is for — what has to
  // be per-pool is the RULE: "req pins the slot" pins THAT slot, and a pin in
  // the Primary section has nothing to do with the Secondary one. The section
  // header is drawn only when there is more than one, as everywhere else.
  $("opt-arcanes").innerHTML = axes
    .map(({ pool, options }, i) => {
      const inPool = options.filter((a) => !q || searchBlob(a).includes(q));
      const ids = new Set(options.map((a) => a.id));
      const marks = Object.keys(opt.arcanes).filter((id) => ids.has(id));
      const pinned = marks.find((id) => opt.arcanes[id] === "fixed") || null;
      const hasPool = marks.some((id) => opt.arcanes[id] === "search");
      const head = axes.length > 1
        ? `<div class="menu-head">${escHtml(tr(ARC_POOL_LABEL[pool] || pool))}</div>`
        : "";
      const rows = inPool.map((a) => row(a, pinned, hasPool)).join("")
        || `<div class="opt dis">${escHtml(tr("no matches"))}</div>`;
      // …AND HOW MANY OF THIS SEAT TO FILL, after the list, for the reason
      // the mods axis states its range after its list: it is a conclusion of
      // the marking and means nothing before it.
      const r = slotRange(opt.arcanes, arcaneEmptyId(pool), ids.add(arcaneEmptyId(pool)));
      const note = r.locked
        ? (r.why === "pinned" ? tr("pinned — this seat is settled")
          : tr("nothing marked, so the seat is searched empty"))
        : r.hi === 0 ? tr("searched unworn — the marks are kept")
          : r.lo === 0 ? tr("unworn is searched beside them")
            : tr("an empty seat can only tie, so it is not searched unless you ask");
      return `<div class="menu-sect">${head}${rows}${slotRangeHtml(`arc:${pool}`, {
        label: tr("This seat holds"), lo: r.lo, hi: r.hi, locked: r.locked, note,
      })}</div>`;
    })
    .join("");
  wireSlotRange($("opt-arcanes"), 1, (key, lo, hi) => setOptRange("arcanes", key.slice(4), lo, hi));
  $("opt-arcanes").querySelectorAll(".seg:not(.dis)").forEach((el) =>
    el.addEventListener("click", (e) => { e.stopPropagation(); markOptArcane(el.dataset.a, el.dataset.s); }));
}

/// AN ARCANE'S MARK. The group is this arcane's OWN seat: pinning a Primary
/// must not clear a Secondary mark, because they fill different slots. A pin
/// settles the seat, so the seat's range (`none:<pool>`) goes with it.
function markOptArcane(aid, want) {
  const own = arcaneById(aid);
  const group = {};
  Object.keys(opt.arcanes).forEach((id) => {
    if (sameArcaneSeat(arcaneById(id), own)) group[id] = opt.arcanes[id];
  });
  setSingleSlotMark(group, aid, want);
  Object.keys(opt.arcanes).forEach((id) => {
    if (sameArcaneSeat(arcaneById(id), own)) delete opt.arcanes[id];
  });
  Object.assign(opt.arcanes, group);
  if (want === "fixed") {
    arcaneSeats(own).forEach((pool) => { delete opt.arcanes[arcaneEmptyId(pool)]; });
  }
  renderOptArcanes(); updateOptEstimate();
}

// Evolution scope — per tier, the option rows with their verbatim description
// and a search toggle (broken evolutions flagged).
function renderOptEvos() {
  const tiers = weaponEvos();
  // The same LADDER the builder draws: a tier is markable only once the one
  // before it has a mark, because every set the search enumerates installs
  // one option per marked tier — mark tier 2 with tier 1 blank and every set
  // it produces skips a rung. The scope cannot express what the sim would
  // then refuse to price.
  const optOpenTo = (() => {
    let n = 0;
    // A TIER SET TO 0–0 OPENS NOTHING. Its `none` is a mark, and counting it
    // would open the tier above over sets that reach it through a rung the
    // search never installs — `ladder_prefix` would then truncate every one of
    // them and the marks above would price nothing. 0–1 DOES open it: half of
    // its sets carry the rung, and the other half are truncated, which is the
    // ladder working rather than a scope that cannot be expressed.
    for (const t of tiers) { if (!evoFillsRung(t.tier)) break; n = t.tier; }
    return n + 1;
  })();
  // A scope preset saved before the rule existed can still carry marks above
  // the gap. Drop them here so what is drawn, what is counted in the estimate
  // and what is sent all say the same thing — the server truncates the sets
  // either way, and a mark that changes nothing is worse than no mark.
  tiers.forEach((t) => { if (t.tier > optOpenTo) delete opt.evos[t.tier]; });
  $("opt-evos").innerHTML = tiers.map((t) => {
    const sel = opt.evos[t.tier] || {};
    const pinned = evoPinned(t.tier);
    const locked = t.tier > optOpenTo;
    const hasPool = Object.values(sel).some((s) => s === "search");
    const rows = t.options.map((o) => {
      const st = sel[o.id] || "off";
      // Neither mark blocks the other — clicking one rewrites the tier.
      const desc = evoLines(o).map((x) => `<div>${escHtml(x)}</div>`).join("");
      return `<div class="opt ${st === "off" ? "" : st} ${o.broken ? "dis-soft" : ""}">
        <div class="info"><div class="mn">${o.name}${o.broken ? ' <span class="exchip brk">BROKEN</span>' : ""}${
          evoGapChips(o, "span")
        }${optGainChipFor(o.id)}</div><div class="me">${desc}</div>${optPairingNoteFor(o.id)}</div>
        ${oseg(`data-t="${t.tier}" data-e="${o.id}"`, st, {
          extra: locked ? "tlocked" : "",
          poolTitle: pinned && pinned !== o.id ? tr("pooling opens the tier — the pin gives way") : "",
          reqTitle: hasPool ? tr("req pins the tier — the pool marks give way") : "",
        })}
      </div>`;
    }).join("");
    const r = slotRange(sel, "none", new Set([...t.options.map((o) => o.id), "none"]));
    const note = r.locked
      ? (r.why === "pinned" ? tr("pinned — this tier is settled")
        : tr("nothing marked, so this tier is searched empty"))
      : r.hi === 0 ? tr("searched empty — the marks are kept")
        : r.lo === 0 ? tr("empty is searched beside them")
          : tr("every searched build installs one of them");
    const range = locked ? "" : slotRangeHtml(`evo:${t.tier}`, {
      label: tr("This tier installs"), lo: r.lo, hi: r.hi, locked: r.locked, note,
    });
    return `<div class="opt-tier-block${locked ? " locked" : ""}" ${locked
      ? `title="${escHtml(tr("install the previous tier first"))}"` : ""
    }><div class="opt-tier-h">EVO ${ROMAN(t.tier)}</div><div class="combo-menu opt-evolist">${rows}</div>${range}</div>`;
  }).join("");
  wireSlotRange($("opt-evos"), 1, (key, lo, hi) => setOptRange("evolutions", key.slice(4), lo, hi));
  $("opt-evos").querySelectorAll(".seg:not(.dis):not(.tlocked)").forEach((el) =>
    el.addEventListener("click", (e) => { e.stopPropagation(); markOptEvo(el.dataset.t, el.dataset.e, el.dataset.s); }));
}

/// AN EVOLUTION'S MARK in its tier. Clearing a tier shuts every tier above it,
/// marks and all — the same cascade the builder does, for the same reason.
function markOptEvo(t, id, want) {
  opt.evos[t] = opt.evos[t] || {};
  setSingleSlotMark(opt.evos[t], id, want);
  if (!evoRealMarks(t).length) {
    weaponEvos().forEach((x) => { if (x.tier > Number(t)) delete opt.evos[x.tier]; });
  }
  renderOptEvos(); updateOptEstimate();
}

