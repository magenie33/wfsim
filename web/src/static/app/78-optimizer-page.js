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
  show("opt-block", !!META);
  if (!META) return;
  renderOptRuns();
  // A weapon's first visit: the four default starts, then the active preset.
  if (!optSeeded) {
    opt.starts = defaultStarts();
    optSeeded = true;
    bootstrapOptPresets();
  }
  renderOptPresetBars();
  renderOptStarts();
  renderOptExclude();
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

// ---- The mods the search may not use -------------------------------------
//
// NOTHING BY DEFAULT: every card the builder's list holds is a candidate, and
// the player names what is not — by the builder's own rows, so a card on the
// every-rank list can be excluded at ONE rank (`card@2`) and keep the others.
// Arcanes are not listed here. Saved in the search preset.

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

const excludeRow = (m, extra = {}) => modRow(m, {
  rank: m.card ? m.rank : m.max_rank,
  attrs: `data-id="${m.id}"`,
  ...extra,
  chips: `${m.card ? ` <span class="rkchip">R${m.rank}</span>` : ""}${extra.chips || ""}`,
});

function renderOptExclude() {
  const box = $("opt-exclude");
  if (!box || !META) return;
  const cards = opt.exclude.map(excludedCard).filter(Boolean);
  box.innerHTML = `<h4 class="sim-h">${escHtml(tr("Excluded mods"))} <span class="sim-hint">${escHtml(tr(
    "none by default — a card named here, or one rank of it, is never a candidate; arcanes are not listed"))}</span></h4>`
    + (cards.length ? `<div class="combo-menu pc-rank-list">${cards.map((m) => excludeRow(m, {
      trailing: `<button class="rk-x" data-x="${escHtml(m.id)}" title="${escHtml(tr("remove"))}">×</button>`,
    })).join("")}</div>` : "")
    + `<div class="opt-start-add"><button type="button" class="ghost-btn small" id="opt-exclude-add">+ ${escHtml(tr("exclude a mod"))}</button>`
    + (cards.length ? `<button type="button" class="ghost-btn small" id="opt-exclude-clear">${escHtml(tr("exclude nothing"))}</button>` : "")
    + `</div>`;
  box.querySelectorAll(".rk-x").forEach((b) => b.addEventListener("click", () => setOptExclude(opt.exclude.filter((x) => x !== b.dataset.x))));
  $("opt-exclude-add").addEventListener("click", (e) => openExcludePicker(e.currentTarget));
  const clear = $("opt-exclude-clear");
  if (clear) clear.addEventListener("click", () => setOptExclude([]));
}

function setOptExclude(list) {
  opt.exclude = [...new Set(list)];
  renderOptExclude();
  updateOptEstimate();
  if (!$("rank-popover").hidden && excludePicking) renderExcludeMenu($("rank-search").value);
}

/// The picker is the every-rank list's popover: a click toggles and it stays
/// open, so several cards are one visit.
let excludePicking = false;
function openExcludePicker(anchor) {
  closePopovers();
  excludePicking = true;
  const pop = $("rank-popover");
  place(pop, anchor);
  const search = $("rank-search");
  search.value = "";
  search.oninput = () => renderExcludeMenu(search.value);
  renderExcludeMenu("");
  search.focus();
}

function renderExcludeMenu(query) {
  const menu = $("rank-menu");
  const q = query.trim().toLowerCase();
  const hits = excludeOffers(q);
  const on = (m) => opt.exclude.includes(m.id);
  menu.innerHTML = hits.length
    ? sectionedRows(hits, (m) => (m.riven ? "Riven" : "Mods"), (m) => excludeRow(m, {
      cls: on(m) ? "cur" : "",
      chips: on(m) ? ` <span class="slotchip cur">${escHtml(tr("excluded"))}</span>` : "",
    }))
    : `<div class="opt dis">${escHtml(tr("no matches"))}</div>`;
  menu.querySelectorAll(".opt:not(.dis)").forEach((o) => o.addEventListener("click", (e) => {
    e.stopPropagation();
    if (e.target.closest("a")) return;
    const id = o.dataset.id;
    setOptExclude(opt.exclude.includes(id) ? opt.exclude.filter((x) => x !== id) : [...opt.exclude, id]);
  }));
}
