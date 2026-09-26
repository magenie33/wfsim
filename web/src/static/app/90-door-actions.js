/// THE ACTIONS, and the queries beside them in the same table — `docs/AGENT.md`.
const AGENT_ACTIONS = [
  {
    id: "enemies.targets.list",
    query: true,
    what: "List the custom targets the reader has made, with the id a fight names each one by (simulator.scenario.set with patch {enemy: id}).",
    anchor: "#enemy-tools, #enemy-all",
    args: {},
    run() {
      return { open: activeEnemyName() || null, targets: loadPresetList(ENEMIES).map((p) => {
        const d = { ...blankEnemy(), ...(p.state || {}) };
        return { name: p.name, id: enemyId(p.name), faction: d.faction, health: d.stats.health, shield: d.stats.shield, armor: d.stats.armor };
      }) };
    },
  },
  {
    id: "enemies.target.new",
    writes: "none",
    what: "Make a blank custom target — a plain humanoid, every number meant to be replaced — and open it.",
    anchor: "#enemy-tools",
    args: {},
    run() { return { target: newEnemy() }; },
  },
  {
    id: "enemies.target.copy",
    writes: "none",
    what: "Duplicate the open custom target and open the copy.",
    anchor: "#enemy-tools",
    args: {},
    run() { const n = copyEnemy(); return n ? { target: n } : agentNo("no_target_open", { try: "enemies.target.open" }); },
  },
  {
    id: "enemies.target.open",
    writes: "none",
    what: "Open a custom target for editing by name.",
    anchor: "#enemy-tools, #enemy-all",
    args: { name: { kind: "string", required: true, what: "the target's name" } },
    run({ name }) {
      const ps = loadPresetList(ENEMIES);
      if (!ps.some((p) => p.name === name)) return agentNo("unknown_target", { alternatives: ps.map((p) => p.name).slice(0, 12) });
      openEnemy(name);
      return agentEnemyDoc();
    },
  },
  {
    id: "enemies.target.set",
    writes: "target",
    what: "Write fields of the open custom target: faction, scaling_faction, can_be_eximus, stats {base_level, health, shield, armor, overguard, affinity}, damage_modifiers (a column of multipliers by damage type, \"faction\" to start one from the faction's, or null for the faction's own), status_immunities (the types whose procs cannot land), body_parts [{name, multiplier, is_head, crit_bonus}].",
    anchor: "#enemy-form",
    args: {
      faction: { kind: "string", what: "faction id or unknown" },
      scaling_faction: { kind: "string", what: "how its level scales", enum: () => SCALING_FACTIONS },
      can_be_eximus: { kind: "boolean", what: "an Eximus variant exists" },
      stats: { kind: "object", what: "the numbers to change" },
      damage_modifiers: { kind: "any", nullable: true, what: "an object of multipliers by damage type, \"faction\", or null" },
      status_immunities: { kind: "array", what: "damage type ids" },
      body_parts: { kind: "array", what: "every part, replacing the list; at least one" },
    },
    run(patch) {
      if (!enemyDoc) return agentNo("no_target_open", { try: "enemies.target.new" });
      const factions = ["unknown"].concat((META.factions || []).map((f) => f.id));
      if (patch.faction != null && !factions.includes(patch.faction)) return agentNo("bad_argument", { argument: "faction", alternatives: factions });
      const statKeys = Object.keys(blankEnemy().stats);
      const badStat = Object.keys(patch.stats || {}).find((k) => !statKeys.includes(k));
      if (badStat) return agentNo("bad_argument", { argument: "stats", got: badStat, alternatives: statKeys });
      const dm = patch.damage_modifiers;
      if (dm !== undefined && dm !== null && dm !== "faction" && (typeof dm !== "object" || Object.keys(dm).some((k) => !DAMAGE_TYPES.includes(k)))) {
        return agentNo("bad_argument", { argument: "damage_modifiers", alternatives: DAMAGE_TYPES });
      }
      if (patch.status_immunities && patch.status_immunities.some((k) => !DAMAGE_TYPES.includes(k))) {
        return agentNo("bad_argument", { argument: "status_immunities", alternatives: DAMAGE_TYPES });
      }
      if (patch.body_parts && !patch.body_parts.length) return agentNo("bad_argument", { argument: "body_parts", because: "a target has at least one part" });
      writeEnemyDoc(enemyDoc, patch);
      saveEnemyDoc(); renderEnemies();
      return agentEnemyDoc();
    },
  },
  {
    id: "builder.forma.read",
    query: true,
    what: "Read the Forma planner: its rules (catalyst, reach max rank, grant slot first, Omni and Umbra Forma use, a Forma limit), which saved builds are planned together with this one, and the board-reach analysis if it has been run.",
    anchor: "#forma-plan",
    needs_weapon: true,
    args: {},
    run() {
      const w = presetWeapon();
      return { rules: formaRules(), planned_with: formaGroup(w), reach: agentReach() };
    },
  },
  {
    id: "builder.forma.rules",
    writes: "prefs",
    what: "Change the Forma planner's rules — this browser's, for every build — which the Forma plan and the reach both obey. forma_limit is a whole number or null.",
    anchor: "#forma-plan",
    needs_weapon: true,
    args: {
      catalyst: { kind: "boolean", what: "an Orokin Catalyst is installed" },
      reach_max_rank: { kind: "boolean", what: "every mod reaches its max rank" },
      grant_slot_first: { kind: "boolean", what: "fill the slot that grants capacity first" },
      fixed_order: { kind: "boolean", what: "no mod is moved: each slot's polarity serves what every build keeps there" },
      omni_forma: { kind: "string", what: "when Omni Forma may be used", enum: () => FORMA_SPECIAL.omni_forma },
      umbra_forma: { kind: "string", what: "when Umbra Forma may be used", enum: () => FORMA_SPECIAL.umbra_forma },
      forma_limit: { kind: "number", nullable: true, min: 0, max: 50, what: "most Forma, or null for none" },
    },
    run(patch) {
      setFormaRules(patch);
      renderBuilderFormaPlan();
      return { rules: formaRules() };
    },
  },
  {
    id: "builder.forma.partner",
    writes: "prefs",
    what: "Plan another saved build of this weapon together with the open one (on=true) or stop — the Forma plan then fits both on one set of polarities.",
    anchor: "#forma-plan",
    needs_weapon: true,
    args: { build: { kind: "string", required: true, what: "saved build id" }, on: { kind: "boolean", required: true, what: "planned together" } },
    run({ build, on }) {
      const own = loadPresetList(BUILDS).map((p) => presetId(p)).filter((id) => id !== activePreset);
      if (!own.includes(build)) return agentNo("bad_argument", { argument: "build", alternatives: own.slice(0, 12) });
      setFormaPartner(presetWeapon(), build, on);
      renderBuilderFormaPlan();
      return { planned_with: formaGroup(presetWeapon()) };
    },
  },
  {
    id: "builder.reach.run",
    writes: "none",
    what: "Work out what one polarity layout reaches across this weapon's board rulers, and what each extra Forma buys; optionally set the scope first. Returns the curve and, at the first point that meets the line, each ruler's best build that fits.",
    anchor: "#forma-plan",
    needs_weapon: true,
    args: {
      rulers: { kind: "array", what: "ruler ids to cover; omit for every ruler with rows" },
      riven: { kind: "boolean", what: "count riven builds" },
      planned_builds_must_fit: { kind: "boolean", what: "the builds planned together must fit the layout too" },
      line: { kind: "number", min: 0.01, max: 1, what: "the share of each ruler's leader to reach, e.g. 0.8" },
    },
    async run({ rulers, riven, planned_builds_must_fit, line }) {
      const w = presetWeapon();
      const all = boardRulersOf(w);
      if (!all.length) return agentNo("no_board_rows", { because: "this weapon has no board builds here" });
      if (rulers && rulers.some((r) => !all.includes(r))) return agentNo("bad_argument", { argument: "rulers", alternatives: all });
      const patch = {};
      if (rulers) patch.benchmarks = all.filter((b) => rulers.includes(b));
      if (riven != null) patch.riven = riven;
      if (planned_builds_must_fit != null) patch.hard = planned_builds_must_fit;
      if (line != null) patch.threshold = line;
      setFormaReachScope(w, patch);
      await runFormaReach();
      renderBuilderFormaPlan();
      return agentReach();
    },
  },
  {
    id: "builder.reach.mark",
    writes: "none",
    what: "Mark a point of the worked-out reach curve by its index, to read and apply that layout.",
    anchor: "#forma-plan",
    needs_weapon: true,
    args: { point: { kind: "number", required: true, min: 0, max: 100, what: "point index" } },
    run({ point }) {
      if (!formaReach || !formaReach.r || !(formaReach.r.curve || [])[point]) return agentNo("no_such_point", { try: "builder.reach.run" });
      formaReach.sel = point;
      renderBuilderFormaPlan();
      return agentReach();
    },
  },
  {
    id: "builder.reach.apply",
    writes: "build",
    what: "Put the open build's slots on the marked point's polarities, as the planner's apply button does.",
    anchor: "#forma-plan",
    needs_weapon: true,
    args: {},
    async run() {
      const pt = formaReach && formaReach.r && (formaReach.r.curve || [])[formaReach.sel];
      if (!pt) return agentNo("no_point_marked", { try: "builder.reach.run" });
      await autoForma({ onto: pt.plan });
      renderMods();
      return { text: `placed on ${reachBill(pt.plan)}` };
    },
  },
  {
    id: "builder.reach.save",
    writes: "none",
    what: "Save a ruler's best-fitting build at the marked point as a build of its own, on that point's polarities.",
    anchor: "#forma-plan",
    needs_weapon: true,
    args: { ruler_index: { kind: "number", required: true, min: 0, max: 20, what: "the ruler's index in the reach read" } },
    run({ ruler_index }) {
      const pt = formaReach && formaReach.r && (formaReach.r.curve || [])[formaReach.sel];
      if (!pt || !pt.picks[ruler_index]) return agentNo("nothing_fits", { try: "builder.reach.run" });
      return { preset: saveReachBuild(ruler_index) };
    },
  },
  {
    id: "simulator.arena.read",
    query: true,
    what: "Read the arena: where the player stands, the target (e1, the body the fight is scored against), every other body in the formation, where the player aims, and the gap to the target in metres. Coordinates are metres on the plane.",
    anchor: "#sim-target-arena",
    needs_weapon: true,
    args: {},
    run() { return agentArenaState(); },
  },
  {
    id: "simulator.arena.distance",
    writes: "scenario",
    what: "Put the target this many metres from the player (the gap between them; 0 is contact, where both boards are scored).",
    anchor: "#sim-target-arena",
    needs_weapon: true,
    args: { meters: { kind: "number", required: true, min: 0, max: 200, what: "gap in metres" } },
    run({ meters }) { return agentArena(() => { setArenaDistance(sim, meters); }); },
  },
  {
    id: "simulator.arena.add",
    writes: "scenario",
    what: "Add bodies around the target, each of the fight's current enemy, in the first free places — the +1/+8 quick sets.",
    anchor: "#sim-target-arena",
    needs_weapon: true,
    args: { count: { kind: "number", required: true, min: 1, max: 50, what: "bodies to add" } },
    run({ count }) {
      return agentArena(() => {
        let n = 0;
        while (n < count && arenaAddFoe(sim)) n++;
        return { added: n, ...agentArenaState() };
      });
    },
  },
  {
    id: "simulator.arena.place",
    writes: "scenario",
    what: "Place one body of the fight's current enemy at a point (metres). Refused where it would overlap another body or the player, or when the floor is full.",
    anchor: "#sim-target-arena",
    needs_weapon: true,
    args: { x: { kind: "number", required: true, min: -500, max: 500, what: "metres" }, y: { kind: "number", required: true, min: -500, max: 500, what: "metres" } },
    run({ x, y }) {
      return agentArena(() => (arenaPlaceBody(sim, [x, y]) ? agentArenaState() : agentNo("place_taken", { because: "overlaps a body or the player, or the floor is full" })));
    },
  },
  {
    id: "simulator.arena.remove",
    writes: "scenario",
    what: "Remove one body from the formation by its id. The target (e1) cannot be removed.",
    anchor: "#sim-target-arena",
    needs_weapon: true,
    args: { body: { kind: "string", required: true, what: "body id" } },
    run({ body }) {
      return agentArena(() => {
        const i = (sim.formation || []).findIndex((f) => f.id === body);
        if (i < 0) return agentNo("bad_argument", { argument: "body", alternatives: (sim.formation || []).map((f) => f.id).slice(0, 20) });
        sim.formation.splice(i, 1);
      });
    },
  },
  {
    id: "simulator.arena.reset",
    writes: "scenario",
    what: "Back to one body, aimed at the target — the fight the boards are measured under.",
    anchor: "#sim-target-arena",
    needs_weapon: true,
    args: {},
    run() { return agentArena(() => { arenaReset(sim); }); },
  },
  {
    id: "simulator.arena.aim",
    writes: "scenario",
    what: "Aim at a point (metres) — aiming is a direction, and the shot hits the first body it crosses — or omit x and y to aim at the target.",
    anchor: "#sim-target-arena",
    needs_weapon: true,
    args: { x: { kind: "number", min: -500, max: 500, what: "metres" }, y: { kind: "number", min: -500, max: 500, what: "metres" } },
    run({ x, y }) {
      if ((x == null) !== (y == null)) return agentNo("bad_argument", { because: "give both x and y, or neither" });
      return agentArena(() => { sim.aim_at = x == null ? null : [x, y]; });
    },
  },
  {
    id: "simulator.auras.list",
    query: true,
    what: "List the squad auras, which of them pay this weapon anything, which stack across the squad, and how many run each in this fight.",
    anchor: "#sim-squad",
    needs_weapon: true,
    args: {},
    run() {
      const paid = new Set((weaponInfo($("weapon").value) || {}).auras || []);
      const on = new Map((sim.auras || []).map((a) => [a.id, a.count || 1]));
      return { auras: AURAS().map((x) => ({ id: x.id, name: LN("auras", x.id, x.name), pays_this_weapon: paid.has(x.id),
        stacks: !!x.squad_stacking, ...(on.has(x.id) ? { running: on.get(x.id) } : {}) })) };
    },
  },
  {
    id: "simulator.aura.set",
    writes: "scenario",
    what: "Set how many of the squad run an aura (1 to 4 for one that stacks, else 1); 0 removes it. An aura is the fight's, never the build's.",
    anchor: "#sim-squad",
    needs_weapon: true,
    args: {
      aura: { kind: "string", required: true, what: "aura id" },
      count: { kind: "number", required: true, min: 0, max: 4, what: "players running it; 0 removes" },
    },
    run({ aura, count }) {
      if (!AURAS().some((x) => x.id === aura)) return agentNo("bad_argument", { argument: "aura", alternatives: AURAS().map((x) => x.id).slice(0, 30) });
      setSquadAura(aura, count);
      refreshPanel(); markScenarioDirty(); renderSim();
      return { auras: sim.auras };
    },
  },
  {
    id: "simulator.abilities.list",
    query: true,
    what: "List the Warframe abilities that can run in the fight as buffs, with each one's value at the fight's ability strength, its element choices, whether it is on, and whether a stronger one of its kind supersedes it.",
    anchor: "#sim-wfbuffs",
    needs_weapon: true,
    args: {},
    run() {
      const running = wfRunning();
      return {
        ability_strength_percent: Math.round(simStrength() * 100),
        ability_strength_typed: typeof sim.ability_strength === "number",
        abilities: wfAbilities().map((a) => {
          const p = wfPick(a.id);
          return { id: a.id, name: wfName(a), frame: a.frame, value: wfValueLabel(a), what: wfEffectLine(a),
            ...((a.elements || []).length ? { elements: a.elements } : {}),
            ...(p ? { on: true, ...(p.element ? { element: p.element } : {}), ...(running.has(a.id) ? {} : { superseded: true }) } : {}) };
        }),
      };
    },
  },
  {
    id: "simulator.ability.set",
    writes: "scenario",
    what: "Run a Warframe ability as a buff for the whole fight (on=true) or stop it; where it offers one, choose its element.",
    anchor: "#sim-wfbuffs",
    needs_weapon: true,
    args: {
      ability: { kind: "string", required: true, what: "ability id" },
      on: { kind: "boolean", required: true, what: "running or not" },
      element: { kind: "string", what: "one of the ability's elements" },
    },
    run({ ability, on, element }) {
      const def = wfAbilities().find((a) => a.id === ability);
      if (!def) return agentNo("bad_argument", { argument: "ability", alternatives: wfAbilities().map((a) => a.id).slice(0, 30) });
      if (element != null && !(def.elements || []).includes(element)) {
        return agentNo("bad_argument", { argument: "element", alternatives: def.elements || [] });
      }
      setWfAbility(ability, on, element);
      markScenarioDirty(); renderSim();
      return { abilities: sim.abilities };
    },
  },
  {
    id: "simulator.strength.set",
    writes: "scenario",
    what: "Override the Warframe's Ability Strength for the fight, in percent as the arsenal shows it; every ability buff scales with it. Without an override the fight reads the linked Warframe build's.",
    anchor: "#sim-wfbuffs",
    needs_weapon: true,
    args: { percent: { kind: "number", required: true, min: 0, max: 1000, what: "e.g. 250" } },
    run({ percent }) {
      setAbilityStrength(percent);
      markScenarioDirty(); renderSim();
      return { ability_strength_percent: percent };
    },
  },
  {
    id: "simulator.triggers.list",
    query: true,
    what: "List the buff triggers the fight can switch off (a buff whose trigger is off never fires, though the run still does the action), by group, with which are off.",
    anchor: "[data-bev], [data-bevg]",
    needs_weapon: true,
    args: {},
    run() {
      const off = new Set(sim.buff_triggers_off || []);
      return { groups: buffTriggerGroups().map((g) => ({ id: g.id, triggers: g.ids.map((id) => ({ id, off: off.has(id) })) })) };
    },
  },
  {
    id: "simulator.trigger.set",
    writes: "scenario",
    what: "Switch a buff trigger off (off=true) or back on, or a whole group of them by the group's id.",
    anchor: "[data-bev], [data-bevg]",
    needs_weapon: true,
    args: {
      id: { kind: "string", required: true, what: "trigger id or group id" },
      off: { kind: "boolean", required: true, what: "true = buffs from it do not fire" },
    },
    run({ id, off }) {
      const groups = buffTriggerGroups();
      if (groups.some((g) => g.id === id)) toggleBuffTriggerGroup(id, off);
      else if ((META.buff_triggers || []).some((t) => t.id === id)) toggleBuffTrigger(id, off);
      else return agentNo("bad_argument", { argument: "id", alternatives: groups.flatMap((g) => [g.id, ...g.ids]).slice(0, 30) });
      return { off: sim.buff_triggers_off || [] };
    },
  },
  {
    id: "simulator.extra.set",
    writes: "scenario",
    what: "Set one of the fight's own stat bonuses — what the weapon is handed by something outside its build (a squad buff, another weapon's arcane) — as a percentage into the same bucket a mod of that stat feeds. 0 or null clears it.",
    anchor: "[data-xk]",
    needs_weapon: true,
    args: {
      stat: { kind: "string", required: true, what: "which stat", enum: () => EXTRA_STAT_KEYS.map(([k]) => k) },
      percent: { kind: "number", required: true, nullable: true, min: -1000, max: 10000, what: "e.g. 30 for +30%" },
    },
    run({ stat, percent }) {
      setExtraStat(stat, percent === null ? 0 : percent / 100);
      renderSim();
      return { extra_stats: sim.extra_stats };
    },
  },
  {
    id: "simulator.rules.list",
    query: true,
    what: "List the per-class rules a fight may make — what the simulator simplifies for a whole weapon class — with each one's default and this fight's value.",
    anchor: "[data-cr]",
    needs_weapon: true,
    args: {},
    run() {
      return { rules: overridablePairs().map(([cls, id]) => ({ class: cls, rule: id, default: classRuleDefault(cls, id),
        ...(classRuleOf(cls, id) !== undefined ? { this_fight: classRuleOf(cls, id) } : {}) })) };
    },
  },
  {
    id: "simulator.rule.set",
    writes: "scenario",
    what: "Set a per-class rule for this fight, or null to fall back to the default. Only the pairs simulator.rules.list gives exist.",
    anchor: "[data-cr]",
    needs_weapon: true,
    args: {
      class: { kind: "string", required: true, what: "weapon class" },
      rule: { kind: "string", required: true, what: "the axis" },
      value: { kind: "scalar", required: true, nullable: true, what: "true/false or a number; null clears" },
    },
    run({ class: cls, rule, value }) {
      if (!overridablePairs().some(([c, id]) => c === cls && id === rule)) {
        return agentNo("bad_argument", { argument: "rule", alternatives: overridablePairs().map((p) => p.join(".")) });
      }
      writeClassRule(cls, rule, value);
      renderSim();
      return { this_fight: classRuleOf(cls, rule) ?? null, default: classRuleDefault(cls, rule) };
    },
  },
  {
    id: "optimizer.plan.read",
    query: true,
    what: "Read the build search: its starts (each a build, with the positions fixed in its answer), its limits (what it may not use per axis — `card@rank` is one rank of a card — and how full it fills) how many builds it answers with and how many fights each candidate gets. Everything the limits leave is a candidate.",
    anchor: "#opt-block",
    needs_weapon: true,
    args: {},
    run() {
      return {
        starts: opt.starts.map((s) => ({ ...startPayload(s), fixed: s.fixed.slice() })),
        limits: JSON.parse(JSON.stringify(opt.limits)),
        blocked: opt.starts.map((s, i) => ({ start: i + 1, ...startConflicts(s) })).filter((x) => x.blocked.length || x.replaced.length),
        candidate_runs: optRun.candidate_runs,
        finalists: optRun.finalists,
        estimate: $("opt-estimate").textContent.trim(),
      };
    },
  },
  {
    id: "optimizer.plan.limit",
    writes: "search",
    what: "Exclude an option from the search, or take it back: a mod as the builder lists it (`card@rank` for one rank of an every-rank card), an arcane, an evolution, a mode or a valence element. Nothing is excluded by default; the last mode or element cannot be.",
    anchor: "#opt-limits",
    needs_weapon: true,
    args: {
      axis: { kind: "string", required: true, what: "which axis", enum: () => LIMIT_AXES },
      id: { kind: "string", required: true, what: "the option's id" },
      excluded: { kind: "boolean", required: true, what: "true excludes it, false takes it back" },
    },
    run({ axis, id, excluded }) {
      const w = weaponInfo($("weapon").value) || {};
      const known = {
        mods: () => excludeOffers("").some((m) => m.id === id),
        arcanes: () => arcaneFitsWeapon(w.id, id),
        evolutions: () => weaponEvos(w.id).some((t) => t.options.some((o) => o.id === id)),
        modes: () => (w.modes || []).length > 1 && w.modes.includes(id),
        valence: () => ((valenceSpec(w.id) || {}).elements || []).includes(id),
      }[axis];
      if (!known()) return agentNo("not_in_scope", { argument: "id", because: `this weapon's ${axis} have no ${id}` });
      if (excluded !== isOut(axis, id)) toggleLimit(axis, id);
      return { excluded: isOut(axis, id), limits: JSON.parse(JSON.stringify(opt.limits)) };
    },
  },
  {
    id: "optimizer.plan.fill",
    writes: "search",
    what: "Set how full the search fills: at most `mods` cards (0-8), whether the exilus is filled, and whether arcane seat `seat` (0-based) is filled.",
    anchor: "#opt-limits",
    needs_weapon: true,
    args: {
      mods: { kind: "number", min: 0, max: 8, what: "the most cards a build holds" },
      exilus: { kind: "boolean", what: "fill the exilus" },
      seat: { kind: "number", min: 0, max: 3, what: "an arcane seat, with `filled`" },
      filled: { kind: "boolean", what: "fill that arcane seat" },
    },
    run({ mods, exilus, seat, filled }) {
      if (mods != null) opt.limits.mods = mods;
      if (exilus != null) opt.limits.exilus = exilus;
      if (seat != null && filled != null) {
        if (seat >= opt.limits.arcane_seats.length) return agentNo("bad_argument", { argument: "seat", because: "this weapon has no such seat" });
        opt.limits.arcane_seats[seat] = filled;
      }
      limitsChanged();
      return { limits: JSON.parse(JSON.stringify(opt.limits)) };
    },
  },
  {
    id: "optimizer.plan.set",
    writes: "search",
    what: "Set how many builds the search answers with (the best N it scored, 1-100) and how many fights each candidate gets while the search compares them (1 or 10).",
    anchor: "#opt-runbar",
    needs_weapon: true,
    args: {
      finalists: { kind: "number", min: 1, max: 100, what: "how many builds the search answers with" },
      candidate_runs: { kind: "number", min: 1, max: 10, what: "1 or 10 fights a candidate" },
    },
    run({ finalists, candidate_runs }) {
      setOptSizes({ finalists, candidate_runs });
      return { finalists: optRun.finalists, candidate_runs: optRun.candidate_runs, estimate: $("opt-estimate").textContent.trim() };
    },
  },
  {
    id: "rivens.cards.list",
    query: true,
    what: "List the rivens saved for this weapon's family, with the id builder.mod.set seats each one by.",
    anchor: "#riven-tools, #riven-all",
    needs_weapon: true,
    args: {},
    run() {
      return { open: activeRivenId() || null, rivens: loadPresetList(RIVENS).map((p) => ({
        id: p.id, name: p.name, seat_as: RIVEN_PREFIX + p.id, card: (rivenNames[p.id] || {}).name || null,
        lines: (rivenNames[p.id] || {}).lines || [] })) };
    },
  },
  {
    id: "rivens.stats.list",
    query: true,
    what: "List the stats a riven for this weapon can roll, which can be the malus, which are modelled, and which are spliced (at most one a card, value unmeasured).",
    anchor: "#riven-stats, #riven-tools",
    needs_weapon: true,
    args: {},
    run() {
      return { shapes: RIVEN_SHAPES.map((x) => x.id), rolls: rivenRules(), stats: rivenPool().map((x) => ({
        id: x.id, name: rivenStatName(x), bonus: x.bonus !== false, malus: !!x.malus, ...(x.modeled ? {} : { modeled: false }),
        ...(x.spliced ? { spliced: true } : {}) })) };
    },
  },
  {
    id: "rivens.card.new",
    writes: "none",
    what: "Make a blank riven for this weapon's family and open it for editing.",
    anchor: "#riven-tools",
    needs_weapon: true,
    args: {},
    run() { return { riven: newRiven() }; },
  },
  {
    id: "rivens.card.copy",
    writes: "none",
    what: "Duplicate the open riven and open the copy.",
    anchor: "#riven-tools",
    needs_weapon: true,
    args: {},
    run() {
      if (!activeRivenId()) return agentNo("no_riven_open", { try: "rivens.card.open" });
      return { riven: copyRiven() };
    },
  },
  {
    id: "rivens.card.open",
    writes: "none",
    what: "Open a saved riven for editing, by the id rivens.cards.list gives.",
    anchor: "#riven-tools, #riven-all",
    needs_weapon: true,
    args: { riven: { kind: "string", required: true, what: "riven id" } },
    run({ riven: id }) {
      const ps = loadPresetList(RIVENS);
      if (!ps.some((p) => p.id === id)) return agentNo("unknown_riven", { alternatives: ps.map((p) => p.id).slice(0, 12) });
      openRiven(id);
      return { text: `opened ${id}` };
    },
  },
  {
    id: "rivens.card.set",
    writes: "riven",
    what: "Write the open riven whole: its shape, each bonus and the malus by stat id with either the value printed on the card or a roll (0.9 to 1.1), its rank and polarity. Returns what the engine made of it — printed values, generated name, anything illegal.",
    anchor: "#riven-shape, #riven-stats, #riven-foot",
    needs_weapon: true,
    args: {
      shape: { kind: "string", required: true, what: "e.g. \"3+1\" (three bonuses and a malus) or \"2+0\"", enum: () => RIVEN_SHAPES.map((x) => x.id) },
      bonuses: { kind: "array", required: true, what: "[{stat, value} or {stat, roll}], one per bonus" },
      malus: { kind: "object", nullable: true, what: "{stat, value} or {stat, roll}; omit or null when the shape has none" },
      rank: { kind: "number", min: 0, max: 8, what: "defaults to the maximum" },
      polarity: { kind: "string", what: "madurai, vazarin or naramon" },
    },
    async run({ shape, bonuses, malus = null, rank, polarity }) {
      if (!activeRivenId() || !riven) return agentNo("no_riven_open", { try: "rivens.card.new" });
      const sh = RIVEN_SHAPES.find((x) => x.id === shape);
      if (bonuses.length !== sh.bonuses) return agentNo("bad_argument", { argument: "bonuses", because: `${shape} has ${sh.bonuses}` });
      if (!!malus !== !!sh.malus) return agentNo("bad_argument", { argument: "malus", because: sh.malus ? `${shape} has a malus` : `${shape} has none` });
      const pool = rivenPool();
      const all = bonuses.concat(malus ? [malus] : []);
      for (const [i, b] of all.entries()) {
        const isMalus = malus && i === all.length - 1;
        const def = pool.find((x) => x.id === (b && b.stat));
        if (!def || (isMalus ? !def.malus : def.bonus === false)) {
          return agentNo("bad_stat", { argument: isMalus ? "malus" : `bonuses[${i}]`, got: b && b.stat,
            alternatives: pool.filter((x) => (isMalus ? x.malus : x.bonus !== false)).map((x) => x.id) });
        }
      }
      if (new Set(all.map((b) => b.stat)).size !== all.length) return agentNo("bad_argument", { because: "a stat appears twice" });
      const pols = rivenRules().polarities || ["madurai", "vazarin", "naramon"];
      if (polarity != null && !pols.includes(polarity)) return agentNo("bad_argument", { argument: "polarity", alternatives: pols });
      const slot = (b) => ({ id: b.stat, roll: typeof b.roll === "number" ? b.roll : 1.0 });
      riven.drafts[riven.shape] = { bonuses: riven.bonuses, malus: riven.malus };
      riven.shape = shape;
      riven.drafts[shape] = { bonuses: bonuses.map(slot), malus: malus ? slot(malus) : null };
      riven.bonuses = riven.drafts[shape].bonuses;
      riven.malus = riven.drafts[shape].malus;
      if (rank != null) riven.rank = rank;
      if (polarity != null) riven.polarity = polarity;
      markRivenDirty();
      renderRivens();
      // A PRINTED VALUE BECOMES ITS ROLL THE WAY THE NUMBER BOX DOES IT: the
      // engine turns it into the roll it implies, clamped, one slot at a time.
      const typed = all.map((b, i) => ({ b, slot: malus && i === all.length - 1 ? "malus" : String(i) }))
        .filter((x) => typeof x.b.value === "number");
      for (const t of typed) await resolveRiven({ slot: t.slot, value: t.b.value });
      await resolveRiven();
      refreshRivenNames();
      return agentRivenCard();
    },
  },
  {
    id: "builder.wielders.list",
    query: true,
    what: "List who can hold this weapon: the Warframes allowed, each with the builds saved for it, and whether the unmodded Prototype (the board's floor) is allowed.",
    anchor: "#wielder-row",
    needs_weapon: true,
    args: {},
    run() { return { current: buildWielder, ...wielderChoices() }; },
  },
  {
    id: "builder.wielder.set",
    writes: "build",
    what: "Choose who holds the weapon: a Warframe unmodded, a Warframe with one of its saved builds, or frame=null for the Prototype. The wielder's abilities and stats change the weapon's numbers.",
    anchor: "#wielder-row",
    needs_weapon: true,
    args: {
      frame: { kind: "string", required: true, nullable: true, what: "Warframe id, or null for the Prototype" },
      build: { kind: "string", what: "one of that frame's saved build ids" },
    },
    run({ frame, build }) {
      const c = wielderChoices();
      if (frame === null) {
        if (!c.prototype_allowed) return agentNo("prototype_not_allowed", { alternatives: c.frames.map((f) => f.id) });
        setWielder(wielderLinkFor(PROTOTYPE_ID));
        return { text: "held by the Prototype" };
      }
      const f = c.frames.find((x) => x.id === frame);
      if (!f) return agentNo("bad_argument", { argument: "frame", alternatives: c.frames.map((x) => x.id).slice(0, 12) });
      if (build != null && !f.builds.some((b) => b.id === build)) {
        return agentNo("bad_argument", { argument: "build", alternatives: f.builds.map((b) => b.id) });
      }
      setWielder(build != null ? { frame, preset: build } : wielderLinkFor(frame));
      return { text: `held by ${f.name}` };
    },
  },
  {
    id: "builder.parts.list",
    query: true,
    what: "List a Kitgun's grips and loaders, with the pair installed now; read their numbers with builder.stats.read after installing.",
    anchor: "#assembly-row",
    needs_weapon: true,
    args: {},
    run() {
      const s = assemblySpec($("weapon").value);
      if (!s) return agentNo("no_parts", { because: "this weapon is not a Kitgun" });
      const row = (x) => ({ id: x.id, name: x.name });
      return { installed: assembly, grips: s.grips.map(row), loaders: s.loaders.map(row), other_slot: slotSibling($("weapon").value) || null };
    },
  },
  {
    id: "builder.part.set",
    writes: "build",
    what: "Swap a Kitgun's grip or loader. The chamber is the weapon itself; the other slot's version of the same chamber is another weapon id (builder.weapon.set).",
    anchor: "#assembly-row",
    needs_weapon: true,
    args: {
      part: { kind: "string", required: true, what: "grip or loader", enum: () => ["grip", "loader"] },
      id: { kind: "string", required: true, what: "the part's id" },
    },
    run({ part, id }) {
      const s = assemblySpec($("weapon").value);
      if (!s) return agentNo("no_parts", { because: "this weapon is not a Kitgun" });
      const list = part === "grip" ? s.grips : s.loaders;
      const hit = list.find((x) => x.id === id);
      if (!hit) return agentNo("bad_argument", { argument: "id", alternatives: list.map((x) => x.id) });
      setAssemblyPart(part, id);
      return { text: `${part} ${hit.name}` };
    },
  },
  {
    id: "builder.board.read",
    query: true,
    what: "Read this weapon's leaderboard: the measured best builds per ruler (the benchmark fight), mode, and with or without a riven, in the page's own order and ranks. A row's id opens it with shell.preset.open (bar \"build\").",
    anchor: "#build-finder",
    needs_weapon: true,
    args: {
      riven: { kind: "string", what: "\"without\" (default), \"with\" or \"any\"", enum: () => ["without", "with", "any"] },
      mode: { kind: "string", what: "only this mode id" },
      limit: { kind: "number", min: 1, max: 20, what: "rows per group, default 3" },
    },
    run({ riven = "without", mode: m, limit = 3 }) {
      const w = weaponInfo($("weapon").value) || {};
      if (!BOARD[w.id]) return agentNo("board_not_loaded", { because: "this weapon has no board rows here yet" });
      const name = (id) => (modById(id) || { name: id }).name;
      const rows = builtinBuilds()
        .filter((p) => (riven === "any" || p.riven === (riven === "with")) && (!m || p.mode === m) && p.rank <= limit)
        .map((p) => {
          const r = p.board || {};
          return {
            id: presetId(p), rank: p.rank, ruler: p.group, mode: p.mode, riven: p.riven, score: p.hint,
            mods: (r.mods || []).filter((x) => x && x !== BOARD_RIVEN_SLOT).map(name),
            ...(r.exilus && r.exilus !== "none" ? { exilus: name(r.exilus) } : {}),
            ...((r.arcanes || []).some((x) => x && x !== "none") ? { arcanes: r.arcanes.filter((x) => x && x !== "none") } : {}),
            ...((r.evolutions || []).some(Boolean) ? { evolutions: r.evolutions.filter(Boolean) } : {}),
            ...(r.riven ? { riven_stats: r.riven } : {}),
          };
        });
      return { rows };
    },
  },
  {
    id: "optimizer.search.start",
    writes: "none",
    what: "Start a build search with the optimizer's current scope against the current fight. Returns at once; a search takes minutes — read its progress with optimizer.search.read.",
    anchor: "#run-opt",
    needs_weapon: true,
    args: {},
    async run() {
      if (optJobId != null) return agentNo("search_running", { try: "optimizer.search.read" });
      if ($("run-opt").disabled) return agentNo("scope_not_runnable", { because: $("opt-estimate").textContent.trim() });
      await runOptimize();
      return { text: "search started", candidates: $("opt-estimate").textContent.trim() };
    },
  },
  {
    id: "optimizer.search.read",
    query: true,
    what: "Read the build search: while it runs, how far along it is; once done, the ranked builds — each with the starts that settled on it (`from_starts`), or the answer it is nearest to and what differs (`near`).",
    anchor: "#opt-results",
    needs_weapon: true,
    args: { limit: { kind: "number", min: 1, max: 100, what: "ranked builds to return, default 5" } },
    run({ limit = 5 }) {
      if (optJobId != null) {
        const st = optLastStatus || {};
        return {
          phase: st.phase || "starting",
          progress: st.sims_planned ? `${Math.round((1000 * st.sims_done) / st.sims_planned) / 10}%` : null,
          elapsed_s: st.elapsed_s != null ? Math.round(st.elapsed_s) : null,
          ...(st.round ? { round: `${st.round}/${st.rounds}` } : {}),
        };
      }
      if (optLast && (optLast.results || []).length) return agentSearchResults(limit);
      return agentNo("no_search", { try: "optimizer.search.start" });
    },
  },
  {
    id: "optimizer.search.stop",
    writes: "none",
    what: "Stop the running build search. What it has ranked so far is kept.",
    anchor: "#run-opt",
    needs_weapon: true,
    args: {},
    async run() {
      if (optJobId == null) return agentNo("no_search_running");
      await cancelOptimize();
      return { text: "stopping" };
    },
  },
  {
    id: "optimizer.result.save",
    writes: "none",
    what: "Save a ranked build from the last search as a new build preset (named opt N). Open it with shell.preset.open to measure it in the simulator.",
    anchor: "#opt-results",
    needs_weapon: true,
    args: { rank: { kind: "number", required: true, min: 1, max: 1000, what: "the row's rank" } },
    async run({ rank }) {
      const res = optLast && (optLast.results || []).find((x) => x.rank === rank);
      if (!res) return agentNo("no_such_result", { argument: "rank", alternatives: ((optLast && optLast.results) || []).map((x) => x.rank).slice(0, 10) });
      return { preset: await addResult(res) };
    },
  },
  {
    id: "shell.presets.list",
    query: true,
    what: "List the saved builds or fight scenarios for this weapon, which one is open, and which are read-only (benchmarks).",
    anchor: "#preset-bar-builder-builds, #preset-bar-simulator-scenarios, #bench-bar-simulator-scenarios, #sim-official-copy, #preset-bar-optimizer",
    needs_weapon: true,
    args: { bar: agentBarArg },
    run({ bar }) { return { rows: agentPresetRows(AGENT_BARS[bar]()) }; },
  },
  {
    id: "shell.preset.open",
    writes: "none",
    what: "Open a saved build or scenario by the id presets.list gives.",
    anchor: "#preset-bar-builder-builds, #preset-bar-simulator-scenarios, #bench-bar-simulator-scenarios, #sim-official-copy, #preset-bar-optimizer",
    needs_weapon: true,
    args: { bar: agentBarArg, preset: { kind: "string", required: true, what: "preset id" } },
    run({ bar, preset }) {
      const cfg = AGENT_BARS[bar]();
      const rows = agentPresetRows(cfg);
      if (!rows.some((r) => r.id === preset)) return agentNo("unknown_preset", { argument: "preset", alternatives: rows.map((r) => r.id).slice(0, 12) });
      pickPreset(cfg, preset);
      return { text: `opened ${preset}` };
    },
  },
  {
    id: "shell.preset.new",
    writes: "none",
    what: "Start a new blank build or scenario and open it; the one open before is kept as it was.",
    anchor: "#preset-bar-builder-builds, #preset-bar-simulator-scenarios, #bench-bar-simulator-scenarios, #sim-official-copy, #preset-bar-optimizer",
    needs_weapon: true,
    args: { bar: agentBarArg },
    run({ bar }) { return { preset: newPreset(AGENT_BARS[bar]()) }; },
  },
  {
    id: "shell.preset.read",
    query: true,
    what: "Read what a saved build holds — its mods, arcanes, evolutions and mode — by the id presets.list gives; the open one is read as it is on screen.",
    anchor: "#preset-bar-builder-builds",
    needs_weapon: true,
    args: { bar: { kind: "string", required: true, what: "which bar", enum: () => ["build"] },
      preset: { kind: "string", required: true, what: "preset id" } },
    run({ preset }) {
      const st = agentBuildState(preset);
      if (!st) return agentNo("unknown_preset", { alternatives: agentPresetRows(buildBarCfg()).map((r) => r.id).slice(0, 12) });
      const name = (id) => (modById(id) || arcaneById(id) || { name: id }).name;
      return {
        mods: (st.slots || []).map((x, i) => (x.mod ? { seat: agentSeatName(i), id: x.mod, name: name(x.mod), rank: x.rank } : null)).filter(Boolean),
        arcanes: (st.arcane || []).filter((x) => x && x !== "none").map((id) => ({ id, name: name(id) })),
        evolutions: st.evoSel, mode: st.mode,
      };
    },
  },
  {
    id: "shell.preset.adopt",
    writes: "build",
    hand: true,
    what: "Write one saved build's contents over another's and open it — the reader taking an agent's copy back. A reader's click only.",
    anchor: "#preset-bar-builder-builds",
    needs_weapon: true,
    args: { bar: { kind: "string", required: true, what: "which bar", enum: () => ["build"] },
      from: { kind: "string", required: true, what: "the build whose contents are taken" },
      into: { kind: "string", required: true, what: "the build they are written over" } },
    run({ from, into }) {
      flushPresetSaves();
      const st = agentBuildState(from);
      const cfg = buildBarCfg();
      const ps = loadPresetList(BUILDS);
      const at = ps.findIndex((p) => presetId(p) === into);
      if (!st || at < 0) return agentNo("unknown_preset", { alternatives: ps.map((p) => presetId(p)).slice(0, 12) });
      ps[at] = { ...ps[at], savedAt: Date.now(), state: JSON.parse(JSON.stringify(st)) };
      storePresetList(BUILDS, ps);
      cfg.setActive(into);
      whileApplying(() => cfg.apply(ps[at].state));
      cfg.rerender();
      return { text: `${from} → ${into}` };
    },
  },
  {
    id: "shell.preset.undo",
    writes: "bar",
    what: "Undo the last change to a bar's documents — the build, the scenario, the search or the riven — or redo it with redo=true.",
    anchor: ".pundo",
    needs_weapon: true,
    args: {
      bar: { kind: "string", required: true, what: "which history", enum: () => Object.keys(AGENT_UNDO) },
      redo: { kind: "boolean", what: "redo instead of undo" },
    },
    run({ bar, redo = false }) {
      const d = AGENT_UNDO[bar];
      if (!(redo ? canRedoIn(d) : canUndoIn(d))) return agentNo(redo ? "nothing_to_redo" : "nothing_to_undo");
      if (redo) redoIn(d); else undoIn(d);
      return { text: redo ? "redone" : "undone" };
    },
  },
  {
    id: "shell.preset.copy",
    writes: "none",
    what: "Duplicate the open build or scenario and open the copy — the way to try changes without touching the reader's own.",
    anchor: "#preset-bar-builder-builds, #preset-bar-simulator-scenarios, #bench-bar-simulator-scenarios, #sim-official-copy, #preset-bar-optimizer",
    needs_weapon: true,
    args: { bar: agentBarArg },
    run({ bar }) { return { preset: copyActivePreset(AGENT_BARS[bar]()) }; },
  },
  {
    id: "builder.stats.read",
    query: true,
    what: "Read the stats panel for the build on screen: every stat per form and part, base and final, with the mod each change came from, and what this weapon's model does not cover.",
    anchor: "#stats-rows",
    needs_weapon: true,
    args: {},
    async run() {
      const r = await api("/api/panel", buildPayload());
      if (!r || r.ok === false) return agentNo("panel_failed", { because: r ? r.error : "no answer" });
      const w = weaponInfo($("weapon").value) || {};
      const gaps = gapsOf(w).slice();
      if (w.passive_unmodeled) gaps.unshift("this weapon's passive is not modelled yet");
      return {
        policy: r.policy,
        not_modelled: gaps.map(trGap),
        forms: (r.forms || []).map((f) => ({
          label: f.label, meta: f.meta,
          stats: (f.stats || []).map(agentStat),
          elements: (f.elements || []).map(agentStat),
          indirect: (f.indirect || []).map(agentStat),
          parts: (f.parts || []).map((pt) => ({
            label: pt.label, meta: pt.meta, damage_total: pt.damage_total, damage: pt.damage,
            stats: (pt.stats || []).map(agentStat),
          })),
        })),
        conditionals: r.conditionals, buffs: r.buffs,
      };
    },
  },
  {
    id: "simulator.result.read",
    query: true,
    what: "Read the last simulated fight for this build: the headline and whether it is still this build's, kills, time to kill, crit and headshot rates, and which damage sources dealt what share.",
    anchor: "#sim-results",
    needs_weapon: true,
    args: {},
    run() { return agentRunSummary() || agentNo("nothing_measured", { try: "simulator.run.start" }); },
  },
  {
    id: "builder.weapons.find",
    query: true,
    what: "Find weapons by name, in any language the page speaks.",
    anchor: "#weapon",
    args: { ...agentFind, query: { ...agentFind.query, required: true } },
    run({ query, limit = 12 }) {
      const q = query.trim().toLowerCase();
      return agentFound((META.weapons || []).filter((w) => searchHit(w, q)), limit,
        (w) => ({ id: w.id, name: w.name, class: w.class }));
    },
  },
  {
    id: "builder.mods.find",
    query: true,
    what: "Find mods this weapon can seat, by name or effect; with a slot, only the ones that slot takes.",
    anchor: "#mod-slots",
    needs_weapon: true,
    args: { ...agentFind, slot: { kind: "seat", what: "0-7, or \"exilus\" / \"stance\"" } },
    run({ query = "", limit = 12, slot }) {
      const q = query.trim().toLowerCase();
      const i = slot == null ? null : agentSeat(slot);
      return agentFound(buildPool().filter((m) => (i == null || modFitsSlot(m, i)) && searchHit(m, q)), limit,
        (m) => ({ id: m.id, name: m.name, polarity: m.polarity, max_rank: m.max_rank,
          drain: modDrain(m, m.max_rank), effects: (m.effects || []).slice(0, 4) }));
    },
  },
  {
    id: "builder.arcanes.find",
    query: true,
    what: "Find the arcanes a seat of this weapon takes.",
    anchor: "#arcane-slots",
    needs_weapon: true,
    args: { ...agentFind, seat: { kind: "number", min: 0, max: 1, what: "arcane seat, default 0" } },
    run({ query = "", limit = 12, seat = 0 }) {
      const q = query.trim().toLowerCase();
      return agentFound(arcanePool(seat).filter((a) => searchHit(a, q)), limit,
        (a) => ({ id: a.id, name: a.name, max_rank: a.max_rank, effects: effectsAt(a, a.max_rank).slice(0, 4) }));
    },
  },
  {
    id: "builder.evolutions.list",
    query: true,
    what: "List this weapon's Incarnon evolution tiers, their options, and which tiers are open.",
    anchor: "#evo-rows",
    needs_weapon: true,
    args: {},
    run() {
      return {
        open_to: evoOpenTo(),
        tiers: weaponEvos().map((t) => ({
          tier: t.tier, installed: evoSel[t.tier] || null,
          options: (t.options || []).map((x) => ({ id: x.id, name: x.name, lines: evoLines(x),
            ...(x.broken ? { broken: "does not work in game; simulated as no effect" } : {}) })),
        })),
      };
    },
  },
  {
    id: "builder.modes.list",
    query: true,
    what: "List how this weapon can be played, and why a mode is unavailable where it is.",
    anchor: "#mode-row",
    needs_weapon: true,
    args: {},
    run() {
      return { current: mode, modes: modeOpts(weaponInfo($("weapon").value) || {})
        .map(([id, label, off]) => ({ id, label, ...(off ? { unavailable: off } : {}) })) };
    },
  },
  {
    id: "simulator.enemies.find",
    query: true,
    what: "Find targets for the fight by name, in any language the page speaks.",
    anchor: "#sim-target",
    args: { ...agentFind, query: { ...agentFind.query, required: true } },
    run({ query, limit = 12 }) {
      const q = query.trim().toLowerCase();
      return agentFound(allEnemies().filter((e) => searchHit(e, q)), limit,
        (e) => ({ id: e.id, name: e.name, faction: e.faction, can_be_eximus: !!e.can_be_eximus }));
    },
  },
  {
    id: "shell.module.open",
    writes: "none",
    what: "Open a module of the page, optionally on another weapon.",
    anchor: "#module-tabs",
    args: {
      module: { kind: "string", required: true, what: "which module", enum: () => [...AGENT_MODULES, "home"] },
      weapon: { kind: "string", what: "weapon id; defaults to the one already open", enum: agentWeaponIds },
    },
    run({ module, weapon }) {
      if (module === "home") { nav("/"); return { text: "opened the roster" }; }
      const id = weapon || agentRoute().weapon;
      if (!id) return agentNo("no_weapon_open", { argument: "weapon" });
      nav(weaponPath(id) + (module === "builder" ? "" : "/" + module));
      return { text: `opened ${module} on ${weaponInfo(id).name}` };
    },
  },
  {
    id: "builder.weapon.set",
    writes: "none",
    what: "Switch the builder to another weapon. Its own presets and fight come with it.",
    anchor: "#weapon, #wsearch-input",
    args: { weapon: { kind: "string", required: true, what: "weapon id", enum: agentWeaponIds } },
    run({ weapon }) {
      switchWeapon(weapon);
      if (!document.querySelector(".config-page").hidden) nav(weaponModPath(weapon));
      return { text: `switched to ${weaponInfo(weapon).name}` };
    },
  },
  {
    id: "builder.mod.set",
    writes: "build",
    what: "Seat a mod in a slot, or empty the slot with mod=null. A mod already seated elsewhere is exchanged with this slot, as it is when a reader picks it.",
    anchor: "#mod-slots, #exilus",
    needs_weapon: true,
    args: {
      slot: { kind: "seat", required: true, what: "0-7, or \"exilus\" / \"stance\"" },
      mod: { kind: "string", required: true, nullable: true, what: "mod id, or null to empty the slot" },
      rank: { kind: "number", min: 0, max: 20, what: "defaults to the mod's maximum" },
    },
    run({ slot, mod, rank }) {
      const i = agentSeat(slot);
      if (mod === null) { equipMod(i, null); renderMods(); return { text: `emptied slot ${slot}` }; }
      const m = modById(mod);
      if (!m) return agentNo("unknown_mod", { argument: "mod", got: mod });
      if (!buildPool().some((x) => x.id === mod) || !modFitsSlot(m, i)) {
        return agentNo("not_equippable", { argument: "mod", because: "this weapon or this slot cannot hold it", try: "builder.mods.find" });
      }
      if (rank != null && rank > m.max_rank) return agentNo("out_of_range", { argument: "rank", min: 0, max: m.max_rank });
      equipMod(i, mod, rank);
      renderMods();
      return { text: `seated ${m.name} in slot ${slot}` };
    },
  },
  {
    id: "builder.mods.clear",
    writes: "build",
    what: "Empty every mod slot, leaving the weapon bare.",
    anchor: "#clear-mods",
    needs_weapon: true,
    args: {},
    run() { clearMods(); return { text: "cleared the build" }; },
  },
  {
    id: "builder.polarity.set",
    writes: "build",
    what: "Set a slot's polarity, or remove it with polarity=null. The mod in the slot stays.",
    anchor: "#mod-slots .pol-btn, #exilus .pol-btn",
    needs_weapon: true,
    args: {
      slot: { kind: "seat", required: true, what: "0-7, or \"exilus\" / \"stance\"" },
      polarity: { kind: "string", required: true, nullable: true, what: "polarity, or null", enum: () => GUN_POLS },
    },
    run({ slot, polarity }) {
      setSlotPolarity(agentSeat(slot), polarity);
      renderMods();
      return { text: `slot ${slot} is ${polarity || "unpolarized"}` };
    },
  },
  {
    id: "builder.forma.plan",
    writes: "build",
    what: "Re-polarize the slots for the fewest Forma that fit the seated mods, as the auto button does.",
    anchor: "#auto-forma",
    needs_weapon: true,
    args: {},
    async run() { await autoForma(); renderMods(); return { text: "planned the Forma" }; },
  },
  {
    id: "builder.arcane.set",
    writes: "build",
    what: "Seat an arcane, or empty the seat with arcane=null. Only arcanes this weapon's seat takes are accepted.",
    anchor: "#arcane-slots",
    needs_weapon: true,
    args: {
      seat: { kind: "number", required: true, min: 0, max: 1, what: "arcane seat, 0 unless the weapon has two" },
      arcane: { kind: "string", required: true, nullable: true, what: "arcane id, or null" },
      rank: { kind: "number", min: 0, max: 10, what: "defaults to the arcane's maximum" },
    },
    run({ seat, arcane, rank }) {
      if (seat >= arcanePools().length) return agentNo("out_of_range", { argument: "seat", min: 0, max: arcanePools().length - 1 });
      if (arcane === null) { setArcane("none", seat); renderArcanes(); return { text: `emptied arcane seat ${seat}` }; }
      const a = arcanePool(seat).find((x) => x.id === arcane);
      if (!a) return agentNo("not_equippable", { argument: "arcane", alternatives: arcanePool(seat).map((x) => x.id).slice(0, 8) });
      if (rank != null && rank > a.max_rank) return agentNo("out_of_range", { argument: "rank", min: 0, max: a.max_rank });
      setArcane(arcane, seat);
      if (rank != null) setArcaneRank(seat, rank);
      renderArcanes();
      return { text: `seated ${a.name}` };
    },
  },
  {
    id: "builder.evolution.set",
    writes: "build",
    what: "Install an Incarnon evolution in a tier, or empty it with evolution=null (which empties every tier after it). Tier N opens only once tier N-1 is filled.",
    anchor: "#evo-rows",
    needs_weapon: true,
    args: {
      tier: { kind: "number", required: true, min: 1, max: 4, what: "evolution tier" },
      evolution: { kind: "string", required: true, nullable: true, what: "evolution id, or null" },
    },
    run({ tier, evolution }) {
      const t = weaponEvos().find((x) => x.tier === tier);
      if (!t) return agentNo("no_such_tier", { argument: "tier", alternatives: weaponEvos().map((x) => x.tier) });
      if (evolution !== null) {
        if (tier > evoOpenTo()) return agentNo("tier_locked", { argument: "tier", open_to: evoOpenTo() });
        if (!(t.options || []).some((x) => x.id === evolution)) {
          return agentNo("bad_argument", { argument: "evolution", alternatives: (t.options || []).map((x) => x.id) });
        }
      }
      pickEvolution(tier, evolution);
      return { text: evolution ? `installed ${evolution}` : `emptied tier ${tier}` };
    },
  },
  {
    id: "builder.mode.set",
    writes: "build",
    what: "Choose how the build is played — the weapon's firing mode or form.",
    anchor: "#mode-row",
    needs_weapon: true,
    args: { mode: { kind: "string", required: true, what: "mode id" } },
    run({ mode: v }) {
      const opts = modeOpts(weaponInfo($("weapon").value) || {});
      const o = opts.find(([id]) => id === v);
      if (!o) return agentNo("bad_argument", { argument: "mode", alternatives: opts.map(([id]) => id) });
      if (o[2]) return agentNo("mode_unavailable", { argument: "mode", because: o[2] });
      setMode(v);
      return { text: `playing ${o[1]}` };
    },
  },
  {
    id: "builder.valence.set",
    writes: "build",
    what: "An adversary weapon's valence: its element and its bonus as a fraction of base damage (clamped to what a Lich can roll).",
    anchor: "#element-cfg",
    needs_weapon: true,
    args: {
      element: { kind: "string", what: "element" },
      bonus: { kind: "number", min: 0, max: 1, what: "e.g. 0.6 for 60%" },
    },
    run({ element, bonus }) {
      const s = valenceSpec($("weapon").value);
      if (!s) return agentNo("no_valence", { because: "this weapon is not an adversary weapon" });
      if (element != null && !s.elements.includes(element)) return agentNo("bad_argument", { argument: "element", alternatives: s.elements });
      setValence({ element, bonus });
      return { text: `valence ${valence.element} ${Math.round(valence.bonus * 1000) / 10}%` };
    },
  },
  {
    id: "simulator.scenario.set",
    writes: "scenario",
    what: "Change fields of the fight — the enemy, its level, the duration, the metric. Fields that are not a single value (the formation, the buffs) have their own editors and are refused here.",
    anchor: "#sim-target, #dd-metric, input[data-k], select[data-k]",
    needs_weapon: true,
    args: { patch: { kind: "object", required: true, what: "fight fields to set" } },
    run({ patch }) {
      const shape = agentScenario();
      for (const [k, v] of Object.entries(patch)) {
        if (!(k in shape)) return agentNo("unknown_scenario_field", { argument: k, alternatives: Object.keys(shape).filter((x) => typeof shape[x] !== "object").slice(0, 12) });
        if (shape[k] !== null && typeof shape[k] === "object") return agentNo("not_a_scalar_field", { argument: k });
        if (v !== null && typeof v === "object") return agentNo("bad_argument", { argument: k, got: v });
      }
      if ("enemy" in patch && !allEnemies().some((e) => e.id === patch.enemy)) {
        return agentNo("unknown_enemy", { argument: "enemy", got: patch.enemy });
      }
      if ("metric" in patch && !(META.metrics || []).some((m) => m.id === patch.metric)) {
        return agentNo("unknown_metric", { argument: "metric", alternatives: (META.metrics || []).map((m) => m.id) });
      }
      setScenarioFields(patch);
      return { text: `set ${Object.keys(patch).join(", ")}` };
    },
  },
  {
    id: "simulator.runs.set",
    writes: "prefs",
    what: "How many times the simulator replays the fight. A preference of this browser, not part of the fight.",
    anchor: "#sim-runs-block",
    needs_weapon: true,
    args: { runs: { kind: "number", required: true, min: 1, max: 20000, what: "replays per measurement" } },
    run({ runs }) { setSimRuns(runs); renderSimRuns(); return { text: `${runs} runs per measurement` }; },
  },
  {
    id: "simulator.run.start",
    writes: "none",
    what: "Run the fight and return the headline number. Takes as long as the reader's own run takes.",
    anchor: "#run-sim",
    needs_weapon: true,
    args: {},
    async run() {
      await runSim();
      const r = agentResult();
      return r ? { result: r, text: `${sig2(r.value)} ${r.unit}` } : agentNo("nothing_measured");
    },
  },
  ...SHAPLEY_ACTIONS, ...ROSTER_ACTIONS]; // …and the ones declared beside what they act on

