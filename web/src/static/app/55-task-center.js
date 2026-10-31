// Types own their labels, capabilities and result readers; the list owns none of their inputs.
let userTasks = null, userTasksReady = null, userTaskChannel = null;
let userTaskPanelOpen = false, userTaskFilter = "all", userTaskDetails = null, userTaskError = "";
let userTaskFrame = null, userTaskRenderState = false, userTaskDeliveryTimer = null;
const userTaskSources = new Map();
const userTaskLocks = new Map();
const userTaskLock = (name, work) => {
  if (navigator.locks) return navigator.locks.request(`wfsim-user-tasks:${name}`, work);
  const next = (userTaskLocks.get(name) || Promise.resolve()).catch(() => {}).then(work);
  userTaskLocks.set(name, next);
  return next;
};

async function initUserTasks() {
  if (userTasksReady) return userTasksReady;
  userTasksReady = (async () => {
    let store;
    try {
      if (!navigator.locks || !await resultsDb()) throw new Error("Session storage only");
      store = await UserTaskBrowserStore.open();
    } catch (_) { store = new UserTaskMemoryStore(); }
    if (store.persistent && typeof BroadcastChannel === "function") {
      userTaskChannel = new BroadcastChannel("wfsim-user-tasks");
    }
    userTasks = new UserTaskQueue({ store, types: USER_TASK_TYPES, engine: ENGINE_ID, lock: userTaskLock,
      changed: scheduleUserTasks, retryDelivery: retryUserTaskDelivery, publish: (message) => userTaskChannel?.postMessage(message) });
    if (userTaskChannel) userTaskChannel.onmessage = (event) => {
      if (event.data.kind === "progress") userTasks.hearProgress(event.data);
      else userTasks.refresh().then(() => userTasks.wake()).catch(userTaskFailed);
    };
    await userTasks.refresh();
    userTasks.wake();
    return userTasks;
  })();
  return userTasksReady;
}
function retryUserTaskDelivery() {
  if (userTaskDeliveryTimer) return;
  userTaskDeliveryTimer = setTimeout(() => {
    userTaskDeliveryTimer = null;
    userTasks?.deliverCommands(); userTasks?.deliverEvents();
  }, 10_000);
}
function userTaskFailed(error) {
  userTaskError = String(error.message || error);
  scheduleUserTasks();
}
function scheduleUserTasks(kind) {
  if (kind !== "progress") userTaskRenderState = true;
  if (userTaskFrame != null) return;
  userTaskFrame = requestAnimationFrame(() => {
    userTaskFrame = null;
    renderUserTasks();
    if (typeof renderSimulationTask === "function") renderSimulationTask(userTaskRenderState);
    if (typeof renderOptimizationTask === "function") renderOptimizationTask(userTaskRenderState);
    if (typeof refreshRivenAnalysis === "function") refreshRivenAnalysis().catch(userTaskFailed);
    if (typeof refreshOptimizerHistory === "function") refreshOptimizerHistory().catch(userTaskFailed);
    userTaskRenderState = false;
  });
}
const userTaskLabel = (state) => tr(({ queued: "Waiting", running: "Running", succeeded: "Completed",
  failed: "Failed", cancelled: "Cancelled", interrupted: "Interrupted", ready: "Ready" })[state] || state);
