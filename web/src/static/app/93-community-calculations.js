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
let profileWeaponPicker = null;
const computationMinute = value => String(value || "").slice(0, 16);
const computationWitnessHtml = witnesses => witnesses.map(w => w.official
  ? `<span>${aT("Official compute")}${w.name ? ` · ${escHtml(w.name)}` : ""}</span>`
  : w.name && w.profile ? userProfileLink(w.profile, w.name)
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
  if (!response.ok || !value.ok) {
    const error = new Error(value.reason === "not_signed_in" ? "Sign in first." : response.status === 410 ? "This calculation record has expired."
      : response.status === 404 ? "User profile is unavailable." : "Calculation records are unavailable. Try again.");
    error.status = response.status; throw error;
  }
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
  const kind = authKindOf(location.pathname);
  if (kind === "profile" && computationProfile?.value) {
    if (!computationProfile.loading && !computationProfile.statsLoading && !computationProfile.recordsLoading) {
      loadComputationProfile(computationProfile, kind); renderAuthPage(kind);
    }
  } else if (["profile", "calculation"].includes(kind)) { computationProfile = null; renderAuthPage(kind); }
});
setInterval(() => { calculationNameRequests.clear(); queueComputationNames(); }, 60_000);

// The API supplies current accepted witnesses; publication is independent of participation.
async function verifiedComputationRows(candidates) {
  if (!candidates.length) return [];
  const value = await api("/api/build/keys", { kind: "build", builds: candidates.map(entry => entry.record) });
  if (!value?.ok || value.keys?.length !== candidates.length) throw new Error("Calculation records are unavailable. Try again.");
  return Promise.all(candidates.filter((entry, i) => entry.identity === value.keys[i] && weaponInfo(entry.weapon)).map(async entry => ({
    ...entry, buildLink: weaponPath(entry.weapon) + "?" + new URLSearchParams({ [SHARE_PARAM]: await shareCode(computationBuild(entry)) }),
  })));
}
const computationPageKey = () => {
  const query = new URLSearchParams(location.search); query.delete("section");
  return location.pathname + (query.size ? "?" + query : "") + ":" + (accountState.account?.id || "");
};
function computationPage(main, kind) {
  if (kind === "profile") normalizeUserProfileAddress();
  if (!META) { main.innerHTML = `<p>${loadingHtml("Loading user profile…")}</p>`; return; }
  const key = computationPageKey();
  if (computationProfile?.key !== key) {
    const owner = location.pathname.split("/").filter(Boolean).at(-1) + ":" + (accountState.account?.id || "");
    const prior = kind === "profile" && computationProfile?.owner === owner ? computationProfile.value : null;
    const selected = new URLSearchParams(location.search).get("kind");
    const tab = selected === "optimization" ? selected : "board";
    computationProfile = { key, owner, tab, loading: true, statsLoading: false, recordsLoading: false, rows: [],
      value: prior ? { ...prior, kind: tab } : null, error: "", statsError: "", recordsError: "", next: null, recordPage: 0, recordCursors: [null], closed: false };
    loadComputationProfile(computationProfile, kind);
  }
  const state = computationProfile, value = state.value;
  const status = state.error ? `<p role="status">${aT(state.error)} <button class="ghost-btn small" data-computation-retry>${aT("Retry")}</button></p>`
    : state.loading ? `<p>${loadingHtml(kind === "profile" ? "Loading user profile…" : "Loading calculation records…")}</p>` : "";
  if (!value) {
    main.innerHTML = `<div class="auth-page"><div class="auth-card">${status}${location.pathname === userProfilePath() && !accountState.account ? `<a href="/login?return=${userProfilePath()}">${aT("Sign in")}</a>` : ""}</div></div>`;
    return;
  }
  if (kind === "calculation") {
    const r = value.record, w = weaponInfo(r.weapon);
    main.innerHTML = `<div class="auth-page"><article class="auth-card computation-record"><h2>${aT(r.kind === "simulation" ? "Simulation" : "Optimization")}${w ? ` · ${escHtml(weaponEntryName(w))}` : ""}</h2>
      <p>${escHtml(measuredText(r.completed_at, pageSay().zone))} · ${aT("engine")} ${escHtml(r.engine)}</p>
      <p class="computation-credits">${computationWitnessHtml(r.witnesses)}</p><p class="fine">${aT("Calculation records are shown for 14 days. Lifetime signatures are kept.")}</p>${status}</article></div>`;
    return;
  }
  const p = value.profile, query = new URLSearchParams(location.search), tab = state.tab;
  const href = selected => { const q = new URLSearchParams(query); q.delete("before");
    q.set("kind", selected);
    return location.pathname + (q.size ? "?" + q : ""); };
  const total = field => p[field] == null ? "…" : Number(p[field]).toLocaleString(accountLocale());
  const stats = state.statsError ? `<p role="status">${aT(state.statsError)} <button class="ghost-btn small" data-computation-retry="summary">${aT("Retry")}</button></p>`
    : state.statsLoading ? `<p class="profile-status">${loadingHtml("Loading contribution…")}</p>` : "";
  const records = state.recordsError ? `<p role="status">${aT(state.recordsError)} <button class="ghost-btn small" data-computation-retry="records">${aT("Retry")}</button></p>`
    : state.recordsLoading ? `<p class="profile-status">${loadingHtml("Loading calculation records…")}</p>` : "";
  const weapon = (META.weapons || []).find(item => item.id === query.get("weapon"));
  const body = `<div class="profile-tools"><button class="profile-search" data-computation-weapon aria-haspopup="dialog" aria-expanded="${!!profileWeaponPicker}">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 5 5"/></svg>
      <span>${escHtml(weapon ? weaponEntryName(weapon) : tr("Search weapons…"))}</span><span class="muted">${aT("Weapon")}</span></button>
      <button class="ghost-btn small" data-computation-retry="records">${aT("Refresh")}</button><span class="muted">${escHtml(trF("Showing {n}", { n: state.rows.length }))}</span></div>
    ${weapon ? `<button class="ghost-btn small profile-filter" data-profile-weapon="" aria-label="${aT("All weapons")}">${escHtml(weaponEntryName(weapon))} <span aria-hidden="true">×</span></button>` : ""}
    <div class="profile-builds home-record-list scroll-thin" aria-busy="${state.recordsLoading}">${state.rows.map(entry => tab === "board" ? computationBuildHtml(entry, p.you) : computationOptimizationHtml(entry, p.you)).join("")}</div>
    ${!state.recordsLoading && !state.rows.length && !state.recordsError && !state.next ? `<p class="muted">${aT("No calculation records here yet.")}</p>` : ""}${records}
    ${computePagerHtml({ page: state.recordPage, hasNext: !!state.next, loading: state.recordsLoading }, page => `data-profile-page="${page}"`)}`;
  main.innerHTML = `<div class="contributor-profile"><header class="profile-heading"><span class="avatar">${escHtml((p.name || "?").slice(0, 1).toUpperCase())}</span>
    <div><span class="profile-eyebrow">${aT(p.you ? "My profile" : "User profile")}</span><h2>${escHtml(p.name)} ${p.contributor_rank == null ? "" : contributorRankBadge(p.contributor_rank)}</h2><span class="muted">@${escHtml(p.username || "")}</span></div>
    <div class="profile-totals"><b>${total("signatures")}</b><span>${aT("Lifetime signatures")}</span></div>
    <div class="profile-totals"><b>${total("points")}</b><span>${aT("Contribution points")}</span></div></header>
    ${userHomeControlsHtml(p)}${stats}
    <h2 class="profile-section-title">${aT("Verified calculations")}</h2>
    <nav class="profile-tabs" aria-label="${aT("Verified calculations")}"><a href="${escHtml(href("board"))}" ${tab === "board" ? 'aria-current="page"' : ""}>${aT("Verified builds")}</a>
      <a href="${escHtml(href("optimization"))}" ${tab === "optimization" ? 'aria-current="page"' : ""}>${aT("Optimizations · last 14 days")}</a></nav>${body}${userHomePrivateHtml(p)}</div>`;
}
const homeRecordOpen = new Set();
function homeRecordHtml(key, card, body) {
  return `<details class="home-record" data-home-record="${escHtml(key)}"${homeRecordOpen.has(key) ? " open" : ""}>${computeCompactCardHtml({ ...card, tag: "summary",
    side: card.side + `<span class="rt-chevron" aria-hidden="true">⌄</span>` })}<div class="rt-detail">${body}</div></details>`;
}
document.addEventListener("toggle", event => {
  const details = event.target;
  if (!details.isConnected || !details.matches("details[data-home-record]")) return;
  if (details.open) homeRecordOpen.add(details.dataset.homeRecord); else homeRecordOpen.delete(details.dataset.homeRecord);
}, true);
const profileVerificationHtml = own => own ? `<span class="rt-pill rt-ok profile-verification"><svg class="rt-tick" viewBox="0 0 12 12" aria-hidden="true"><path d="M2 6.5 5 9.2 10 3"/></svg>${aT("You verified this calculation")}</span>` : "";
function computationBuildHtml(entry, own = false) {
  const w = weaponInfo(entry.weapon);
  if (!w) return "";
  const context = escHtml(benchLabelOr(entry.ruler, entry.ruler)) + " · " + escHtml(modeLabel(w, entry.mode));
  const body = `<p class="fine">${context}</p>${recommendationHistoryCard(entry)}<p class="fine">${escHtml(measuredText(entry.finished_at, pageSay().zone))} · ${aT("engine")} ${escHtml(entry.measured_by.replace(/^verified:/, ""))}</p>
    <div class="rt-acts"><a class="ghost-btn small" data-profile-build-link href="${escHtml(computationBuildLink(entry))}">${aT("View build")}</a>
    <button class="ghost-btn small" data-profile-share="${escHtml(entry.identity)}" data-ruler="${escHtml(entry.ruler)}" data-mode="${escHtml(entry.mode)}">${aT("share")}</button></div>`;
  return `<article class="profile-build">${homeRecordHtml(["verified", entry.identity, entry.ruler, entry.mode].join(":"), {
    image: w.image ? IMG(w.image) : "", name: escHtml(weaponEntryName(w)),
    sub: context,
    result: `<span class="computation-credits">${computationWitnessHtml(entry.witnesses)}</span>`,
    side: `<span class="rt-result">${escHtml(fmtScore(entry.score))} ${escHtml(rulerUnit(tr(benchList().find(bench => bench.id === entry.ruler)?.name || "")))}</span>${profileVerificationHtml(own)}`,
  }, body)}</article>`;
}
const computationBuildLink = entry => entry.buildLink;
function computationBuild(entry) {
  const w = weaponInfo(entry.weapon), defs = {};
  const state = { weapon: w.id, ...headlessBoardRowAxes(pageSay(), w, recommendationRow(entry), defs) };
  const rivens = Object.fromEntries(Object.entries(defs).map(([id, spec]) => [id, { name: boardRivenName(spec), state: boardRivenState(spec) }]));
  return { state, rivens };
}
function computationOptimizationHtml(entry, own = false) {
  const w = weaponInfo(entry.weapon);
  return `<article class="profile-build">${homeRecordHtml("optimization:" + entry.question, {
    image: w?.image ? IMG(w.image) : "", name: `${aT("Optimization")}${w ? ` · ${escHtml(weaponEntryName(w))}` : ""}`,
    sub: escHtml(measuredText(entry.completed_at, pageSay().zone)),
    result: `<span class="computation-credits">${computationWitnessHtml(entry.witnesses)}</span>`, side: profileVerificationHtml(own),
  }, `<p class="fine">${aT("engine")} ${escHtml(entry.engine)}</p><a class="ghost-btn small" href="/calculations/${entry.question}">${aT("Calculation details")}</a>`)}</article>`;
}
function renderComputationState(state, kind = "profile") {
  if (userHomeEditing()) return;
  if (state === computationProfile && state.key === computationPageKey()) renderAuthPage(kind);
}
function rejectComputationProfile(state, error) {
  if ([401, 404].includes(error.status)) { state.closed = true; state.value = null; state.rows = []; state.error = error.message; }
}
async function loadProfileSummary(state) {
  state.statsLoading = true; state.statsError = "";
  try {
    const value = await computationCall(state.api + "?view=summary");
    if (state !== computationProfile || state.closed) return;
    state.value.profile = { ...state.value.profile, ...value.profile };
  } catch (error) {
    if (state !== computationProfile) return;
    rejectComputationProfile(state, error); state.statsError = error.message;
  } finally { state.statsLoading = false; renderComputationState(state); }
}
async function loadProfileRecords(state, page = 0) {
  const generation = state.recordsGeneration = (state.recordsGeneration || 0) + 1;
  if (page === state.recordPage + 1) state.recordCursors[page] = state.next;
  state.recordsLoading = true; state.recordsError = "";
  try {
    const query = new URLSearchParams(state.query);
    query.set("view", "records"); query.set("limit", String(LIST_PAGE_SIZE)); query.set("kind", state.tab);
    if (page && state.recordCursors[page]) query.set("before", JSON.stringify(state.recordCursors[page])); else query.delete("before");
    const value = await computationCall(state.api + "?" + query);
    if (state !== computationProfile || state.closed || generation !== state.recordsGeneration) return;
    const rows = value.kind === "board" ? await verifiedComputationRows(value.candidates || []) : value.candidates || [];
    if (state !== computationProfile || state.closed || generation !== state.recordsGeneration) return;
    state.value.profile = { ...state.value.profile, ...value.profile };
    state.rows = rows; state.next = value.next; state.recordPage = page;
    state.recordCursors.length = page + 1;
  } catch (error) {
    if (state !== computationProfile || generation !== state.recordsGeneration) return;
    rejectComputationProfile(state, error); state.recordsError = error.message;
  } finally { if (generation === state.recordsGeneration) { state.recordsLoading = false; renderComputationState(state); } }
}
async function loadComputationProfile(state, kind, more = false) {
  if (more && kind === "profile") { await loadProfileRecords(state, state.recordPage + 1); return; }
  state.loading = true; state.error = "";
  const id = location.pathname.split("/").filter(Boolean).at(-1);
  state.api = kind === "calculation" ? "/api/computations/" + id : id === "me" ? "/api/account/contribution-profile" : "/api/contributors/profile/" + id;
  state.query = location.search;
  try {
    const value = await computationCall(state.api + (kind === "profile" ? "?view=identity" : ""));
    if (state !== computationProfile) return;
    if (kind === "calculation") { state.value = value; return; }
    if (id === "me" && value.profile.you) {
      history.replaceState(history.state, "", userProfilePath(value.profile.id) + location.search + location.hash);
      state.key = computationPageKey();
      state.owner = value.profile.id + ":" + (accountState.account?.id || "");
      state.api = "/api/contributors/profile/" + value.profile.id;
    }
    state.value = { profile: { ...state.value?.profile, ...value.profile }, kind: state.tab };
    state.loading = false; state.statsLoading = true; state.recordsLoading = true;
    renderComputationState(state);
    await Promise.all([loadProfileSummary(state), loadProfileRecords(state), ...(value.profile.you ? [loadRecommendationPoints()] : [])]);
  } catch (error) {
    if (state !== computationProfile) return;
    state.value = null; state.error = error.message;
  } finally { state.loading = false; renderComputationState(state, kind); }
}
function setProfileWeapon(id, context = profileWeaponPicker?.dataset.context || "records") {
  closeProfileWeaponPicker(false);
  if (context === "recommendations") {
    recommendationHistoryWeapon = id; recommendationHistoryOffset = 0;
    recommendationHistory = null; loadRecommendationHistory(); renderAuthPage("profile"); return;
  }
  const query = new URLSearchParams(location.search); query.delete("before");
  if (id) query.set("weapon", id); else query.delete("weapon");
  nav(location.pathname + (query.size ? "?" + query : "") + location.hash);
}
function closeProfileWeaponPicker(restore = true) {
  if (!profileWeaponPicker) return;
  profileWeaponPicker.remove(); profileWeaponPicker = null;
  document.body.classList.remove("profile-picker-open");
  const button = document.querySelector("[data-computation-weapon]");
  button?.setAttribute("aria-expanded", "false");
  if (restore) button?.focus({ preventScroll: true });
}
function openProfileWeaponPicker(context = "records") {
  if (profileWeaponPicker || authKindOf(location.pathname) !== "profile") return;
  setAccountPanel(false, false); closePopovers();
  const overlay = document.createElement("div"); overlay.className = "profile-weapon-window";
  overlay.dataset.context = context;
  overlay.innerHTML = `<section class="profile-weapon-dialog" role="dialog" aria-modal="true" aria-labelledby="profile-weapon-title">
    <header class="jump-head"><h2 id="profile-weapon-title">${aT("Search verified calculations")}</h2><button class="ghost-btn small" data-profile-picker-close>${aT("Close")} · Esc</button></header>
    <div class="profile-picker-tools"><div class="addbar"><input type="search" aria-label="${aT("Search weapons…")}" placeholder="${aT("Search weapons…")}"></div><button class="ghost-btn small" data-profile-weapon="">${aT("All weapons")}</button><span class="muted" role="status" data-profile-picker-count></span></div>
    <div class="combo-menu scroll-thin profile-picker-results"></div></section>`;
  profileWeaponPicker = overlay; document.body.append(overlay); document.body.classList.add("profile-picker-open");
  document.querySelector("[data-computation-weapon]")?.setAttribute("aria-expanded", "true");
  const input = overlay.querySelector("input"), results = overlay.querySelector(".combo-menu");
  const selected = context === "recommendations" ? recommendationHistoryWeapon : new URLSearchParams(location.search).get("weapon");
  const weapons = (META.weapons || []).filter(w => !w.hidden && (context !== "recommendations" || recommendationHistory?.weapons?.includes(w.id))).slice().sort((a, b) => weaponEntryName(a).localeCompare(weaponEntryName(b), LANG));
  const render = () => {
    const query = input.value.trim().toLocaleLowerCase(), matches = weapons.filter(w => searchHit(w, query));
    overlay.querySelector("[data-profile-picker-count]").textContent = trF("{n} results", { n: matches.length });
    results.innerHTML = matches.map(w => `<button type="button" class="opt" data-profile-weapon="${escHtml(w.id)}" aria-pressed="${w.id === selected}">
      ${imgTag(IMG(w.image), "mod")}<span class="info"><span class="mn">${escHtml(weaponEntryName(w))}</span><span class="me">${escHtml(tr(w.subtype || w.category || ""))}</span></span>${w.id === selected ? '<span aria-hidden="true">✓</span>' : ""}</button>`).join("") || `<p class="sim-empty">${aT("No matches")}</p>`;
  };
  onTyped(input, render);
  overlay.addEventListener("click", event => {
    if (event.target === overlay || event.target.closest("[data-profile-picker-close]")) closeProfileWeaponPicker();
    const pick = event.target.closest("[data-profile-weapon]");
    if (pick) setProfileWeapon(pick.dataset.profileWeapon, context);
  });
  overlay.addEventListener("keydown", event => {
    if (imeComposing(event)) return;
    if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); closeProfileWeaponPicker(); return; }
    const rows = [...results.querySelectorAll("button")];
    if (["ArrowDown", "ArrowUp"].includes(event.key) && (event.target === input || rows.includes(event.target))) {
      event.preventDefault(); event.stopPropagation();
      const index = rows.indexOf(event.target), next = index + (event.key === "ArrowDown" ? 1 : -1);
      (next < 0 ? input : rows[Math.min(next, rows.length - 1)])?.focus(); return;
    }
    if (event.key === "Enter" && event.target === input) { event.preventDefault(); rows[0]?.click(); }
    if (event.key !== "Tab") return;
    const nodes = [...overlay.querySelectorAll("button,input")].filter(node => !node.disabled && node.getClientRects().length);
    if (event.shiftKey && document.activeElement === nodes[0]) { event.preventDefault(); nodes.at(-1)?.focus(); }
    else if (!event.shiftKey && document.activeElement === nodes.at(-1)) { event.preventDefault(); nodes[0]?.focus(); }
  }, true);
  render(); input.focus({ preventScroll: true });
}
document.addEventListener("click", event => {
  const share = event.target.closest("[data-profile-share]");
  if (share) {
    const entry = computationProfile?.rows.find(row => row.identity === share.dataset.profileShare && row.ruler === share.dataset.ruler && row.mode === share.dataset.mode);
    if (entry) openBuildShare("profile", share.closest(".profile-build"), computationBuild(entry));
  }
  const search = event.target.closest("[data-computation-weapon]");
  if (search) openProfileWeaponPicker(search.dataset.computationWeapon || "records");
  const weapon = event.target.closest(".profile-filter[data-profile-weapon]");
  if (weapon) setProfileWeapon(weapon.dataset.profileWeapon);
  const retry = event.target.closest("[data-computation-retry]");
  if (retry) {
    if (retry.dataset.computationRetry && computationProfile?.value) {
      if (retry.dataset.computationRetry === "summary" && !computationProfile.statsLoading) loadProfileSummary(computationProfile);
      if (retry.dataset.computationRetry === "records" && !computationProfile.recordsLoading) loadProfileRecords(computationProfile);
      renderAuthPage("profile");
    } else { computationProfile = null; renderAuthPage(authKindOf(location.pathname)); }
  }
  const page = event.target.closest("[data-profile-page]");
  if (page && computationProfile && !computationProfile.recordsLoading) {
    const to = Number(page.dataset.profilePage);
    if (to >= 0 && Number.isInteger(to) && (to < computationProfile.recordPage || computationProfile.next)) {
      loadProfileRecords(computationProfile, to); renderAuthPage("profile");
    }
  }
});
// Public content is shared; account-owned controls and collections are rendered only for the owner.
function userHomeOwner(section = null) {
  return authKindOf(location.pathname) === "profile" && computationProfile?.key === computationPageKey()
    && !!accountState.account && !!computationProfile.value?.profile.you
    && (!section || (new URLSearchParams(location.search).get("section") || "recommendations") === section);
}
function userHomeEditing() {
  return userHomeOwner() && authFlow.open === "profile" && !!document.activeElement?.closest(".profile-inline-editor");
}
function userHomeSectionPath(section) {
  const query = new URLSearchParams(location.search); query.set("section", section); query.delete("before");
  return location.pathname + "?" + query + location.hash;
}
function userHomeControlsHtml(profile) {
  if (!profile.you || !userHomeOwner()) return "";
  return `<nav class="profile-private" aria-label="${aT("Personal center")}">
    <button class="ghost-btn small" data-auth="open" data-open="profile">${aT(authFlow.open === "profile" ? "Close" : "Edit profile")}</button>
    <a class="ghost-btn small" data-user-section="presets" href="${escHtml(userHomeSectionPath("presets"))}">${aT("My presets")}</a>
    <a class="ghost-btn small" data-user-section="recommendations" href="${escHtml(userHomeSectionPath("recommendations"))}">${aT("My recommendations")}</a>
    ${contributionNameButton(profile)}</nav>
    <p class="fine">${aT(profile.named ? "Your name is public on calculations and the ranking." : "You are anonymous. Your calculation credit is kept.")}</p>
    ${authFlow.open === "profile" ? `<div class="profile-inline-editor">${accountProfileBlock(accountState.account)}</div>` : ""}`;
}
function userHomePrivateHtml(profile) {
  if (!profile.you || !userHomeOwner()) return "";
  const section = new URLSearchParams(location.search).get("section") === "presets" ? "presets" : "recommendations";
  let content;
  if (section === "presets") {
    cloudForget(accountState.account.id);
    content = `<div class="block" id="cloud-items">${cloudListHtml()}</div><a class="ghost-btn small" href="/account/sync">${aT("Cloud sync")}</a>`;
  } else content = recommendationsPage(accountState.account, true);
  return `<section class="profile-owner-section" aria-labelledby="profile-owner-title"><header class="profile-owner-heading"><h2 id="profile-owner-title">${aT("Personal center")}</h2><span class="tag muted">${aT("Only you")}</span></header>
    <nav class="profile-tabs" aria-label="${aT("Personal center")}">${[["recommendations", "My recommendations"], ["presets", "My presets"]].map(([id, label]) => `<a data-user-section="${id}" href="${escHtml(userHomeSectionPath(id))}"${section === id ? ' aria-current="page"' : ""}>${aT(label)}</a>`).join("")}</nav>${content}</section>`;
}
document.addEventListener("click", event => {
  const link = event.target.closest("[data-user-section]");
  if (!link || !userHomeOwner() || event.button || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
  event.preventDefault(); event.stopPropagation();
  history.pushState(history.state, "", link.href); renderAuthPage("profile");
  document.querySelector(".profile-owner-section")?.scrollIntoView({ block: "start" });
}, true);
onTyped(document, event => {
  if (!["auth-username", "auth-display"].includes(event.target?.id) || authFlow.open !== "profile") return;
  const field = event.target.id === "auth-username" ? "username" : "display_name";
  authFlow.profileDraft = { ...(authFlow.profileDraft || accountState.account), [field]: event.target.value };
});
