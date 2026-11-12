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
    const record = await resultAddCaptured(result, frozenTaskBuild(task.origin.build, task.request, task.origin.input_aliases), task.origin.context, task.attempt, persistent);
    resultMem.set(resultMemKey(task.origin.context.weapon, task.origin.context.preset), { ...record, r: result });
    track("simulator.run", task.request.weapon, task.request.runs);
    noteSimRun(task.request.runs);
    return record.id;
  },
  async completed(task) {
    if (!results.some(record => record.id === task.result_ref)) await loadResults();
    let record = results.find(record => record.id === task.result_ref);
    if (!record) return;
    if (task.source_id && task.source_id !== taskSources(task)[0].id) {
      record = await resultAddCaptured(record.r, frozenTaskBuild(task.origin.build, task.request, task.origin.input_aliases),
        task.origin.context, `${task.result_ref}:${task.source_id}`, userTasks.store.persistent);
    }
    resultMem.set(resultMemKey(task.origin.context.weapon, task.origin.context.preset), record);
    if (task.origin.board) await offerBoardSubmit({ body: task.origin.board, context: task.origin.context });
    if (boardReviewEnabled()) await requestBoardReview(task, record.r, record);
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
    && taskSources(task).some(source => source.origin.context?.weapon === presetWeapon() && source.origin.context?.preset === activePreset) && task.request.runs === simRuns());
  if (!candidates.length) return null;
  const key = simKey();
  return candidates.find((task) => taskSources(task).some(source => source.origin.context?.key === key)) || null;
}
function simulationResultForPage() {
  if (!userTasks || activePreset) return null;
  const key = simKey();
  const task = userTasks.view().slice().reverse().find((task) => task.kind === "simulation" && task.state === "succeeded"
    && taskSources(task).some(source => source.origin.context?.weapon === presetWeapon() && !source.origin.context?.preset && source.origin.context?.key === key));
  if (!task) return null;
  const source = taskSources(task).find(source => source.origin.context?.weapon === presetWeapon() && !source.origin.context?.preset && source.origin.context?.key === key);
  const id = source.id === taskSources(task)[0].id ? task.result_ref : `${task.result_ref}:${source.id}`;
  return results.find(record => record.id === id) || null;
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

// A review observes one public verification round without holding local calculation resources.
const boardReviewTrackers = new Map();
const boardReviewEnabled = () => localStorage.getItem("wfsim-board-review") !== "off";
let boardReviewTimer = null, boardReviewPolling = false;
const boardReviewEnded = state => ["succeeded", "superseded", "failed", "unavailable"].includes(state);
const BOARD_PUBLICATION_MAX_ATTEMPTS = 30;
const boardReviewNeedsPublication = task => ["succeeded", "superseded"].includes(task.state)
  && task.publication !== "published" && (task.publication_attempts || 0) < BOARD_PUBLICATION_MAX_ATTEMPTS;
const boardReviewActive = task => !boardReviewEnded(task.state) || boardReviewNeedsPublication(task);
const boardReviewLink = task => `${weaponPath(task.weapon)}/benchmark?bench=${encodeURIComponent(task.ruler)}`;
USER_TASK_TYPES.set("board-review", {
  icon: "↻",
  describe(task) { return { title: tr("Board calibration"), note: `${weaponInfo(task.weapon)?.name || task.weapon} · ${benchmarkName(task.ruler)}` }; },
  actions(task, action) { return `<a class="ghost-btn" data-review-board="${escHtml(task.id)}" href="${boardReviewLink(task)}">${escHtml(tr("View board"))}</a>` + action("hide", "Hide"); },
  details(task) { return `<p>${escHtml(tr("The published score stays until independent verification finishes."))}</p>`
    + `<p>${escHtml(tr("Hiding this item keeps the shared review running."))}</p>`
    + (task.score == null ? "" : `<b>${escHtml(tr("Verified score"))}: ${escHtml(fmtScore(task.score))}</b>`); },
  async act(action, task) {
    if (action !== "hide") throw new Error("Unknown task action.");
    boardReviewTrackers.delete(task.id); await saveBoardReviewTrackers(); renderBoardReviewNote();
  },
});
userTaskSources.set("board-review", () => [...boardReviewTrackers.values()].map(task => ({ ...task, kind: "board-review",
  state: task.state === "submitting" ? "queued" : boardReviewNeedsPublication(task) ? "running" : task.state === "waiting" ? task.phase === "Waiting for community compute" ? "queued" : "running"
    : ["succeeded", "superseded"].includes(task.state) ? "succeeded" : "failed",
  progress: { done: task.witnesses || 0, total: 2, phase: task.phase || "Waiting for community compute" } })));
function boardReviewCanonical(value) {
  if (Array.isArray(value)) return value.map(boardReviewCanonical);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(Object.keys(value).sort().filter(key => value[key] !== undefined).map(key => [key, boardReviewCanonical(value[key])]));
}
function boardReviewRequest(value) {
  const body = taskClone(value);
  for (const key of ["replay", "replay_follow", "__weapon"]) delete body[key];
  const definitions = new Map((body.rivens || []).map(entry => [RIVEN_PREFIX + (entry.id || entry.name), entry.spec]));
  const valence = headlessDefaultValence(META, body.weapon, null);
  body.valence_element ??= valence.element; body.valence_bonus ??= valence.bonus;
  const used = [];
  body.mods = (body.mods || []).map(id => {
    if (!isRivenId(id)) return id;
    const spec = definitions.get(id);
    if (!spec) return id;
    const plain = taskClone(spec);
    for (const key of ["drafts", "polarity", "shape", "__weapon"]) delete plain[key];
    plain.bonuses = (plain.bonuses || []).slice().sort((a, b) => a.id.localeCompare(b.id));
    plain.rank ??= META.riven_rules.max_rank;
    used.push({ id: "review", spec: plain });
    return "riven:review";
  });
  body.rivens = used;
  body.arcane_rank = (body.arcane || []).map((id, i) => body.arcane_rank?.[i] ?? arcaneById(id)?.max_rank ?? null);
  return JSON.stringify(boardReviewCanonical(body));
}
function boardReviewRecord(weapon, row) { return headlessBoardRecord(weapon, row); }

// Compare only fetched publication with the frozen record and current fact.
// Missing rows count only when the existing inclusive entry-line rule excludes them.
function boardReviewPublication(task, rows, indexRows) {
  if (!task.record || task.record.weapon !== task.weapon || !Number.isFinite(task.current_score)
    || !task.current_by || !Array.isArray(rows) || !Array.isArray(indexRows)) return false;
  const key = value => JSON.stringify(boardReviewCanonical(boardReviewRecord(task.weapon, value)));
  const wanted = key(task.record);
  const cell = values => values.filter(row => row.benchmark === task.ruler && (row.mode || "base") === task.mode);
  const agrees = row => {
    const completed = Date.parse(task.completed_at), measured = Date.parse(row.measured_at);
    return String(row.measured_by || "").replace(/^verified:/, "") === String(task.current_by).replace(/^verified:/, "")
      && Math.abs(row.score - task.current_score) <= 1e-12 * Math.max(1, Math.abs(task.current_score))
      && (task.state === "superseded" || (Number.isFinite(completed) && Number.isFinite(measured)
        && measured >= Math.floor(completed / 60_000) * 60_000));
  };
  const published = cell(rows), index = cell(indexRows);
  const matching = published.filter(row => key(row) === wanted), indexed = index.filter(row => key(row) === wanted);
  if (indexed.some(row => !agrees(row))) return false;
  if (matching.length) return matching.every(agrees);
  if (task.record.wielder || (task.record.riven_pos?.length && !Array.isArray(task.record.riven_rolls))) return false;
  if (!published.length || !index.length || !(META.board_entry_share > 0)) return false;
  return !boardGroupLeaders(published)({ benchmark: task.ruler, mode: task.mode, score: task.current_score,
    ...(task.record.riven_pos?.length ? { riven: {} } : {}) }, META.board_entry_share);
}
function boardReviewEffective(value, weapon, buffs) {
  const body = taskClone(value), cls = weaponInfo(weapon)?.weapon_class;
  body.buffs = Object.fromEntries(Object.entries(body.buffs || {}).filter(([id, setting]) => buffs.has(id)
    && !(Object.keys(setting).every(key => ["stacks", "locked"].includes(key))
      && setting.stacks === buffs.get(id).default_stacks && !!setting.locked === !!buffs.get(id).default_locked)));
  if (body.class_rules) {
    body.class_rules = Object.fromEntries(Object.entries(body.class_rules).filter(([key]) => key === cls
      || !(META.class_rules?.classes || []).includes(key)));
    if (!Object.keys(body.class_rules).length) delete body.class_rules;
  }
  return body;
}
async function matchingBoardReview(task) {
  if (task.engine !== ENGINE_ID) return null;
  if (!benchList().some(bench => task.request.runs === bench.scenario.runs
    && (!task.origin.benchmark || task.origin.benchmark === bench.id))) return null;
  const weapon = task.request.weapon;
  await loadBoard(weapon);
  if (!BOARD_HAVE.has(weapon)) throw new Error("The board is unavailable.");
  const input = await api("/api/computation/input", { kind: "simulation", request: task.request });
  if (!input?.ok) return null;
  const mods = JSON.stringify((input.request.mods || []).map(id => isRivenId(id) ? BOARD_RIVEN_SLOT : id));
  const rows = taskClone((BOARD[weapon] || []).filter(row => JSON.stringify((row.mods || []).concat(row.exilus ? [row.exilus] : [])) === mods));
  for (const row of rows) {
    if ((row.mode || "base") !== task.request.mode || (task.origin.benchmark && row.benchmark !== task.origin.benchmark)) continue;
    const bench = benchList().find(item => item.id === row.benchmark);
    if (!bench || task.request.runs !== bench.scenario.runs) continue;
    const ordered = await api("/api/board/order", { record: boardReviewRecord(weapon, row), ruler: row.benchmark,
      mode: row.mode || "base", observation: task.request });
    if (!ordered?.ok || ordered.matches_observation !== true || !/^[a-f0-9]{32}$/.test(ordered.identity || "")) continue;
    return { row, identity: ordered.identity, ruler: row.benchmark, mode: row.mode || "base", weapon };
  }
  return null;
}
async function requestBoardReview(task, result, record) {
  const matched = await matchingBoardReview(task);
  if (!matched) return;
  const metric = metricOf((benchList().find(bench => bench.id === matched.ruler)?.scenario || {}).metric);
  const measured = metricValue(metric, result);
  const old = matched.row.score;
  if (!Number.isFinite(measured) || !Number.isFinite(old)) return;
  if (Math.abs(measured - old) <= 1e-12 * Math.max(Math.abs(measured), Math.abs(old))) return;
  const id = "board-review-request:" + record.id;
  boardReviewTrackers.set(id, { id, identity: matched.identity, weapon: matched.weapon, ruler: matched.ruler, mode: matched.mode,
    engine: task.engine, observed_score: measured, baseline_score: old, metric: metric.id,
    request: structuredClone(task.request), source: record.id, sources: [record.id], state: "submitting", phase: "Requesting board calibration" });
  await saveBoardReviewTrackers(); renderBoardReviewNote(); scheduleUserTasks(); scheduleBoardReviews(0);
}
async function acceptBoardReview(task) {
  const response = await fetch("/api/board/review", { method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ identity: task.identity, ruler: task.ruler, mode: task.mode, engine: task.engine,
      observed_score: task.observed_score, baseline_score: task.baseline_score, metric: task.metric, request: task.request }), signal: timeoutSignal(15_000) });
  const accepted = await response.json().catch(() => null);
  if (!boardReviewTrackers.has(task.id)) return;
  if (response.ok && accepted?.ok && accepted.skipped) {
    boardReviewTrackers.delete(task.id); await saveBoardReviewTrackers(); return;
  }
  if (!response.ok || !accepted?.ok) {
    if ([400, 404, 409].includes(response.status)) boardReviewTrackers.set(task.id, { ...task, state: "failed", error: accepted?.error });
    return;
  }
  if (!/^[a-f0-9-]{36}$/.test(accepted.id || "")) return;
  const id = "board-review:" + accepted.id, existing = boardReviewTrackers.get(id);
  boardReviewTrackers.delete(task.id);
  boardReviewTrackers.set(id, { ...task, ...existing, id, review: accepted.id,
    sources: [...new Set([...(task.sources || [task.source]), ...(existing?.sources || [])])],
    state: "waiting", phase: "Waiting for community compute", publication: "waiting", publication_attempts: 0 });
}

