// The library owns its query; the Builder owns the selected build.
let libraryActive = false, libraryKey = "", libraryScroll = 0, libraryRestoring = false;
let libraryObserver = null, libraryNavigating = false;
const libraryViews = new Map(), libraryLastUrls = new Map(), libraryDrains = new Map();
const LIBRARY_SORTS = ["score", "score-asc", "recent", "drain", "heat"];
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
  const key = JSON.stringify([w.id, finder.b, mode, LANG]);
  if (box.dataset.libraryKey !== key || box.libraryRows !== rows || box.libraryLocale !== I18N) {
    box.innerHTML = headlessLibrarySummary(pageSay(), w, rows, finder.b, mode, benchmarkName(finder.b));
    box.dataset.libraryKey = key; box.libraryRows = rows; box.libraryLocale = I18N; wireFolds(box);
    const facts = headlessLibraryFacts(META, w, rows, finder.b, mode);
    box.querySelectorAll("[data-library-build]").forEach((a) => { a.onclick = (e) => {
      e.preventDefault(); e.stopPropagation();
      const row = facts[a.dataset.libraryBuild];
      openLibraryBuild(all.find((p) => boardRowIdentity(p.board) === boardRowIdentity(row) && p.benchmark === row.benchmark && p.mode === row.mode));
    }; });
  }
  box.hidden = false;
}
function libraryLeaving(mod) {
  if (recommendationLibrary && recommendationLibrary.weapon !== routeWeaponId()) recommendationLibrary = null;
  if (libraryActive && (mod !== "build-library" || finder.weapon !== routeWeaponId())) {
    librarySaveView(); libraryObserver?.disconnect(); libraryActive = false;
    finderDataMemo = null; libraryStatRun = null; libraryStatCache.clear(); libraryDrains.clear();
    const box = $("build-finder");
    box.querySelectorAll("[data-dd]").forEach((node) => ddReg.delete(node.dataset.dd));
    box.replaceChildren(); box.onclick = null; box.onchange = null; box.oninput = null;
    const summary = $("doc");
    if (summary?.libraryRows) { summary.replaceChildren(); summary.libraryRows = null; summary.libraryLocale = null; delete summary.dataset.libraryKey; }
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
  const recommended = new URLSearchParams(location.search).get("recommend_build");
  if (/^[a-f0-9]{32}$/.test(recommended || "")) q.set("recommend_build", recommended);
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
  return nav(weaponPath($("weapon").value) + "/builder");
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
    + `</div>${recommendationLibraryNote()}<div id="library-stat-status" class="library-stat-status" hidden aria-live="polite"></div></div>`;
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
  loadLibraryRecommendations(w, all);
  const changed = libraryWriteQuery();
  renderLibrarySummary(w, all);
  const box = $("build-finder");
  if (!boardHasCell(w.id, finder.b)) {
    const state = boardFileState(w.id), failed = state.failed.has(boardCellDir(finder.b));
    const count = box.querySelector(".library-result-count");
    count.textContent = tr(failed ? "The board could not be reached." : "Loading…");
    if (failed) {
      const retry = document.createElement("button"); retry.className = "ghost-btn small"; retry.textContent = tr("Retry");
      retry.onclick = () => { state.failed.delete(boardCellDir(finder.b)); loadWeaponBoard(w.id).then(renderBuildFinder); }; count.append(" ", retry);
    }
  }
  if (changed && finder.shown !== FINDER_FIRST) {
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

// Recommendation identity comes from the engine; votes and periods come from the account service.
let recommendationLibrary = null, recommendationHistory = null, recommendationHistoryLoading = false;
let recommendationHistoryFilter = "available", recommendationHistoryWeapon = "", recommendationHistoryOffset = 0;
let recommendationAccount = { id: null, points: null, day: null, revision: 0, pending: new Set(), known: new Set() };
let recommendationTimer = null;
let recommendationQueued = false, recommendationObserver = null;
const recommendationBusy = new Set();
const recommendationDeferred = new WeakSet();
const RECOMMENDATION_INTENT = "wfsim-recommendation-intent";
const recommendationIcon = `<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 10v11H3V10zM7 10l5-8c2 0 3 1 2 4l-1 4h6a2 2 0 0 1 2 2l-2 7a3 3 0 0 1-3 2H7"/></svg>`;
const recommendationAvailable = () => accountState.providers.length > 0 || !!accountState.account;
const recommendationSurfaces = ["library", "preset", "preset_info"];
function recommendationContext(w = weaponInfo(presetWeapon())) {
  if (!w) return null;
  const rows = BOARD[w.id], previous = recommendationLibrary;
  if (!previous || previous.weapon !== w.id || previous.rows !== rows) {
    recommendationLibrary = { weapon: w.id, rows, ids: new WeakMap(), heat: null, loading: false, error: "",
      presets: previous?.weapon === w.id ? previous.presets : new Map() };
  }
  return recommendationLibrary;
}
function queueRecommendationViews() {
  if (recommendationQueued) return;
  recommendationQueued = true;
  queueMicrotask(() => {
    recommendationQueued = false;
    const nodes = [...document.querySelectorAll("[data-recommend-build]")];
    if (!nodes.length) return;
    const data = recommendationContext();
    if (!data) return;
    const connected = new Set(nodes.map(node => node.dataset.recommendBuild));
    for (const id of data.presets.keys()) if (!connected.has(id)) data.presets.delete(id);
    if (!recommendationObserver && typeof IntersectionObserver !== "undefined") {
      recommendationObserver = new IntersectionObserver(entries => {
        for (const { target, isIntersecting } of entries) {
          if (!isIntersecting || !accountState.loaded) continue;
          const surface = target.dataset.recommendSurface;
          if (!recommendationSurfaces.includes(surface)) continue;
          track("recommendation.view", surface, Number(!!accountState.account));
          if (target.querySelector("[data-recommend]:not([disabled])")) track("recommendation.ready", surface, Number(!!accountState.account));
          recommendationObserver.unobserve(target);
        }
      });
    }
    recommendationObserver?.disconnect();
    nodes.forEach(node => recommendationObserver?.observe(node));
    const w = weaponInfo(presetWeapon());
    if (w) loadLibraryRecommendations(w, libraryActive ? builtinBuilds(finder.b) : [...data.presets.values()]);
  });
}
function recommendationFailure(error, surface, stage) {
  const allowed = ["rate_limited", "day_changed", "not_signed_in", "unknown_build", "bad_target", "invalid_response", "unavailable"];
  const reason = allowed.includes(error?.code) ? error.code
    : ["TimeoutError", "AbortError"].includes(error?.name) ? "timeout" : "request_failed";
  track("recommendation.error", `${surface}_${stage}_${reason}`);
}
const recommendationRequested = () => {
  const value = new URLSearchParams(location.search).get("recommend_build");
  return /^[a-f0-9]{32}$/.test(value || "") ? value : null;
};
const recommendationMatches = p => !recommendationRequested() || recommendationId(p) === recommendationRequested();
const recommendationSource = p => ({ ...boardReviewRecord(presetWeapon(), p.board), benchmark: p.benchmark });
function recommendationOwner() {
  const id = accountState.account?.id || null;
  if (recommendationAccount.id !== id) {
    recommendationAccount = { id, points: null, day: null, revision: 0, pending: new Set(), known: new Set() };
    recommendationHistory = null;
    if (recommendationLibrary) recommendationLibrary.error = "";
  }
  return id;
}
async function recommendationCall(path, body) {
  const response = await fetch(path, { method: body ? "POST" : "GET", credentials: "same-origin", cache: "no-cache",
    headers: body ? { "content-type": "application/json" } : {}, ...(body ? { body: JSON.stringify(body) } : {}), signal: timeoutSignal(15000) });
  const value = await response.json();
  if (value.reason === "day_changed") {
    recommendationPeriod(value); recommendationAccount.known.clear(); recommendationAccount.pending.clear();
    if (recommendationLibrary) recommendationLibrary.heat = null;
  }
  if (!response.ok || !value.ok) throw Object.assign(new Error(value.reason === "rate_limited" ? "Too many requests. Try again shortly."
    : value.reason === "day_changed" ? "The UTC day changed. Try again."
    : value.reason === "not_signed_in" ? "Sign in first." : "Recommendations are unavailable. Try again."), { code: value.reason });
  return value;
}
async function loadRecommendationPoints() {
  const account = recommendationOwner();
  if (!account) return;
  try {
    const value = await recommendationCall("/api/recommendations/state", { identities: [] });
    if (recommendationOwner() === account) recommendationPeriod(value);
  } catch (error) { recommendationFailure(error, "account", "points"); }
}
function recommendationPeriod(value) {
  const owner = recommendationAccount;
  if (owner.day && value.day < owner.day) return false;
  if (recommendationHistory?.day && recommendationHistory.day !== value.day) recommendationHistory = null;
  if (recommendationLibrary?.period && recommendationLibrary.period.day !== value.day) recommendationLibrary.heat = null;
  if (owner.day !== value.day) {
    if (owner.day) { recommendationHistoryFilter = "available"; recommendationHistoryOffset = 0; }
    owner.pending.clear(); owner.known.clear();
  }
  owner.day = value.day;
  if (value.points != null) owner.points = value.points;
  clearTimeout(recommendationTimer);
  const wait = Date.parse(value.next_settlement) - Date.now();
  recommendationTimer = setTimeout(() => {
    owner.known.clear(); owner.pending.clear(); recommendationHistory = null;
    if (recommendationLibrary) recommendationLibrary.heat = null;
    if (authKindOf(location.pathname) === "recommendations") loadRecommendationHistory();
    else refreshLibraryRecommendations();
  }, Math.min(Math.max(250, wait + 250), 2147483647));
  return true;
}
function recommendationSettlement(value) {
  if (!value?.next_settlement) return "";
  const date = new Date(value.next_settlement);
  return `<span class="recommendation-period">${escHtml(trF("Next settlement: {time} · UTC 00:00", {
    time: date.toLocaleString(accountLocale(), { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", timeZoneName: "short" }) }))}</span>`;
}
function recommendationId(p) { return recommendationLibrary?.ids.get(p.board); }
function recommendationHeat(p) {
  const data = recommendationLibrary, id = data?.ids.get(p.board);
  return data?.heat && id ? data.heat[id] || 0 : undefined;
}
function recommendationControl(p, surface = "library") {
  if (!p?.builtin || !p.board || (p.scope || p.state?.weapon || presetWeapon()) !== presetWeapon()) return "";
  if (!recommendationSurfaces.includes(surface)) surface = "library";
  const data = recommendationContext();
  if (!data) return "";
  data.presets.set(presetId(p), p);
  queueRecommendationViews();
  recommendationOwner();
  const id = recommendationId(p), heat = recommendationHeat(p), pending = id && recommendationAccount.pending.has(id);
  const enabled = recommendationAvailable(), busy = id && recommendationBusy.has(id);
  return `<div class="recommendation-control" data-recommend-build="${escHtml(presetId(p))}" data-recommend-surface="${surface}"><div class="recommendation-heat"><b>${heat == null ? "—" : heat.toFixed(1)}</b> <span>${escHtml(tr("Popularity score"))}</span></div>`
    + (enabled ? `<button type="button" class="recommendation-vote${pending ? " on" : ""}" data-recommend="${escHtml(presetId(p))}" data-recommendation-id="${id || ""}" aria-pressed="${!!pending}"${busy || !id ? " disabled" : ""}>${recommendationIcon}${escHtml(tr(accountState.account ? pending ? "Recommended" : "Recommend" : "Sign in to recommend"))}</button>` : "")
    + (pending ? `<span class="recommendation-pending">${escHtml(tr("Pending settlement"))} · <button type="button" data-recommend-withdraw="${escHtml(presetId(p))}"${busy ? " disabled" : ""}>${escHtml(tr("Withdraw"))}</button></span>` : "")
    + (data.error && surface !== "library" ? `<span class="recommendation-error" role="status">${escHtml(tr(data.error))} <button type="button" class="ghost-btn small" data-recommend-retry>${escHtml(tr("Retry"))}</button></span>` : "") + `</div>`;
}
function refreshLibraryRecommendations() {
  const data = recommendationLibrary;
  if (!data) return;
  document.querySelectorAll("[data-recommend-build]").forEach(node => {
    const p = data.presets.get(node.dataset.recommendBuild);
    if (p) node.outerHTML = recommendationControl(p, node.dataset.recommendSurface);
  });
  if (!libraryActive) return;
  const box = $("build-finder");
  const layout = recommendationRequested() || finder.query?.orderBy.some(term => term.field === "heat") || libraryQueryLeaves().some(term => term.field === "heat");
  if (layout) {
    const active = document.activeElement;
    if (active && box.contains(active) && active.matches("input[type=text],textarea")) {
      if (!recommendationDeferred.has(active)) {
        recommendationDeferred.add(active);
        active.addEventListener("blur", () => { recommendationDeferred.delete(active); refreshLibraryRecommendations(); }, { once: true });
      }
      return;
    }
    renderBuildFinder(); return;
  }
  const note = box.querySelector(".recommendation-library-note");
  if (note) note.outerHTML = recommendationLibraryNote();
  queueRecommendationViews();
}
async function loadLibraryRecommendations(w, all) {
  const data = recommendationContext(w), account = recommendationOwner();
  if (data.loading || data.error) return;
  all = [...new Map([...all, ...data.presets.values()].map(p => [presetId(p), p])).values()];
  const nodes = [...document.querySelectorAll("[data-recommend-build]")].filter(node => node.getClientRects().length);
  const shownIds = new Set(nodes.map(el => el.dataset.recommendBuild));
  const visible = all.filter(p => shownIds.has(presetId(p)));
  const whole = libraryActive && (recommendationRequested() || finder.query?.orderBy.some(term => term.field === "heat") || libraryQueryLeaves().some(term => term.field === "heat"));
  if (!visible.length && !whole) return;
  const wanted = (whole ? all : visible).filter(p => !data.ids.has(p.board));
  const privateNeeded = account && visible.some(p => data.ids.get(p.board) && !recommendationAccount.known.has(data.ids.get(p.board)));
  if (data.heat && !wanted.length && !privateNeeded) return;
  data.loading = true;
  const note = $("build-finder").querySelector(".recommendation-library-note");
  if (note && !note.querySelector("[data-recommend-loading]")) {
    const status = document.createElement("span"); status.dataset.recommendLoading = "";
    status.setAttribute("role", "status"); status.textContent = tr("Loading recommendations…"); note.append(status);
  }
  let stage = "heat";
  try {
    if (!data.heat) {
      const heat = await recommendationCall("/api/recommendations/heat/" + w.id);
      if (recommendationLibrary !== data) return;
      if (!recommendationPeriod(heat)) throw new Error("The UTC day changed. Try again.");
      data.heat = heat.heat; data.period = heat;
    }
    stage = "identity";
    for (let at = 0; at < wanted.length; at += 200) {
      const part = wanted.slice(at, at + 200);
      const response = await api("/api/build/keys", { kind: "build", builds: part.map(recommendationSource) });
      if (recommendationLibrary !== data || presetWeapon() !== w.id) return;
      if (!response?.ok) throw new Error("Recommendations are unavailable. Try again.");
      if (!Array.isArray(response.keys) || response.keys.length !== part.length || response.keys.some(id => !id)) {
        throw Object.assign(new Error("Recommendations are unavailable. Try again."), { code: "invalid_response" });
      }
      part.forEach((p, i) => data.ids.set(p.board, response.keys[i]));
      await new Promise(resolve => setTimeout(resolve, 0));
    }
    stage = "state";
    const identities = [...new Set(visible.map(p => data.ids.get(p.board)).filter(id => id && !recommendationAccount.known.has(id)))];
    if (account && identities.length) {
      for (let at = 0; at < identities.length; at += 200) {
        const part = identities.slice(at, at + 200);
        const revision = recommendationAccount.revision;
        const value = await recommendationCall("/api/recommendations/state", { identities: part });
        if (recommendationOwner() !== account) return;
        if (!recommendationPeriod(value)) return;
        if (recommendationAccount.revision !== revision) return;
        part.forEach(id => { recommendationAccount.known.add(id); recommendationAccount.pending.delete(id); });
        value.pending.forEach(id => recommendationAccount.pending.add(id));
      }
    }
    if (account) {
      let intent;
      try { intent = JSON.parse(sessionStorage.getItem(RECOMMENDATION_INTENT)); } catch (_) { /* no pending sign-in action */ }
      if (intent && intent.weapon === w.id && recommendationRequested() === intent.identity && Date.now() - intent.at < 600000) {
        sessionStorage.removeItem(RECOMMENDATION_INTENT);
        const surface = recommendationSurfaces.includes(intent.surface) ? intent.surface : "library";
        track("recommendation.signin", `${surface}_return`, 1);
        const target = all.find(p => data.ids.get(p.board) === intent.identity);
        if (target && !recommendationAccount.pending.has(intent.identity)) await sendRecommendation(target, false, surface);
      }
    }
  } catch (error) {
    data.error = String(error.message || error);
    for (const surface of new Set(nodes.map(node => node.dataset.recommendSurface))) recommendationFailure(error, surface, stage);
  }
  finally {
    data.loading = false;
    if (recommendationLibrary === data) refreshLibraryRecommendations();
  }
}
async function sendRecommendation(p, withdraw, surface = "library") {
  const id = recommendationId(p);
  if (!id || recommendationBusy.has(id)) return { ok: false, reason: "recommendations_not_ready" };
  if (!recommendationOwner()) {
    const query = new URLSearchParams(libraryActive ? location.search : { bench: p.benchmark });
    query.set("view", "library"); query.set("recommend_build", id);
    const back = weaponPath(presetWeapon()) + "?" + query;
    track("recommendation.signin", `${surface}_start`, 0);
    sessionStorage.setItem(RECOMMENDATION_INTENT, JSON.stringify({ identity: id, weapon: presetWeapon(), at: Date.now(), surface }));
    nav("/login?return=" + encodeURIComponent(back)); return { ok: false, reason: "not_signed_in" };
  }
  const account = recommendationAccount.id;
  recommendationBusy.add(id); refreshLibraryRecommendations();
  try {
    const value = await recommendationCall("/api/recommendations", { identity: id, day: recommendationAccount.day,
      ruler: p.benchmark, mode: p.mode || "base", action: withdraw ? "withdraw" : "recommend" });
    if (recommendationOwner() !== account) return { ok: false, reason: "account_changed" };
    if (!recommendationPeriod(value)) return { ok: false, reason: "day_changed" };
    if (value.pending) recommendationAccount.pending.add(id); else recommendationAccount.pending.delete(id);
    recommendationAccount.revision++;
    recommendationAccount.known.add(id); recommendationHistory = null;
    if (recommendationLibrary) recommendationLibrary.error = "";
    return { ok: true, identity: id, pending: value.pending, day: value.day };
  } catch (error) {
    recommendationFailure(error, surface, withdraw ? "withdraw" : "submit");
    if (recommendationLibrary) recommendationLibrary.error = String(error.message || error);
    return { ok: false, reason: "recommendation_failed", because: String(error.message || error) };
  } finally { recommendationBusy.delete(id); refreshLibraryRecommendations(); }
}
function recommendationLibraryActions(event) {
  const element = event.target.closest("[data-recommend],[data-recommend-withdraw],[data-recommend-retry],[data-recommend-clear]");
  if (!element) return false;
  event.preventDefault(); event.stopPropagation();
  if (element.hasAttribute("data-recommend-clear")) {
    const query = new URLSearchParams(location.search); query.delete("recommend_build");
    history.replaceState(history.state, "", location.pathname + (query.size ? "?" + query : ""));
    renderBuildFinder(); return true;
  }
  if (element.hasAttribute("data-recommend-retry")) {
    if (recommendationLibrary) recommendationLibrary.error = "";
    refreshLibraryRecommendations(); return true;
  }
  const withdraw = element.hasAttribute("data-recommend-withdraw");
  const p = recommendationLibrary?.presets.get(withdraw ? element.dataset.recommendWithdraw : element.dataset.recommend);
  const surface = element.closest("[data-recommend-surface]")?.dataset.recommendSurface || "library";
  track("recommendation.click", `${surface}_${withdraw ? "withdraw" : "recommend"}`, Number(!!accountState.account));
  if (p && (!recommendationAccount.pending.has(recommendationId(p)) || withdraw)) {
    wfsim.do("builder.recommendation.set", { build: presetId(p), recommended: !withdraw, surface });
  }
  return true;
}
document.addEventListener("click", recommendationLibraryActions, true);
function recommendationLibraryNote() {
  const data = recommendationLibrary;
  return `<div class="recommendation-library-note">${recommendationSettlement(data?.period)}<a href="/account/recommendations">${escHtml(tr("My recommendations"))}</a>`
    + (data?.loading ? `<span data-recommend-loading role="status">${escHtml(tr("Loading recommendations…"))}</span>` : "")
    + (recommendationRequested() ? `<span>${escHtml(tr("Your recommended build"))} <button type="button" class="ghost-btn small" data-recommend-clear>${escHtml(tr("Show all builds"))}</button></span>` : "")
    + (data?.error ? `<span role="status">${escHtml(tr(data.error))} <button class="ghost-btn small" type="button" data-recommend-retry>${escHtml(tr("Retry"))}</button></span>` : "") + `</div>`;
}
function recommendationRow(entry) {
  const rec = entry.record;
  return { ...rec, benchmark: entry.ruler, mode: entry.mode,
    ...(rec.riven_pos?.length ? { riven: { bonuses: rec.riven_pos, malus: rec.riven_neg || "", rolls: rec.riven_rolls || [] } } : {}) };
}
function recommendationHistoryCard(entry) {
  const w = weaponInfo(entry.weapon);
  if (!w) return "";
  const row = recommendationRow(entry), defs = {};
  const st = headlessBoardRowAxes(pageSay(), w, row, defs);
  return `<div class="fd-card">${headlessCardOfState(pageSay(), st, w, id => ({ name: tr("Riven"),
    section: defs[id] ? rivenSpecCardHtml(w, { name: "", spec: boardRivenState(defs[id]) }) : "" }))}</div>`;
}
function recommendationHistoryLink(entry) {
  const query = new URLSearchParams({ view: "library", bench: entry.ruler, recommend_build: entry.identity });
  return weaponPath(entry.weapon) + "?" + query;
}
const recommendationVoteRows = votes => votes.map(vote => `<tr><td>${escHtml(vote.day)}</td><td>${vote.weight == null ? aT("Pending settlement") : vote.weight.toFixed(1)}</td></tr>`).join("");
async function loadRecommendationHistory() {
  const account = recommendationOwner();
  if (!account || recommendationHistoryLoading) return;
  recommendationHistoryLoading = true;
  const query = new URLSearchParams({ filter: recommendationHistoryFilter, weapon: recommendationHistoryWeapon, offset: String(recommendationHistoryOffset) });
  try {
    const value = await recommendationCall("/api/account/recommendations?" + query);
    if (recommendationOwner() !== account) return;
    if (!recommendationPeriod(value)) return;
    if (query.get("filter") !== recommendationHistoryFilter || query.get("weapon") !== recommendationHistoryWeapon || Number(query.get("offset")) !== recommendationHistoryOffset) return;
    recommendationHistory = value;
    const weapons = [...new Set(value.builds.map(build => build.weapon))];
    const heats = await Promise.allSettled(weapons.map(weapon => recommendationCall("/api/recommendations/heat/" + weapon)));
    if (recommendationHistory !== value) return;
    value.heat = {};
    heats.forEach((item, index) => {
      if (item.status !== "fulfilled") { recommendationFailure(item.reason, "history", "heat"); return; }
      value.builds.filter(entry => entry.weapon === weapons[index]).forEach(entry => {
        value.heat[entry.identity] = item.value.heat[entry.identity] || 0;
      });
    });
  } catch (error) {
    recommendationFailure(error, "history", "load");
    recommendationHistory = { error: String(error.message || error) };
  }
  finally {
    recommendationHistoryLoading = false;
    if (authKindOf(location.pathname) === "recommendations") renderAuthPage("recommendations");
  }
}
function recommendationsPage(account) {
  recommendationOwner();
  if (!META) return `<div class="settings">${settingsNav(account, "recommendations")}<div class="set-main"><div class="block"><div class="bh"><h2>${aT("My recommendations")}</h2></div><div class="bb">${aT("Loading…")}</div></div></div></div>`;
  if (!recommendationHistory && !recommendationHistoryLoading) loadRecommendationHistory();
  const value = recommendationHistory;
  const points = recommendationAccount.points;
  const contribution = devicesState?.points;
  const stats = `<dl class="recommendation-points"><div><dt>${aT("Contribution points")}</dt><dd>${contribution == null ? "—" : escHtml(computePts(contribution))}</dd></div><div><dt>${aT("Recommendation points")}</dt><dd>${points == null ? "—" : escHtml(points.toLocaleString(accountLocale()))}</dd></div></dl>`;
  const status = value?.error ? `<p role="status">${aT(value.error)} <button type="button" class="ghost-btn" data-recommend-history-retry>${aT("Retry")}</button></p>` : !value ? `<p>${aT("Loading…")}</p>` : "";
  const counts = value?.total != null ? `<div class="recommendation-filters">${[["available", "Available today", value.available], ["pending", "Recommended today", value.pending], ["all", "All", value.total]].map(([id, label, n]) => `<button type="button" class="ghost-btn${recommendationHistoryFilter === id ? " on" : ""}" data-recommend-filter="${id}" aria-pressed="${recommendationHistoryFilter === id}">${aT(label)} <span>${n}</span></button>`).join("")}`
    + `<label>${aT("Weapon")}<select id="recommendation-history-weapon"><option value="">${aT("All weapons")}</option>${(value.weapons || []).map(id => `<option value="${escHtml(id)}"${recommendationHistoryWeapon === id ? " selected" : ""}>${escHtml(weaponInfo(id)?.name || id)}</option>`).join("")}</select></label></div>` : "";
  const builds = (value?.builds || []).map(entry => {
    const w = weaponInfo(entry.weapon), heat = value.heat?.[entry.identity];
    const date = entry.last_day + " UTC";
    return `<article class="recommendation-history-build"><header>${w?.image ? imgTag(IMG(w.image), "recommendation-weapon") : ""}<div><a href="${escHtml(recommendationHistoryLink(entry))}">${escHtml(w?.name || entry.weapon)}</a><p>${escHtml(trF("Last recommendation: {date}", { date }))}</p></div><div class="recommendation-heat"><b>${heat == null ? "—" : heat.toFixed(1)}</b> <span>${aT("Popularity score")}</span>`
      + (entry.measurement ? `<small class="recommendation-score" title="${escHtml(entry.measurement.finished_at)}">${escHtml(fmtScore(entry.measurement.score))} ${escHtml(rulerUnit(tr(benchList().find(bench => bench.id === entry.ruler)?.name || "")))}</small>` : "") + `</div></header>`
      + recommendationHistoryCard(entry) + `<footer><span>${aT(entry.pending ? "Recommended today" : "Available today")}${entry.pending ? ` · ${aT("Pending settlement")}` : ""}</span><a class="fd-open" href="${escHtml(recommendationHistoryLink(entry))}">${aT("Open Build Library")} →</a></footer>`
      + `<details><summary>${aT("Recommendation history")} · ${entry.points + Number(entry.pending)}</summary><p class="set-note">${aT("Settled recommendations add permanent points. Heat decreases separately.")}</p>`
      + `<table class="items"><thead><tr><th>${aT("Recommendation day")}</th><th>${aT("Current weight")}</th></tr></thead><tbody>${recommendationVoteRows(entry.votes)}</tbody></table>`
      + (entry.points + Number(entry.pending) > entry.votes.length ? `<button type="button" class="ghost-btn small" data-recommend-older="${entry.identity}" data-before="${entry.votes.at(-1)?.day || ""}">${aT("Show older recommendations")}</button>` : "") + `</details></article>`;
  }).join("");
  return `<div class="settings">${settingsNav(account, "recommendations")}<div class="set-main"><div class="block"><div class="bh"><h2>${aT("Mine")}</h2></div><div class="bb">${stats}</div></div><div class="block"><div class="bh"><h2>${aT("My recommendations")}</h2></div><div class="bb">${recommendationSettlement(value)}${counts}${status}</div>${builds || (value?.builds ? `<div class="bb"><p class="set-note">${aT("No recommendations in this view.")}</p></div>` : "")}`
    + (value?.builds ? `<div class="bb recommendation-pages">${recommendationHistoryOffset ? `<button type="button" class="ghost-btn" data-recommend-page="${Math.max(0, recommendationHistoryOffset - 50)}">${aT("Previous page")}</button>` : ""}${value.next != null ? `<button type="button" class="ghost-btn" data-recommend-page="${value.next}">${aT("Next page")}</button>` : ""}</div>` : "") + `</div></div></div>`;
}
document.addEventListener("click", event => {
  const older = event.target.closest("[data-recommend-older]");
  if (older) {
    older.disabled = true;
    const query = new URLSearchParams({ identity: older.dataset.recommendOlder, before: older.dataset.before });
    recommendationCall("/api/account/recommendations/votes?" + query).then(value => {
      older.closest("details").querySelector("tbody").insertAdjacentHTML("beforeend", recommendationVoteRows(value.votes));
      if (value.before) { older.dataset.before = value.before; older.disabled = false; } else older.remove();
    }).catch(error => { recommendationFailure(error, "history", "older"); older.disabled = false; older.textContent = tr(String(error.message || error)); });
    return;
  }
  const filter = event.target.closest("[data-recommend-filter]"), page = event.target.closest("[data-recommend-page]");
  if (filter) { recommendationHistoryFilter = filter.dataset.recommendFilter; recommendationHistoryOffset = 0; }
  else if (page) recommendationHistoryOffset = Number(page.dataset.recommendPage);
  else if (!event.target.closest("[data-recommend-history-retry]")) return;
  recommendationHistory = null; loadRecommendationHistory();
});
document.addEventListener("change", event => {
  if (event.target.id !== "recommendation-history-weapon") return;
  recommendationHistoryWeapon = event.target.value; recommendationHistoryOffset = 0;
  recommendationHistory = null; loadRecommendationHistory();
});
window.addEventListener("focus", () => {
  if (!recommendationAvailable()) return;
  if (recommendationLibrary && [...document.querySelectorAll("[data-recommend-build]")].some(node => node.getClientRects().length)) {
    recommendationAccount.known.clear(); recommendationLibrary.heat = null; refreshLibraryRecommendations();
  } else if (authKindOf(location.pathname) === "recommendations") {
    recommendationHistory = null; loadRecommendationHistory();
  }
});

AGENT_ACTIONS.push(
  {
    id: "builder.recommendations.read",
    query: true,
    what: "Read board build ids, popularity and your pending recommendations.",
    anchor: "#build-finder,#preset-bar-builder-builds",
    needs_weapon: true,
    args: { limit: { kind: "number", min: 1, max: 200, what: "rows; default 20" } },
    run({ limit = 20 }) {
      if (!recommendationLibrary) return agentNo("no_recommendations_open");
      recommendationOwner();
      return { points: recommendationAccount.points, day: recommendationAccount.day,
        builds: [...recommendationLibrary.presets.values()].filter(p => recommendationId(p)).slice(0, limit).map(p => ({ build: presetId(p), identity: recommendationId(p),
          heat: recommendationHeat(p), pending: recommendationAccount.pending.has(recommendationId(p)) })) };
    },
  },
  {
    id: "builder.recommendation.set",
    writes: "none",
    what: "Recommend or withdraw today from the Library or a board preset.",
    anchor: "#build-finder,#preset-bar-builder-builds",
    needs_weapon: true,
    args: { build: { kind: "string", required: true, what: "id from recommendations.read" },
      recommended: { kind: "boolean", required: true, what: "true recommends; false withdraws" },
      surface: { kind: "string", enum: () => recommendationSurfaces, what: "entry point" } },
    async run({ build, recommended, surface = libraryActive ? "library" : "preset" }) {
      if (!recommendationLibrary) return agentNo("no_recommendations_open");
      recommendationOwner();
      const p = recommendationLibrary.presets.get(build) || [...recommendationLibrary.presets.values()].find(p => recommendationId(p) === build);
      if (!p || !recommendationId(p)) return agentNo("unknown_build");
      if (recommendationAccount.pending.has(recommendationId(p)) === recommended) return { identity: recommendationId(p), pending: recommended };
      return sendRecommendation(p, !recommended, surface);
    },
  },
);
