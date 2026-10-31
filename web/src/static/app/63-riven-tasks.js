// Saved cards use the same search, replay and grade as the chat appraisal.
const appraisalSelection = new Set();
let appraisalRuler = null;
USER_TASK_TYPES.set("appraisal", {
  ...USER_TASK_TYPES.get("optimization"), icon: "◇", community: { kind: "optimization", finish: finishSavedAppraisal },
  validate(request) {
    USER_TASK_TYPES.get("optimization").validate(request);
    if (!request.rivens?.length) throw new Error("An appraisal needs a riven.");
  },
  describe(task) {
    return { title: `${task.origin.riven_name || tr("Riven")} · ${tr("Riven analysis")}`,
      note: [weaponInfo(task.request.weapon)?.name, benchmarkName(task.origin.ruler), tr(USER_TASK_EXECUTORS.get(task.destination)?.label || "This device")].filter(Boolean).join(" · ") };
  },
  resultLink(task) { return `${weaponPath(task.request.weapon)}/riven-analyst?analysis-result=${encodeURIComponent(task.result_ref)}`; },
  async execute(task, execution) {
    const checkpoint = await execution.checkpoint();
    const saved = checkpoint?.engine === task.engine && checkpoint.schema === task.request_schema ? checkpoint.value?.search : null;
    const search = saved || await executeOptimization(task, execution);
    await execution.checkpoint({ search });
    return finishSavedAppraisal(task, execution, search);
  },
});
async function finishSavedAppraisal(task, execution, search) {
  const replay = async () => {
    if (execution.signal.aborted) throw new Error("Cancelled");
    const release = holdForeground();
    try { return await replaySavedAppraisal(task, execution, search); } finally { release(); }
  };
  return task.destination === "community" ? userTaskLock("resource:local", replay) : replay();
}
async function replaySavedAppraisal(task, execution, search) {
  const best = (search.results || []).find((row) => row?.mods?.length);
  if (!best) throw new Error("the search ranked nothing");
  const job = { ruler: task.origin.ruler, riven: task.request.rivens[0].spec,
    result: { build: boardPayloadFromResult(best, task.origin.board) } };
  const verdict = await headlessAppraisalReplay(pageSay(), weaponInfo(task.request.weapon), job, task.origin.reference_rows, {
    check: (body) => api("/api/board/check", body),
    simulate: (body) => {
      if (execution.signal.aborted) throw new Error("Cancelled");
      return simulateFleet(body, (done, total) => execution.report({ phase: "Appraisal replay", done, total }), { signal: execution.signal });
    },
  });
  if (!verdict.ok) throw new Error(verdict.reason || "the replay did not run");
  return { search, verdict };
}
async function submitSavedAppraisals(ids, ruler, destination = "local", capture = null) {
  const weapon = capture?.weapon || presetWeapon();
  const saved = taskClone((capture?.cards || loadPresetList(RIVENS, weapon)).filter((card) => ids.includes(card.id)));
  if (!saved.length) throw new Error("Select at least one saved riven.");
  await loadBoard(weapon);
  const batch = crypto.randomUUID(), queue = await initUserTasks();
  const entries = [];
  for (const card of saved) {
    const resolved = await api("/api/riven", { weapon, ...card.state });
    if (resolved.illegal?.length || !card.state.bonuses?.every((stat) => stat.id) || card.state.bonuses.length < 2)
      throw new Error(`${card.name}: ${resolved.illegal?.join(", ") || tr("Finish this riven before appraising it.")}`);
    const asked = headlessRivenGainRequest(META, { weapon, ruler, card: card.state, rivenId: card.id });
    if (!asked) throw new Error("Choose a ruler.");
    entries.push({ kind: "appraisal", request: asked.request, destination, origin: { weapon, ruler,
      riven_id: card.id, riven_name: card.name, board: asked.context, reference_rows: taskClone((BOARD[weapon] || []).filter(row => row.benchmark === ruler)),
      preset_name: META.search_presets.presets.find(preset => preset.riven)?.name, scenario: ruler, batch, batch_name: "Riven analysis", appraisal: null } });
  }
  return queue.createMany(entries);
}
function renderSavedAppraisal(record) {
  const verdict = record.result.verdict, box = $("opt-results");
  const summary = document.createElement("div"); summary.className = "appraisal";
  summary.innerHTML = `<b>${escHtml(record.origin.riven_name || tr("Riven analysis"))}</b> · ${escHtml(tr("Riven grade"))}: ${verdict.grade}/100`
    + `<div>${escHtml(verdict.shown)} · ${escHtml(tr("vs the best riven-free build"))}: ${verdict.gain == null ? "—" : escHtml(gainPct(verdict.gain))}</div>`
    + `<div>${escHtml(tr("Graded {g}.").replace("{g}", new Date(verdict.at).toLocaleString()))} · ${escHtml(record.request.weapon)}</div>`;
  box.prepend(summary);
}
function renderRivenAppraisalControls() {
  let box = $("riven-appraisals");
  if (!box) return;
  const cards = loadPresetList(RIVENS), rulers = META.benchmarks || [];
  for (const id of appraisalSelection) if (!cards.some((card) => card.id === id)) appraisalSelection.delete(id);
  if (!rulers.some((ruler) => ruler.id === appraisalRuler)) appraisalRuler = rulers[0]?.id;
  box.innerHTML = `<button type="button" class="ghost-btn" id="appraise-selected"${!appraisalSelection.size ? " disabled" : ""}>${escHtml(tr("Analyze selected"))} (${appraisalSelection.size})</button> `
    + `<button type="button" class="ghost-btn" id="appraise-all"${!cards.length ? " disabled" : ""}>${escHtml(tr("Analyze all"))} (${cards.length})</button><span id="riven-appraisal-note" role="status"></span>`;
  const submit = (ids) => {
    pendingRivenAnalysis = { weapon: presetWeapon(), ids: [...ids], ruler: appraisalRuler };
    nav(weaponPath(presetWeapon()) + "/riven-analyst");
  };
  $("appraise-selected").onclick = () => submit([...appraisalSelection]);
  $("appraise-all").onclick = () => submit(cards.map((card) => card.id));
}
async function importTypedRivens(text) {
  const weapon = presetWeapon(), pool = rivenPool(), locales = [I18N].concat(ALT_NAMES || []).filter(Boolean);
  const blocks = text.trim().split(/\n\s*\n/).filter(Boolean);
  if (!blocks.length || blocks.length > 50) throw new Error("Enter 1–50 cards, separated by blank lines.");
  const specs = [];
  for (const block of blocks) {
    const read = readRivenLines(block, pool, locales);
    if (read.error) throw new Error(`${tr("Could not read this riven")}: ${read.word || read.error}`);
    const bonuses = read.lines.filter((line) => !line.malus), malus = read.lines.filter((line) => line.malus);
    if (bonuses.length < 2 || bonuses.length > 3 || malus.length > 1 || new Set(read.lines.map((line) => line.id)).size !== read.lines.length)
      throw new Error("A riven needs 2–3 different bonuses and at most one malus.");
    const field = (line) => ({ id: line.id, roll: 1, value: line.value });
    const response = await api("/api/riven", { weapon, bonuses: bonuses.map(field), malus: malus.length ? field(malus[0]) : null,
      rank: rivenRules().max_rank, polarity: "madurai" });
    if (response.illegal?.length) throw new Error(response.illegal.join(", "));
    const roll = (slot) => response.stats?.find((stat) => stat.slot === slot)?.roll ?? 1;
    specs.push({ bonuses: bonuses.map((line, index) => ({ id: line.id, roll: roll(String(index)) })),
      malus: malus.length ? { id: malus[0].id, roll: roll("malus") } : null, rank: rivenRules().max_rank, polarity: "madurai" });
  }
  if (presetWeapon() !== weapon) throw new Error("The weapon changed. Import these cards on their weapon page.");
  for (const spec of specs) appraisalSelection.add(newRiven(spec));
  renderRivens();
  return specs.length;
}
function wireTypedRivens() {
  const button = $("riven-text-import");
  if (!button || button.onclick) return;
  button.onclick = async () => {
    button.disabled = true;
    try { const count = await importTypedRivens($("riven-text").value); $("riven-text-note").textContent = tr("{n} cards imported. Review them in the editor before appraising.").replace("{n}", count); }
    catch (error) { $("riven-text-note").textContent = tr(error.message); }
    finally { button.disabled = false; }
  };
}

