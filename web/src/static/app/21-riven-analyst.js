// Personal analysis uses immutable optimization artifacts; published comparisons read the board.

/// The ruler the reader picked, by id. Null = the first one with a riven row.
let raRuler = null;
/// The riven row the reader has open, by its board key.
let raPick = null;

const raStat = (id) => {
  const s = rivenStat(id);
  return s ? rivenStatName(s) : id;
};
const raShown = (r) => String(r.shown != null ? r.shown : (r.score || 0).toFixed(4));
/// A board riven's stats WITH the roll each was stored at — the shape alone is
/// not the card the score was measured on.
const raRolls = (rv) => {
  const rows = rv.bonuses.map((id, i) => ({ id, slot: String(i), roll: rv.rolls?.[i] }))
    .concat(rv.malus ? [{ id: rv.malus, slot: "malus", roll: rv.rolls?.[rv.bonuses.length] }] : []);
  return rivenDisplayOrder(rivenPoolAll(), rows).map((s) => `${s.slot === "malus" ? "−" : "+"}${raStat(s.id)}`
    + (s.roll == null ? "" : ` ×${Number(Number(s.roll).toFixed(3))}`)).join(" · ");
};

/// A BOARD ROW AS THE SIMULATOR DRAWS A BUILD, and the way into the Builder.
function raBuild(w, x, head) {
  const rec = measuredRecordHtml(x.row);
  return `<div class="ra-build"><div class="sb-h">${escHtml(head)} · <b>${escHtml(raShown(x.row))}</b>${
    rec ? ` <span class="fd-when" title="${escHtml(measuredNote())}">${rec}</span>` : ""}</div>`
    + cardOfState(boardRowState(w, x.row), w)
    + `<a class="ghost-btn small sb-edit" href="${weaponPath(w.id)}/builder" data-ra-open="${escHtml(x.key)}">${
      escHtml(tr("open in Builder"))}</a></div>`;
}

/// THE ROW OPENED THE WAY THE BUILD BAR OPENS IT — its ruler, then the row,
/// through the door — and then the Builder, where an official build is read-only.
async function raOpen(w, x) {
  await agentDo("shell.preset.open", { bar: "scenario", preset: x.row.benchmark });
  await agentDo("shell.preset.open", { bar: "build", preset: x.key });
  nav(weaponPath(w.id) + "/builder");
}

function renderPublishedRivens() {
  const box = $("riven-published");
  const w = weaponInfo($("weapon").value);
  if (!box || !w || !META) return;
  // AS DEEP AS THE BUILD BAR READS, so every row drawn here is one it can open.
  const deep = boardGroupLeaders(BOARD[w.id]);
  const groups = rivenGroups(META, w, (BOARD[w.id] || []).filter((r) => deep(r, boardDepth)));
  if (!benchList().some((b) => b.id === raRuler)) raRuler = benchList()[0]?.id;
  const ruler = raRuler;
  const selector = benchSelectorHtml(ruler);
  const wire = () => wireBenchSelector(box, (id) => { raRuler = id; raPick = null; renderPublishedRivens(); });
  if (!groups.some((g) => g.ruler_id === ruler)) {
    box.innerHTML = selector + `<p class="wb-empty">${escHtml(tr("No riven build of this weapon has been measured yet."))}</p>`;
    wire(); return;
  }
  const mine = groups.filter((g) => g.ruler_id === ruler);
  const all = mine.flatMap((g) => g.rivens);
  const pick = all.find((x) => x.key === raPick) || all[0];
  const group = mine.find((g) => g.rivens.includes(pick));
  const tables = mine.map((g) => {
    const rows = g.rivens.map((x) => `<tr class="wb-row${x === pick ? " sel" : ""}" data-ra="${escHtml(x.key)}">`
      + `<td>${escHtml(raRolls(x.row.riven))}</td><td class="wb-score">${escHtml(raShown(x.row))}</td>`
      + `<td class="wb-score">${x.gain == null ? "—" : escHtml(gainPct(x.gain))}</td>`
      + `<td class="wb-when">${measuredRecordHtml(x.row, false)}</td></tr>`).join("");
    return `<p class="wb-ceiling">${escHtml(modeLabel(w, g.mode))} · ${escHtml(tr("The board's best riven-free build"))}: <b>${
      g.top ? escHtml(raShown(g.top.row)) : "—"}</b></p>`
      + `<table class="wb-tab"><thead><tr><th>${escHtml(tr("Riven"))}</th><th>${escHtml(tr("Score"))}</th>`
      + `<th>${escHtml(tr("vs the best riven-free build"))}</th><th>${escHtml(tr("Record"))}</th></tr></thead><tbody>${rows}</tbody></table>`;
  }).join("");
  const builds = [pick && raBuild(w, pick, tr("With this riven")),
    group && group.top && raBuild(w, group.top, tr("The board's best riven-free build"))].filter(Boolean).join("");
  box.innerHTML = selector
    + `<p class="wb-terms">${escHtml(benchmarkName(ruler))}</p>` + tables
    + `<div class="ra-builds">${builds}</div>`;
  const byKey = new Map(mine.flatMap((g) => (g.top ? [g.top] : []).concat(g.rivens)).map((x) => [x.key, x]));
  wire();
  box.querySelectorAll("[data-ra]").forEach((el) => el.addEventListener("click", () => {
    raPick = el.dataset.ra;
    renderPublishedRivens();
  }));
  box.querySelectorAll("[data-ra-open]").forEach((el) => el.addEventListener("click", (e) => {
    e.preventDefault();
    const x = byKey.get(el.dataset.raOpen);
    if (x) raOpen(w, x);
  }));
}

