// ---- The agent door ----------
//
// ONE DOOR, and everything that drives this page from outside goes through it:
// the checks, an in-page agent, a future bot. A consumer that pokes the DOM
// instead writes its own vocabulary of the page, which is what the checks'
// raw clicks already cost — they break when a button moves, and none of them
// can be reused by anything that is not a check.
//
// AN ACTION IS SOMETHING A READER CAN DO, and nothing else. Each one names the
// control it stands for (`anchor`) and `check_agent_door.mjs` asserts that
// control is on the page, so an action cannot outlive its button and the page
// cannot grow a door no reader has. That is also what makes an agent's work
// VISIBLE: the action moves the state the click moves, and the page redraws.

/// THE ID IS THE WIRE: `<module>.<subject>.<verb>`, named after the DOMAIN and
/// never after the widget — the same namespace `docs/ANALYTICS.md` fixes for
/// event names, for the same reason. A name that dies with a button breaks
/// every agent that already learned it, and an agent cannot be migrated.
const AGENT_DOOR_V = 1;
const AGENT_MODULES = ["builder", "simulator", "optimizer", "rivens", "enemies"];
const AGENT_ID = /^[a-z]+\.[a-z]+\.[a-z]+$/;

/// Where the reader is, READ OFF THE PAGE rather than parsed a second time.
/// `route()` already turned the path into these classes; parsing the path here
/// too would be a second answer to one question, free to disagree with the
/// first the day a route is added.
const agentRoute = () => {
  const onWeapon = !document.querySelector(".config-page").hidden;
  const mod = AGENT_MODULES.find((x) => document.body.classList.contains("on-" + x));
  return {
    module: onWeapon ? (mod || "builder") : null,
    weapon: onWeapon ? $("weapon").value : null,
    path: location.pathname,
  };
};

/// A SEAT, NOT AN INDEX. The exilus and the stance are 8 and 9 because the
/// slots are one array; that is this file's bookkeeping and no caller of the
/// door should have to know it. Numbers still work for the eight main slots.
const agentSeat = (v) =>
  v === "exilus" ? EXILUS : v === "stance" ? STANCE
  : (Number.isInteger(v) && v >= 0 && v < slots.length) ? v : -1;
const agentSeatName = (i) => i === EXILUS ? "exilus" : i === STANCE ? "stance" : i;

/// The fight, BOUNDED. A scenario carries the formation, and a 361-body one is
/// not an observation, it is a dump — an agent pays for every byte of it in
/// the same window it has to think in. Scalars travel as themselves, and
/// everything else reports its SIZE, which is the part that can be acted on.
/// Derived, not listed: a field added to a scenario is observed without this
/// being edited.
const agentScenario = () => {
  const out = {};
  for (const [k, v] of Object.entries(snapshotScenario())) {
    out[k] = (v === null || typeof v !== "object")
      ? v : { n: Array.isArray(v) ? v.length : Object.keys(v).length };
  }
  return out;
};

/// The last run, and WHETHER IT STILL DESCRIBES WHAT IS ON SCREEN. A number
/// attached to a build that never produced it is the one lie this surface
/// cannot tell, so `fresh` is stated rather than left to be inferred from a
/// timestamp the caller would have to interpret.
function agentResult() {
  const p = loadPresetList(BUILDS).find((x) => x.name === activePreset);
  const lr = p && p.lastResult;
  if (!lr || !lr.r) return null;
  const m = metricOf(sim.metric);
  return {
    metric: m.id, unit: metricLabel(m), value: metricValue(m, lr.r),
    duration: lr.r.duration, fresh: lr.key === simKey(),
  };
}

