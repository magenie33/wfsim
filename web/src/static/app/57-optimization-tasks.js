// Search execution and artifacts belong to a task; the editor is an observer.
const optimizationKey = (request) => JSON.stringify(request);
let optimizationReport = null;
function frozenTaskBuild(state, request) {
  const captured = taskClone(state), weapon = request.weapon;
  const definitions = new Map((request.rivens || []).map((entry) => [RIVEN_PREFIX + (entry.id || entry.name), entry.spec]));
  for (const slot of captured.slots || []) {
    const spec = definitions.get(slot.mod);
    if (spec) slot.mod = READONLY_RIVEN_PREFIX + encodeURIComponent(JSON.stringify([rivenScope(weapon), headlessWithDrafts(META, spec)]));
  }
  return captured;
}
function captureOptimizationOrigin() {
  const context = taskClone(boardBuildContext());
  context.rivens = Object.fromEntries(rivenPayload().map((r) => [RIVEN_PREFIX + r.id, headlessRivenShape(r.spec)]));
  return { weapon: presetWeapon(), preset: activeOptPreset, preset_name: (loadOptPresets().find((p) => p.id === activeOptPreset) || {}).name,
    scenario: activeScenario, scenario_name: (scenarioNamed(activeScenario) || {}).name, board: context,
    build: taskClone(snapshotState()), fight: taskClone(sim), search: taskClone(snapshotOpt()),
    appraisal: null };
}

function observeOptimizationStatus(status, previous = null) {
  const work = (s) => JSON.stringify([s.phase, s.activity, s.enumerated, s.sims_done,
    s.batches_completed, s.batch_builds_done, s.batch_builds_total, s.batch_slices_done,
    (s.starts || []).map(lane => [lane.settled, lane.pass, lane.round, lane.at])]);
  const now = Date.now();
  const progressed = previous ? work(status) !== work(previous)
    : status.enumerated > 0 || status.sims_done > 0 || status.batches_completed > 0;
  return { ...status, observed_at: now, work_last_at: progressed ? now : previous?.work_last_at || null };
}

