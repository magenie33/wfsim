// The library owns its query; the Builder owns the selected build.
let libraryActive = false, libraryKey = "", libraryScroll = 0, libraryRestoring = false;
let libraryObserver = null, libraryNavigating = false;
const libraryViews = new Map(), libraryLastUrls = new Map(), libraryDrains = new Map();
const LIBRARY_SORTS = ["score", "score-asc", "recent", "drain"];
const libraryDefaultBenchmark = () => (benchList().find((b) => b.primary) || benchList()[0] || {}).id;

function normalizeLibraryAddress() {
  const m = location.pathname.match(/^\/weapons\/([^/]+?)(?:\/(builder|build-library|benchmark))?\/?$/);
  if (!m) return;
  const q = new URLSearchParams(location.search), alias = ["build-library", "benchmark"].includes(m[2]);
  let module = m[2] === "builder" ? "builder" : "";
  if (!module) {
    if (q.has("measurement")) module = "simulator";
    else if (q.has("task-result")) module = "optimizer";
    else if (q.has(SHARE_PARAM) || SHORT_SHARE_ID || q.has("task") || q.has("build")) module = "builder";
    else if (!alias && q.get("view") !== "library" && ["bench", "mode", "riven"].some((k) => q.has(k))) module = "builder";
  }
  if (alias && !module && ["bench", "mode", "riven"].some((k) => q.has(k))) q.set("view", "library");
  const path = "/weapons/" + m[1] + (module ? "/" + module : "");
  const next = path + (q.size ? "?" + q : "") + location.hash;
  if (location.pathname + location.search + location.hash !== next) history.replaceState(history.state, "", next);
}
function libraryModuleSuffix() {
  const m = location.pathname.match(/^\/weapons\/[^/]+(?:\/([^/]+))?\/?$/);
  if (!m) return "";
  const module = m[1] || "build-library";
  return module === "builder" ? "" : module === "benchmark" ? "build-library" : module;
}
function libraryReset(w) {
  Object.assign(finder, { weapon: w.id, b: libraryDefaultBenchmark(), mo: FINDER_EVERY_MODE, rv: "all",
    req: new Set(), exc: new Set(), off: null, query: null, simpleSnapshot: null, advanced: false, advancedSort: false, sort: "score", minScore: null, maxDrain: null,
    open: null, ref: null, shown: FINDER_FIRST, hi: 0, ties: new Set(), filters: false });
}
function librarySaveView() {
  if (!libraryActive || !libraryKey) return;
  libraryViews.set(libraryKey, { shown: finder.shown, open: finder.open, ref: finder.ref,
    ties: new Set(finder.ties), filters: finder.filters, advanced: finder.advanced, advancedSort: finder.advancedSort, y: libraryScroll });
}
function libraryBeforeRoute() { librarySaveView(); }
function renderLibrarySummary(w, all) {
  const box = $("doc");
  if (!box) return;
  const rows = BOARD[w.id] || all.map((p) => p.board), mode = finder.mo === FINDER_EVERY_MODE ? null : finder.mo;
  const key = JSON.stringify([w.id, finder.b, mode, rows.length, boardMeasuredAt(rows), LANG]);
  if (box.dataset.libraryKey !== key || box.libraryRows !== rows) {
    box.innerHTML = headlessLibrarySummary(pageSay(), w, rows, finder.b, mode, benchmarkName(finder.b));
    box.dataset.libraryKey = key; box.libraryRows = rows; wireFolds(box);
  }
  box.hidden = false;
  const facts = headlessLibraryFacts(META, w, rows, finder.b, mode);
  box.querySelectorAll("[data-library-build]").forEach((a) => { a.onclick = (e) => {
    e.preventDefault(); e.stopPropagation();
    const row = facts[a.dataset.libraryBuild];
    openLibraryBuild(all.find((p) => boardRowIdentity(p.board) === boardRowIdentity(row) && p.benchmark === row.benchmark && p.mode === row.mode));
  }; });
}
function libraryLeaving(mod) {
  if (libraryActive && mod !== "build-library") {
    librarySaveView(); libraryObserver?.disconnect(); libraryActive = false;
  }
  libraryNavigating = false;
}
function libraryReadQuery(w) {
  const q = new URLSearchParams(location.search);
  libraryReset(w);
  if (benchList().some((b) => b.id === q.get("bench"))) finder.b = q.get("bench");
  if (w.modes.includes(q.get("mode"))) finder.mo = q.get("mode");
  finder.rv = q.get("riven") === "1" ? "riven" : q.get("riven") === "0" ? "plain" : "all";
  finder.sort = LIBRARY_SORTS.includes(q.get("sort")) ? q.get("sort") : "score";
  for (const [field, key] of [["minScore", "min"], ["maxDrain", "capacity"]]) {
    const n = Number(q.get(key)); finder[field] = q.has(key) && Number.isFinite(n) && n >= 0 ? n : null;
  }
  finder.req = new Set(q.getAll("require").filter((s) => s.length < 200));
  finder.exc = new Set(q.getAll("exclude").filter((s) => s.length < 200 && !finder.req.has(s)));
  if (q.has("off")) finder.off = new Set(q.getAll("off").filter((s) => s));
  if (q.has("depth") && isBoardDepth(Number(q.get("depth")))) setBoardDepth(Number(q.get("depth")));
  libraryReadAttributes(q);
  const saved = libraryViews.get(location.pathname + location.search);
  if (saved) Object.assign(finder, { shown: saved.shown, open: saved.open, ref: saved.ref,
    ties: new Set(saved.ties), filters: saved.filters, advanced: saved.advanced, advancedSort: saved.advancedSort });
  return saved?.y || 0;
}
function libraryWriteQuery() {
  if (!libraryActive || libraryNavigating || libraryRestoring) return;
  const q = new URLSearchParams();
  libraryWriteAttributes(q);
  if (finder.b !== libraryDefaultBenchmark()) q.set("bench", finder.b);
  if (q.size || boardDepth !== BOARD_DEPTH_DEFAULT) q.set("depth", boardDepth);
  if (q.size) q.set("view", "library");
  const path = weaponPath(finder.weapon) + (q.size ? "?" + q : "");
  const changed = libraryKey && libraryKey !== path;
  if (location.pathname + location.search !== path) history.replaceState(history.state, "", path + location.hash);
  libraryKey = path; libraryLastUrls.set(finder.weapon, path);
  return changed;
}
function renderBuildLibrary() {
  const w = weaponInfo($("weapon").value), entry = $("library-entry");
  if (entry && w) {
    entry.hidden = modSuffix() !== "";
    entry.innerHTML = `<a class="ghost-btn small" href="${escHtml(libraryLastUrls.get(w.id) || weaponPath(w.id))}">${escHtml(tr("Browse Build Library"))} →</a>`;
  }
  if (!w || modSuffix() !== "build-library") return;
  const key = location.pathname + location.search;
  const arriving = !libraryActive || libraryKey !== key || finder.weapon !== w.id;
  let y = 0;
  if (arriving) { libraryRestoring = true; y = libraryReadQuery(w); libraryKey = key; }
  libraryActive = true;
  renderBuildFinder();
  libraryRestoring = false; libraryWriteQuery();
  if (arriving && y) requestAnimationFrame(() => { window.scrollTo({ top: y, behavior: "instant" }); libraryScroll = y; });
}
function openLibraryBuild(p) {
  if (!p) return;
  p = builtinBuilds().find((current) => sameBoardRef(boardRef(current), boardRef(p)));
  if (!p) return;
  libraryScroll = scrollY; librarySaveView(); libraryNavigating = true;
  const back = libraryKey;
  rememberPublished(buildBarCfg(), p);
  if (presetId(p) !== activePreset) pickPreset(buildBarCfg(), presetId(p));
  history.replaceState(history.state, "", back);
  nav(weaponPath($("weapon").value) + "/builder");
}
function libraryBaseDrain(p) {
  const w = weaponInfo(finder.weapon), key = w.id + "\0" + boardRowIdentity(p.board);
  if (libraryDrains.has(key)) return libraryDrains.get(key);
  const slots = headlessBoardRowAxes(pageSay(), w, p.board, {}).slots;
  let total = 0;
  for (const s of slots.slice(0, 9)) {
    if (!s.mod) continue;
    if (s.mod.startsWith("riven:")) { total += rivenDrainAt(8); continue; }
    const m = modById(s.mod);
    if (!m) { total = Infinity; break; }
    total += modDrain(m, s.rank);
  }
  libraryDrains.set(key, total); return total;
}
const libraryScore = (p) => Number(p.board.shown ?? p.board.score);
const libraryCompare = (a, b) => libraryQueryCompare(a, b);
const libraryPasses = (p) => libraryAttributePasses(p);
function libraryFilterHeading() {
  const n = libraryQueryLeaves().filter((c) => c.value != null).length + (boardDepth !== BOARD_DEPTH_DEFAULT);
  return `<div class="library-panel-heading library-filter-heading"><h2 id="library-filter-title">${escHtml(tr("Filters"))}${n ? ` · ${n}` : ""}</h2><div class="library-panel-actions">`
    + (n ? `<button type="button" class="ghost-btn small" data-library-clear>${escHtml(tr("Clear filters"))}</button>` : "")
    + `<button type="button" class="ghost-btn small" data-library-advanced aria-pressed="${finder.advanced}">${escHtml(tr(finder.advanced ? "Simple filters" : "Advanced filters"))}</button>`
    + `<button type="button" class="ghost-btn small" data-library-filters aria-expanded="${finder.filters}">${escHtml(tr(finder.filters ? "Hide filters" : "Show filters"))}</button></div></div>`;
}
function libraryToolbar(count) {
  return `<div class="library-toolbar"><div class="library-toolbar-row"><h2>${escHtml(tr("Results"))}</h2>`
    + `<button type="button" class="library-context" data-library-board title="${escHtml(benchmarkName(finder.b))}">${escHtml(benchmarkName(finder.b))}</button>`
    + `<button type="button" class="ghost-btn small" data-library-filter-jump>${escHtml(tr("Filters"))}</button>`
    + `<button type="button" class="ghost-btn small" data-library-sort-jump>${escHtml(tr("Sort"))}</button>`
    + `<button type="button" class="ghost-btn small" data-library-top title="${escHtml(tr("Back to results"))}">↑</button></div>`
    + `<div class="library-result-count" aria-live="polite">${escHtml(trF("{n} matching builds", { n: count }))}`
    + `</div><div id="library-stat-status" class="library-stat-status" hidden aria-live="polite"></div></div>`;
}
function renderLibraryBoard() {
  const box = $("library-board");
  if (!box) return;
  box.hidden = false;
  box.innerHTML = `<h2>${escHtml(tr("Board conditions"))}</h2>` + benchSelectorHtml(finder.b);
  wireBenchSelector(box, (id) => {
    finder.b = id; finder.shown = FINDER_FIRST; finder.open = null; finder.ref = null; finder.ties.clear();
    renderBuildFinder();
  });
}
function libraryNumericFilters() {
  return `<div class="library-numeric"><label>${escHtml(tr("Minimum score"))}<input type="number" min="0" step="any" data-library-number="minScore" value="${finder.minScore ?? ""}"></label>`
    + `<label>${escHtml(tr("Maximum base MOD drain"))}<input type="number" min="0" step="1" data-library-number="maxDrain" value="${finder.maxDrain ?? ""}"></label></div>`;
}
function libraryFilterChips(all) {
  const chips = [];
  if (finder.mo !== FINDER_EVERY_MODE) chips.push(["mo", tr("Mode"), (all.find((p) => p.mode === finder.mo) || {}).modeName || finder.mo]);
  if (finder.rv !== "all") chips.push(["rv", tr("Riven"), tr(finder.rv === "riven" ? "With riven" : "Without riven")]);
  if (finder.minScore != null) chips.push(["minScore", tr("Minimum score"), finder.minScore]);
  if (finder.maxDrain != null) chips.push(["maxDrain", tr("Maximum base MOD drain"), finder.maxDrain]);
  if (boardDepth !== BOARD_DEPTH_DEFAULT) chips.push(["depth", tr("Depth"), `≥${Math.round(boardDepth * 100)}%`]);
  for (const id of finderOff()) chips.push(["off:" + id, tr("refuse builds with a part triggered by"),
    tr(BUFF_TRIGGER_NAME[id] || id)]);
  return chips.map(([key, label, value]) => `<span class="fd-tok req"><i>${escHtml(label)}</i> <b>${escHtml(value)}</b>`
    + `<button type="button" data-library-remove="${escHtml(key)}" aria-label="${escHtml(tr("remove"))}">×</button></span>`).join("");
}
function libraryAfterRender(w, all, list, groups, drawList) {
  if (!libraryActive || libraryNavigating) return;
  const changed = libraryWriteQuery();
  renderLibrarySummary(w, all);
  const box = $("build-finder");
  if (changed) {
    finder.shown = FINDER_FIRST;
    box.querySelector(".fd-main").innerHTML = drawList();
  }
  const conditions = box.querySelector(".fd-conds");
  conditions.querySelector("#fd-input").insertAdjacentHTML("beforebegin", libraryFilterChips(all) + libraryAttributeChips());
  box.onchange = (e) => {
    librarySaveView();
    if (libraryAttributeChanged(e.target)) {}
    else if (e.target.matches("[data-library-sort]")) {
      const order = LIBRARY_SIMPLE_ORDERS[e.target.value];
      if (order) finder.query.orderBy = [{ field: order[0], direction: order[1] }];
      libraryProjectQuery(); finder.sortDraft = null; finder.sortError = "";
    }
    else if (e.target.matches("[data-library-number]")) {
      const value = e.target.value;
      finder[e.target.dataset.libraryNumber] = value === "" ? null : Math.max(0, Number(value) || 0);
    } else return;
    finder.shown = FINDER_FIRST; finder.open = null; finder.ties.clear(); renderBuildFinder();
  };
  const previous = box.onclick;
  box.onclick = (e) => {
    if (libraryQueryAction(e.target)) return;
    if (e.target.closest("[data-library-sort-advanced]")) {
      finder.advancedSort = !finder.advancedSort; return renderBuildFinder();
    }
    if (e.target.closest("[data-library-sort-edit]")) { finder.advancedSort = true; return renderBuildFinder(); }
    if (e.target.closest("[data-library-filter-edit]")) { finder.advanced = true; finder.filters = true; return renderBuildFinder(); }
    if (e.target.closest("[data-library-advanced]")) {
      finder.advanced = !finder.advanced; finder.filters = true; return renderBuildFinder();
    }
    if (e.target.closest("[data-library-board]")) return $("library-board").scrollIntoView({ block: "start", behavior: "instant" });
    if (e.target.closest("[data-library-filter-jump]")) return $("library-filters").scrollIntoView({ block: "start", behavior: "instant" });
    if (e.target.closest("[data-library-sort-jump]")) return $("library-sort").scrollIntoView({ block: "start", behavior: "instant" });
    if (e.target.closest("[data-library-top]")) return libraryTop();
    if (e.target.closest("[data-library-filters]")) {
      finder.filters = !finder.filters; box.querySelector(".fd-scope").hidden = !finder.filters;
      box.querySelectorAll("[data-library-filters]").forEach((b) => {
        b.setAttribute("aria-expanded", finder.filters);
        b.textContent = tr(finder.filters ? "Hide filters" : "Show filters");
      });
      return;
    }
    if (e.target.closest("[data-library-clear]")) {
      finder.req.clear(); finder.exc.clear(); setFinderOff(new Set()); finder.rv = "all";
      finder.mo = FINDER_EVERY_MODE; finder.minScore = finder.maxDrain = null; finder.query.where = { op: "and", children: [] }; finder.queryDraft = null; libraryProjectQuery();
      setBoardDepth(BOARD_DEPTH_DEFAULT);
      finder.open = null; finder.shown = FINDER_FIRST; finder.ties.clear(); return renderBuildFinder();
    }
    const remove = e.target.closest("[data-library-remove]")?.dataset.libraryRemove;
    if (remove) {
      if (remove === "expression") {
        finder.query.where = { op: "and", children: librarySimpleTerms() }; libraryProjectQuery(); finder.queryDraft = null;
      }
      else if (remove === "mo") finder.mo = FINDER_EVERY_MODE;
      else if (remove === "rv") finder.rv = "all";
      else if (remove === "depth") setBoardDepth(BOARD_DEPTH_DEFAULT);
      else if (remove.startsWith("off:")) { const off = finderOff(); off.delete(remove.slice(4)); setFinderOff(off); }
      else finder[remove] = null;
      finder.open = null; finder.shown = FINDER_FIRST; finder.ties.clear(); return renderBuildFinder();
    }
    if (e.target.closest("[data-library-stats-retry]")) return libraryEnsureAttributes(w, all, true);
    librarySaveView(); previous?.(e);
  };
  const text = $("library-query-text");
  if (text) text.oninput = () => { finder.queryDraft = text.value; };
  const sortText = $("library-sort-text");
  if (sortText) sortText.oninput = () => { finder.sortDraft = sortText.value; };
  libraryObserve(groups.length, drawList);
  libraryEnsureAttributes(w, all);
}
function libraryObserve(total, drawList) {
  libraryObserver?.disconnect();
  const marker = $("build-finder")?.querySelector("[data-library-more]");
  if (!marker || finder.shown >= total || !("IntersectionObserver" in window)) return;
  libraryObserver = new IntersectionObserver((entries) => {
    if (!entries.some((entry) => entry.isIntersecting) || !libraryActive) return;
    libraryObserver.disconnect(); finder.shown = Math.min(total, finder.shown + FINDER_STEP);
    $("build-finder").querySelector(".fd-main").innerHTML = drawList();
    librarySaveView(); libraryObserve(total, drawList);
  }, { rootMargin: "600px 0px" });
  libraryObserver.observe(marker);
}
function libraryTop() {
  window.scrollTo({ top: 0, behavior: "instant" });
}
addEventListener("scroll", () => { if (libraryActive) libraryScroll = scrollY; }, { passive: true });
