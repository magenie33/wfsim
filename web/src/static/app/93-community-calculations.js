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
        while (!signal.aborted && next < pieces.length && (ownTabBusy() || index >= communityLanes())) await new Promise((resolve) => setTimeout(resolve, 250));
        if (signal.aborted) throw new Error("Cancelled");
        if (next >= pieces.length) break;
        const at = next++, from = at * count, n = Math.min(count, runs - from), began = performance.now();
        const part = await lane.send({ kind: "shard", body: request, from, count: n }, null, true);
        if (!part || part.ok === false || part.cancelled) throw Object.assign(new Error(part?.error || "The worker could not finish."), { engineRefused: part?.ok === false && !part.worker_dead });
        pieces[at] = part; finished += n;
        computeProgress(finished, runs);
        await lane.send({ kind: "rest", ms: communityRest(performance.now() - began) }, null, true);
      }
    }));
    if (signal.aborted) throw new Error("Cancelled");
    return await lanes[0].send({ kind: "merge", body: request, shards: pieces }, null, true);
  } finally { signal.removeEventListener("abort", stop); stop(); }
}
async function communityOptimizationReport(request, { signal }) {
  const job = quickFleet(request, () => communityLanes(), async () => {
    while (ownTabBusy() && !signal.aborted) await new Promise((resolve) => setTimeout(resolve, 250));
  }, { keep: true, rest: communityRest });
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
    const report = await compute(work.request, { signal: controller.signal });
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
