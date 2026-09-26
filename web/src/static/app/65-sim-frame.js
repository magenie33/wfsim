// ---- WHAT THE FRAME DOES ------------------------------------------------
//
// docs/BUFFS.md §"Cast, or assumed up" and §"The Operator's actions". Under the
// simulator's build card: every conditional source the wielder's build carries,
// each OFF, ASSUMED up, or SIMULATED by the fight — and the rules the fight's
// action list plans for the frame. Each state is stored where its fact lives —
// a tick on the Operator build, an action in the scenario's list — so this
// block holds no state of its own.

const aplRules = () => (Array.isArray(sim.apl) ? sim.apl : (sim.apl = []));
const aplIsOp = (a) => (a || {}).do === "operator";
const aplIsSling = (a) => aplIsOp(a) && !!a.sling;
const aplIsCast = (id) => (a) => (a || {}).do === "cast" && a.ability === id;
const aplHas = (pred) => aplRules().some((r) => pred(r.action));
const aplDrop = (pred) => { sim.apl = aplRules().filter((r) => !pred(r.action)); };

/// THE OPERATOR THE WIELDER LINKS — the preset a tick is written to, its state
/// and its school. The same resolution the payload makes (`operatorPickFor`).
function simOperator() {
  const held = buildWielder;
  if (!WFCAT || !held || isHostId(held.frame)) return null;
  const f = (META.warframes || []).find((x) => x.id === held.frame);
  if (!f) return null;
  const b = wielderBuild(held);
  const st = wfNormalize(b ? b.state : null, f.id);
  const preset = opBuildOf(st.operator);
  const state = opNormalize(preset ? preset.state : null);
  return { preset, state, school: focusSchool(state.school) };
}

/// A TICK IS THE OPERATOR BUILD'S, so it is written there — every Warframe
/// build linking that Operator reads the same answer.
function setOperatorTick(preset, node, on) {
  const ps = opList();
  const i = ps.findIndex((p) => p.id === preset.id);
  if (i < 0) return;
  const st = opNormalize(ps[i].state);
  st.assumed = on ? [...new Set([...st.assumed, node])] : st.assumed.filter((x) => x !== node);
  ps[i] = { ...ps[i], savedAt: Date.now(), state: st };
  storePresetList(OPS, ps);
  // THE WIELDER MOVED, so the panel that reports it is asked again.
  if (typeof refreshPanel === "function") refreshPanel();
}

/// A NODE'S STATE: simulated when the list performs its action, else the tick.
const nodeState = (o, n) =>
  (n.trigger === "operator_sling" && aplHas(aplIsSling)) ? "sim"
    : o.state.assumed.includes(n.id) ? "on" : "off";

/// ONE OPERATOR TRIP: a Chained Sling and/or the school's ability. A sling is
/// done ONCE before an Exalted weapon's summon — the claws keep the snapshot,
/// so re-slinging only stands still — and kept up otherwise.
const opRule = (sling, ability, node, w) => ({ action: { do: "operator", sling, ability },
  when: (w && w.summoned_by) || !sling ? { if: "once" } : { if: "buff_remains_under", ability: node, seconds: 0 } });
const slingNode = () => ((((simOperator() || {}).school) || { nodes: [] }).nodes
  .find((n) => n.trigger === "operator_sling") || { id: "sling_strength" }).id;
/// The list's one Operator trip, if it has one.
const aplOpRule = () => aplRules().find((r) => aplIsOp(r.action));
/// TURN ONE PART OF THE TRIP ON OR OFF; a trip with neither part left is dropped.
function setOpPart(part, on) {
  const r = aplOpRule();
  const w = weaponInfo($("weapon").value);
  if (!r) {
    // THE OPERATOR ACTS FIRST: a cast below it reads what it earned.
    if (on) sim.apl = [opRule(part === "sling", part === "ability", slingNode(), w), ...aplRules()];
    return;
  }
  r.action[part] = on;
  if (!r.action.sling && !r.action.ability) aplDrop(aplIsOp);
  else if (!r.action.sling) r.when = { if: "once" };
}
const castRule = (id) => ({ action: { do: "cast", ability: id }, when: { if: "always" } });

function setNodeState(o, n, s) {
  if (s === "sim") {
    if (o.preset) setOperatorTick(o.preset, n.id, false);
    setOpPart("sling", true);
  } else {
    if (n.trigger) setOpPart("sling", false);
    if (o.preset) setOperatorTick(o.preset, n.id, s === "on");
  }
  markScenarioDirty();
  renderSim();
}