/// ONE READ, and the agent's whole view of the page. It is deliberately not a
/// DOM dump: what an action can be aimed at is state, and `can` says which
/// actions are open from here so the caller does not have to guess and be
/// refused.
function agentObserve() {
  if (!window.__wfsimReady) return { v: AGENT_DOOR_V, ready: false };
  const r = agentRoute();
  const out = { v: AGENT_DOOR_V, ready: true, route: r, lang: LANG };
  if (r.weapon) {
    const st = snapshotState();
    out.weapon = { id: r.weapon, name: weaponInfo(r.weapon).name };
    // A SLOT WITH A POLARITY AND NO MOD IS STILL PART OF THE BUILD — it is
    // what the next mod will cost — so a slot is left out only when it carries
    // neither.
    const f = formaCount();
    out.build = {
      slots: st.slots
        .map((s, i) => ({ seat: agentSeatName(i), mod: s.mod, rank: s.rank, pol: s.pol }))
        .filter((s) => s.mod || s.pol),
      arcanes: st.arcane.map((id, i) => ({ seat: i, arcane: id === "none" ? null : id, rank: st.arcaneRank[i] })),
      evolutions: st.evoSel, mode: st.mode,
      valence: st.valence, assembly: st.assembly, wielder: st.wielder,
      capacity: { used: capacityUsed(), max: builderCap() },
      forma: { regular: f.regular, umbra: f.umbra, omni: f.omni },
    };
    out.scenario = agentScenario();
    out.result = agentResult();
    // WHAT IS OPEN in each bar — the document an edit would write — and
    // whether the fight is an official ruler, which no edit may touch.
    out.open = { build: activePreset || null, scenario: activeScenario || null, search: activeOptPreset || null,
      riven: activeRivenId() || null, target: activeEnemyName() || null };
    out.official_scenario = officialScenarioActive();
  }
  out.can = AGENT_ACTIONS.filter((a) => !a.needs_weapon || r.weapon).map((a) => a.id);
  return out;
}

/// A REFUSAL IS AN ANSWER. The engine saying a build cannot hold this mod, and
/// the door saying there is no such weapon, are both results — so they carry a
/// machine-readable `reason` and, wherever the question has near answers, the
/// ones that would have worked. An exception would say only "no".
const agentNo = (reason, extra = {}) => ({ ok: false, reason, ...extra });
const agentNear = (id) => {
  const head = String(id).split(".")[0];
  const ids = AGENT_ACTIONS.map((a) => a.id);
  return (ids.filter((x) => x.startsWith(head + ".")).length ? ids.filter((x) => x.startsWith(head + ".")) : ids).slice(0, 5);
};

const AGENT_KINDS = {
  string: (v) => typeof v === "string" && !!v,
  number: (v) => typeof v === "number" && Number.isFinite(v),
  object: (v) => !!v && typeof v === "object" && !Array.isArray(v),
  array: (v) => Array.isArray(v),
  boolean: (v) => typeof v === "boolean",
  scalar: (v) => typeof v === "boolean" || (typeof v === "number" && Number.isFinite(v)),
  any: (v) => v !== undefined,
  seat: (v) => agentSeat(v) >= 0,
};

function agentCheckArgs(a, args) {
  for (const [k, spec] of Object.entries(a.args || {})) {
    const has = k in args && !(args[k] === null && !spec.nullable);
    if (!has) {
      if (spec.required) return agentNo("missing_argument", { argument: k, wants: spec.kind });
      continue;
    }
    if (args[k] === null && spec.nullable) continue;
    if (!AGENT_KINDS[spec.kind](args[k])) {
      return agentNo("bad_argument", { argument: k, wants: spec.kind, got: args[k] });
    }
    if (spec.enum && !spec.enum().includes(args[k])) {
      return agentNo("bad_argument", { argument: k, alternatives: spec.enum().slice(0, 8) });
    }
    if (spec.kind === "number" && (args[k] < spec.min || args[k] > spec.max)) {
      return agentNo("out_of_range", { argument: k, min: spec.min, max: spec.max });
    }
  }
  for (const k of Object.keys(args)) {
    if (!(a.args || {})[k]) return agentNo("unknown_argument", { argument: k, wants: Object.keys(a.args || {}) });
  }
  return null;
}

/// WHAT CHANGED, so the caller never needs a screenshot to find out. The diff
/// is over the observation's own sections, which is the same granularity the
/// actions are written at.
const agentDiff = (a, b) => {
  const out = {};
  for (const k of new Set([...Object.keys(a), ...Object.keys(b)])) {
    if (k === "v" || k === "can") continue;
    if (JSON.stringify(a[k]) !== JSON.stringify(b[k])) out[k] = b[k];
  }
  return out;
};

