// ---- The optimizer tab: the run bar, the starts, the results, the fight ----
//
// There is no scope to mark: the candidates are the quick calc's (docs/
// OPTIMIZER.md, "The quick descent"), and a start's pins are the one way to
// keep something. The fight is the simulator's, drawn as a card.

/// THE FINAL ROUND'S RUN COUNT — the simulator's Runs control, said again.
/// Saved by no preset (`OPT_RUNS_KEY`): how hard to measure right now is the
/// reader's, and neither the search nor the fight has an opinion about it.
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
    updateOptEstimate();
  });
}

function renderOpt() {
  ["opt-block", "opt-fight-block", "opt-run-block"].forEach((id) => show(id, !!META));
  if (!META) return;
  renderOptRuns();
  // A weapon's first visit: one blank start, then the active preset.
  if (!optSeeded) {
    opt.starts = [blankStart()];
    opt.limits = normalizeLimits(null);
    optSeeded = true;
    bootstrapOptPresets();
  }
  renderOptPresetBars();
  renderOptStarts();
  renderOptLimits();
  renderOptFight();
  updateOptEstimate();
}

/// THE FIGHT, READ BACK: which scenario, where it is edited, and the card. Not
/// a preset bar — that bar renames, duplicates and deletes, and the scenario
/// collection is the simulator's to edit. Redrawn wherever the fight changes.
function renderOptFight() {
  if (!META) return;
  const box = $("opt-fight-brief");
  if (box) box.innerHTML = fightCardHtml();
  const ref = $("opt-scenario-ref");
  if (ref) {
    const w = weaponInfo($("weapon").value) || {};
    ref.innerHTML =
      `<span class="plabel">${escHtml(tr("Scenario"))}</span>` +
      `<span class="pchip sel" title="${escHtml(tr("the scenario the simulator is set to"))}">${escHtml(presetLabel(scenarioNamed(activeScenario)) || tr("current"))}</span>` +
      `<a class="pchip" href="${weaponPath(w.id)}/simulator">${escHtml(tr("edit in the Simulator"))} →</a>`;
  }
}

// ---- ② THE LIMITS: what the search may not use, and how full it fills ----
//
// NOTHING BY DEFAULT. Every option of every axis is a candidate until the
// player excludes it, in the builder's order under the builder's own numbers
// and names — a mod by the builder's rows, so a card on the every-rank list can
// go at ONE rank (`card@2`); an arcane at every rank. The fill: at most `mods`
// cards, the exilus and each arcane seat filled or left empty. Saved in the
// search preset; a start that pins something a limit rules out cannot run.
// docs/OPTIMIZER.md, "Limits".

const LIMIT_AXES = ["mods", "arcanes", "evolutions", "modes", "valence"];
const blankLimits = () => ({ exclude: Object.fromEntries(LIMIT_AXES.map((k) => [k, []])),
  mods: 8, exilus: true, arcane_seats: [] });

/// A stored limits object, cleaned against THIS weapon: what it cannot hold drops.
function normalizeLimits(l) {
  const out = blankLimits();
  const w = weaponInfo($("weapon").value) || {};
  const ex = (l && l.exclude) || {};
  out.exclude.mods = (ex.mods || []).filter((id) => excludedCard(id));
  out.exclude.arcanes = (ex.arcanes || []).filter((id) => arcaneFitsWeapon(w.id, id));
  const evoIds = new Set(weaponEvos(w.id).flatMap((t) => t.options.map((o) => o.id)));
  out.exclude.evolutions = (ex.evolutions || []).filter((id) => evoIds.has(id));
  out.exclude.modes = (ex.modes || []).filter((id) => (w.modes || []).includes(id));
  out.exclude.valence = (ex.valence || []).filter((e) => ((valenceSpec(w.id) || {}).elements || []).includes(e));
  out.mods = Math.max(0, Math.min(8, l && l.mods != null ? l.mods : 8));
  out.exilus = !(l && l.exilus === false);
  out.arcane_seats = (w.arcane_pools || []).map((_, i) => !(l && (l.arcane_seats || [])[i] === false));
  return out;
}

/// The builder's list for this weapon: every card, and a row per rank for the
/// cards on the every-rank list.
const excludeOffers = (q) => buildPool()
  .filter((m) => !m.stance && (!q || searchHit(m, q)))
  .flatMap((m) => [m, ...lowerRanks(m)])
  .sort((a, b) => ((b.riven ? 1 : 0) - (a.riven ? 1 : 0))
    || String(a.name).localeCompare(String(b.name))
    || (b.card ? b.rank : b.max_rank) - (a.card ? a.rank : a.max_rank));

