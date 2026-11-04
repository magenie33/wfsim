// Recommendation identity comes from the engine; votes and periods come from the account service.
let recommendationLibrary = null, recommendationHistory = null, recommendationHistoryLoading = false;
let recommendationHistoryFilter = "available", recommendationHistoryWeapon = "", recommendationHistoryOffset = 0;
let recommendationAccount = { id: null, points: null, day: null, revision: 0, pending: new Set(), known: new Set() };
let recommendationTimer = null;
const recommendationBusy = new Set();
const recommendationDeferred = new WeakSet();
const RECOMMENDATION_INTENT = "wfsim-recommendation-intent";
const recommendationIcon = `<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 10v11H3V10zM7 10l5-8c2 0 3 1 2 4l-1 4h6a2 2 0 0 1 2 2l-2 7a3 3 0 0 1-3 2H7"/></svg>`;
const recommendationAvailable = () => accountState.providers.length > 0 || !!accountState.account;
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
  if (!response.ok || !value.ok) throw new Error(value.reason === "rate_limited" ? "Too many requests. Try again shortly."
    : value.reason === "day_changed" ? "The UTC day changed. Try again."
    : value.reason === "not_signed_in" ? "Sign in first." : "Recommendations are unavailable. Try again.");
  return value;
}
async function loadRecommendationPoints() {
  const account = recommendationOwner();
  if (!account) return;
  try {
    const value = await recommendationCall("/api/recommendations/state", { identities: [] });
    if (recommendationOwner() === account) recommendationPeriod(value);
  } catch (_) { /* a recommendation outage does not block account settings */ }
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
    else if (libraryActive) renderBuildFinder();
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
function recommendationControl(p) {
  if (!libraryActive) return "";
  recommendationOwner();
  const id = recommendationId(p), heat = recommendationHeat(p), pending = id && recommendationAccount.pending.has(id);
  const enabled = recommendationAvailable(), busy = id && recommendationBusy.has(id);
  return `<div class="recommendation-control" data-recommend-build="${escHtml(presetId(p))}"><div class="recommendation-heat"><b>${heat == null ? "—" : heat.toFixed(1)}</b> <span>${escHtml(tr("Popularity score"))}</span></div>`
    + (enabled ? `<button type="button" class="recommendation-vote${pending ? " on" : ""}" data-recommend="${escHtml(presetId(p))}" data-recommendation-id="${id || ""}" aria-pressed="${!!pending}"${busy || !id ? " disabled" : ""}>${recommendationIcon}${escHtml(tr(accountState.account ? pending ? "Recommended" : "Recommend" : "Sign in to recommend"))}</button>` : "")
    + (pending ? `<span class="recommendation-pending">${escHtml(tr("Pending settlement"))} · <button type="button" data-recommend-withdraw="${escHtml(presetId(p))}"${busy ? " disabled" : ""}>${escHtml(tr("Withdraw"))}</button></span>` : "") + `</div>`;
}
function refreshLibraryRecommendations() {
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
  const all = builtinBuilds(finder.b), presets = new Map(all.map(p => [presetId(p), p]));
  box.querySelectorAll("[data-recommend-build]").forEach(node => {
    const p = presets.get(node.dataset.recommendBuild);
    if (p) node.outerHTML = recommendationControl(p);
  });
  const note = box.querySelector(".recommendation-library-note");
  if (note) note.outerHTML = recommendationLibraryNote();
  const weapon = weaponInfo(presetWeapon());
  if (weapon) loadLibraryRecommendations(weapon, all);
}
async function loadLibraryRecommendations(w, all) {
  const rows = BOARD[w.id];
  if (!recommendationLibrary || recommendationLibrary.weapon !== w.id || recommendationLibrary.rows !== rows) {
    recommendationLibrary = { weapon: w.id, rows, ids: new WeakMap(), heat: null, loading: false, error: "" };
  }
  const data = recommendationLibrary, account = recommendationOwner();
  if (data.loading || data.error) return;
  const shownIds = new Set([...$("build-finder").querySelectorAll("[data-recommend]")].map(el => el.dataset.recommend));
  const visible = all.filter(p => shownIds.has(presetId(p)));
  const whole = recommendationRequested() || finder.query?.orderBy.some(term => term.field === "heat") || libraryQueryLeaves().some(term => term.field === "heat");
  const wanted = (whole ? all : visible).filter(p => !data.ids.has(p.board));
  const privateNeeded = account && visible.some(p => data.ids.get(p.board) && !recommendationAccount.known.has(data.ids.get(p.board)));
  if (data.heat && !wanted.length && !privateNeeded) return;
  data.loading = true;
  const note = $("build-finder").querySelector(".recommendation-library-note");
  if (note && !note.querySelector("[data-recommend-loading]")) {
    const status = document.createElement("span"); status.dataset.recommendLoading = "";
    status.setAttribute("role", "status"); status.textContent = tr("Loading recommendations…"); note.append(status);
  }
  try {
    if (!data.heat) {
      const heat = await recommendationCall("/api/recommendations/heat/" + w.id);
      if (recommendationLibrary !== data) return;
      if (!recommendationPeriod(heat)) throw new Error("The UTC day changed. Try again.");
      data.heat = heat.heat; data.period = heat;
    }
    for (let at = 0; at < wanted.length; at += 200) {
      const part = wanted.slice(at, at + 200);
      const response = await api("/api/build/keys", { kind: "build", builds: part.map(recommendationSource) });
      if (recommendationLibrary !== data || !libraryActive) return;
      if (!response?.ok) throw new Error("Recommendations are unavailable. Try again.");
      part.forEach((p, i) => data.ids.set(p.board, response.keys[i] || null));
      await new Promise(resolve => setTimeout(resolve, 0));
    }
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
        const target = all.find(p => data.ids.get(p.board) === intent.identity);
        if (target && !recommendationAccount.pending.has(intent.identity)) await sendRecommendation(target, false);
      }
    }
  } catch (error) { data.error = String(error.message || error); }
  finally {
    data.loading = false;
    if (recommendationLibrary === data && libraryActive) refreshLibraryRecommendations();
  }
}
async function sendRecommendation(p, withdraw) {
  const id = recommendationId(p);
  if (!id || recommendationBusy.has(id)) return { ok: false, reason: "recommendations_not_ready" };
  if (!recommendationOwner()) {
    const query = new URLSearchParams(location.search); query.set("view", "library"); query.set("recommend_build", id);
    const back = location.pathname + "?" + query;
    sessionStorage.setItem(RECOMMENDATION_INTENT, JSON.stringify({ identity: id, weapon: presetWeapon(), at: Date.now() }));
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
    if (recommendationLibrary) recommendationLibrary.error = String(error.message || error);
    return { ok: false, reason: "recommendation_failed", because: String(error.message || error) };
  } finally { recommendationBusy.delete(id); if (libraryActive) refreshLibraryRecommendations(); }
}
function recommendationLibraryActions(event, all) {
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
    renderBuildFinder(); return true;
  }
  const withdraw = element.hasAttribute("data-recommend-withdraw");
  const p = all.find(item => presetId(item) === (withdraw ? element.dataset.recommendWithdraw : element.dataset.recommend));
  if (p && (!recommendationAccount.pending.has(recommendationId(p)) || withdraw)) {
    wfsim.do("builder.recommendation.set", { build: presetId(p), recommended: !withdraw });
  }
  return true;
}
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
      if (item.status !== "fulfilled") return;
      value.builds.filter(entry => entry.weapon === weapons[index]).forEach(entry => {
        value.heat[entry.identity] = item.value.heat[entry.identity] || 0;
      });
    });
  } catch (error) { recommendationHistory = { error: String(error.message || error) }; }
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
    }).catch(error => { older.disabled = false; older.textContent = tr(String(error.message || error)); });
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
  if (libraryActive && recommendationLibrary) {
    recommendationAccount.known.clear(); recommendationLibrary.heat = null; refreshLibraryRecommendations();
  } else if (authKindOf(location.pathname) === "recommendations") {
    recommendationHistory = null; loadRecommendationHistory();
  }
});