async function agentDo(id, args = {}, opts = {}) {
  const a = AGENT_ACTIONS.find((x) => x.id === id);
  if (!a) return agentNo("unknown_action", { alternatives: agentNear(id) });
  // A HAND ACTION IS THE READER'S GESTURE: the page's own control passes
  // `hand`, and nothing a model can reach does — it is not in `tools()`.
  if (a.hand && !opts.hand) return agentNo("reader_only", { because: "this is a reader's click, not a tool" });
  if (a.needs_weapon && !agentRoute().weapon) return agentNo("no_weapon_open", { alternatives: ["shell.module.open"] });
  const bad = agentCheckArgs(a, args);
  if (bad) return bad;
  // AN OFFICIAL RULER'S FIGHT IS LOCKED, and the door is not a way round the
  // lock the page puts on every one of its controls: a ruler edited in memory
  // reports a modified fight under the ruler's own name.
  if (agentWritesFight(a) && officialScenarioActive()) {
    return agentNo("official_scenario", { try: "shell.preset.copy with bar scenario" });
  }
  // IT NEVER REJECTS. A caller that must wrap every call in a try is a caller
  // that will forget once, and the page's own handlers call through here too —
  // an unhandled rejection from a click is a failure with no reader-visible
  // symptom at all.
  try {
    if (a.query) {
      const out = await a.run(args);
      return out && out.ok === false ? out : { ok: true, did: id, ...(out || {}) };
    }
    const before = agentObserve();
    const out = await a.run(args);
    if (out && out.ok === false) return out;
    return { ok: true, did: id, changed: agentDiff(before, agentObserve()), ...(out || {}) };
  } catch (e) {
    return agentNo("action_failed", { because: String((e && e.message) || e) });
  }
}

/// THE SKILLS: one per module of the table, and what it is for in one line. An
/// agent that cannot carry the whole table sees these and loads one module's
/// part when it needs it (docs/NONA.md §"Skills"). Which actions a skill holds
/// is read off their ids; a module an action names and this does not fails
/// `check_agent_door`.
const AGENT_SKILLS = {
  builder: "the build on screen: weapon, mods, arcanes, evolutions, mode, valence, Forma, the stats panel, finders and the leaderboard",
  simulator: "the fight: the scenario, enemy and level, buffs, abilities, auras, the arena, and running it",
  optimizer: "the build search: its scope, starting it, reading it, stopping it, saving a result",
  rivens: "riven cards: listing, making, copying, opening and writing one",
  enemies: "custom targets: listing, making, copying, opening and editing one",
  shell: "documents and moving around: saved builds and scenarios, copies, undo, opening a module",
};
const agentSkills = () => Object.entries(AGENT_SKILLS).map(([id, what]) => ({
  id, what, actions: AGENT_ACTIONS.filter((a) => !a.hand && a.id.startsWith(id + ".")).map((a) => a.id),
}));

/// The table as a model sees it. DERIVED — a tool definition hand-written
/// beside the action it describes is a second declaration, and the day they
/// disagree the agent is calling something that does not exist.
const agentTools = () => AGENT_ACTIONS.filter((a) => !a.hand).map((a) => ({
  name: a.id,
  description: a.what,
  input_schema: {
    type: "object",
    properties: Object.fromEntries(Object.entries(a.args || {}).map(([k, s]) => [k, {
      // A NULLABLE argument says so in its type, or a model that follows the
      // schema can never send the null that empties a slot.
      // AN "any" ARGUMENT HAS NO TYPE in the schema; the action checks it.
      ...(s.kind === "any" ? {} : { type: ((ts) => (ts.length === 1 ? ts[0] : ts))(
        [].concat(s.kind === "seat" ? ["string", "integer"] : s.kind === "scalar" ? ["boolean", "number"] : s.kind,
          s.nullable ? ["null"] : [])) }),
      description: s.what,
      ...(s.kind === "array" ? { items: { type: "object" } } : {}),
      ...(s.enum ? { enum: s.enum() } : {}),
    }])),
    required: Object.entries(a.args || {}).filter(([, s]) => s.required).map(([k]) => k),
  },
}));