/// One excluded id as the builder's row: `card@rank` is that card at that rank.
function excludedCard(id) {
  const [card, rank] = splitRank(id);
  const m = modById(card);
  return m && (rank == null ? m : { ...m, id, card, rank });
}

const isOut = (axis, id) => opt.limits.exclude[axis].includes(id);
const outChip = () => ` <span class="slotchip cur">${escHtml(tr("excluded"))}</span>`;

/// Every axis's section heading is the builder block's own number and name —
/// `name` where the block's title is not the axis's (the mod block names its pools).
function limitHead(blockId, fallback, name) {
  const b = $(blockId);
  const n = b && b.querySelector(".bh .n"), h2 = b && b.querySelector(".bh h2");
  const label = name ? tr(name) : h2 ? h2.textContent.trim() : tr(fallback);
  return `<h4 class="sim-h">${n ? `${escHtml(n.textContent.trim())} · ` : ""}${escHtml(label)}</h4>`;
}

const plainRow = (axis, id, label, extra = "") => `<div class="opt ${isOut(axis, id) ? "cur opt-out" : ""}" data-axis="${axis}" data-id="${escHtml(id)}">`
  + `<div class="info"><div class="mn">${escHtml(label)}${isOut(axis, id) ? outChip() : ""}${extra}</div></div></div>`;

function renderOptLimits() {
  const box = $("opt-limits");
  if (!box || !META) return;
  const w = weaponInfo($("weapon").value) || {};
  const AX = weaponAxes(w.id);
  const L = opt.limits;
  const q = (($("opt-limit-filter") || {}).value || "").trim().toLowerCase();
  const toggle = (on, key, label) => `<label class="check"><input type="checkbox" data-fill="${key}"${on ? " checked" : ""}> ${escHtml(label)}</label>`;
  let html = "";
  if ((w.modes || []).length > 1) {
    html += limitHead("mode-block", "Mode") + `<div class="combo-menu opt-limit-list">${
      w.modes.map((id) => plainRow("modes", id, modeLabel(w, id))).join("")}</div>`;
  }
  html += limitHead("mod-block", "Mods", "Mods")
    + `<div class="opt-limit-fill"><label>${escHtml(tr("fill at most"))} <select data-fill="mods">${
      [8, 7, 6, 5, 4, 3, 2, 1, 0].map((n) => `<option value="${n}"${n === L.mods ? " selected" : ""}>${n}</option>`).join("")}</select> ${escHtml(tr("mods"))}</label>`
    + (AX.hasExilus ? toggle(L.exilus, "exilus", tr("fill the exilus")) : "") + `</div>`
    + `<input id="opt-limit-filter" type="text" placeholder="${escHtml(tr("search mods by name or effect…"))}" value="${escHtml(q)}" autocomplete="off">`
    + `<div class="combo-menu opt-limit-list" id="opt-limit-mods">${sectionedRows(excludeOffers(q), (m) => (m.riven ? "Riven" : "Mods"),
      (m) => modRow(m, { rank: m.card ? m.rank : m.max_rank, attrs: `data-axis="mods" data-id="${m.id}"`,
        cls: isOut("mods", m.id) ? "cur opt-out" : "",
        chips: `${m.card ? ` <span class="rkchip">R${m.rank}</span>` : ""}${isOut("mods", m.id) ? outChip() : ""}` }))}</div>`;
  if (AX.arcanes.length) {
    const arcs = [...new Map(AX.arcanes.flatMap((s) => s.options).filter((a) => a.id !== "none").map((a) => [a.id, a])).values()];
    html += limitHead("arcane-block", "Arcane") + `<div class="opt-limit-fill">${
      AX.arcanes.map((s, i) => toggle(L.arcane_seats[i] !== false, `arcane:${i}`,
        AX.arcanes.length > 1 ? `${tr("fill the seat")} ${tr(s.pool)}` : tr("fill the seat"))).join("")}</div>`
      + `<div class="combo-menu opt-limit-list">${arcs.map((a) => arcaneRow(a, { attrs: `data-axis="arcanes"`,
        cls: isOut("arcanes", a.id) ? "cur opt-out" : "", chips: isOut("arcanes", a.id) ? outChip() : "" })).join("")}</div>`;
  }
  if (AX.evolutions.length) {
    html += limitHead("evo-block", "Evolution") + AX.evolutions.map((t) =>
      `<div class="opt-limit-tier">${escHtml(tr("tier {n}").replace("{n}", t.tier))}</div><div class="combo-menu opt-limit-list">${
        t.options.map((o) => plainRow("evolutions", o.id, o.name)).join("")}</div>`).join("");
  }
  const vs = valenceSpec(w.id);
  if (vs) {
    html += limitHead("element-block", "Valence") + `<div class="combo-menu opt-limit-list">${
      vs.elements.map((e) => plainRow("valence", e, DT(e))).join("")}</div>`;
  }
  box.innerHTML = html;
  box.querySelectorAll(".opt[data-axis]").forEach((o) => o.addEventListener("click", (e) => {
    if (e.target.closest("a")) return;
    toggleLimit(o.dataset.axis, o.dataset.id);
  }));
  box.querySelectorAll("[data-fill]").forEach((el) => el.addEventListener("change", () => {
    const k = el.dataset.fill;
    if (k === "mods") opt.limits.mods = Number(el.value);
    else if (k === "exilus") opt.limits.exilus = el.checked;
    else opt.limits.arcane_seats[Number(k.split(":")[1])] = el.checked;
    limitsChanged();
  }));
  const f = $("opt-limit-filter");
  f.addEventListener("input", () => {
    const at = f.selectionStart;
    renderOptLimits();
    const g = $("opt-limit-filter");
    g.focus(); g.setSelectionRange(at, at);
  });
}

