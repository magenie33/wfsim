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
    for (;;) {
      if (signal.aborted) throw new Error("Cancelled");
      const status = await api("/api/optimize/status", { id }, null, { signal });
      if (!status || status.ok === false || status.phase === "error") throw new Error(status?.result?.error || status?.error || "no data");
      const fraction = optProgressOf(status);
      report({ phase: "Optimization", done: fraction == null ? 0 : fraction, total: fraction == null ? 0 : 1,
        status, builds: status.enumerated || 0, fights: status.sims_done || 0 });
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
  validate(request) { if (!request.weapon || !Array.isArray(request.starts)) throw new Error("Invalid optimization request."); },
  describe(task) {
    return { title: `${weaponInfo(task.request.weapon)?.name || task.request.weapon} · ${tr("Optimization")}`,
      note: [task.origin.preset_name, task.origin.scenario_name, tr(USER_TASK_EXECUTORS.get(task.destination)?.label || "This device")].filter(Boolean).join(" · ") };
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