const agentWeaponIds = () => (META.weapons || []).map((w) => w.id);
/// WHAT AN ACTION WRITES, declared on the action: the document of one bar
/// (build, scenario, search, riven, target), this browser's preferences, the
/// bar its `bar` argument names, or nothing. The official ruler's lock and an
/// agent's copy-before-write both read it, so they cannot disagree about what
/// an edit is. `check_agent_door` requires it of every action that is not a
/// query.
const AGENT_WRITES = ["build", "scenario", "search", "riven", "target", "prefs", "bar", "none"];
const agentWritesFight = (a) => a.writes === "scenario";

/// A FOUND LIST IS CAPPED, and says how many it left out, so a caller knows to
/// narrow the query rather than believe the list is complete.
const agentFound = (xs, limit, row) => ({
  found: xs.length, rows: xs.slice(0, limit).map(row),
  ...(xs.length > limit ? { more: xs.length - limit } : {}),
});
const agentFind = { query: { kind: "string", what: "name or effect words, in any language the page speaks" },
  limit: { kind: "number", min: 1, max: 40, what: "rows to return, default 12" } };

/// ONE STAT ROW as the panel draws it — the page's own wording, sources named.
/// A row is the server's answer, never recomputed here.
const agentStat = (x) => ({
  label: x.label, base: x.base, final: x.final,
  ...(x.note ? { note: x.note } : {}), ...(x.rule ? { rule: x.rule } : {}),
  ...(x.sources && x.sources.length
    ? { sources: x.sources.map((y) => `${y.mod} ${y.value}${y.note ? ` (${y.note})` : ""}`) } : {}),
});

/// THE LAST RUN, without the arrays that exist to draw a chart or a replay —
/// the reader reads those as pictures, and the numbers they summarise are here.
function agentRunSummary() {
  const p = loadPresetList(BUILDS).find((x) => x.name === activePreset);
  const r = p && p.lastResult && p.lastResult.r;
  if (!r) return null;
  const n = (v) => (typeof v === "number" ? Number(sig2(v)) || v : v);
  const total = (r.damage_sources || []).reduce((a, x) => a + (x.dmg || 0), 0) || 1;
  return {
    headline: agentResult(),
    runs: r.runs, duration: r.duration,
    dps: n(r.dps), dps_se: n(r.dps_se), burst_dps: n(r.burst_dps),
    kills: n(r.kills), kills_min: r.kills_min, kills_max: r.kills_max, ttk: r.ttk,
    crit_rate: n(r.crit_rate), headshot_rate: n(r.headshot_rate), procs: n(r.procs),
    shots: n(r.shots), reloads: n(r.reloads), max_hit: n(r.max_hit), overkill_rate: n(r.overkill_rate),
    target: r.target,
    damage_sources: (r.damage_sources || []).slice().sort((a, b) => b.dmg - a.dmg).slice(0, 10)
      .map((x) => ({ source: x.source, share: `${Math.round((x.dmg / total) * 1000) / 10}%`, by_type: x.by_type })),
  };
}

/// A BUILD'S STATE AS IT IS NOW — live if it is the one on screen, stored
/// otherwise. Pending auto-saves are flushed on every switch
/// (`flushPresetSaves`), so the stored one is never behind.
const agentBuildState = (id) => (activePreset === id ? snapshotState()
  : ((loadPresetList(BUILDS).find((p) => presetId(p) === id) || {}).state || null));

/// THE PRESET BARS the door reaches: the build's and the fight's. Picking,
/// "+ new" and duplicate are the moves; rename and delete stay a reader's.
const AGENT_BARS = { build: () => buildBarCfg(), scenario: () => scenarioBarCfg(), search: () => optBarCfg() };
const agentPresetRows = (cfg) => cfg.load().map((p) => ({
  id: presetId(p), name: presetLabel(p), active: presetId(p) === cfg.active(),
  ...(cfg.readonly && cfg.readonly(p) ? { read_only: true } : {}),
}));
/// Which undo history each bar's edits land in.
const AGENT_UNDO = { build: BUILDS, scenario: SCENARIOS, search: OPT_DOMAIN, riven: RIVENS };
const agentBarArg = { kind: "string", required: true, what: "which bar", enum: () => Object.keys(AGENT_BARS) };

