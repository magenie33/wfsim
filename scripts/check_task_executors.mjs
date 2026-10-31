import { openApp } from "./cdp.mjs";
const app = await openApp({ boot: 13000, lang: "en" });
try {
  await app.load("/weapons/Torid/optimizer", 7000);
  const result = await app.evaluate(`(async () => {
    const reportTab = window.open('about:blank', 'search-result-report');
    const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
    const until = async (condition, limit = 400) => { for (let i = 0; i < limit; i++) { if (await condition()) return; await pause(100); } throw new Error('executor check timed out'); };
    await initUserTasks(); gainPrefs.on = false; restartCalc();
    const out = {};
    const limits = normalizeLimits(null);
    limits.mods = 1; limits.exilus = false; limits.arcane_seats = limits.arcane_seats.map(() => false);
    limits.exclude.mods = buildPool().filter(mod => mod.id !== 'serration').map(mod => mod.id);
    applyOptState({ starts: [{ build: stateFromBuild({ mods: ['serration'] }, 'torid'), fixed: ['mods:0'] }], limits, candidate_runs: 1, finalists: 2 });
    sim.duration = 1; setSimRuns(1);
    const expected = taskClone(nameableOptimizeBody(optimizeBody()));
    const task = await runOptimize();
    history.pushState(null, '', '/weapons/Phenmor/rivens'); await route(); await pause(400);
    const editor = JSON.stringify(snapshotState());
    const completed = await userTasks.wait(task.id);
    const record = await userTasks.store.value('artifact:' + completed.result_ref);
    out.realSearch = completed.state === 'succeeded' && record.result.results.length > 0;
    out.fixedSearch = JSON.stringify(record.request) === JSON.stringify(expected) && record.origin.weapon === 'torid';
    out.editor = presetWeapon() === 'phenmor' && JSON.stringify(snapshotState()) === editor;
    out.reportLink = USER_TASK_TYPES.get('optimization').resultLink(completed).includes('task-result=');
    if (!reportTab) throw new Error('Could not open result report');
    reportTab.location.href = USER_TASK_TYPES.get('optimization').resultLink(completed);
    await until(() => reportTab.__wfsimReady && reportTab.eval('!!optimizationReport && !!optLast'));
    reportTab.eval("boardRowsLanded('torid')");
    out.openReport = !!reportTab.eval('optLast') && reportTab.location.search.includes('task-result=') && reportTab.eval('optimizationReport.id') === record.id && presetWeapon() === 'phenmor';
    reportTab.close();
    out.artifact = record.id === completed.attempt && record.result.results.every(row => !!row.replay);
    const imported = await importTypedRivens('+150% Critical Chance +90% Multishot -40% Zoom');
    out.typed = imported === 1 && loadPresetList(RIVENS).some(card => card.state.bonuses.length === 2 && card.state.malus.id === 'zoom');
    const spec = { ...loadPresetList(RIVENS).find(card => card.id === [...appraisalSelection][0]).state, rank: 3, polarity: 'vazarin' };
    const frozen = frozenTaskBuild({ slots: [{ mod: 'riven:captured' }] }, { weapon: presetWeapon(), rivens: [{ id: 'captured', spec }] });
    const resolved = readOnlyRiven(frozen.slots[0].mod);
    out.frozenRiven = resolved.state.rank === 3 && resolved.state.polarity === 'vazarin';
    renderRivenAll(); out.batchControls = !!document.querySelector('[data-appraise-riven]') && !!document.getElementById('appraise-all');
    const id = [...appraisalSelection][0], ruler = META.benchmarks[0].id;
    const expectedAppraisal = headlessRivenGainRequest(META, { weapon: presetWeapon(), ruler, card: loadPresetList(RIVENS).find(card => card.id === id).state, rivenId: id });
    document.getElementById('appraise-selected').click();
    await until(() => !!rivenAnalysisDialog);
    out.analysisEntry = location.pathname.endsWith('/riven-analyst') && !!document.querySelector('#analysis-card-preview [data-riven-card]');
    out.analysisDefault = document.querySelector('#analysis-board [data-board-selected]').dataset.boardSelected === ruler
      && document.querySelector('[name="analysis-executor"]:checked').value === 'local';
    const capability = communityTaskCapabilities;
    communityTaskCapabilities = null; await refreshRivenAnalysis();
    const community = document.querySelector('[name="analysis-executor"][value="community"]');
    out.executorAvailability = community.disabled;
    communityTaskCapabilities = { kinds: ['optimization'] }; await refreshRivenAnalysis();
    out.executorAvailability = out.executorAvailability && !community.disabled;
    communityTaskCapabilities = capability; await refreshRivenAnalysis();
    const prior = new Set(userTasks.list().map(task => task.id));
    const displayedCards = taskClone(loadPresetList(RIVENS));
    const editedCards = taskClone(displayedCards); editedCards.find(card => card.id === id).state.bonuses[0].roll = 0.9;
    storePresetList(RIVENS, editedCards);
    document.getElementById('analysis-submit').click();
    await until(() => userTasks.list().some(task => task.kind === 'appraisal' && !prior.has(task.id)));
    const tasks = userTasks.list().filter(task => task.kind === 'appraisal' && !prior.has(task.id));
    storePresetList(RIVENS, displayedCards);
    out.fxRequest = JSON.stringify(tasks[0].request) === JSON.stringify(expectedAppraisal.request)
      && JSON.stringify(tasks[0].origin.board) === JSON.stringify(expectedAppraisal.context);
    out.analysisLink = USER_TASK_TYPES.get('appraisal').resultLink({ ...tasks[0], result_ref: 'capture' }).includes('/riven-analyst?analysis-result=');
    out.appraisalCapture = tasks.length === 1 && tasks[0].kind === 'appraisal' && !!tasks[0].origin.batch && tasks[0].origin.ruler === ruler;
    await userTasks.cancel(tasks[0].id);
    // Short fixture terms exercise the whole search/replay/grade pipeline.
    const benchmark = META.benchmarks.find(item => item.id === ruler), preset = META.search_presets.presets.find(item => item.riven);
    const oldTerms = taskClone(benchmark.scenario), oldPreset = taskClone(preset);
    benchmark.scenario.duration = 2; benchmark.scenario.runs = 2; preset.candidate_runs = 1; preset.finalists = 1;
    let graded;
    try {
      const batch = await submitSavedAppraisals([id], ruler);
      const finished = await userTasks.wait(batch[0].id);
      if (finished.state !== 'succeeded') throw new Error('Appraisal failed: ' + finished.error);
      graded = await userTasks.store.value('artifact:' + finished.result_ref);
    } finally { benchmark.scenario = oldTerms; Object.assign(preset, oldPreset); }
    out.grade = graded.result.verdict.ok && Number.isFinite(graded.result.verdict.grade) && graded.result.search.results.length > 0;
    out.reference = Array.isArray(graded.origin.reference_rows) && !!graded.result.verdict.at;
    history.pushState(null, '', rivenAnalysisPath(presetWeapon(), graded.id)); await route();
    await refreshRivenAnalysis(true);
    out.reportGrade = document.querySelector('.analysis-grade b').textContent === graded.result.verdict.grade + '/100'
      && document.querySelector('#riven-analysis-report').textContent.includes(graded.result.verdict.shown);
    out.reportStandard = !!document.querySelector('.analysis-standard') && !!document.querySelector('#riven-analysis-report [data-analysis-share]');
    const state = analysisFrozenState(graded), frozenSpec = readOnlyRiven(state.slots.find(slot => isReadOnlyRivenId(slot.mod)).mod).state;
    const cards = loadPresetList(RIVENS); cards.find(card => card.id === id).state.bonuses[0].roll = 0.9;
    storePresetList(RIVENS, cards.filter(card => card.id !== id));
    await userTasks.remove(graded.task);
    await refreshRivenAnalysis(true);
    out.analysisHistory = document.querySelector('#riven-analysis-history').textContent.includes(graded.result.verdict.grade + '/100');
    const sharedCode = await decodeShare(await shareCode({ state }));
    out.reportFrozen = JSON.stringify(sharedCode.rivens[0].s.bonuses) === JSON.stringify(frozenSpec.bonuses)
      && document.querySelector('.analysis-grade b').textContent === graded.result.verdict.grade + '/100';
    await openAnalysisShare(graded, state, document.querySelector('.analysis-share'));
    out.analysisShare = document.querySelector('.analysis-share textarea').value.includes(graded.result.verdict.grade + '/100')
      && document.querySelector('.analysis-share textarea').value.includes(benchmarkName(ruler));
    history.pushState(null, '', analysisOptimizerLink(graded)); await route();
    await refreshOptimizerHistory(true);
    out.optimizerRecord = optimizationReport?.id === graded.id && document.querySelector('#optimizer-history-rows').textContent.includes(graded.result.verdict.grade + '/100');
    window.analysisCheckReport = rivenAnalysisPath(presetWeapon(), graded.id);
    const existingBuilds = new Set(loadPresetList(BUILDS).map(build => build.id));
    await openAnalysisBuild(graded);
    await until(() => location.pathname.endsWith('/builder') && !existingBuilds.has(activePreset) && slots.some(slot => isRivenId(slot.mod)));
    const openedRiven = rivenSpecFor(slots.find(slot => isRivenId(slot.mod)).mod).spec;
    out.analysisBuild = JSON.stringify(openedRiven.bonuses) === JSON.stringify(frozenSpec.bonuses);
    const old = quickTask.replace('old', { weapon: 'torid' }), next = quickTask.replace('new', { weapon: 'phenmor' });
    out.transient = !quickTask.report(old, 'succeeded', {}) && quickTask.report(next, 'running', { done: 1, total: 2 });
    gainScan.want = null; observeQuickTask(); out.reset = quickTask.current === null;
    return out;
  })()`, { userGesture: true });
  for (const [key, label] of Object.entries({ analysisEntry: "saved-card analysis opens the dedicated page and shared card preview", analysisDefault: "analysis defaults to the official ruler and local execution", executorAvailability: "the open dialog follows community availability after loading", fxRequest: "the accepted analysis is exactly the fx request and context", analysisLink: "the task result opens its analysis report", reportGrade: "the report displays its artifact's grade and score", reportStandard: "the report exposes the fx standard and sharing", analysisHistory: "deleting the card and task leaves immutable analysis history", reportFrozen: "the shared build keeps the submitted rolls after the owned card is deleted", analysisShare: "sharing includes the dated grade, ruler and frozen build", optimizerRecord: "the same artifact appears in optimizer history", analysisBuild: "opening the winner creates a build with the original submitted riven", realSearch: "the original optimizer executes through the task queue", fixedSearch: "optimization keeps its captured scope and owner", editor: "search completion leaves another weapon's editor unchanged", reportLink: "the search report has a durable result link", openReport: "the saved search report opens without changing the original editor", frozenRiven: "captured riven rolls and rank survive later edits to the owned card", artifact: "every ranked result retains its original replay request", typed: "typed cards use the shared reader and become editable saved rivens", batchControls: "saved rivens expose selection and batch appraisal", appraisalCapture: "appraisal captures the card, ruler and batch before execution", grade: "a real search is replayed and graded through the shared appraisal pipeline", reference: "the appraisal report retains its reference board and measurement time", transient: "a replaced automatic scan cannot report into its successor", reset: "resetting the calculator clears its transient running item" }))
    app.check(label, result[key], JSON.stringify(result));
} finally { await app.close(); }
process.exitCode = app.failures ? 1 : 0;
console.log(app.failures ? `${app.failures} failed` : "task executors preserve their inputs and results");