let optimizationClockTimer = null;
function optimizationClockText(clock, now = Date.now()) {
  const seconds = Math.floor(Math.max(0, Number(clock.elapsed_ms) + now - Number(clock.at)) / 1000);
  const time = seconds >= 3600 ? `${Math.floor(seconds / 3600)}:${String(Math.floor(seconds / 60) % 60).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`
    : `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
  const parts = [trF("Elapsed {time}", { time })];
  const age = at => Math.floor(Math.max(0, now - Number(at)) / 1000);
  if (clock.work_at) parts.push(trF("Last progress {s}s ago", { s: age(clock.work_at) }));
  if (clock.engine_known) {
    if (clock.activity === "loading") parts.push(tr("Waiting for compute to load"));
    else if (clock.activity === "resting") parts.push(tr("Waiting for compute time"));
    else if (!clock.engine_at) parts.push(tr("Waiting for the first calculation response"));
    else parts.push(trF(age(clock.engine_at) > 15 ? "Waiting for a calculation response ({s}s)" : "Last calculation response {s}s ago", { s: age(clock.engine_at) }));
  }
  return parts.join(" · ");
}
function optimizationClockHtml(clock, now = Date.now()) {
  return `<div class="search-clock" data-search-clock data-elapsed-ms="${clock.elapsed_ms}" data-observed-at="${clock.at}" data-work-at="${clock.work_at || 0}" data-engine-at="${clock.engine_at || 0}" data-engine-known="${clock.engine_known ? 1 : 0}" data-activity="${escHtml(clock.activity || "")}">${escHtml(optimizationClockText(clock, now))}</div>`;
}
function refreshOptimizationClocks() {
  const clocks = [...document.querySelectorAll("[data-search-clock]")].filter(node => node.getClientRects().length);
  for (const node of clocks) {
    const d = node.dataset;
    node.textContent = optimizationClockText({ elapsed_ms: Number(d.elapsedMs), at: Number(d.observedAt),
      work_at: Number(d.workAt), engine_at: Number(d.engineAt), engine_known: d.engineKnown === "1", activity: d.activity });
  }
  if (!clocks.length && optimizationClockTimer != null) { clearInterval(optimizationClockTimer); optimizationClockTimer = null; }
  return clocks.length;
}
function watchOptimizationClocks() {
  if (refreshOptimizationClocks() && optimizationClockTimer == null) optimizationClockTimer = setInterval(refreshOptimizationClocks, 1000);
}

function optimizationFinalStage(st) {
  return st.activity === "final_simulation" || st.rounds > 0 || st.phase === "running" && st.jobs > 0;
}
function optimizationTaskProgress(task, now = Date.now()) {
  const p = task.progress;
  if (!p) return null;
  const replay = p.phase === "Appraisal replay";
  const waiting = p.phase === "Waiting for appraisal replay";
  if (!p.status && !replay && !waiting && p.phase !== "Optimization") return null;
  const st = replay || waiting ? {} : p.status || {};
  const final = optimizationFinalStage(st);
  const phase = waiting ? tr("Waiting to replay the build") : replay ? tr("Replaying the build") : final && st.rounds > 1 && st.round < st.rounds ? tr("Comparing candidate builds") : final ? tr("Verifying finalists")
    : st.activity === "loading" ? tr("Preparing compute") : st.activity === "resting" ? tr("Waiting for compute time")
    : st.activity === "planning" || st.phase === "enumerating" ? tr("Finding candidate builds") : tr("Improving builds");
  const count = waiting || replay && !(p.total > 0) ? "" : replay ? trF("{done} / {total} fights completed", { done: (p.done || 0).toLocaleString(), total: (p.total || 0).toLocaleString() })
    : trF("{b} builds evaluated · {s} fights completed", { b: (st.enumerated || 0).toLocaleString(), s: (st.sims_done || 0).toLocaleString() });
  const details = [];
  if (!replay && !final && st.batch_builds_total > 0) details.push(trF("Current batch: {done} / {total} builds", { done: (st.batch_builds_done || 0).toLocaleString(), total: st.batch_builds_total.toLocaleString() }));
  if (!replay && final && st.jobs > 0) details.push(trF("{n} candidate builds to verify", { n: st.jobs.toLocaleString() }));
  if (!replay && st.workers_active != null) details.push(trF("{n} computing · {m} preparing", { n: st.workers_active, m: st.workers_loading || 0 }));
  const started = Date.parse(task.started_at || task.created_at || "");
  const instant = value => Number.isFinite(Number(value)) && Number(value) > 0 ? Number(value) : null;
  const clock = { elapsed_ms: Number.isFinite(st.elapsed_s) ? Math.max(0, st.elapsed_s * 1000) : Number.isFinite(started) ? Math.max(0, now - started) : 0,
    at: instant(st.observed_at) || now, work_at: instant(st.work_last_at), engine_at: instant(st.engine_last_at),
    engine_known: task.destination !== "community" && Object.hasOwn(st, "engine_last_at"), activity: st.activity };
  const fraction = p.total > 0 ? Math.max(0, Math.min(1, p.done / p.total)) : null;
  return { fraction, phase, count, detail_html: (details.length ? `<div class="search-work">${escHtml(details.join(" · "))}</div>` : "") + optimizationClockHtml(clock, now) };
}
async function executeOptimization(task, { signal, report, checkpoint }) {
  const release = holdForeground();
  let id = null, cancel;
  try {
    const saved = await checkpoint();
    const compatible = saved?.engine === task.engine && saved.schema === task.request_schema ? saved.value : null;
    if (!WASM && compatible?.job_id != null) {
      const status = await api("/api/optimize/status", { id: compatible.job_id }, null, { signal });
      if (status?.ok !== false && !["cancelled", "error"].includes(status.phase)) id = compatible.job_id;
    }
    if (id == null) {
      if (signal.aborted) throw new Error("Cancelled");
      const response = await api("/api/optimize", { ...task.request, ...(compatible?.cp ? { __resume: compatible.cp } : {}) }, null,
        { checkpoint: (cp) => checkpoint({ cp }).catch(userTaskFailed) });
      if (!response || response.ok === false) throw new Error(response?.error || "no data");
      id = response.job_id;
      if (!WASM) await checkpoint({ job_id: id });
    }
    cancel = () => api("/api/optimize/cancel", { id }).catch(() => {});
    signal.addEventListener("abort", cancel, { once: true });
    if (signal.aborted) { await cancel(); throw new Error("Cancelled"); }
    track("optimizer.start", task.request.weapon);
    let previousStatus = null;
    for (;;) {
      if (signal.aborted) throw new Error("Cancelled");
      const status = await api("/api/optimize/status", { id }, null, { signal });
      if (!status || status.ok === false || status.phase === "error") throw new Error(status?.result?.error || status?.error || "no data");
      const observed = observeOptimizationStatus(status, previousStatus);
      previousStatus = observed;
      const fraction = optProgressOf(observed);
      report({ phase: "Optimization", done: fraction == null ? 0 : fraction, total: fraction == null ? 0 : 1,
        status: observed, builds: status.enumerated || 0, fights: status.sims_done || 0 });
      if (status.phase === "cancelled") throw new Error("Cancelled");
      if (status.phase === "done") {
        if (!status.result) throw new Error("The search returned no result.");
        return status.result;
      }
      await new Promise((resolve) => setTimeout(resolve, 500));
    }
  } finally {
    if (cancel) signal.removeEventListener("abort", cancel);
    release();
  }
}
USER_TASK_TYPES.set("optimization", {
  schema: 1, icon: "↗", community: { kind: "optimization" }, execute: executeOptimization,
  progress: optimizationTaskProgress,
  validate(request) { if (!request.weapon || !Array.isArray(request.starts)) throw new Error("Invalid optimization request."); },
  describe(task) {
    return { title: `${weaponInfo(task.request.weapon)?.name || task.request.weapon} · ${tr("Optimization")}`,
      note: [task.origin.preset_name, benchLabelOr(task.origin.scenario, task.origin.scenario_name), tr(USER_TASK_EXECUTORS.get(task.destination)?.label || "This device")].filter(Boolean).join(" · ") };
  },
  details(task) {
    const status = task.progress?.status;
    return `<div>${escHtml(tr("{n} finalists").replace("{n}", task.request.finalists))}</div>`
      + (status ? `<div>${escHtml(tr("{b} builds scored, {s} fights").replace("{b}", status.enumerated || 0).replace("{s}", status.sims_done || 0))}</div>` : "")
      + `<div>${escHtml(new Date(task.created_at).toLocaleString())}</div>`;
  },
  async save(task, result) {
    await userTasks.store.value(`artifact:${task.attempt}`, { id: task.attempt, task: task.id, kind: task.kind,
      engine: task.engine, request: task.request, origin: task.origin, result, at: taskInstant() });
    track("optimizer.run", task.request.weapon);
    return task.attempt;
  },
  async completed(task) {
    const record = await userTasks.store.value(`artifact:${task.result_ref}`);
    if (!record) return;
    await offerOptBoardSubmit(record.result.search || record.result, { task: task.id, context: task.origin.board, appraisal: !!task.origin.appraisal });
    if (task.origin.appraisal) await sendCapturedAppraisal(task, record.result.search || record.result);
  },
  async recover(task) { return await userTasks.store.value(`artifact:${task.attempt}`) ? task.attempt : null; },
  resultLink(task) { return `${weaponPath(task.request.weapon)}/optimizer?task-result=${encodeURIComponent(task.result_ref)}`; },
});
function optimizationTaskForPage() {
  if (!userTasks || !META || !$('weapon')?.value || optimizationReport) return null;
  const key = optimizationKey(nameableOptimizeBody(optimizeBody()));
  return userTasks.view().find((task) => task.kind === "optimization" && userTaskActive(task) && optimizationKey(task.request) === key) || null;
}
let optimizationShown = null;
function renderOptimizationTask(redraw = false) {
  const box = $("opt-results"), button = $("run-opt");
  if (!box || !button || !META || !$('weapon')?.value) return;
  const task = optimizationTaskForPage();
  optJobId = task?.id || null;
  optLastStatus = task?.progress?.status || null;
  if (task) {
    optimizationShown = task.id;
    button.disabled = true; button.textContent = tr(task.state === "queued" ? "Waiting" : "Optimizing…");
    if (optLastStatus) renderOptProgress(optLastStatus);
    else box.innerHTML = `<div class="placeholder">${escHtml(tr("Waiting"))}</div>`;
  } else if (optimizationShown) {
    const ended = userTasks?.rows.find((row) => row.id === optimizationShown);
    optimizationShown = null;
    button.textContent = tr("Run Optimizer"); updateOptEstimate();
    if (ended?.state === "succeeded" && redraw) {
      const weapon = $("weapon").value, key = optimizationKey(nameableOptimizeBody(optimizeBody()));
      userTasks.store.value(`artifact:${ended.result_ref}`).then((record) => {
        if (record && optimizationKey(record.request) === key && optimizationKey(nameableOptimizeBody(optimizeBody())) === key && $("weapon").value === weapon && !optimizationTaskForPage()) renderOptResults(record.result);
      }).catch(userTaskFailed);
    } else if (ended) box.innerHTML = `<div class="placeholder">${escHtml(tr(ended.error || userTaskLabel(ended.state)))}</div>`;
  }
  watchOptimizationClocks();
}
async function openOptimizationReport(id) {
  if (!id) { optimizationReport = null; return; }
  const weapon = $("weapon").value;
  optimizationReport = { id, unavailable: true };
  history.replaceState(null, "", `${weaponPath(weapon)}/optimizer?task-result=${encodeURIComponent(id)}`);
  const queue = await initUserTasks();
  const record = await queue.store.value(`artifact:${id}`);
  if (!record || record.request.weapon !== weapon) {
    $("opt-results").innerHTML = `<div class="placeholder">${escHtml(tr("This result is no longer available."))}</div>`;
    return;
  }
  optimizationReport = record;
  if (record.origin.build) whileApplying(() => restoreState(record.origin.build, weapon));
  if (record.origin.fight) { sim = taskClone(record.origin.fight); sim.__weapon = weapon; }
  if (record.origin.scenario) activeScenario = record.origin.scenario;
  if (record.origin.search) {
    activeOptPreset = null; applyOptState(record.origin.search); optSeeded = true; renderOpt();
  }
  history.replaceState(null, "", `${weaponPath(weapon)}/optimizer?task-result=${encodeURIComponent(id)}`);
  renderOptResults(record.result.search || record.result);
  if (record.kind === "appraisal") renderSavedAppraisal(record);
}

function leaveOptimizationReport() {
  if (!optimizationReport) return;
  optimizationReport = null;
  history.replaceState(null, "", `${weaponPath($("weapon").value)}/optimizer`);
}
window.addEventListener("wfsim:ready", () => {
  for (const id of ["opt-block", "opt-runbar"]) {
    const root = $(id);
    for (const name of ["input", "change"]) root?.addEventListener(name, leaveOptimizationReport, true);
    root?.addEventListener("click", (event) => { if (event.target.closest("button")) leaveOptimizationReport(); }, true);
  }
}, { once: true });