/// A SEARCH'S ANSWER as a caller reads it: the ranking the page draws, with
/// mods named. The page re-measures each row in the simulator after drawing it,
/// so a caller that wants the number to quote saves the row and runs the fight.
function agentSearchResults(limit) {
  const r = optLast;
  const rows = (r.results || []).slice(0, limit).map((res) => ({
    rank: res.rank,
    kpm: Number(sig2(kpm(res.kill_progress ?? res.kills, r.duration))),
    dps: Math.round(res.dps || res.effective_dps || 0),
    forma: res.forma && res.forma.used,
    mods: (res.mods || []).map((id) => (modById(id) || { name: id }).name),
    ...(res.exilus ? { exilus: (modById(res.exilus) || { name: res.exilus }).name } : {}),
    ...(res.arcane && res.arcane.length ? { arcanes: [].concat(res.arcane) } : {}),
    ...(res.evolutions ? { evolutions: res.evolutions } : {}),
    ...(res.mode ? { mode: res.mode } : {}),
    ...(res.valence ? { valence: res.valence } : {}),
  }));
  return {
    phase: r.cancelled ? "cancelled" : "done",
    duration: r.duration, ranked_by: "kills per minute",
    ...(r.exhaustive ? { covered: "every candidate" } : r.coverage != null ? { covered: `${Math.round(r.coverage * 1000) / 10}% of ${r.space} candidates, sampled uniformly` } : {}),
    results: rows,
    note: "search numbers; the simulator re-measures a saved row",
  };
}

/// WHAT A READER CAN TOUCH THAT THE DOOR DOES NOT, AND WHY. `check_agent_coverage`
/// fails on any control that is neither inside an action's anchor nor in here,
/// so a feature added to the page without a door row is loud rather than a
/// thing Nona silently cannot do. `todo` only shrinks: it is the door's backlog.
const AGENT_EXEMPT = [
  { sel: ".bh", kind: "view", why: "a block's header folds it" },
  { sel: ".fold-h", kind: "view", why: "a section's header folds it" },
  { sel: "#jump-grip", kind: "view", why: "the jump menu scrolls the page" },
  { sel: "#topmenu", kind: "view", why: "moves between the site's pages" },
  { sel: "#tbmore-toggle", kind: "view", why: "opens the topbar's overflow" },
  { sel: "#sim-buffs-all", kind: "view", why: "shows every buff that could apply, not only this build's" },
  { sel: "#opt-mod-filter", kind: "view", why: "filters the list on screen" },
  { sel: "#opt-arc-filter", kind: "view", why: "filters the list on screen" },
  { sel: "#opt-picker-tools", kind: "view", why: "sorts and filters the list on screen" },
  { sel: "#opk-dir", kind: "view", why: "flips the list's sort order" },
  { sel: "#nona-fab", kind: "view", why: "opens Nona herself" },
  { sel: "#theme-toggle", kind: "pref", why: "light or dark, for this browser" },
  { sel: "#quick-calc", kind: "pref", why: "the quick calc's own settings, for this browser" },
  { sel: "#opt-runs", kind: "pref", why: "final-round runs, a preference of this browser" },
  { sel: "#board-no", kind: "pref", why: "whether this browser sends results to the board" },
  { sel: "#w-name a", kind: "outward", why: "links to the wiki and the market" },
  { sel: ".opt .mn a", kind: "outward", why: "links to the wiki and the market" },
  { sel: "#qq-copy-foot", kind: "outward", why: "copies the community group number" },
  { sel: ".pop.ren", kind: "reader", why: "renaming a build is the reader's" },
  { sel: ".pop.del", kind: "reader", why: "deleting a build is the reader's" },
  { sel: "#opk-gain", kind: "pref", why: "the search list's own quick-calc scan" },
  { sel: "#opt-fight-half", kind: "view", why: "the simulator's fight, shown read-only beside the search" },
  { sel: ".cu-ren", kind: "reader", why: "renaming a riven or a target is the reader's" },
  { sel: ".cu-del", kind: "reader", why: "deleting a riven or a target is the reader's" },
];

