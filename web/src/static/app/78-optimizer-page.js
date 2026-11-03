// ---- The optimizer tab: the run bar, the starts, the results, the fight ----
//
// There is no scope to mark: the candidates are the quick calc's (docs/
// OPTIMIZER.md, "The quick descent"). Pins fix a start's positions; global
// requirements apply to every final build. The fight is the simulator's card.

function renderOpt() {
  ["opt-block", "opt-fight-block", "opt-run-block"].forEach((id) => show(id, !!META));
  if (!META) return;
  // A weapon's first visit: one blank start, then the active preset.
  if (!optSeeded) {
    opt.starts = [blankStart()];
    opt.limits = normalizeLimits(null);
    optSeeded = true;
    bootstrapOptPresets();
  }
  if (modSuffix() !== "optimizer") return;
  renderOptPresetBars();
  renderOptStarts();
  renderOptLimits();
  renderOptFight();
  updateOptEstimate();
}

/// The fight read back, with an entry to the one global scenario editor.
function renderOptFight() {
  if (!META) return;
  const box = $("opt-fight-brief");
  if (box) box.innerHTML = fightCardHtml();
  const ref = $("opt-scenario-ref");
  if (ref) {
    ref.innerHTML =
      `<span class="plabel">${escHtml(tr("Scenario"))}</span>` +
      `<span class="pchip sel" title="${escHtml(tr("the scenario the simulator is set to"))}">${escHtml(presetLabel(scenarioNamed(activeScenario)) || tr("current"))}</span>` +
      `<button type="button" class="pchip" data-global-scenario>${escHtml(tr("Edit global scenario"))} →</button>`;
    ref.querySelector("[data-global-scenario]").onclick = openCalculationScenario;
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

/// A stored limits object, cleaned against THIS weapon: what it cannot hold
/// drops, onto the limits that rule nothing out (`headlessBlankLimits`).
function normalizeLimits(l) {
  const w = weaponInfo($("weapon").value) || {};
  const out = headlessBlankLimits(META, w.id);
  const ex = (l && l.exclude) || {};
  out.require.mods = [...new Set(((l && l.require) || {}).mods || [])];
  // A RIVEN THIS BROWSER DOES NOT HOLD stays excluded (`holdAbsent`): the
  // search reads the list as a filter, so an id it does not know costs nothing.
  out.exclude.mods = (ex.mods || []).filter((id) => excludedCard(id) || (isRivenId(id) && !customHeld(id)));
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
const excludeOffers = (q) => poolWithRivens()
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

const requiredLabel = (id) => {
  const m = excludedCard(id);
  return (m && m.name || id) + (m && m.card ? ` R${m.rank}` : "");
};

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
  const required = (id) => L.require.mods.includes(id);
  let html = `<p class="sim-hint">${escHtml(tr("Required mods must appear in every final build, in any eligible slot, from every start."))}</p>`
    + `<div class="sb-chips">${L.require.mods.map((id) => `<button type="button" class="pchip sel" data-require="${escHtml(id)}">${escHtml(requiredLabel(id))} ×</button>`).join("")}</div>`;
  if ((w.modes || []).length > 1) {
    html += limitHead("mode-block", "Mode") + `<div class="combo-menu opt-limit-list">${
      w.modes.map((id) => plainRow("modes", id, modeLabel(w, id))).join("")}</div>`;
  }
  html += limitHead("mod-block", "Mods", "Mods")
    + `<div class="opt-limit-fill"><label>${escHtml(tr("fill at most"))} <select data-fill="mods">${
      [8, 7, 6, 5, 4, 3, 2, 1, 0].map((n) => `<option value="${n}"${n === L.mods ? " selected" : ""}>${n}</option>`).join("")}</select> ${escHtml(tr("mods"))}</label>`
    + (AX.hasExilus ? toggle(L.exilus, "exilus", tr("fill the exilus")) : "") + `</div>`
    + `<input id="opt-limit-filter" type="text" data-view-only placeholder="${escHtml(tr("search mods by name or effect…"))}" value="${escHtml(q)}" autocomplete="off">`
    + `<div class="combo-menu opt-limit-list" id="opt-limit-mods">${sectionedRows(excludeOffers(q), (m) => (m.riven ? "Riven" : "Mods"),
      (m) => modRow(m, { rank: m.card ? m.rank : m.max_rank, attrs: `data-axis="mods" data-id="${m.id}"`,
        trailing: `<button type="button" class="pchip${required(m.id) ? " sel" : ""}" data-require="${escHtml(m.id)}" aria-pressed="${required(m.id)}">${escHtml(tr(required(m.id) ? "required" : "require"))}</button>`,
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
  const tiers = AX.evolutions.filter((t) => !evoFixed(t));
  if (tiers.length) {
    html += limitHead("evo-block", "Evolution") + tiers.map((t) =>
      `<div class="opt-limit-tier">${escHtml(tr("tier {n}").replace("{n}", t.tier))}</div><div class="combo-menu opt-limit-list">${
        t.options.map((o) => plainRow("evolutions", o.id, o.name)).join("")}</div>`).join("");
  }
  const vs = valenceSpec(w.id);
  if (vs) {
    html += limitHead("element-block", "Valence") + `<div class="combo-menu opt-limit-list">${
      vs.elements.map((e) => plainRow("valence", e, DT(e))).join("")}</div>`;
  }
  // A CLICK REDRAWS THE BOX, and a redraw must not move the reader: each list
  // scrolls on its own, and a fresh one opens at its top — so a pick deep in
  // the mod list threw the reader back to its first row. Kept by position,
  // which the sections do not change between two draws.
  const kept = [...box.querySelectorAll(".opt-limit-list")].map((el) => el.scrollTop);
  const pageY = window.scrollY;
  box.innerHTML = html;
  box.querySelectorAll(".opt-limit-list").forEach((el, i) => { el.scrollTop = kept[i] || 0; });
  if (window.scrollY !== pageY) window.scrollTo(window.scrollX, pageY);
  // A BUILT-IN CANNOT BE EDITED, and a row is a click target the lock's
  // `disabled` does not reach — so a row asks, and a redraw locks again.
  box.querySelectorAll(".opt[data-axis]").forEach((o) => o.addEventListener("click", (e) => {
    if (e.target.closest("a, [data-require]") || builtinSearchActive()) return;
    toggleLimit(o.dataset.axis, o.dataset.id);
  }));
  box.querySelectorAll("[data-require]").forEach((el) => el.addEventListener("click", (e) => {
    e.stopPropagation();
    if (builtinSearchActive()) return;
    const id = el.dataset.require;
    opt.limits.require.mods = required(id) ? L.require.mods.filter((x) => x !== id) : [...L.require.mods, id];
    limitsChanged();
  }));
  box.querySelectorAll("[data-fill]").forEach((el) => el.addEventListener("change", () => {
    const k = el.dataset.fill;
    if (k === "mods") opt.limits.mods = Number(el.value);
    else if (k === "exilus") opt.limits.exilus = el.checked;
    else opt.limits.arcane_seats[Number(k.split(":")[1])] = el.checked;
    limitsChanged();
  }));
  const f = $("opt-limit-filter");
  onTyped(f, () => {
    const at = f.selectionStart;
    renderOptLimits();
    const g = $("opt-limit-filter");
    g.focus(); g.setSelectionRange(at, at);
  });
  lockBuiltinSearch();
}

/// Exclude an option or take it back. An axis that would be left with nothing
/// — the last mode, the last element, an evolution tier's last option — keeps
/// it: the server refuses an empty tier rather than install its default.
function toggleLimit(axis, id) {
  const list = opt.limits.exclude[axis];
  if (list.includes(id)) opt.limits.exclude[axis] = list.filter((x) => x !== id);
  else {
    const w = weaponInfo($("weapon").value) || {};
    const tier = axis === "evolutions" ? weaponEvos(w.id).find((t) => t.options.some((o) => o.id === id)) : null;
    const all = axis === "modes" ? (w.modes || []) : axis === "valence" ? ((valenceSpec(w.id) || {}).elements || [])
      : tier ? tier.options.map((o) => o.id) : null;
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
  out.blocked.push(...requiredConflicts(s));
  return out;
}

/// Any start a limit blocks — the run waits until one side changes.
const startsBlocked = () => requiredConflicts({}).length || opt.starts.some((s) => startConflicts(s).blocked.length);

/// Requirements and pins share the weapon's equip families and available slots.
function requiredConflicts(s) {
  const L = opt.limits, b = s.build || {}, errors = [];
  const required = L.require.mods;
  const pinned = (i) => (s.fixed || []).includes("mods:" + i);
  const offered = new Set(poolWithRivens().filter((m) => !m.stance).map((m) => m.id));
  const cards = new Map();
  for (const id of required) {
    const [card, rank] = splitRank(id), m = excludedCard(id);
    if (!offered.has(card) || !m || (rank != null && (rank < 0 || rank >= m.max_rank))) errors.push(tr("{x} is required but unavailable").replace("{x}", requiredLabel(id)));
    if (L.exclude.mods.includes(id)) errors.push(tr("{x} is both required and excluded").replace("{x}", requiredLabel(id)));
    cards.set(id, excludedCard(id));
  }
  const pinnedMain = new Set();
  let ex = null, empty = 0;
  (b.slots || []).slice(0, 9).forEach((x, i) => {
    if (!pinned(i)) return;
    if (!x || !x.mod) { if (i < L.mods) empty++; return; }
    const id = rankedId(x.mod, x.rank);
    cards.set(id, excludedCard(id));
    if (i < 8) pinnedMain.add(id); else ex = id;
  });
  const all = [...cards].filter(([, m]) => m);
  const family = (id, m) => m.family || splitRank(id)[0];
  all.forEach(([id, m], i) => all.slice(0, i).forEach(([other, n]) => {
    if (family(id, m) === family(other, n)) errors.push(tr("{a} and {b} are mutually exclusive").replace("{a}", requiredLabel(id)).replace("{b}", requiredLabel(other)));
  }));
  const ax = weaponAxes($("weapon").value);
  const canExilus = ax.hasExilus && L.exilus && all.some(([id, m]) => m.exilus && !pinnedMain.has(id) && (!pinned(8) || ex === id));
  if (cards.size - (canExilus ? 1 : 0) > L.mods - empty) errors.push(tr("required and pinned mods exceed the available slots"));
  const w = weaponInfo($("weapon").value) || {};
  for (const [id, m] of all) {
    for (const t of weaponEvos(w.id)) {
      const choices = (s.fixed || []).includes("evo:0") ? [b.evoSel && b.evoSel[t.tier]].filter(Boolean)
        : t.options.map((o) => o.id).filter((e) => !L.exclude.evolutions.includes(e));
      if (choices.length && choices.every((e) => ((w.evo_forbids || {})[e] || []).includes(splitRank(id)[0])))
        errors.push(tr("{x} conflicts with the allowed evolutions").replace("{x}", m.name));
    }
  }
  return [...new Set(errors)];
}