/// Exclude an option or take it back. An axis that would be left with nothing
/// — the last mode, the last element — keeps it.
function toggleLimit(axis, id) {
  const list = opt.limits.exclude[axis];
  if (list.includes(id)) opt.limits.exclude[axis] = list.filter((x) => x !== id);
  else {
    const w = weaponInfo($("weapon").value) || {};
    const all = axis === "modes" ? (w.modes || []) : axis === "valence" ? ((valenceSpec(w.id) || {}).elements || []) : null;
    if (all && all.filter((x) => !list.includes(x)).length <= 1) return;
    opt.limits.exclude[axis] = [...list, id];
  }
  limitsChanged();
}

function limitsChanged() {
  renderOptLimits();
  renderOptStarts();
  updateOptEstimate();
}

/// WHAT A LIMIT DOES TO A START: a position the start PINS that a limit rules
/// out blocks the run (`blocked`); one it only holds is replaced (`replaced`).
function startConflicts(s) {
  const L = opt.limits, b = s.build || {};
  const pinned = (k) => (s.fixed || []).includes(k);
  const out = { blocked: [], replaced: [] };
  const note = (k, text) => (pinned(k) ? out.blocked : out.replaced).push(text);
  (b.slots || []).forEach((x, i) => {
    if (!x || !x.mod || i === STANCE) return;
    const id = rankedId(x.mod, x.rank);
    const name = ((modById(x.mod) || {}).name || x.mod) + (id.includes("@") ? ` R${x.rank}` : "");
    if (i === EXILUS) {
      if (!L.exilus) note("mods:8", tr("{x}: the exilus is left empty").replace("{x}", name));
      else if (isOut("mods", id)) note("mods:8", tr("{x} is excluded").replace("{x}", name));
      return;
    }
    if (i >= L.mods) out.blocked.push(tr("{x}: more cards than the limit of {n}").replace("{x}", name).replace("{n}", L.mods));
    else if (isOut("mods", id)) note("mods:" + i, tr("{x} is excluded").replace("{x}", name));
  });
  (b.arcane || []).forEach((a, i) => {
    if (!a || a === "none") return;
    const name = (arcaneById(a) || {}).name || a;
    if (L.arcane_seats[i] === false) note("arcane:" + i, tr("{x}: the seat is left empty").replace("{x}", name));
    else if (isOut("arcanes", a)) note("arcane:" + i, tr("{x} is excluded").replace("{x}", name));
  });
  Object.values(b.evoSel || {}).filter(Boolean).forEach((id) => {
    if (isOut("evolutions", id)) note("evo:0", tr("{x} is excluded").replace("{x}", evoName(id)));
  });
  if (b.mode && isOut("modes", b.mode)) note("mode:0", tr("{x} is excluded").replace("{x}", modeLabel(weaponInfo($("weapon").value) || {}, b.mode)));
  const el = (b.valence || {}).element;
  if (el && isOut("valence", el)) note("valence:0", tr("{x} is excluded").replace("{x}", DT(el)));
  return out;
}

/// Any start a limit blocks — the run waits until one side changes.
const startsBlocked = () => opt.starts.some((s) => startConflicts(s).blocked.length);