/// THE PUBLIC NAME. Everything an outside caller may touch, and nothing else:
/// `observe` to see, `do` to act, `tools` to learn the table, `actions` to
/// read it.
window.wfsim = {
  v: AGENT_DOOR_V,
  observe: agentObserve,
  do: agentDo,
  tools: agentTools,
  get skills() { return agentSkills(); },
  // THE PAGE'S UI KIT, for a panel that lives on the page (Nona's): its
  // translation, its dropdown and its escaping, so such a panel looks and
  // reads like the page without reaching into it.
  ui: { tr: (s) => tr(s), dd: (id, cfg) => ddButton(id, cfg), esc: (s) => escHtml(s) },
  get actions() {
    return AGENT_ACTIONS.map((a) => ({ id: a.id, what: a.what, anchor: a.anchor, query: !!a.query,
      writes: a.writes || null, hand: !!a.hand }));
  },
};

// THE BOOT IS OVER, one way or the other, and the page must say which.
//
// Writing a failure into `.config-page` puts it where the home page hides it,
// so a boot that fails there leaves a blank screen and no reason. The banner
// installed in the document head is visible on every page and outlives an
// app.js that never
// parsed; this hands it the reason and clears the placeholder on success.
init()
  .then(() => {
    window.__wfsimReady = true;
    const b = document.getElementById("booting");
    if (b) b.remove();
    // NONA IS A MODULE OF HER OWN (`nona/index.js`), loaded after this script;
    // she mounts on this event, or off `observe().ready` if she loads later.
    window.dispatchEvent(new Event("wfsim:ready"));
    // THE DESKTOP BUILD IS THE ONLY ONE WITH ANYTHING TO MOUNT HERE. The web
    // build's download entry is a static link in the topbar's overflow panel,
    // so there is nothing for it to draw.
    try {
      if (window.__WFSIM_DESKTOP__) mountDesktopUpdater();
    } catch (_) { /* the app runs without it */ }
  })
  .catch((e) => {
    // …unless the reason already put its own, better sentence on the page.
    if (bootReported) return;
    if (window.__wfsimBootFailed) {
      window.__wfsimBootFailed(
        "WFSim could not start. / WFSim 启动失败。",
        String((e && (e.stack || e.message)) || e),
      );
    }
  });
