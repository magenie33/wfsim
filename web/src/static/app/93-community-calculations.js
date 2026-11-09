// Borrowed work uses the existing engine and community power controls.
async function communitySimulationReport(request, signal) {
  const runs = request.runs || 1, count = 64, pieces = new Array(Math.ceil(runs / count));
  const lanes = Array.from({ length: Math.min(communityLanes(), pieces.length) }, () => makeLane("community"));
  let next = 0, finished = 0;
  const stop = () => lanes.forEach((lane) => { lane.abandon(); lane.worker.terminate(); });
  signal.addEventListener("abort", stop, { once: true });
  try {
    await Promise.all(lanes.map(async (lane, index) => {
      while (next < pieces.length && !signal.aborted) {
        while (!signal.aborted && next < pieces.length && index >= communityLanes()) await new Promise((resolve) => setTimeout(resolve, 250));
        if (signal.aborted) throw new Error("Cancelled");
        if (next >= pieces.length) break;
        const at = next++, from = at * count, n = Math.min(count, runs - from);
        const part = await lane.send({ kind: "shard", body: request, from, count: n }, null, true);
        if (!part || part.ok === false || part.cancelled) throw Object.assign(new Error(part?.error || "The worker could not finish."),
          { engineRefused: part?.ok === false && !part.worker_dead && !part.cancelled && !signal.aborted });
        pieces[at] = part; finished += n;
        computeProgress(finished, runs);
      }
    }));
    if (signal.aborted) throw new Error("Cancelled");
    return await lanes[0].send({ kind: "merge", body: request, shards: pieces }, null, true);
  } finally { signal.removeEventListener("abort", stop); stop(); }
}
async function communityOptimizationReport(request, { signal, buildSource }) {
  if (signal.aborted) throw new Error("Cancelled");
  const job = quickFleet(request, () => communityLanes(), async () => {},
    { keep: true, kind: "together", onBuilds: builds => keepOptimizationBuilds(builds, buildSource) });
  const stop = () => { job.cancelled = true; job.workers.forEach((worker) => worker.terminate()); };
  signal.addEventListener("abort", stop, { once: true });
  try {
    while (!job.result && !signal.aborted) {
      if (job.status) computeProgress(job.status.sims_done || 0, 0, job.status);
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
    if (signal.aborted) throw new Error("Cancelled");
    return job.result;
  } finally { signal.removeEventListener("abort", stop); }
}
const COMMUNITY_CALCULATIONS = new Map([
  ["simulation", (request, execution) => communitySimulationReport(request, execution.signal)],
  ["optimization", communityOptimizationReport],
]);
async function flushCommunityReports() {
  const queue = await initUserTasks(), saved = await queue.store.value("community-reports") || [];
  for (const row of saved) {
    const response = await postBoardWork("/api/user-tasks/answer", row.body);
    if (!response?.ok) break;
    await userTaskLock("community-reports", async () => {
      const current = await queue.store.value("community-reports") || [];
      await queue.store.value("community-reports", current.filter((item) => item.id !== row.id));
    });
  }
}
async function keepCommunityReport(row) {
  const queue = await initUserTasks();
  await userTaskLock("community-reports", async () => {
    const saved = await queue.store.value("community-reports") || [];
    await queue.store.value("community-reports", saved.filter((item) => item.id !== row.id).concat(row));
  });
  await flushCommunityReports();
}
async function userCalculationOnce(work, verifier) {
  const controller = new AbortController(), began = performance.now();
  let renewAt = 0, renewing = false;
  heldLease = { lease: work.lease, verifier };
  computeStart({ kind: "user_calculation", weapon: work.request.weapon, calculation: work.calculation });
  const timer = setInterval(() => {
    computeBeats += 1;
    if (computeFrozen || !boardVerifyOn() || computeHeld()) controller.abort();
    if (!renewing && Date.now() - renewAt >= RIVEN_RENEW_MS) {
      renewAt = Date.now(); renewing = true;
      postBoardWork("/api/board/renew", { lease: work.lease, verifier, question: work.question })
        .then((response) => { if (response?.held === false) controller.abort(); }).finally(() => { renewing = false; });
    }
  }, 1000);
  try {
    const compute = COMMUNITY_CALCULATIONS.get(work.calculation);
    if (!compute) throw new Error("This task type cannot run here.");
    const buildSource = work.calculation === "optimization" ? await communityOptimizationBuildSource(work, verifier) : null;
    const report = await compute(work.request, { signal: controller.signal, buildSource });
    if (controller.signal.aborted || !report || report.ok === false) throw Object.assign(new Error(report?.error || "Cancelled"), { engineRefused: !controller.signal.aborted && report?.ok === false && !report.worker_dead });
    const row = { id: work.question, body: { lease: work.lease, question: work.question, verifier, engine: ENGINE_ID,
      report, work: report.work || 0, compute_ms: Math.round(performance.now() - began), lanes: communityLanes() } };
    await keepCommunityReport(row);
    heldLease = null; computeFinished += 1;
    computeEnd({ ms: Math.round(performance.now() - began), work: report.work || 0 });
  } catch (error) {
    if (error.engineRefused) await keepCommunityReport({ id: work.question, body: { lease: work.lease, question: work.question,
      verifier, engine: ENGINE_ID, refused: true } }).catch(() => {});
    giveBack(); computeEnd(null);
  }
  finally { clearInterval(timer); }
  return true;
}
USER_TASK_TYPES.set("community-work", {
  icon: "↗",
  describe(task) { return { title: tr("Computing for other players"), note: task.origin.name }; },
  details() { return `<a href="/compute">${escHtml(tr("View contribution"))}</a>`; },
});
userTaskSources.set("community-work", () => computeNow ? [{ id: "community-work", state: "running", origin: { name: computeKind(computeNow).what(computeNow) },
  progress: { done: computeNow.done, total: computeNow.total, phase: computeKind(computeNow).name } }] : []);

// Public names resolve on arrival; stored calculation records carry only their reference.
const computationRows = new Map();
const computationTokens = new WeakMap();
let computationSequence = 0, computationTimer = null, computationBusy = false;
let computationProfile = null;
const computationMinute = value => String(value || "").slice(0, 16);
const computationWitnessHtml = witnesses => witnesses.map(w => w.official
  ? `<span>${aT("Official compute")}${w.name ? ` · ${escHtml(w.name)}` : ""}</span>`
  : w.name && w.profile ? `<a href="/people/${escHtml(w.profile)}">${escHtml(w.name)}</a>`
    : `<span class="muted">${aT("Anonymous contributor")}</span>`).join(`<span aria-hidden="true"> · </span>`);
function calculationRecordHtml(record) {
  if (!record) return "";
  const computation = record.computation || record.r?.computation;
  const at = computation?.completed_at || record.at;
  return `<div class="calculation-record fine">${at ? escHtml(new Date(at).toLocaleString(accountLocale())) + " · " : ""}${record.engine ? `${aT("engine")} ${escHtml(record.engine)} · ` : ""}${computation?.question
    ? `<a href="/calculations/${escHtml(computation.question)}">${aT("Computed and verified by")}</a><span data-calculation-names="${escHtml(computation.question)}"></span>`
    : aT(computation?.local ? "This device" : "Contributor not recorded")}</div>`;
}
const calculationNameRequests = new Map();
async function refreshCalculationNames() {
  for (const node of document.querySelectorAll("[data-calculation-names]")) {
    if (!node.getClientRects().length) continue;
    const id = node.dataset.calculationNames;
    if (!calculationNameRequests.has(id)) calculationNameRequests.set(id, computationCall("/api/computations/" + id)
      .then(value => computationWitnessHtml(value.record.witnesses)).catch(() => ""));
    const html = await calculationNameRequests.get(id), wanted = html ? " · " + html : "";
    if (node.isConnected && node.innerHTML !== wanted) node.innerHTML = wanted;
  }
}

function computationBadge(weapon, row) {
  if (!weapon || !row?.measured_at || !row.measured_by || !row.benchmark || !Number.isFinite(row.score)) return "";
  let byWeapon = computationTokens.get(row);
  if (!byWeapon) { byWeapon = new Map(); computationTokens.set(row, byWeapon); }
  let token = byWeapon.get(weapon);
  if (!token) {
    token = String(++computationSequence); byWeapon.set(weapon, token);
    computationRows.set(token, { weapon, row, at: 0, html: "", identity: null });
  }
  const item = computationRows.get(token);
  return `<span class="computation-names" data-computation="${token}">${item.html}</span>`;
}
async function computationCall(path, body) {
  const response = await fetch(path, { method: body ? "POST" : "GET", cache: "no-store", credentials: "same-origin",
    headers: body ? { "content-type": "application/json" } : {}, ...(body ? { body: JSON.stringify(body) } : {}), signal: timeoutSignal(15000) });
  const value = await response.json();
  if (!response.ok || !value.ok) throw new Error(value.reason === "not_signed_in" ? "Sign in first."
    : response.status === 410 ? "This calculation record has expired." : "Calculation records are unavailable. Try again.");
  return value;
}
async function refreshComputationNames() {
  if (computationBusy || document.hidden) return;
  const nodes = [...document.querySelectorAll("[data-computation]")].filter(node => node.getClientRects().length);
  const items = [...new Set(nodes.map(node => computationRows.get(node.dataset.computation)).filter(item => item && Date.now() - item.at > 60_000))].slice(0, 200).filter(item => {
    const library = recommendationLibrary?.weapon === item.weapon ? recommendationLibrary : null;
    if (library?.ids.has(item.row)) {
      item.identity = library.ids.get(item.row);
      if (!item.identity) { item.at = Date.now(); item.html = ""; return false; }
    } else if (library?.loading) return false;
    return true;
  });
  for (const node of nodes) {
    const item = computationRows.get(node.dataset.computation);
    if (item && node.innerHTML !== item.html) node.innerHTML = item.html;
  }
  if (!items.length) return;
  computationBusy = true;
  try {
    const missing = items.filter(item => !item.identity);
    if (missing.length) {
      const value = await api("/api/build/keys", { kind: "build", builds: missing.map(item => ({ ...boardReviewRecord(item.weapon, item.row), benchmark: item.row.benchmark })) });
      if (!value?.ok || value.keys?.length !== missing.length) throw new Error("identity unavailable");
      missing.forEach((item, i) => { item.identity = value.keys[i]; });
    }
    const value = await computationCall("/api/computations/names", { rows: items.filter(item => item.identity).map(item => ({ identity: item.identity,
      ruler: item.row.benchmark, mode: item.row.mode || "base", score: item.row.score, measured_at: item.row.measured_at, measured_by: item.row.measured_by })) });
    for (const item of items) {
      const found = value.rows.find(row => row.identity === item.identity && row.ruler === item.row.benchmark && row.mode === (item.row.mode || "base")
        && computationMinute(row.measured_at) === computationMinute(item.row.measured_at));
      item.html = found?.witnesses?.length ? ` · ${computationWitnessHtml(found.witnesses)}` : "";
      item.at = Date.now();
    }
  } catch (_) {
    for (const item of items) { item.html = ""; item.at = Date.now(); }
  } finally {
    computationBusy = false;
    for (const node of document.querySelectorAll("[data-computation]")) {
      const item = computationRows.get(node.dataset.computation);
      if (item && node.innerHTML !== item.html) node.innerHTML = item.html;
    }
  }
}
function queueComputationNames() {
  if (computationTimer != null) return;
  computationTimer = setTimeout(() => { computationTimer = null; refreshComputationNames(); refreshCalculationNames(); }, 80);
}
new MutationObserver(queueComputationNames).observe(document.documentElement, { childList: true, subtree: true });
window.addEventListener("scroll", queueComputationNames, { passive: true });
window.addEventListener("focus", () => {
  for (const item of computationRows.values()) item.at = 0;
  calculationNameRequests.clear();
  queueComputationNames();
  if (["profile", "calculation"].includes(authKindOf(location.pathname))) { computationProfile = null; renderAuthPage(authKindOf(location.pathname)); }
});
setInterval(() => { calculationNameRequests.clear(); queueComputationNames(); }, 60_000);

// Published membership and canonical identity are both checked, including the measurement clock.
async function publishedComputationRows(candidates) {
  const groups = new Map();
  for (const entry of candidates) {
    const dir = boardCellDir(entry.ruler), path = `/board/${dir ? dir + "/" : ""}${entry.weapon}.json`;
    if (!groups.has(path)) groups.set(path, []);
    groups.get(path).push(entry);
  }
  const matched = new Set();
  await Promise.all([...groups].map(async ([path, entries]) => {
    const response = await fetch(path, { cache: "no-store", signal: timeoutSignal(15000) });
    if (!response.ok) throw new Error("The board is unavailable.");
    const rows = await response.json();
    if (!Array.isArray(rows)) throw new Error("The board is unavailable.");
    const relevant = rows.filter(row => entries.some(entry => row.benchmark === entry.ruler && (row.mode || "base") === entry.mode
      && computationMinute(row.measured_at) === computationMinute(entry.finished_at) && row.measured_by === entry.measured_by.replace(/^verified:/, "")
      && Math.abs(row.score - entry.score) <= 1e-12 * Math.max(1, Math.abs(entry.score))));
    for (let at = 0; at < relevant.length; at += 200) {
      const part = relevant.slice(at, at + 200), value = await api("/api/build/keys", { kind: "build",
        builds: part.map(row => ({ ...boardReviewRecord(entries[0].weapon, row), benchmark: row.benchmark })) });
      if (!value?.ok || value.keys?.length !== part.length) throw new Error("The board is unavailable.");
      part.forEach((row, i) => {
        for (const entry of entries) if (entry.identity === value.keys[i] && row.benchmark === entry.ruler && (row.mode || "base") === entry.mode) matched.add(entry);
      });
    }
  }));
  return candidates.filter(entry => matched.has(entry));
}
const computationPageKey = () => location.pathname + location.search + ":" + (accountState.account?.id || "");
function computationPage(main, kind) {
  if (!META) { main.innerHTML = `<p role="status">${aT("Loading calculation records…")}</p>`; return; }
  const key = computationPageKey();
  if (computationProfile?.key !== key) {
    computationProfile = { key, loading: true, rows: [], value: null, error: "", next: null };
    loadComputationProfile(computationProfile, kind);
  }
  const state = computationProfile, value = state.value;
  const status = state.error ? `<p role="status">${aT(state.error)} <button class="ghost-btn small" data-computation-retry>${aT("Retry")}</button></p>`
    : state.loading ? `<p role="status">${aT("Loading calculation records…")}</p>` : "";
  if (!value) { main.innerHTML = `<div class="auth-page"><div class="auth-card">${status}${location.pathname === "/people/me" && !accountState.account ? `<a href="/login?return=/people/me">${aT("Sign in")}</a>` : ""}</div></div>`; return; }
  if (kind === "calculation") {
    const r = value.record, w = weaponInfo(r.weapon);
    main.innerHTML = `<div class="auth-page"><article class="auth-card computation-record"><h2>${aT(r.kind === "simulation" ? "Simulation" : "Optimization")}${w ? ` · ${escHtml(weaponEntryName(w))}` : ""}</h2>
      <p>${escHtml(measuredText(r.completed_at, pageSay().zone))} · ${aT("engine")} ${escHtml(r.engine)}</p>
      <p class="computation-credits">${computationWitnessHtml(r.witnesses)}</p><p class="fine">${aT("Calculation records are shown for 14 days. Lifetime signatures are kept.")}</p>${status}</article></div>`;
    return;
  }
  const p = value.profile, query = new URLSearchParams(location.search), tab = value.kind;
  const href = selected => { const q = new URLSearchParams(query); q.set("kind", selected); q.delete("before"); return location.pathname + "?" + q; };
  const weapons = (META?.weapons || []).filter(w => !w.hidden).slice().sort((a, b) => weaponEntryName(a).localeCompare(weaponEntryName(b)));
  main.innerHTML = `<div class="contributor-profile"><header class="profile-heading"><span class="avatar">${escHtml((p.name || "?").slice(0, 1).toUpperCase())}</span>
    <div><h2>${escHtml(p.name)} ${contributorRankBadge(p.contributor_rank)}</h2><span class="muted">@${escHtml(p.username || "")}</span></div>
    <div class="profile-totals"><b>${Number(p.signatures).toLocaleString(accountLocale())}</b><span>${aT("Lifetime signatures")}</span></div>
    <div class="profile-totals"><b>${Number(p.points).toLocaleString(accountLocale())}</b><span>${aT("Contribution points")}</span></div></header>
    ${p.you ? `<p class="fine">${aT(p.named ? "Your name is public on calculations and the ranking." : "You are anonymous. Your calculation credit is kept.")} <a href="/contributors">${aT("Name visibility")}</a></p>` : ""}
    <nav class="profile-tabs" aria-label="${aT("Calculation records")}"><a href="${escHtml(href("board"))}" ${tab === "board" ? 'aria-current="page"' : ""}>${aT("Current board builds")}</a>
      <a href="${escHtml(href("optimization"))}" ${tab === "optimization" ? 'aria-current="page"' : ""}>${aT("Optimizations · last 14 days")}</a></nav>
    <div class="profile-tools"><label>${aT("Weapon")} <select data-computation-weapon><option value="">${aT("All weapons")}</option>${weapons.map(w => `<option value="${escHtml(w.id)}"${query.get("weapon") === w.id ? " selected" : ""}>${escHtml(weaponEntryName(w))}</option>`).join("")}</select></label>
      <button class="ghost-btn small" data-computation-retry>${aT("Refresh")}</button><span class="muted">${escHtml(trF("Showing {n}", { n: state.rows.length }))}</span></div>
    <p class="fine">${aT(tab === "board" ? "Current board builds credited to this contributor. Lifetime signatures remain when a build leaves the board."
      : "Calculation records are shown for 14 days. Lifetime signatures are kept.")}</p>
    <div class="profile-builds">${state.rows.map(entry => tab === "board" ? computationBuildHtml(entry) : computationOptimizationHtml(entry)).join("")}</div>
    ${!state.loading && !state.rows.length && !state.error ? `<p class="muted">${aT("No calculation records here yet.")}</p>` : ""}${status}
    ${state.next && !state.loading ? `<button class="ghost-btn" data-computation-more>${aT("Load more")}</button>` : ""}</div>`;
}
function computationBuildHtml(entry) {
  const w = weaponInfo(entry.weapon);
  if (!w) return "";
  return `<article class="profile-build"><h3><a href="${escHtml(computationBuildLink(entry))}">${escHtml(weaponEntryName(w))}</a><span>${escHtml(String(entry.score.toFixed(4)))}</span></h3>
    <p class="fine">${escHtml(benchLabelOr(entry.ruler, entry.ruler))} · ${escHtml(modeLabel(w, entry.mode))} · ${escHtml(measuredText(entry.finished_at, pageSay().zone))} · ${aT("engine")} ${escHtml(entry.measured_by.replace(/^verified:/, ""))}</p>
    <div class="computation-credits">${computationWitnessHtml(entry.witnesses)}</div>${recommendationHistoryCard(entry)}
    <a class="ghost-btn small" href="${escHtml(computationBuildLink(entry))}">${aT("View build")}</a></article>`;
}
const computationBuildLink = entry => weaponPath(entry.weapon) + "?" + new URLSearchParams({ view: "library", bench: entry.ruler, computed_build: entry.identity });
function computationOptimizationHtml(entry) {
  const w = weaponInfo(entry.weapon);
  return `<article class="profile-build"><h3><a href="/calculations/${entry.question}">${aT("Optimization")}${w ? ` · ${escHtml(weaponEntryName(w))}` : ""}</a></h3>
    <p class="fine">${escHtml(measuredText(entry.completed_at, pageSay().zone))} · ${aT("engine")} ${escHtml(entry.engine)}</p>
    <div class="computation-credits">${computationWitnessHtml(entry.witnesses)}</div></article>`;
}
async function loadComputationProfile(state, kind, more = false) {
  state.loading = true; state.error = "";
  try {
    const query = new URLSearchParams(location.search);
    if (more && state.next) query.set("before", JSON.stringify(state.next));
    const id = location.pathname.split("/").filter(Boolean).at(-1);
    const path = kind === "calculation" ? "/api/computations/" + id : id === "me" ? "/api/account/contribution-profile" : "/api/contributors/profile/" + id;
    const value = await computationCall(path + "?" + query);
    if (state !== computationProfile) return;
    const rows = kind === "profile" && value.kind === "board" ? await publishedComputationRows(value.candidates) : value.candidates || [];
    if (state !== computationProfile) return;
    state.value = value; state.rows = more ? state.rows.concat(rows) : rows; state.next = value.next;
  } catch (error) { state.error = error.message; }
  finally { state.loading = false; if (state === computationProfile && state.key === computationPageKey()) renderAuthPage(kind); }
}
document.addEventListener("click", event => {
  if (event.target.closest("[data-computation-retry]")) { computationProfile = null; renderAuthPage(authKindOf(location.pathname)); }
  if (event.target.closest("[data-computation-more]") && computationProfile && !computationProfile.loading) {
    loadComputationProfile(computationProfile, "profile", true); renderAuthPage("profile");
  }
});
document.addEventListener("change", event => {
  if (!event.target.matches("[data-computation-weapon]")) return;
  const query = new URLSearchParams(location.search); query.delete("before");
  if (event.target.value) query.set("weapon", event.target.value); else query.delete("weapon");
  nav(location.pathname + "?" + query);
});