/// THE OPEN RIVEN AS A CALLER READS IT: what the engine made of it — the
/// printed values, its generated name, and anything illegal about it.
const agentRivenCard = () => ({
  id: activeRivenId(), seat_as: RIVEN_PREFIX + activeRivenId(),
  shape: riven && riven.shape, rank: riven && riven.rank, polarity: riven && riven.polarity,
  name: rivenResolved && rivenResolved.name,
  stats: ((rivenResolved && rivenResolved.stats) || []).map((x) => ({
    slot: x.slot, stat: x.id, text: x.text, roll: x.roll, range: `${x.min} to ${x.max}`, ...(x.modeled ? {} : { modeled: false }) })),
  ...(rivenResolved && rivenResolved.illegal && rivenResolved.illegal.length ? { illegal: rivenResolved.illegal } : {}),
});

/// A SEARCH AXIS'S MARKS, grouped the way the scope reads: pinned, pooled,
/// and whether the slot may stay empty (the `none` marks are its range).
const agentMarks = (map, name) => {
  const out = { fixed: [], search: [] };
  for (const [id, st] of Object.entries(map || {})) {
    if (id === "none" || id.startsWith("none:")) { out.empty = st === "fixed" ? "only" : "allowed"; continue; }
    if (out[st]) out[st].push(name ? name(id) : id);
  }
  return out;
};

/// AN ARENA EDIT IS A FIGHT EDIT: the page repaints and saves the scenario
/// after it (and `agentDo` refuses it on an official ruler, as the drag does).
const agentArena = (fn) => {
  const out = fn();
  if (out && out.ok === false) return out;
  markScenarioDirty(); renderSim();
  return out || agentArenaState();
};
const agentArenaState = () => ({
  player_at: sim.player_at, target: { id: "e1", at: sim.target_at, enemy: sim.enemy },
  formation: (sim.formation || []).map((f) => ({ id: f.id, at: f.at, enemy: f.enemy || sim.enemy })),
  aim_at: sim.aim_at || null,
  gap_m: Math.round((arenaSpan(sim) - CONTACT_M) * 100) / 100, max_bodies: ARENA_MAX_BODIES(),
});

/// THE ROSTER AS A CALLER READS IT — every seat in the fight, the open build
/// first. Seat 1 is the one the page is about and the one a board takes; the
/// rest are links into other weapons' presets.
const agentRoster = () => ({
  seats: [
    { seat: 1, weapon: $("weapon").value, preset: activePreset || null, open: true },
    ...alsoActing().map((r, i) => ({ seat: i + 2, weapon: r.weapon, preset: r.preset, open: false })),
  ],
  max_seats: ROSTER_MAX + 1,
});

/// THE REACH AS A CALLER READS IT: each point of the curve — what it costs in
/// Forma and the weakest ruler's share of its leader — and, at the marked
/// point, each ruler's best build that fits.
function agentReach() {
  const w = presetWeapon();
  const x = formaReach && formaReach.at === w ? formaReach : null;
  const scope = formaReachScope(w);
  const base = { scope: { rulers: scope.benchmarks || boardRulersOf(w), riven: scope.riven, planned_builds_must_fit: scope.hard, line: scope.threshold } };
  if (!x || x.busy) return { ...base, state: x ? "working" : "not run" };
  if (x.error || (x.r && (!x.r.ok || !x.r.fits))) return { ...base, state: "no plan", because: x.error || (x.r && (x.r.error || x.r.reason)) };
  const curve = x.r.curve || [];
  const pt = curve[x.sel];
  return {
    ...base, exhaustive: !!x.r.exhaustive,
    curve: curve.map((p, i) => ({ point: i, forma: reachBill(p.plan), weakest_ruler: reachPct(p.worst), marked: i === x.sel })),
    at_marked: pt ? x.groups.map((g, gi) => {
      const k = pt.picks[gi];
      const b = k && g.builds[k.build];
      return { ruler: benchmarkName(g.benchmark), ruler_index: gi, ...(b ? { share: reachPct(k.ratio), mode: b.row.mode || "base", riven: rowHasRiven(b.row) } : { fits: false }) };
    }) : [],
  };
}

/// THE OPEN CUSTOM TARGET as a caller reads it.
const agentEnemyDoc = () => ({ name: activeEnemyName(), id: enemyId(activeEnemyName()), ...JSON.parse(JSON.stringify(enemyDoc)) });