async function saveBoardReviewTrackers() {
  const queue = await initUserTasks();
  while (boardReviewTrackers.size > 50) {
    const discarded = [...boardReviewTrackers.values()].find(task => !boardReviewActive(task)) || boardReviewTrackers.values().next().value;
    boardReviewTrackers.delete(discarded.id);
  }
  const list = [...boardReviewTrackers.values()].slice(-50).map(({ id, review, identity, weapon, ruler, mode, engine, observed_score, baseline_score, metric, request, source, sources, state, publication, publication_attempts }) => ({ id, review, identity, weapon, ruler, mode, engine, observed_score, baseline_score, metric, request, source, sources, state, publication, publication_attempts }));
  await queue.store.value("board-review-trackers", list);
}
function scheduleBoardReviews(ms = 10_000) {
  if (boardReviewTimer) clearTimeout(boardReviewTimer);
  boardReviewTimer = setTimeout(() => { boardReviewTimer = null; pollBoardReviews().catch(() => scheduleBoardReviews()); }, ms);
}
async function pollBoardReviews() {
  if (boardReviewPolling) return;
  const pending = [...boardReviewTrackers.values()].filter(task => task.state === "submitting");
  const tracks = [...boardReviewTrackers.values()].filter(task => !!task.review && boardReviewActive(task));
  if (!tracks.length && !pending.length) return;
  boardReviewPolling = true;
  try {
    if (pending[0]) {
      try { await acceptBoardReview(pending[0]); } catch (_) { /* the persisted request retries */ }
      await saveBoardReviewTrackers(); renderBoardReviewNote(); scheduleUserTasks();
    }
    if (!tracks.length) return;
    const response = await fetch("/api/board/review/status", { method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ ids: tracks.map(task => task.review) }), signal: timeoutSignal(15_000) });
    const payload = await response.json();
    if (!response.ok || !payload.ok) throw new Error("Board review is unavailable.");
    for (const task of tracks) {
      const current = boardReviewTrackers.get(task.id);
      if (!current) continue;
      const row = payload.reviews?.find(row => row.id === task.review);
      const next = row ? { ...current, ...row, id: task.id, review: task.review } : { ...current, state: "unavailable" };
      if (boardReviewNeedsPublication(next)) {
        next.publication_attempts = (next.publication_attempts || 0) + 1;
        let published = false;
        try { published = await refreshReviewedBoard(next); } catch (_) { /* publication retries independently */ }
        next.publication = published ? "published" : next.publication_attempts >= BOARD_PUBLICATION_MAX_ATTEMPTS ? "unavailable" : "waiting";
        next.phase = published ? "Board calibration verified" : "Verified scores appear on the board after publication.";
      }
      if (boardReviewTrackers.get(task.id) === current) boardReviewTrackers.set(task.id, next);
    }
    await saveBoardReviewTrackers(); renderBoardReviewNote(); scheduleUserTasks();
  } finally {
    boardReviewPolling = false;
    if ([...boardReviewTrackers.values()].some(boardReviewActive)) scheduleBoardReviews();
  }
}
async function refreshReviewedBoard(task) {
  const { rows, indexRows } = await reloadReviewedBoard(task.weapon, task.ruler);
  if (presetWeapon() === task.weapon) {
    renderPresetBar();
    if (!$("bench-page").hidden) renderBenchBoard();
    renderBuildFinder();
  }
  return boardReviewPublication(task, rows, indexRows);
}
function renderBoardReviewNote() {
  const box = $("sim-board-review");
  if (!box) return;
  const record = simulationReport || (resultViewing && results.find(row => row.id === resultViewing))
    || resultLatest(presetWeapon(), activePreset) || simulationResultForPage();
  const task = [...boardReviewTrackers.values()].find(task => (task.sources || [task.source]).includes(record?.id));
  if (!task) { box.innerHTML = ""; return; }
  const text = task.state === "submitting" ? tr("Requesting board calibration") : task.state === "succeeded" ? tr("Board calibration verified") : task.state === "superseded" ? tr("The board was updated while this review ran.")
    : boardReviewEnded(task.state) ? tr("Board review is unavailable.") : tr("Board calibration queued");
  box.innerHTML = `<div class="board-outcome"><b>${escHtml(text)}</b>${task.score == null ? "" : ` · ${escHtml(tr("Verified score"))}: ${escHtml(fmtScore(task.score))}`}
    <p>${escHtml(tr(task.publication === "unavailable" ? "Board publication is not confirmed. Open the board to retry."
      : ["succeeded", "superseded"].includes(task.state) ? "Verified scores appear on the board after publication." : "The published score stays until independent verification finishes."))}</p>
    <a class="ghost-btn small" data-review-board="${escHtml(task.id)}" href="${boardReviewLink(task)}">${escHtml(tr("View board"))}</a></div>`;
}
document.addEventListener("click", event => {
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
  const link = event.target.closest("[data-review-board]"), task = link && boardReviewTrackers.get(link.dataset.reviewBoard);
  if (!task) return;
  event.preventDefault(); event.stopImmediatePropagation();
  boardReviewTrackers.set(task.id, { ...task, state: "waiting", publication: "waiting", publication_attempts: 0 });
  saveBoardReviewTrackers().catch(() => {}); scheduleBoardReviews(0);
  nav(boardReviewLink(task));
}, true);
window.addEventListener("wfsim:ready", async () => {
  const queue = await initUserTasks();
  for (const task of await queue.store.value("board-review-trackers") || []) boardReviewTrackers.set(task.id, { ...task, state: task.review ? "waiting" : task.state === "failed" ? "failed" : "submitting" });
  scheduleBoardReviews(0);
}, { once: true });
