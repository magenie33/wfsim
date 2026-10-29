// Execution reads only the captured request; page state is used solely for presentation.
const simulationSubmitting = new Map();
const simulationContextId = (context) => JSON.stringify([context.weapon, context.preset, context.key]);
USER_TASK_TYPES.set("simulation", {
  schema: 1, icon: "▷", community: { kind: "simulation" },
  validate(request) {
    if (!request.weapon || !Number.isInteger(request.runs) || request.runs < 1) throw new Error("Invalid simulation request.");
  },
  describe(task) {
    const weapon = weaponInfo(task.request.weapon);
    return { title: `${weapon?.name || task.request.weapon} · ${tr("Simulation")}`,
      note: [task.origin.preset_name, benchLabelOr(task.origin.context?.scenario, task.origin.scenario_name), tr(USER_TASK_EXECUTORS.get(task.destination)?.label || "This device")].filter(Boolean).join(" · ") };
  },
  details(task) {
    return `<div>${escHtml(tr("running {n} simulations…").replace("{n}", task.request.runs))}</div><div>${escHtml(new Date(task.created_at).toLocaleString())}</div>`;
  },
  async execute(task, { signal, report }) {
    const release = holdForeground(), began = Date.now();
    track("simulator.start", task.request.weapon, task.request.runs);
    try {
      const result = await simulateFleet(task.request, (done, total) => {
        const fraction = total ? done / total : 0;
        report({ done, total, phase: "Simulation", eta_seconds: fraction >= 0.05 && done > 0
          ? (Date.now() - began) / 1000 / done * (total - done) : null });
      }, { signal });
      if (signal.aborted || result?.cancelled) throw new Error("stopped — nothing was measured");
      if (!result || result.ok === false) throw new Error(result?.error || "no data");
      return result;
    } finally { release(); }
  },
  async save(task, result, { persistent }) {
    const record = await resultAddCaptured(result, frozenTaskBuild(task.origin.build, task.request), task.origin.context, task.attempt, persistent);
    resultMem.set(resultMemKey(task.origin.context.weapon, task.origin.context.preset), { ...record, r: result });
    track("simulator.run", task.request.weapon, task.request.runs);
    noteSimRun(task.request.runs);
    if (task.origin.board) offerBoardSubmit({ body: task.origin.board, context: task.origin.context }).catch(() => {});
    return record.id;
  },
  async recover(task) {
    await loadResults();
    return results.some((record) => record.id === task.attempt) ? task.attempt : null;
  },
  resultLink(task) { return `${weaponPath(task.request.weapon)}/simulator?measurement=${encodeURIComponent(task.result_ref)}`; },
});

function simulationTaskForPage() {
  if (!userTasks || !META || !$("weapon").value) return null;
  const candidates = userTasks.view().filter((task) => task.kind === "simulation" && userTaskActive(task)
    && task.origin.context.weapon === presetWeapon() && task.origin.context.preset === activePreset && task.request.runs === simRuns());
  if (!candidates.length) return null;
  const key = simKey();
  return candidates.find((task) => task.origin.context.key === key) || null;
}
function simulationResultForPage() {
  if (!userTasks || activePreset) return null;
  const key = simKey();
  const task = userTasks.view().slice().reverse().find((task) => task.kind === "simulation" && task.state === "succeeded"
    && task.origin.context.weapon === presetWeapon() && !task.origin.context.preset && task.origin.context.key === key);
  return task ? results.find((record) => record.id === task.result_ref) || null : null;
}
function renderSimulationTask(redrawResult = false) {
  const box = $("sim-results"), btn = $("run-sim");
  if (!box || !btn || !META || !$("weapon").value) return false;
  const task = simulationTaskForPage();
  btn.disabled = !!task || simulationSubmitting.has(simulationContextId(resultContext()));
  btn.textContent = tr(task ? "Simulating…" : btn.disabled ? "Adding to queue…" : "Run Simulation");
  if (!task) {
    const shown = box.querySelector(".simrun[data-user-task]");
    const ended = shown && userTasks?.rows.find((row) => row.id === shown.dataset.userTask);
    if (ended?.state === "cancelled") box.innerHTML = `<div class="placeholder">${escHtml(tr("stopped — nothing was measured"))}</div>`;
    else if (ended?.state === "failed" || ended?.state === "interrupted") box.innerHTML = `<div class="error">${escHtml(tr(ended.error || "Interrupted"))}</div>`;
    else if (ended?.state === "succeeded" && redrawResult) {
      renderStoredSimResult();
    }
    return false;
  }
  show("sim-results-block", true);
  if (box.querySelector(".simrun")?.dataset.userTask !== task.id) {
    box.innerHTML = `<div class="placeholder simrun" data-user-task="${task.id}"><div class="simrun-t"></div><div class="simrun-bar"><span id="sim-prog" style="width:0%"></span></div><div class="simrun-n" id="sim-prog-n"></div><button type="button" id="sim-stop" class="ghost-btn small">${escHtml(tr("stop"))}</button></div>`;
    $("sim-stop").onclick = () => userTasks.cancel(task.id).catch(userTaskFailed);
  }
  box.querySelector(".simrun-t").textContent = task.state === "queued" ? tr("Waiting") : tr("running {n} simulations…").replace("{n}", task.request.runs);
  const progress = task.progress;
  $("sim-prog").style.width = `${progress?.total > 0 ? Math.min(100, 100 * progress.done / progress.total) : 0}%`;
  $("sim-prog-n").textContent = progress?.total > 0 ? `${progress.done} / ${progress.total}`
    + (progress.eta_seconds > 1 ? ` · ${tr("about {s}s left").replace("{s}", Math.ceil(progress.eta_seconds))}` : "") : "";
  return true;
}