const userTaskActive = (task) => ["queued", "running"].includes(task.state);
function userTaskItems() {
  const stored = userTasks ? userTasks.view().map((task) => ({ ...task, ...USER_TASK_EXECUTORS.get(task.destination || "local")?.observe?.(task) })) : [];
  const transient = [...userTaskSources].flatMap(([kind, source]) => source().map((task) => ({ ...task, kind, transient: true })));
  return [...transient, ...stored];
}
function userTaskProgress(task) {
  const progress = task.progress;
  return { fraction: progress?.total > 0 ? Math.max(0, Math.min(1, progress.done / progress.total)) : null,
    phase: tr(progress?.phase || "calculating"),
    count: (progress?.total > 0 ? `${progress.done} / ${progress.total}` : "")
      + (progress?.eta_seconds > 1 ? ` · ${tr("about {s}s left").replace("{s}", Math.ceil(progress.eta_seconds))}` : "") };
}
function userTaskRow(task, waiting) {
  const type = USER_TASK_TYPES.get(task.kind), say = type?.describe ? type.describe(task) : { title: task.kind };
  const { fraction, phase, count } = userTaskProgress(task);
  const bar = task.state === "running" ? `<div class="task-progress${fraction == null ? " indeterminate" : ""}" role="progressbar" aria-label="${escHtml(say.title)}"${fraction == null ? "" : ` aria-valuemin="0" aria-valuemax="100" aria-valuenow="${Math.round(fraction * 100)}"`}><i style="width:${fraction == null ? 30 : fraction * 100}%"></i></div>
    <div class="task-progress-note"><span>${escHtml(phase)}</span><span>${escHtml(count)}</span></div>` : "";
  const action = (name, label) => `<button type="button" class="ghost-btn" data-user-task="${escHtml(task.id)}" data-user-task-action="${name}">${escHtml(tr(label))}</button>`;
  let actions = "";
  if (task.transient) actions = type?.actions ? type.actions(task, action) : "";
  else {
    if (userTaskActive(task)) {
      actions += action("cancel", "Cancel");
      if (task.destination === "community") actions += action("local", "Run on this device");
      else if (communityTaskCapable(task)) actions += action("community", "Send to community");
    }
    if (["failed", "cancelled", "interrupted"].includes(task.state) && task.engine === ENGINE_ID && type
        && task.request_schema === type.schema && USER_TASK_EXECUTORS.get(task.destination || "local")?.supports(type)) actions += action("retry", "Retry");
    if (task.result_ref && type?.resultLink) actions += `<a class="ghost-btn task-result" data-native target="_blank" rel="noopener" href="${escHtml(type.resultLink(task))}">${escHtml(tr("View result"))}</a>`;
    if (!userTaskActive(task)) actions += action("remove", "Remove from list");
    if (task.delivery_pending) actions += action("deliver", "Retry delivery");
    if (task.state === "queued" && USER_TASK_EXECUTORS.get(task.destination || "local")?.reorder) {
      waiting = waiting.filter((row) => (row.destination || "local") === (task.destination || "local"));
      const index = waiting.findIndex((row) => row.id === task.id);
      actions += `<span class="task-order"><button class="ghost-btn" data-user-task="${task.id}" data-user-task-action="up" aria-label="${escHtml(tr("Move earlier"))}" title="${escHtml(tr("Move earlier"))}"${index < 1 ? " disabled" : ""}>↑</button><button class="ghost-btn" data-user-task="${task.id}" data-user-task-action="down" aria-label="${escHtml(tr("Move later"))}" title="${escHtml(tr("Move later"))}"${index >= waiting.length - 1 ? " disabled" : ""}>↓</button></span>`;
    }
  }
  const details = userTaskDetails === task.id;
  return `<article class="task-row" data-task-id="${escHtml(task.id)}" data-state="${task.state}"><div class="task-title-row"><span class="task-kind" aria-hidden="true">${escHtml(type?.icon || "≡")}</span><div class="task-title"><button type="button" class="task-name" data-user-task="${escHtml(task.id)}" data-user-task-action="details" aria-expanded="${details}">${escHtml(say.title)}</button><div class="task-origin">${escHtml(say.note || "")}</div></div><span class="task-state">${escHtml(userTaskLabel(task.state))}</span></div>${bar}<div class="task-actions">${actions}</div>${userTaskActive(task) && (task.destination === "community" || communityTaskCapable(task)) ? `<div class="task-origin">${escHtml(tr("Transfer restarts from the same inputs."))}</div>` : ""}${details ? `<div class="task-details">${type?.details ? type.details(task) : ""}${userTaskActive(task) && communityTaskCapable(task) ? `<p>${escHtml(tr(task.destination === "community" ? "Moving to this device restarts from the fixed inputs." : "Community compute receives these calculation inputs. Transfer restarts the calculation; the task and inputs stay the same."))}</p>` : ""}${task.error ? `<p class="error">${escHtml(tr(task.error))}</p>` : ""}</div>` : ""}</article>`;
}
function renderUserTasks() {
  const toggle = $("user-tasks-toggle"), panel = $("user-tasks-panel");
  if (!toggle || !panel || !META) return;
  const items = userTaskItems(), active = items.filter(userTaskActive);
  const queueError = userTasks?.error || userTasks?.commandError || userTasks?.eventError;
  const history = items.filter((task) => !task.transient && !userTaskActive(task));
  const count = $("user-tasks-count");
  count.textContent = String(active.length).padStart(2, "0");
  count.hidden = false;
  toggle.classList.toggle("has-unread", items.some((task) => !task.transient && USER_TASK_TERMINAL.has(task.state) && !task.read_at));
  toggle.setAttribute("aria-expanded", String(userTaskPanelOpen));
  toggle.classList.toggle("has-error", items.some((task) => ["failed", "interrupted"].includes(task.state)));
  toggle.title = tr("Task center");
  toggle.setAttribute("aria-label", tr("Task center"));
  panel.setAttribute("aria-label", tr("Task center"));
  panel.hidden = !userTaskPanelOpen;
  if (!userTaskPanelOpen) return;
  const focused = document.activeElement;
  const focusTask = focused?.dataset.userTask, focusAction = focused?.dataset.userTaskAction;
  const focusFilter = focused?.dataset.userTaskFilter;
  const oldScroll = panel.querySelector(".task-list")?.scrollTop || 0;
  const visible = items.filter((task) => userTaskFilter === "all" || (userTaskFilter === "active" ? userTaskActive(task) : !task.transient && !userTaskActive(task)));
  const signature = JSON.stringify([userTaskFilter, userTaskDetails, visible.map((task) => [task.id, task.state, task.error, task.result_ref, task.delivery_pending]), userTaskError, queueError, userTasks?.store.persistent, !!communityTaskCapabilities]);
  if (panel.dataset.signature === signature) {
    for (const row of panel.querySelectorAll("[data-task-id]")) {
      const task = visible.find((item) => item.id === row.dataset.taskId);
      if (!task) continue;
      const type = USER_TASK_TYPES.get(task.kind), say = type?.describe ? type.describe(task) : { title: task.kind };
      row.querySelector(".task-name").textContent = say.title;
      row.querySelector(".task-origin").textContent = say.note || "";
      const bar = row.querySelector(".task-progress");
      if (bar) {
        const { fraction, phase, count } = userTaskProgress(task);
        bar.classList.toggle("indeterminate", fraction == null);
        bar.querySelector("i").style.width = `${fraction == null ? 30 : fraction * 100}%`;
        if (fraction == null) bar.removeAttribute("aria-valuenow"); else bar.setAttribute("aria-valuenow", Math.round(fraction * 100));
        const notes = row.querySelector(".task-progress-note").children;
        notes[0].textContent = phase; notes[1].textContent = count;
      }
      if (task.transient && userTaskDetails === task.id && type?.details) row.querySelector(".task-details").innerHTML = type.details(task);
    }
    return;
  }
  const groups = [["Automatic", visible.filter((task) => task.transient && task.kind !== "community-work")], ["For other players", visible.filter((task) => task.kind === "community-work")], ["Queue", visible.filter((task) => !task.transient && userTaskActive(task))], ["History", visible.filter((task) => !task.transient && !userTaskActive(task)).reverse()]];
  const waiting = items.filter((task) => !task.transient && task.state === "queued");
  const batches = [...new Set(items.map((task) => task.origin?.batch).filter(Boolean))].map((batch) => {
    const tasks = items.filter((task) => task.origin?.batch === batch);
    return `<div class="task-batch">${escHtml(tr(tasks[0].origin.batch_name))} · ${escHtml(tr("{done} of {total} completed").replace("{done}", tasks.filter((task) => task.state === "succeeded").length).replace("{total}", tasks.length))} · ${escHtml(tr("{n} failed").replace("{n}", tasks.filter((task) => task.state === "failed").length))}</div>`;
  }).join("");
  panel.innerHTML = `<div class="task-head"><h2>${escHtml(tr("Task center"))}</h2><button type="button" class="ghost-btn" data-user-task-action="close" aria-label="${escHtml(tr("Close"))}">×</button></div>
    <div class="task-filters">${[["all", "All", items.length], ["active", "In progress", active.length], ["history", "History", history.length]].map(([id, word, n]) => `<button type="button" class="pchip${userTaskFilter === id ? " sel" : ""}" data-user-task-filter="${id}" aria-pressed="${userTaskFilter === id}">${escHtml(tr(word))}<small>${n}</small></button>`).join("")}</div>
    <div class="task-list scroll-thin">${batches}${groups.map(([label, tasks]) => tasks.length ? `<div class="task-group">${escHtml(tr(label))} · ${tasks.length}</div>${tasks.map((task) => userTaskRow(task, waiting)).join("")}` : "").join("") || `<div class="task-empty">${escHtml(tr("Tasks from calculation pages appear here."))}</div>`}</div>
    ${(userTaskError || queueError) ? `<div class="task-error" role="status">${escHtml(tr(userTaskError || queueError))}</div>` : ""}<div class="task-foot">${escHtml(tr(userTasks?.store.persistent ? "Saved in this browser" : "Tasks are kept for this session only."))}</div>`;
  panel.querySelector(".task-list").scrollTop = oldScroll;
  panel.dataset.signature = signature;
  if (focusTask && focusAction) [...panel.querySelectorAll("[data-user-task]")].find((element) => element.dataset.userTask === focusTask && element.dataset.userTaskAction === focusAction)?.focus({ preventScroll: true });
  if (focusFilter) [...panel.querySelectorAll("[data-user-task-filter]")].find((element) => element.dataset.userTaskFilter === focusFilter)?.focus({ preventScroll: true });
}
async function userTaskAction(action, id) {
  userTaskError = "";
  if (action === "close") { userTaskPanelOpen = false; $("user-tasks-toggle").focus(); }
  else if (action === "details") userTaskDetails = userTaskDetails === id ? null : id;
  else {
    const task = userTaskItems().find((row) => row.id === id);
    if (!task) throw new Error("This task is no longer available.");
    if (task.transient) await USER_TASK_TYPES.get(task.kind).act(action, task);
    else if (action === "deliver") { userTasks.deliverEvents(); userTasks.deliverCommands(); }
    else if (["local", "community"].includes(action)) await userTasks.transfer(id, action);
    else if (action === "up" || action === "down") await userTasks.move(id, action === "up" ? -1 : 1);
    else if (["cancel", "retry", "remove"].includes(action)) await userTasks[action](id);
    else throw new Error("Unknown task action.");
  }
  renderUserTasks();
}
function wireUserTasks() {
  const toggle = $("user-tasks-toggle"), panel = $("user-tasks-panel");
  if (!toggle || !panel) return;
  toggle.onclick = () => { userTaskPanelOpen = !userTaskPanelOpen; renderUserTasks(); if (userTaskPanelOpen) userTasks?.markRead().catch(userTaskFailed); };
  panel.onclick = (event) => {
    const filter = event.target.closest("[data-user-task-filter]");
    if (filter) { userTaskFilter = filter.dataset.userTaskFilter; renderUserTasks(); return; }
    const action = event.target.closest("[data-user-task-action]");
    if (action && !action.disabled) userTaskAction(action.dataset.userTaskAction, action.dataset.userTask).catch(userTaskFailed);
  };
  document.addEventListener("click", (event) => {
    if (userTaskPanelOpen && !event.composedPath().some((node) => node === panel || node === toggle)) {
      userTaskPanelOpen = false; renderUserTasks();
    }
  });
  document.addEventListener("keydown", (event) => {
    if (imeComposing(event)) return;
    if (event.key === "Escape" && userTaskPanelOpen) { userTaskPanelOpen = false; renderUserTasks(); toggle.focus(); }
  });
  initUserTasks().catch(userTaskFailed);
  renderUserTasks();
}
window.addEventListener("wfsim:ready", wireUserTasks, { once: true });