/// What a planned rule is called on the page.
function aplRuleLabel(r) {
  const a = r.action || {};
  if (aplIsOp(a)) return tr("Operator: Transference and back");
  if (a.do === "cast") {
    const card = ((WFCAT && WFCAT.abilities) || []).find((x) => x.id === a.ability)
      || wfAbilities().find((x) => x.id === a.ability);
    const name = ((I18N && I18N.abilities) || {})[a.ability] || (card && card.name) || a.ability;
    return `${tr("Cast")} ${name}`;
  }
  return a.do || "?";
}

/// THE ACTIONS THE LIST COULD ADD: the Operator's sling, the ability that
/// summons this weapon, and every ticked buff with a stated price.
function aplCandidates(w) {
  const out = [];
  if (!aplOpRule()) {
    out.push({ key: "sling", label: tr("Operator: Transference and back"), rule: opRule(true, false, slingNode(), w) });
  }
  if (w && w.summoned_by && !aplHas(aplIsCast(w.summoned_by))) {
    out.push({ key: w.summoned_by, label: aplRuleLabel(castRule(w.summoned_by)), rule: castRule(w.summoned_by) });
  }
  for (const p of sim.abilities || []) {
    const def = wfAbilities().find((a) => a.id === p.id);
    if (def && def.castable && !aplHas(aplIsCast(p.id))) {
      out.push({ key: p.id, label: aplRuleLabel(castRule(p.id)), rule: castRule(p.id) });
    }
  }
  return out;
}

/// The seconds early a rule re-acts: `buff_remains_under` names them.
const aplLead = (r) => ((r.when || {}).if === "buff_remains_under" ? Number(r.when.seconds) || 0 : 0);
const aplOnce = (r) => (r.when || {}).if === "once";
/// `once`, or KEPT UP — re-done when it lapses, or `seconds` before.
function setAplWhen(r, once, seconds) {
  const a = r.action || {};
  const key = aplIsSling(a) ? (((r.when || {}).ability) || slingNode()) : a.ability;
  r.when = once ? { if: "once" }
    : seconds > 0 || aplIsSling(a) ? { if: "buff_remains_under", ability: key, seconds: Math.max(0, seconds) }
    : { if: "always" };
}

/// THE WIELDER'S WARFRAME BUILD AS STORED — the preset a setting is written to
/// (null for the blank build, which stores nothing), and its arcanes.
function simWarframe() {
  const held = buildWielder;
  if (!WFCAT || !held || isHostId(held.frame) || held.frame === PROTOTYPE_ID) return null;
  const preset = wielderBuild(held);
  const st = wfNormalize(preset ? preset.state : null, held.frame);
  return { preset, frame: held.frame, arcanes: st.arcanes.filter((a) => a.id) };
}

/// AN ARCANE'S OPENING STACKS ARE THE WARFRAME BUILD'S, so they are written there.
function setArcaneStacks(id, stacks) {
  const f = simWarframe();
  if (!f || !f.preset) return;
  const ps = presetListWithIds(WF_BUILDS, f.frame);
  const i = ps.findIndex((p) => p.id === f.preset.id);
  if (i < 0) return;
  const st = JSON.parse(JSON.stringify(ps[i].state || {}));
  (st.arcanes || []).forEach((a) => { if (a && a.id === id) a.stacks = stacks; });
  ps[i] = { ...ps[i], savedAt: Date.now(), state: st };
  storePresetList(WF_BUILDS, ps, f.frame);
  if (typeof refreshPanel === "function") refreshPanel();
  renderSim();
}

/// THE WIELDER'S ARCANES THAT A FIGHT RUNS, each with the one setting its rule
/// has — Molt Augmented's opening stacks — or what earns it.
function arcaneRows(locked) {
  const f = simWarframe();
  if (!f) return "";
  const op = aplOpRule();
  const rows = f.arcanes.map((p) => {
    const a = wfArcane(p.id);
    const rule = a && (a.fight_rules || [])[0];
    if (!rule) return "";
    const head = `<div class="sf-name"><b>${escHtml(a.name)}</b>`;
    if (rule.rule === "per_kill") {
      const off = f.preset ? "" : " disabled";
      return `<div class="sf-row">${head}<span class="sb-empty">${escHtml(tr(f.preset
          ? "stacks when the fight opens; kills during it add none yet"
          : "save the Warframe build to set its opening stacks"))}</span></div>
        <input type="number" class="sf-lead" data-arc-stacks="${escHtml(p.id)}" min="0" max="${rule.max_stacks}" value="${p.stacks || 0}"${off}>
        <button class="ghost-btn small" data-arc-full="${escHtml(p.id)}" data-max="${rule.max_stacks}"${off}>${escHtml(tr("full"))}</button></div>`;
    }
    const armed = !!(op && op.action.ability);
    const note = rule.rule === "after_operator_ability"
      ? (armed ? tr("spent by the first cast after the Operator's school ability")
        : tr("needs the Operator's school ability in the action list"))
      : rule.rule === "per_cast_stack" ? tr("each cast in the action list arms the next; the same ability twice running starts over")
      : rule.rule === "per_max_health" ? tr("from the build's Max Health")
      : tr("armed by the weapon's own hits in the fight");
    const arm = rule.rule === "after_operator_ability" && !armed && !locked
      ? `<button class="ghost-btn small" data-arc-vigor="1">+ ${escHtml(tr("school ability"))}</button>` : "";
    return `<div class="sf-row">${head}<span class="sb-empty">${escHtml(note)}</span></div>${arm}</div>`;
  }).join("");
  return rows ? `<div class="sb-h">${escHtml(tr("Arcanes"))}</div>${rows}` : "";
}