const quickTask = new UserTransientTask("quick");
function observeQuickTask() {
  if (!gainScan.want) { if (quickTask.current) { quickTask.cancel(); quickTask.current = null; } return; }
  if (!quickTask.current || quickTask.current.request !== gainScan.want) {
    if (!gainScan.want) return;
    quickTask.replace(gainScan.want, { weapon: $("weapon").value, scenario_name: gainScenario().name });
  }
  if (quickTask.current.state === "cancelled" && !gainScan.running) return;
  quickTask.report(quickTask.generation, gainScan.running ? "running" : gainScan.failed ? "failed" : gainScan.key ? "succeeded" : "ready",
    { phase: gainScan.phase || gainScan.note || "ranking the options", done: gainScan.done, total: gainScan.total,
      eta_seconds: gainScan.etaMs > 0 ? gainScan.etaMs / 1000 : null });
}
USER_TASK_TYPES.set("quick", {
  icon: "ϟ",
  describe(task) { return { title: tr("Quick calculation"), note: [weaponInfo(task.origin.weapon)?.name, task.origin.scenario_name].filter(Boolean).join(" · ") }; },
  details() { return quickCalculationDetails(); },
  actions(task, action) { return (task.state === "running" ? action("cancel", "Cancel") : "") + action("reset", "rebuild the calculator"); },
  act(action) {
    if (action === "cancel") {
      gainGen += 1; gainPending = null; gainScan.running = false;
      quickTask.cancel(); calcDirty();
    } else if (action === "reset") { restartCalc(); refreshGains(); }
    else throw new Error("Unknown task action.");
  },
});
userTaskSources.set("quick", () => {
  if (!META || !$("weapon").value) return [];
  observeQuickTask();
  return [quickTask.current || { id: "quick", state: "ready", origin: { weapon: $("weapon").value, scenario_name: gainScenario().name } }];
});

let simulationReport = null;
async function openSimulationReport(weapon, id) {
  if (!id) { simulationReport = null; return; }
  history.replaceState(null, "", `${weaponPath(weapon.id)}/simulator?measurement=${encodeURIComponent(id)}`);
  await loadResults();
  const record = results.find((row) => row.id === id && row.weapon === weapon.id);
  if (!record) { simulationReport = { id }; renderStoredSimResult(); return; }
  simulationReport = record;
  const path = `${weaponPath(weapon.id)}/simulator?measurement=${encodeURIComponent(id)}`;
  whileApplying(() => restoreState(record.build, weapon.id));
  activePreset = record.preset;
  activeScenario = record.scenario;
  sim = taskClone(record.fight);
  sim.__weapon = weapon.id;
  history.replaceState(null, "", path);
  $("sim-history").replaceChildren();
  show("sim-viewing", false);
  renderSim();
  show("sim-results-block", true);
  renderResults(record.r, record.at, record.build);
}