// Side shortcuts delegate to the page controls and mirror their availability.
let executionDockFrame = 0;
function scheduleExecutionDock() {
  if (executionDockFrame) return;
  executionDockFrame = requestAnimationFrame(() => { executionDockFrame = 0; refreshExecutionDock(); });
}
function refreshExecutionDock() {
  let dock = $("execution-dock");
  const enabled = !!META && !!routeWeaponId() && !EMBED && ["", "simulator", "optimizer"].includes(modSuffix());
  if (!dock && !enabled) return;
  if (!dock) {
    dock = document.createElement("aside"); dock.id = "execution-dock";
    dock.setAttribute("aria-label", tr("Simulation and optimization"));
    dock.innerHTML = [["run-sim", "▷", "Simulate"], ["run-opt", "★", "Optimize"]].map(([id, icon, label]) =>
      `<button type="button" class="execution-shortcut" data-execution-run="${id}"><span aria-hidden="true">${icon}</span><span>${escHtml(tr(label))}</span></button>`).join("");
    document.body.append(dock);
    dock.querySelectorAll("[data-execution-run]").forEach(button => {
      button.onclick = () => { const source = $(button.dataset.executionRun); if (source && !source.disabled) source.click(); };
    });
    const observer = new MutationObserver(scheduleExecutionDock);
    for (const id of ["run-sim", "run-opt", "opt-estimate"]) {
      const node = $(id); if (!node) continue;
      observer.observe(node, {subtree:true,childList:true,characterData:true,attributes:true,attributeFilter:["disabled","title"]});
    }
  }
  dock.hidden = !enabled;
  dock.querySelectorAll("[data-execution-run]").forEach(button => {
    const source = $(button.dataset.executionRun);
    button.disabled = !source || source.disabled;
    const label = source?.textContent.trim() || "";
    button.setAttribute("aria-label", label);
    button.title = label + (source?.disabled && button.dataset.executionRun === "run-opt" ? " · " + $("opt-estimate").textContent.trim() : "");
  });
}