async function renderSimFrame(host) {
  await loadWarframeCatalog();
  if (!host || !host.isConnected) return;
  const w = weaponInfo($("weapon").value);
  const o = simOperator();
  const locked = typeof officialScenarioActive === "function" && officialScenarioActive();
  const seg = (id, s, now, label, off) =>
    `<span class="seg${s === now ? " on" : ""}${off ? " dis" : ""}" data-node="${escHtml(id)}" data-s="${s}">${escHtml(label)}</span>`;

  // THE BUILD'S CONDITIONAL SOURCES — the linked Operator's nodes that are not
  // always on. OFF and ASSUMED are the Operator build's tick; SIMULATED is the
  // fight's list performing the action, offered only where the fight can.
  const nodes = o && o.school ? o.school.nodes.filter((n) => !n.always) : [];
  const nodeRows = nodes.map((n) => {
    const now = nodeState(o, n);
    return `<div class="sf-row"><div class="sf-name"><b>${escHtml(n.name)}</b>
        <span class="sb-empty">${escHtml(n.text)}</span></div>
      <span class="oseg">${seg(n.id, "off", now, tr("off"), !o.preset)}${seg(n.id, "on", now, tr("assumed up"), !o.preset)}${
        n.trigger ? seg(n.id, "sim", now, tr("simulated"), locked) : ""}</span></div>`;
  }).join("");

  // THE LIST'S PLANNED RULES, in the order the fight scans them.
  const rules = aplRules();
  const ruleRows = rules.map((r, i) => {
    const lead = aplLead(r);
    const summon = (r.action || {}).do === "cast" && w && r.action.ability === w.summoned_by;
    const once = aplOnce(r);
    const op = aplIsOp(r.action);
    // A TRIP WITH NO SLING EARNS NO WINDOW, so it has nothing to keep up.
    const fixedOnce = summon || (op && !r.action.sling);
    const part = (k, label) => `<span class="seg${r.action[k] ? " on" : ""}${locked ? " dis" : ""}" data-apl-part="${k}">${escHtml(label)}</span>`;
    return `<div class="sf-row"><div class="sf-name"><code>${escHtml(aplRuleLabel(r))}</code>
        <span class="sb-empty">${escHtml(summon ? tr("once, at its turn — the snapshot holds for the fight")
          : once ? tr("once, at its turn") : tr("again when it lapses, or this many seconds before"))}</span></div>
      ${op ? `<span class="oseg">${part("sling", tr("Chained Sling"))}${part("ability", tr("school ability"))}</span>` : ""}
      ${fixedOnce ? "" : `<span class="oseg"><span class="seg${once ? " on" : ""}${locked ? " dis" : ""}" data-apl-once="${i}" data-s="1">${escHtml(tr("once"))}</span><span class="seg${once ? "" : " on"}${locked ? " dis" : ""}" data-apl-once="${i}" data-s="0">${escHtml(tr("keep up"))}</span></span>`}
      ${fixedOnce || once ? "" : `<input type="number" class="sf-lead" data-apl-lead="${i}" min="0" max="60" step="0.5" value="${lead}"${locked ? " disabled" : ""}>`}
      <button class="ghost-btn small" data-apl-up="${i}"${i === 0 || locked ? " disabled" : ""}>↑</button>
      <button class="ghost-btn small" data-apl-down="${i}"${i === rules.length - 1 || locked ? " disabled" : ""}>↓</button>
      <button class="ghost-btn small" data-apl-del="${i}"${locked ? " disabled" : ""}>×</button></div>`;
  }).join("");
  const adds = aplCandidates(w);
  const frame = shownResult && shownResult.r && shownResult.r.frame;
  const pct = (x) => `${Math.round(x * 100)}%`;

  host.innerHTML = `<div class="sb-h">${escHtml(tr("What the frame does"))}</div>`
    + (o ? (o.school
      ? `<div class="sb-empty">${escHtml(o.school.name)}${o.preset ? ` · ${escHtml(o.preset.name)}` : ` · ${escHtml(tr("no linked Operator — link one on the Warframe page to tick its nodes"))}`}</div>${nodeRows}`
      : "") : `<div class="sb-empty">${escHtml(tr("no Warframe holds this weapon"))}</div>`)
    + arcaneRows(locked)
    + `<div class="sb-h">${escHtml(tr("Planned actions"))}</div>`
    + (ruleRows || `<div class="sb-empty">${escHtml(tr("none — every ticked source is assumed up and nobody paid for it"))}</div>`)
    + (adds.length && !locked
      ? `<div class="sf-add">${adds.map((c) => `<button class="ghost-btn small" data-apl-add="${escHtml(c.key)}">+ ${escHtml(c.label)}</button>`).join(" ")}</div>`
      : "")
    + (frame ? `<div class="sb-empty">${escHtml(tr("last run"))}: ${escHtml(tr("Ability Strength"))} ${pct(frame.ability_strength)}${
        frame.summon_strength != null ? ` · ${escHtml(tr("summoned at"))} ${pct(frame.summon_strength)}` : ""}${
        frame.busy_seconds > 0 ? ` · ${escHtml(tr("not attacking for"))} ${frame.busy_seconds.toFixed(1)} s` : ""}</div>` : "");

  const redraw = () => { markScenarioDirty(); renderSim(); };
  host.querySelectorAll("[data-node]").forEach((el) => el.addEventListener("click", () => {
    const n = nodes.find((x) => x.id === el.dataset.node);
    if (n && !el.classList.contains("dis") && nodeState(o, n) !== el.dataset.s) setNodeState(o, n, el.dataset.s);
  }));
  host.querySelectorAll("[data-apl-add]").forEach((el) => el.addEventListener("click", () => {
    const c = adds.find((x) => x.key === el.dataset.aplAdd);
    if (!c) return;
    // THE OPERATOR ACTS FIRST, so the casts after it snapshot what it earned.
    sim.apl = c.key === "sling" ? [c.rule, ...aplRules()] : [...aplRules(), c.rule];
    if (c.key === "sling" && o && o.preset) {
      nodes.filter((n) => n.trigger === "operator_sling").forEach((n) => setOperatorTick(o.preset, n.id, false));
    }
    redraw();
  }));
  host.querySelectorAll("[data-apl-del]").forEach((el) => el.addEventListener("click", () => {
    sim.apl = aplRules().filter((_, i) => i !== Number(el.dataset.aplDel));
    redraw();
  }));
  const move = (i, by) => {
    const r = aplRules().slice();
    const j = i + by;
    if (j < 0 || j >= r.length) return;
    [r[i], r[j]] = [r[j], r[i]];
    sim.apl = r;
    redraw();
  };
  host.querySelectorAll("[data-apl-up]").forEach((el) => el.addEventListener("click", () => move(Number(el.dataset.aplUp), -1)));
  host.querySelectorAll("[data-apl-down]").forEach((el) => el.addEventListener("click", () => move(Number(el.dataset.aplDown), 1)));
  host.querySelectorAll("[data-apl-lead]").forEach((el) => el.addEventListener("change", () => {
    const r = aplRules()[Number(el.dataset.aplLead)];
    if (r) { setAplWhen(r, false, Number(el.value) || 0); redraw(); }
  }));
  host.querySelectorAll("[data-apl-once]").forEach((el) => el.addEventListener("click", () => {
    const r = aplRules()[Number(el.dataset.aplOnce)];
    const once = el.dataset.s === "1";
    if (r && !el.classList.contains("dis") && once !== aplOnce(r)) { setAplWhen(r, once, 0); redraw(); }
  }));
  host.querySelectorAll("[data-apl-part]").forEach((el) => el.addEventListener("click", () => {
    const r = aplOpRule();
    if (!r || el.classList.contains("dis")) return;
    const k = el.dataset.aplPart;
    if (k === "sling" && o && o.preset && !r.action.sling) {
      nodes.filter((n) => n.trigger === "operator_sling").forEach((n) => setOperatorTick(o.preset, n.id, false));
    }
    setOpPart(k, !r.action[k]);
    redraw();
  }));
  host.querySelectorAll("[data-arc-stacks]").forEach((el) => el.addEventListener("change", () => {
    setArcaneStacks(el.dataset.arcStacks, Math.max(0, Math.round(Number(el.value) || 0)));
  }));
  host.querySelectorAll("[data-arc-full]").forEach((el) => el.addEventListener("click", () => {
    setArcaneStacks(el.dataset.arcFull, Number(el.dataset.max) || 0);
  }));
  host.querySelectorAll("[data-arc-vigor]").forEach((el) => el.addEventListener("click", () => {
    setOpPart("ability", true);
    redraw();
  }));
}
