// One paced observation covers this browser's pending community requests.
let communityTaskCapabilities = null, communityTaskPoll = null, communityTaskPollAt = 0;
const communityTaskStates = new Map();
const communityTaskCapable = (task) => !!communityTaskCapabilities?.kinds.includes(USER_TASK_TYPES.get(task.kind)?.community?.kind);
async function communityTaskOwner() {
  const queue = await initUserTasks();
  let secret = await queue.store.value("community-owner");
  if (!secret) {
    secret = [...crypto.getRandomValues(new Uint8Array(32))].map((byte) => byte.toString(16).padStart(2, "0")).join("");
    await userTaskLock("community-owner", async () => {
      const stored = await queue.store.value("community-owner");
      if (stored) secret = stored; else await queue.store.value("community-owner", secret);
    });
  }
  return secret;
}
async function communityTaskCall(action, body) {
  const response = await fetch(`/api/user-tasks/${action}`, { method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ ...body, owner: await communityTaskOwner() }) });
  let result;
  try { result = await response.json(); } catch (_) { throw new Error("Community execution is unavailable."); }
  if (!response.ok || !result.ok) {
    const error = new Error(result.error || "Community execution is unavailable."); error.status = response.status; throw error;
  }
  return result;
}
async function communityTaskAccept(task, command) {
  return communityTaskCall("accept", { id: command, task: task.id, kind: USER_TASK_TYPES.get(task.kind)?.community?.kind || task.kind, engine: task.engine, request: task.request });
}
async function observeCommunityTasks(id, signal) {
  if (!communityTaskPoll && (Date.now() >= communityTaskPollAt || !communityTaskStates.has(id))) {
    const ids = [...new Set([id, ...userTasks.rows.filter((row) => userTaskActive(row) && row.destination === "community").map((row) => row.remote_command || row.attempt).filter(Boolean)])].slice(0, 100);
    communityTaskPoll = communityTaskCall("status", { ids, include_results: false }).then((response) => {
      ids.forEach((id) => communityTaskStates.set(id, response.tasks.find((row) => row.id === id) || { id, state: "expired" }));
      communityTaskPollAt = Date.now() + Math.max(10_000, response.next_ms || 0);
    }).finally(() => { communityTaskPoll = null; });
  }
  if (communityTaskPoll) await communityTaskPoll;
  if (signal.aborted) throw new Error("Cancelled");
  return communityTaskStates.get(id);
}
USER_TASK_EXECUTORS.get("local").accept = async () => ({ id: null });
USER_TASK_EXECUTORS.set("community", {
  label: "Community compute",
  supports: (type) => !!type?.community,
  accept: communityTaskAccept,
  observe(task) {
    const observed = communityTaskStates.get(task.remote_command || task.attempt);
    if (task.state !== "queued" || !observed) return {};
    return { state: observed.state === "waiting" ? observed.execution_state || "queued" : "running",
      progress: { done: 0, total: 0, phase: observed.state === "succeeded" ? "Saving result" : observed.phase } };
  },
  cancel: (id) => id ? communityTaskCall("cancel", { ids: [id] }) : Promise.resolve(),
  async run(task, type, execution) {
    const id = task.remote_command || task.attempt;
    for (;;) {
      if (execution.signal.aborted) throw new Error("Cancelled");
      try { await communityTaskAccept(task, id); break; }
      catch (error) {
        if (error.status && error.status < 500 && error.status !== 429) throw error;
        execution.report({ phase: "Reconnecting to community compute", done: 0, total: 0 });
        await new Promise((resolve) => setTimeout(resolve, 10_000));
      }
    }
    // The acceptance identity is retained before observing or yielding the runner.
    await userTasks.change((state) => {
      const row = state.tasks.find((row) => row.id === task.id);
      if (row?.attempt === task.attempt && row.state === "running") row.remote_command = id;
      else (state.commands ||= []).push({ id: crypto.randomUUID(), destination: "community", remote: id, action: "cancel", delivered_at: null });
    });
    for (;;) {
      if (execution.signal.aborted) { userTasks.deliverCommands(); throw new Error("Cancelled"); }
      let observed;
      try { observed = await observeCommunityTasks(id, execution.signal); }
      catch (error) {
        if (execution.signal.aborted || (error.status && error.status < 500 && error.status !== 429)) throw error;
        communityTaskPollAt = Date.now() + 10_000;
        execution.report({ phase: "Reconnecting to community compute", done: 0, total: 0 });
        await new Promise((resolve) => setTimeout(resolve, 10_000)); continue;
      }
      if (observed?.state === "succeeded") {
        let response;
        try { response = await communityTaskCall("status", { ids: [id], include_results: true }); }
        catch (error) {
          if (error.status && error.status < 500 && error.status !== 429) throw error;
          execution.report({ phase: "Reconnecting to community compute", done: 0, total: 0 });
          await new Promise((resolve) => setTimeout(resolve, 10_000)); continue;
        }
        const result = response.tasks[0]?.result;
        if (!result) throw new Error("This result is no longer available.");
        communityTaskStates.delete(id);
        return type.community.finish ? type.community.finish(task, execution, result) : result;
      }
      if (["failed", "expired", "cancelled"].includes(observed?.state)) throw new Error(tr(observed.error || (observed.state === "failed" ? "The community reports disagreed. Retry this calculation." : "This community request has ended.")));
      execution.report({ phase: observed?.phase || "Waiting for community compute", done: 0, total: 0 });
      await new Promise((resolve) => setTimeout(resolve, 1000));
    }
  },
});
async function initCommunityTasks() {
  try {
    const response = await fetch("/api/user-tasks/capabilities");
    const result = response.ok && await response.json();
    communityTaskCapabilities = result?.available ? result : null;
  } catch (_) { communityTaskCapabilities = null; }
  scheduleUserTasks();
}
window.addEventListener("wfsim:ready", initCommunityTasks, { once: true });