let optimizerHistorySignature = "", optimizerHistoryGeneration = 0;
async function refreshOptimizerHistory(force = false) {
  if (!META || !document.body.classList.contains("on-optimizer") || !$("optimizer-history-rows")) return;
  const weapon = presetWeapon(), queue = await initUserTasks();
  const signature = JSON.stringify([weapon, LANG, queue.view().filter(task => ["optimization", "appraisal"].includes(task.kind) && task.request.weapon === weapon).map(task => [task.id, task.state, task.result_ref])]);
  if (!force && signature === optimizerHistorySignature) return;
  optimizerHistorySignature = signature;
  const generation = ++optimizerHistoryGeneration;
  const records = (await queue.store.entries("artifact:")).filter(record => ["optimization", "appraisal"].includes(record.kind) && record.request.weapon === weapon).sort((a, b) => b.at.localeCompare(a.at));
  if (generation !== optimizerHistoryGeneration || presetWeapon() !== weapon || !document.body.classList.contains("on-optimizer")) return;
  $("optimizer-history-rows").innerHTML = records.length ? records.map(record => `<div class="analysis-history-item"><div><b>${escHtml(record.origin.riven_name || record.origin.preset_name || tr("Optimization"))}</b><p class="sim-hint">${escHtml(benchmarkName(record.origin.ruler || record.origin.scenario) || record.origin.scenario_name || "")} · ${escHtml(new Date(record.at).toLocaleString())}</p></div><div class="analysis-actions">${record.kind === "appraisal" ? `<a class="ghost-btn" href="${rivenAnalysisPath(weapon, record.id)}">${escHtml(tr("Riven grade"))}: ${record.result.verdict.grade}/100</a>` : ""}<a class="ghost-btn" href="${analysisOptimizerLink(record)}">${escHtml(tr("View result"))}</a></div></div>`).join("") : `<p class="wb-empty">${escHtml(tr("No saved optimization results yet."))}</p>`;
}