// Personal reports read the task artifact, including its submitted card and board.
let pendingRivenAnalysis = null, rivenAnalysisDialog = null;
let rivenAnalysisGeneration = 0, rivenAnalysisSignature = "";
const rivenAnalysisPath = (weapon, id) => `${weaponPath(weapon)}/riven-analyst${id ? "?analysis-result=" + encodeURIComponent(id) : ""}`;
function openRivenAnalysisLink(event, anchor) {
  event.stopPropagation();
  if (event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
  event.preventDefault(); closePopovers(); nav(anchor.pathname + anchor.search);
}
function rivenAnalysisLink(id) {
  if (!isRivenId(id) || isReadOnlyRivenId(id)) return "";
  return `<a class="rv-analyze ghost-btn small" onclick="openRivenAnalysisLink(event, this)" href="${weaponPath(presetWeapon())}/riven-analyst?analyze=${encodeURIComponent(id.slice(RIVEN_PREFIX.length))}" title="${escHtml(tr("Analyze this riven"))}">${escHtml(tr("Analyze"))}</a>`;
}
function renderRivenAnalyst() {
  const box = $("riven-analyst"), weapon = presetWeapon();
  if (!box || !weapon || !META) return;
  if (box.dataset.weapon !== weapon || box.dataset.lang !== LANG || !$("riven-analysis-report")) {
    box.dataset.weapon = weapon; box.dataset.lang = LANG;
    box.innerHTML = `<section class="analysis-panel"><div class="analysis-heading"><div><h3>${escHtml(tr("My analyses"))}</h3><p class="sim-hint">${escHtml(tr("Your card's actual rolls, the fx search and the official fight."))}</p></div>`
      + `<div class="analysis-actions"><a class="ghost-btn" href="${weaponPath(weapon)}/rivens">${escHtml(tr("Manage rivens"))}</a><button class="cu-btn" id="riven-analysis-new">${escHtml(tr("Analyze a riven"))}</button></div></div>`
      + `<div id="riven-analysis-queue"></div><div id="riven-analysis-report"></div><div id="riven-analysis-history"></div></section>`
      + `<details class="analysis-panel" id="riven-published-panel"><summary>${escHtml(tr("Published rivens"))}</summary><div id="riven-published"></div></details>`;
    $("riven-analysis-new").onclick = () => openRivenAnalysisDialog();
  }
  renderPublishedRivens();
  refreshRivenAnalysis(true).catch(userTaskFailed);
  const query = new URLSearchParams(location.search), asked = query.get("analyze");
  const pending = pendingRivenAnalysis?.weapon === weapon ? pendingRivenAnalysis : null;
  pendingRivenAnalysis = null;
  if (asked || pending) {
    history.replaceState(history.state, "", rivenAnalysisPath(weapon));
    openRivenAnalysisDialog(pending?.ids || [asked], pending?.ruler);
  }
}
function closeRivenAnalysisDialog(restore = true) {
  const dialog = rivenAnalysisDialog;
  if (!dialog) return;
  closePopovers(); dialog.overlay.remove(); rivenAnalysisDialog = null;
  document.body.classList.remove("analysis-dialog-open");
  if (restore && dialog.focus?.isConnected) dialog.focus.focus({ preventScroll: true });
}
function openRivenAnalysisDialog(ids = [], ruler = appraisalRuler || benchList()[0]?.id) {
  closeRivenAnalysisDialog(false);
  const weapon = presetWeapon(), cards = taskClone(loadPresetList(RIVENS));
  const selected = new Set(ids.filter(id => cards.some(card => card.id === id)));
  const overlay = document.createElement("div"); overlay.className = "analysis-overlay";
  overlay.id = "riven-analysis-dialog";
  rivenAnalysisDialog = { overlay, focus: document.activeElement, weapon };
  overlay.innerHTML = `<div class="analysis-dialog" role="dialog" aria-modal="true" aria-labelledby="analysis-dialog-title"><div class="jump-head"><h3 id="analysis-dialog-title">${escHtml(tr("Analyze a riven"))}</h3><button class="ghost-btn" data-analysis-close aria-label="${escHtml(tr("Close"))}">×</button></div>`
    + `<div class="analysis-dialog-body"><p class="sim-hint">${escHtml(tr("Your card's actual rolls, the fx search and the official fight."))}</p><div id="analysis-cards"></div><div id="analysis-card-preview"></div><div id="analysis-board"></div>`
    + `<fieldset class="analysis-executors"><legend>${escHtml(tr("Compute with"))}</legend><label><input type="radio" name="analysis-executor" value="local" checked> ${escHtml(tr("This device"))}</label>`
    + `<label><input type="radio" name="analysis-executor" value="community"${communityTaskCapable({ kind: "appraisal" }) ? "" : " disabled"}> ${escHtml(tr("Community compute"))}</label></fieldset>`
    + `<p class="sim-hint" id="analysis-executor-note"></p><p class="sim-hint">${escHtml(tr("The card and board are fixed when you start. Editing your riven later leaves this report unchanged."))}</p>`
    + `<a class="ghost-btn" href="${weaponPath(weapon)}/rivens">${escHtml(tr("Import or edit in Rivens"))} →</a></div>`
    + `<div class="analysis-dialog-foot"><span id="analysis-submit-note" role="status"></span><button class="cu-btn" id="analysis-submit">${escHtml(tr("Start analysis"))}</button></div></div>`;
  document.body.append(overlay); document.body.classList.add("analysis-dialog-open");
  const submit = $("analysis-submit");
  const drawCards = () => {
    $("analysis-cards").innerHTML = cards.length ? cards.map(card => `<label class="analysis-card-choice"><input type="checkbox" data-analysis-card="${escHtml(card.id)}"${selected.has(card.id) ? " checked" : ""}><span><b>${escHtml(card.name)}</b><span>${escHtml(raRolls({ bonuses: (card.state.bonuses || []).map(stat => stat.id), malus: card.state.malus?.id, rolls: (card.state.bonuses || []).map(stat => stat.roll).concat(card.state.malus ? [card.state.malus.roll] : []) }))}</span></span></label>`).join("") : `<p class="wb-empty">${escHtml(tr("no rivens for this weapon yet"))}</p>`;
    submit.disabled = !selected.size;
    const preview = () => { $("analysis-card-preview").innerHTML = cards.filter(card => selected.has(card.id)).map(card => rivenSpecCardHtml(weaponInfo(weapon), { spec: card.state })).join(""); };
    preview();
    $("analysis-cards").querySelectorAll("[data-analysis-card]").forEach(input => input.onchange = () => {
      if (input.checked) selected.add(input.dataset.analysisCard); else selected.delete(input.dataset.analysisCard);
      submit.disabled = !selected.size; preview();
    });
  };
  const drawBoard = () => { $("analysis-board").innerHTML = benchSelectorHtml(ruler); wireBenchSelector($("analysis-board"), id => { ruler = id; drawBoard(); }); };
  const executorNote = () => { $("analysis-executor-note").textContent = tr(overlay.querySelector('[name="analysis-executor"]:checked').value === "community"
    ? "Community compute receives these calculation inputs. The official replay runs on this device; keep WFSim open."
    : "The search and official replay run on this device. You can keep using WFSim while they run."); };
  overlay.querySelectorAll('[name="analysis-executor"]').forEach(input => input.onchange = executorNote);
  rivenAnalysisDialog.refreshExecutor = () => {
    const community = overlay.querySelector('[name="analysis-executor"][value="community"]');
    community.disabled = !communityTaskCapable({ kind: "appraisal" });
    community.title = community.disabled ? tr("Community execution is unavailable.") : "";
    if (community.disabled && community.checked) overlay.querySelector('[name="analysis-executor"][value="local"]').checked = true;
    executorNote();
  };
  drawCards(); drawBoard(); rivenAnalysisDialog.refreshExecutor();
  overlay.querySelector("[data-analysis-close]").onclick = () => closeRivenAnalysisDialog();
  overlay.onclick = event => { if (event.target === overlay) closeRivenAnalysisDialog(); };
  overlay.addEventListener("keydown", event => {
    if (imeComposing(event)) return;
    if (event.key === "Escape" && !document.querySelector('.popover:not([hidden])')) { event.preventDefault(); event.stopPropagation(); closeRivenAnalysisDialog(); }
    if (event.key !== "Tab") return;
    const nodes = [...overlay.querySelectorAll('button,input,a[href],[tabindex="0"]')].filter(node => !node.disabled && node.offsetParent !== null);
    if (event.shiftKey ? document.activeElement === nodes[0] : document.activeElement === nodes.at(-1)) {
      event.preventDefault(); (event.shiftKey ? nodes.at(-1) : nodes[0])?.focus();
    }
  }, true);
  submit.onclick = async () => {
    if (presetWeapon() !== weapon) return closeRivenAnalysisDialog(false);
    submit.disabled = true;
    try {
      const destination = overlay.querySelector('[name="analysis-executor"]:checked').value;
      await submitSavedAppraisals([...selected], ruler, destination, { weapon, cards });
      appraisalRuler = ruler;
      if (rivenAnalysisDialog?.overlay === overlay) closeRivenAnalysisDialog();
      await refreshRivenAnalysis(true);
    } catch (error) {
      if (rivenAnalysisDialog?.overlay === overlay) { $("analysis-submit-note").textContent = tr(error.message); submit.disabled = !selected.size; }
    }
  };
  overlay.querySelector("[data-analysis-close]").focus({ preventScroll: true });
}
function analysisFrozenState(record) {
  const verdict = record.result.verdict;
  const refs = [...new Set((verdict.state.slots || []).map(slot => slot.mod).filter(isRivenId))];
  return frozenTaskBuild(verdict.state, { weapon: record.request.weapon,
    rivens: refs.map(id => ({ id: id.slice(RIVEN_PREFIX.length), spec: verdict.riven })) });
}
const analysisOptimizerLink = record => `${weaponPath(record.request.weapon)}/optimizer?task-result=${encodeURIComponent(record.id)}`;
async function analysisArtifacts(weapon) {
  const queue = await initUserTasks();
  return (await queue.store.entries("artifact:")).filter(record => record.kind === "appraisal" && record.request.weapon === weapon && record.result?.verdict)
    .sort((a, b) => b.at.localeCompare(a.at));
}
async function refreshRivenAnalysis(redraw = false) {
  rivenAnalysisDialog?.refreshExecutor?.();
  if (!META || !document.body.classList.contains("on-ranalyst") || !$("riven-analysis-history")) return;
  const weapon = presetWeapon(), queue = await initUserTasks();
  if (weapon !== presetWeapon() || !document.body.classList.contains("on-ranalyst")) return;
  const tasks = userTaskItems().filter(task => task.kind === "appraisal" && task.request.weapon === weapon);
  const pending = tasks.filter(task => task.state !== "succeeded");
  const waiting = tasks.filter(task => task.state === "queued");
  const focused = document.activeElement?.dataset;
  $("riven-analysis-queue").innerHTML = pending.map(task => userTaskRow(task, waiting)).join("");
  $("riven-analysis-queue").onclick = event => {
    const action = event.target.closest("[data-user-task-action]");
    if (action) userTaskAction(action.dataset.userTaskAction, action.dataset.userTask).then(() => refreshRivenAnalysis(true)).catch(userTaskFailed);
  };
  if (focused?.userTask) [...$("riven-analysis-queue").querySelectorAll("[data-user-task]")].find(node => node.dataset.userTask === focused.userTask && node.dataset.userTaskAction === focused.userTaskAction)?.focus({ preventScroll: true });
  const chosen = new URLSearchParams(location.search).get("analysis-result");
  const signature = JSON.stringify([weapon, chosen, tasks.map(task => [task.id, task.state, task.result_ref]), LANG]);
  if (!redraw && signature === rivenAnalysisSignature) return;
  rivenAnalysisSignature = signature;
  const generation = ++rivenAnalysisGeneration;
  const records = await analysisArtifacts(weapon);
  if (generation !== rivenAnalysisGeneration || weapon !== presetWeapon() || !document.body.classList.contains("on-ranalyst")) return;
  const selected = chosen ? records.find(record => record.id === chosen) || await queue.store.value(`artifact:${chosen}`) : records[0];
  if (generation !== rivenAnalysisGeneration || weapon !== presetWeapon() || !document.body.classList.contains("on-ranalyst")) return;
  const box = $("riven-analysis-history");
  box.innerHTML = `<h3>${escHtml(tr("Analysis history"))}</h3><p class="sim-hint">${escHtml(tr("Saved in this browser"))}</p>` + (records.length ? `<div class="analysis-table-wrap"><table class="wb-tab"><thead><tr>${["Riven", "Board", "Riven grade", "vs the best riven-free build", "Record"].map(label => `<th>${escHtml(tr(label))}</th>`).join("")}</tr></thead><tbody>${records.map(record => `<tr><td><a href="${rivenAnalysisPath(weapon, record.id)}">${escHtml(record.origin.riven_name || tr("Riven"))}</a></td><td>${escHtml(benchmarkName(record.origin.ruler))}</td><td>${record.result.verdict.grade}/100</td><td>${record.result.verdict.gain == null ? "—" : escHtml(gainPct(record.result.verdict.gain))}</td><td>${escHtml(new Date(record.at).toLocaleString())}</td></tr>`).join("")}</tbody></table></div>` : `<p class="wb-empty">${escHtml(tr("No analyses yet. Choose a saved riven to start."))}</p>`);
  if (selected?.kind === "appraisal" && selected.request.weapon === weapon && selected.result?.verdict) renderRivenAnalysisReport(selected);
  else $("riven-analysis-report").innerHTML = chosen ? `<p class="wb-empty">${escHtml(tr("This result is no longer available."))}</p>` : "";
}
function renderRivenAnalysisReport(record) {
  const box = $("riven-analysis-report"), v = record.result.verdict, w = weaponInfo(record.request.weapon);
  const state = analysisFrozenState(record);
  const notes = headlessRivenGradeNotes(pageSay(), v);
  box.innerHTML = `<article class="analysis-report"><div class="analysis-heading"><div><h3>${escHtml(record.origin.riven_name || tr("Riven"))}</h3><p class="sim-hint">${escHtml(benchmarkName(record.origin.ruler))}</p></div><div class="analysis-grade"><b>${v.grade}<small>/100</small></b><span>${escHtml(tr("Riven grade"))}</span></div></div>`
    + rivenSpecCardHtml(w, { spec: v.riven })
    + `<div class="analysis-metrics"><div><span>${escHtml(tr("Score"))} · ${escHtml(tr((META.metrics || []).find(metric => metric.id === v.metric)?.label || v.metric))}</span><b>${escHtml(v.shown)}</b></div><div><span>${escHtml(tr("vs the best riven-free build"))}</span><b>${v.gain == null ? "—" : escHtml(gainPct(v.gain))}</b></div><div><span>${escHtml(tr("The board's best riven-free build"))}</span><b>${v.top == null ? "—" : escHtml(v.top)}</b></div></div>`
    + `<p class="sim-hint">${escHtml(new Date(v.at).toLocaleString())} · ${escHtml(tr("Engine"))}: ${escHtml(record.engine)}${record.engine !== ENGINE_ID ? ` · ${escHtml(tr("Measured with a different engine version."))}` : ""}</p>`
    + `<details class="analysis-standard"><summary>${escHtml(tr("Scoring standard (fx)"))}</summary>${notes.map(note => `<p>${escHtml(note)}</p>`).join("")}</details>`
    + `<div class="analysis-actions"><button class="ghost-btn" data-analysis-builder>${escHtml(tr("open in Builder"))}</button><a class="ghost-btn" href="${analysisOptimizerLink(record)}">${escHtml(tr("Optimizer record"))}</a><button class="cu-btn" data-analysis-share>${escHtml(tr("Share analysis"))}</button></div>`
    + `<div class="analysis-share" hidden></div><details class="analysis-build"><summary>${escHtml(tr("With this riven"))}</summary>${cardOfState(state, w)}</details></article>`;
  box.querySelector("[data-analysis-builder]").onclick = async event => {
    event.currentTarget.disabled = true;
    try { await openAnalysisBuild(record); }
    catch (error) { presetToast(tr(error.message)); }
    finally { box.querySelector("[data-analysis-builder]")?.removeAttribute("disabled"); }
  };
  box.querySelector("[data-analysis-share]").onclick = () => openAnalysisShare(record, state, box.querySelector(".analysis-share")).catch(userTaskFailed);
}
async function openAnalysisBuild(record) {
  if (presetWeapon() !== record.request.weapon) throw new Error("This result belongs to another weapon.");
  const path = location.pathname;
  const url = new URL(await shareUrl(null, null, { state: analysisFrozenState(record) }));
  if (location.pathname === path && presetWeapon() === record.request.weapon) nav(url.pathname + "/builder" + url.search);
}
async function openAnalysisShare(record, state, host) {
  if (!host.hidden) { host.hidden = true; return; }
  host.hidden = false; host.textContent = tr("loading…");
  const url = await shareUrl(null, null, { state });
  if (!host.isConnected || presetWeapon() !== record.request.weapon) return;
  const v = record.result.verdict;
  const text = [`${weaponInfo(record.request.weapon)?.name} · ${tr("Riven Analyst")} — WFSim`, benchmarkName(record.origin.ruler),
    `${tr("Riven grade")}: ${v.grade}/100 · ${tr("Score")}: ${v.shown}`,
    `${tr("vs the best riven-free build")}: ${v.gain == null ? "—" : gainPct(v.gain)}`,
    ...await shareText({ state }), ...headlessRivenGradeNotes(pageSay(), v), `${tr("Engine")}: ${record.engine}`, url].join("\n");
  host.innerHTML = `<p class="sim-hint">${escHtml(tr("Share the analysis text with its build link. The link opens the build; the dated score stays in the text."))}</p><textarea rows="7" readonly aria-label="${escHtml(tr("Share analysis"))}"></textarea><button class="ghost-btn" data-analysis-copy>${escHtml(tr("copy as text"))}</button><span role="status"></span>`;
  host.querySelector("textarea").value = text;
  host.querySelector("[data-analysis-copy]").onclick = async () => {
    try { await navigator.clipboard.writeText(text); host.querySelector('[role="status"]').textContent = tr("text copied"); }
    catch (_) { host.querySelector("textarea").select(); host.querySelector('[role="status"]').textContent = tr("press Ctrl+C to copy the selected link"); }
  };
}
