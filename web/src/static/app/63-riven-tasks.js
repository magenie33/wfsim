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
    return { title: `${task.origin.riven_name || tr("Riven")} · ${tr("Riven appraisal")}`,
      note: [weaponInfo(task.request.weapon)?.name, benchmarkName(task.origin.ruler), tr(USER_TASK_EXECUTORS.get(task.destination)?.label || "This device")].filter(Boolean).join(" · ") };
  },
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
async function submitSavedAppraisals(ids, ruler, destination = "local") {
  const weapon = presetWeapon(), saved = taskClone(loadPresetList(RIVENS).filter((card) => ids.includes(card.id)));
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
      riven_id: card.id, riven_name: card.name, board: asked.context, reference_rows: taskClone(BOARD[weapon] || []),
      batch, batch_name: "Riven appraisal", appraisal: null } });
  }
  return queue.createMany(entries);
}
function renderSavedAppraisal(record) {
  const verdict = record.result.verdict, box = $("opt-results");
  const summary = document.createElement("div"); summary.className = "appraisal";
  summary.innerHTML = `<b>${escHtml(record.origin.riven_name || tr("Riven appraisal"))}</b> · ${escHtml(tr("Riven grade"))}: ${verdict.grade}/100`
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
  box.innerHTML = benchSelectorHtml(appraisalRuler)
    + `<button type="button" class="ghost-btn" id="appraise-selected"${!appraisalSelection.size ? " disabled" : ""}>${escHtml(tr("Appraise selected"))} (${appraisalSelection.size})</button> `
    + `<button type="button" class="ghost-btn" id="appraise-all"${!cards.length ? " disabled" : ""}>${escHtml(tr("Appraise all"))} (${cards.length})</button><span id="riven-appraisal-note" role="status"></span>`;
  wireBenchSelector(box, (id) => { appraisalRuler = id; renderRivenAppraisalControls(); });
  const submit = async (ids) => {
    $("appraise-selected").disabled = true; $("appraise-all").disabled = true;
    try {
      await submitSavedAppraisals(ids, appraisalRuler);
      userTaskPanelOpen = true; renderUserTasks();
      $("riven-appraisal-note").textContent = tr("Added to queue");
    } catch (error) { $("riven-appraisal-note").textContent = tr(error.message); }
    finally { $("appraise-selected").disabled = !appraisalSelection.size; $("appraise-all").disabled = !cards.length; }
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
