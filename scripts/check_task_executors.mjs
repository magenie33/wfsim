import { openApp } from "./cdp.mjs";
const app = await openApp({ boot: 13000, lang: "en" });
try {
  await app.load("/weapons/Torid/optimizer", 7000);
  await app.evaluate(`window.executorReportTab = window.open('about:blank', 'search-result-report')`, { userGesture: true });
  await app.send("Page.bringToFront");
  const result = await app.evaluate(`(async () => {
    const reportTab = window.executorReportTab;
    const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
    const until = async (condition, limit = 400) => { for (let i = 0; i < limit; i++) { if (await condition()) return; await pause(100); } throw new Error('executor check timed out'); };
    await initUserTasks(); gainPrefs.on = false; restartCalc();
    const out = {};
    const heldExecutor = kind => {
      const type = USER_TASK_TYPES.get(kind), original = type.execute;
      let report, release, observed;
      const gate = new Promise(resolve => { release = resolve; });
      type.execute = async (task, execution) => { report = execution.report; await gate; throw new Error('Released progress fixture'); };
      return { get report() { return report; }, release,
        restore() { type.execute = original; },
        emit(raw) {
          observed = typeof observeOptimizationStatus === 'function' ? observeOptimizationStatus(raw, observed) : raw;
          const fraction = optProgressOf(observed);
          report({ phase: 'Optimization', done: fraction ?? 0, total: fraction == null ? 0 : 1,
            status: observed, builds: observed.enumerated || 0, fights: observed.sims_done || 0 });
          return observed;
        } };
    };
    const workStatus = (patch = {}) => ({ phase: 'running', activity: 'evaluating', rounds: 0,
      starts: Array.from({ length: 4 }, () => ({ pass: 1, passes: 3, round: 1, at: 0, of: 8, settled: false })),
      enumerated: 17, sims_done: 170, elapsed_s: 12, batches_completed: 2,
      batch_builds_done: 7, batch_builds_total: 20, batch_slices_done: 1, batch_slices_total: 4,
      engine_last_at: Date.now(), response_last_at: Date.now(), ...patch });
    const progressRow = (root, id) => document.querySelector(root + ' [data-task-id="' + id + '"]');
    const hasCounts = (node, builds, fights) => node?.textContent.includes(builds + ' builds evaluated')
      && node.textContent.includes(fights + ' fights completed') && !node.textContent.includes('0 / 1');
    const limits = normalizeLimits(null);
    limits.mods = 1; limits.exilus = false; limits.arcane_seats = limits.arcane_seats.map(() => false);
    limits.exclude.mods = buildPool().filter(mod => mod.id !== 'serration').map(mod => mod.id);
    applyOptState({ starts: [{ build: stateFromBuild({ mods: ['serration'] }, 'torid'), fixed: ['mods:0'] }], limits, candidate_runs: 1, finalists: 2 });
    sim.duration = 1; setSimRuns(1);
    const expected = taskClone(nameableOptimizeBody(optimizeBody()));
    const heldSearch = heldExecutor('optimization');
    let probe;
    try {
      probe = await runOptimize(); await until(() => !!heldSearch.report);
      userTaskPanelOpen = true; renderUserTasks();
      heldSearch.emit({ phase: 'starting', rounds: 0, starts: [], enumerated: 0, sims_done: 0, elapsed_s: 0 });
      await pause(150);
      const unknownBar = progressRow('#user-tasks-panel', probe.id)?.querySelector('.task-progress.indeterminate');
      out.progressUnknown = !!document.querySelector('#opt-results .opt-bar.indeterminate')
        && !!unknownBar && !unknownBar.hasAttribute('aria-valuenow');
      const first = heldSearch.emit(workStatus()); await pause(150);
      out.progressCounts = hasCounts(document.getElementById('opt-results'), 17, 170)
        && hasCounts(progressRow('#user-tasks-panel', probe.id), 17, 170)
        && document.querySelector('#opt-results .opt-bar i')?.style.width === '0%'
        && progressRow('#user-tasks-panel', probe.id)?.querySelector('.task-progress')?.getAttribute('aria-valuenow') === '0';
      await userTaskAction('details', probe.id);
      const expandedDetails = progressRow('#user-tasks-panel', probe.id)?.querySelector('.task-details');
      const clock = document.querySelector('#opt-results [data-search-clock]'), initialClock = clock?.textContent;
      const cancelButton = document.getElementById('opt-cancel');
      if (clock) await until(() => clock.textContent !== initialClock, 30);
      out.progressClock = !!clock && clock.textContent !== initialClock && document.getElementById('opt-cancel') === cancelButton;
      const repeated = heldSearch.emit(workStatus({ elapsed_s: 14 })); await pause(150);
      out.progressActivity = Number.isFinite(first.work_last_at) && repeated.work_last_at === first.work_last_at
        && repeated.observed_at > first.observed_at;
      const advanced = heldSearch.emit(workStatus({ enumerated: 18, sims_done: 180, elapsed_s: 14 })); await pause(150);
      out.progressActivity = out.progressActivity && advanced.work_last_at > repeated.work_last_at;
      out.progressCounts = out.progressCounts && hasCounts(document.getElementById('opt-results'), 18, 180)
        && hasCounts(progressRow('#user-tasks-panel', probe.id), 18, 180)
        && document.querySelector('#opt-results .opt-bar i')?.style.width === '0%';
      const updatedRow = progressRow('#user-tasks-panel', probe.id);
      out.progressDetails = !!expandedDetails && expandedDetails === updatedRow?.querySelector('.task-details')
        && hasCounts(updatedRow.querySelector('.task-progress-note'), 18, 180)
        && expandedDetails.textContent.includes('2 finalists')
        && !expandedDetails.textContent.includes('17 builds scored') && !expandedDetails.textContent.includes('170 fights');
      const batch = heldSearch.emit(workStatus({ enumerated: 18, sims_done: 180, batch_slices_done: 2 })); await pause(150);
      out.progressActivity = out.progressActivity && batch.work_last_at > advanced.work_last_at;
      const currentTask = userTasks.view().find(item => item.id === probe.id);
      out.progressStale = typeof optimizationTaskProgress === 'function'
        && JSON.stringify(optimizationTaskProgress(currentTask, Date.now() + 20_000)).includes('Waiting for a calculation response');
      const native = workStatus({ enumerated: 18, sims_done: 180 }); delete native.engine_last_at; delete native.response_last_at;
      heldSearch.emit(native); await pause(150);
      const nativeClock = document.querySelector('#opt-results [data-search-clock]')?.textContent || '';
      out.progressNative = !!nativeClock && !nativeClock.includes('calculation response');
      heldSearch.emit(workStatus({ activity: 'final_simulation', starts: workStatus().starts.map(start => ({ ...start, settled: true })), sims_planned: 0 }));
      await pause(150);
      out.progressFinalizing = !!document.querySelector('#opt-results .opt-bar.indeterminate')
        && !!progressRow('#user-tasks-panel', probe.id)?.querySelector('.task-progress.indeterminate')
        && optProgressOf({ phase: 'done' }) === 1;
      userTaskPanelOpen = false; renderUserTasks();
      history.pushState(null, '', '/weapons/Torid/builder'); await route(); await pause(1250);
      out.progressNavigation = typeof optimizationClockTimer !== 'undefined' && optimizationClockTimer == null;
      await userTasks.cancel(probe.id);
      heldSearch.emit(workStatus({ enumerated: 999, sims_done: 999 }));
      history.pushState(null, '', '/weapons/Torid/optimizer'); await route(); await pause(1250);
      out.progressCancelled = !userTasks.view().find(item => item.id === probe.id)?.progress
        && !document.getElementById('opt-results').textContent.includes('999')
        && typeof optimizationClockTimer !== 'undefined' && optimizationClockTimer == null;
    } finally {
      if (probe && userTaskActive(userTasks.list().find(item => item.id === probe.id))) await userTasks.cancel(probe.id);
      heldSearch.release();
      if (probe) await until(() => !userTasks.controllers.has(probe.id));
      heldSearch.restore();
      if (probe) await userTasks.remove(probe.id);
    }
    const beforeValidationApi = api;
    let releaseValidation, validationHeld = false, task, editor;
    const validationGate = new Promise(resolve => { releaseValidation = resolve; });
    api = (path, body, ...rest) => {
      if(path === '/api/optimize/check' && body?.weapon === 'torid' && !validationHeld) {
        validationHeld = true;
        return validationGate.then(() => beforeValidationApi(path, body, ...rest));
      }
      return beforeValidationApi(path, body, ...rest);
    };
    try {
      const pending = runOptimize();
      await until(() => validationHeld);
      history.pushState(null, '', '/weapons/Phenmor/rivens'); await route(); await pause(400);
      editor = JSON.stringify(snapshotState());
      releaseValidation(); task = await pending;
    } finally { releaseValidation(); api = beforeValidationApi; }
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
    out.executorAvailability = out.executorAvailability && community.disabled;
    communityTaskCapabilities = { kinds: ['appraisal'] }; await refreshRivenAnalysis();
    out.executorAvailability = out.executorAvailability && !community.disabled;
    communityTaskCapabilities = capability; await refreshRivenAnalysis();
    const prior = new Set(userTasks.list().map(task => task.id));
    const displayedCards = taskClone(loadPresetList(RIVENS));
    const editedCards = taskClone(displayedCards); editedCards.find(card => card.id === id).state.bonuses[0].roll = 0.9;
    storePresetList(RIVENS, editedCards);
    let releaseCapture;
    const captureLock = userTaskLock('resource:local', () => new Promise(resolve => { releaseCapture = resolve; }));
    await until(() => !!releaseCapture);
    document.getElementById('analysis-submit').click();
    await until(() => userTasks.list().some(task => task.kind === 'appraisal' && !prior.has(task.id)));
    const tasks = userTasks.list().filter(task => task.kind === 'appraisal' && !prior.has(task.id));
    storePresetList(RIVENS, displayedCards);
    out.fxRequest = JSON.stringify(tasks[0].request) === JSON.stringify(expectedAppraisal.request)
      && JSON.stringify(tasks[0].origin.board) === JSON.stringify(expectedAppraisal.context);
    out.analysisLink = USER_TASK_TYPES.get('appraisal').resultLink({ ...tasks[0], result_ref: 'capture' }).includes('/riven-analyst?analysis-result=');
    out.appraisalCapture = tasks.length === 1 && tasks[0].kind === 'appraisal' && !!tasks[0].origin.batch && tasks[0].origin.ruler === ruler;
    const heldAppraisal = heldExecutor('appraisal');
    try {
      releaseCapture(); await captureLock; await until(() => !!heldAppraisal.report);
      userTaskPanelOpen = true; renderUserTasks();
      heldAppraisal.emit(workStatus({ enumerated: 23, sims_done: 230 })); await pause(150);
      out.analysisProgress = hasCounts(progressRow('#riven-analysis-queue', tasks[0].id), 23, 230)
        && hasCounts(progressRow('#user-tasks-panel', tasks[0].id), 23, 230)
        && progressRow('#riven-analysis-queue', tasks[0].id)?.querySelector('.task-progress')?.getAttribute('aria-valuenow') === '0';
      const analysisRow = progressRow('#riven-analysis-queue', tasks[0].id);
      heldAppraisal.emit(workStatus({ enumerated: 24, sims_done: 240 })); await pause(150);
      out.analysisProgress = out.analysisProgress && hasCounts(progressRow('#riven-analysis-queue', tasks[0].id), 24, 240)
        && hasCounts(progressRow('#user-tasks-panel', tasks[0].id), 24, 240);
      out.analysisProgressNodes = analysisRow === progressRow('#riven-analysis-queue', tasks[0].id);
      heldAppraisal.report({ phase: 'Appraisal replay', done: 2, total: 5 }); await pause(150);
      const replayRows = [progressRow('#riven-analysis-queue', tasks[0].id), progressRow('#user-tasks-panel', tasks[0].id)];
      out.analysisReplayProgress = replayRows.every(row => (row?.textContent.includes('Appraisal replay') || row?.textContent.includes('Replaying the build'))
        && row.textContent.includes('2 / 5') && !row.textContent.includes('builds evaluated')
        && row.querySelector('.task-progress')?.getAttribute('aria-valuenow') === '40');
    } finally {
      await userTasks.cancel(tasks[0].id); heldAppraisal.release();
      await until(() => !userTasks.controllers.has(tasks[0].id)); heldAppraisal.restore();
      userTaskPanelOpen = false; renderUserTasks();
    }
    // Short fixture terms exercise the whole search/replay/grade pipeline.
    const benchmark = META.benchmarks.find(item => item.id === ruler), preset = META.search_presets.presets.find(item => item.riven);
    const oldTerms = taskClone(benchmark.scenario), oldPreset = taskClone(preset);
    benchmark.scenario.duration = 2; benchmark.scenario.runs = 2; preset.candidate_runs = 1; preset.finalists = 1;
    let graded;
    const originalFetch = window.fetch, originalReplay = headlessAppraisalReplay, calibrations = [];
    headlessAppraisalReplay = (...args) => {
      const running = userTasks.view().find(task => task.kind === 'appraisal' && task.state === 'running');
      const progress = running?.progress;
      out.analysisReplayEntry = progress?.phase === 'Appraisal replay' && progress.done === 0 && progress.total === 0
        && !progress.status && userTaskProgress(running).fraction == null;
      return originalReplay(...args);
    };
    window.fetch = (url, options) => {
      if (String(url).endsWith('/api/board/calibrate')) {
        calibrations.push(JSON.parse(options.body));
        return Promise.reject(new Error('calibration unavailable'));
      }
      return originalFetch(url, options);
    };
    try {
      const batch = await submitSavedAppraisals([id], ruler);
      const finished = await userTasks.wait(batch[0].id);
      if (finished.state !== 'succeeded') throw new Error('Appraisal failed: ' + finished.error);
      graded = await userTasks.store.value('artifact:' + finished.result_ref);
    } finally { window.fetch = originalFetch; headlessAppraisalReplay = originalReplay; benchmark.scenario = oldTerms; Object.assign(preset, oldPreset); }
    out.calibration = calibrations.length === 0;
    out.grade = graded.result.verdict.ok && Number.isFinite(graded.result.verdict.grade) && graded.result.search.results.length > 0;
    out.reference = Array.isArray(graded.result.verdict.reference_rows) && !!graded.result.verdict.at;
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
  for (const [key, label] of Object.entries({
    progressUnknown: "a search with no known work total remains indeterminate",
    progressCounts: "optimizer and task center show completed work while the descent percentage stays zero",
    progressDetails: "expanded task details retain their fixed inputs without a stale duplicate work count",
    progressClock: "elapsed and last-activity clocks advance without replacing the active controls",
    progressActivity: "repeated responses preserve the last work time and completed work advances it",
    progressStale: "an old calculation response is shown as waiting rather than fresh work",
    progressNative: "native progress without engine activity does not claim calculation responses",
    progressFinalizing: "final verification without a work total does not claim one hundred percent",
    progressNavigation: "search clock scheduling stops when its progress is no longer visible",
    progressCancelled: "cancellation stops the clock and ignores late progress",
    analysisProgress: "riven analysis and task center show actual work at the same flat percentage",
    analysisProgressNodes: "riven progress updates preserve its current row",
    analysisReplayProgress: "appraisal replay replaces search counts with its own completed work",
    analysisReplayEntry: "appraisal enters an indeterminate replay stage before waiting for its first fight",
  })) app.check(label, result[key] === true, JSON.stringify(result));
  app.check("an appraisal batch creates no independent calibration", result.calibration, JSON.stringify(result));
  for (const [key, label] of Object.entries({ analysisEntry: "saved-card analysis opens the dedicated page and shared card preview", analysisDefault: "analysis defaults to the official ruler and local execution", executorAvailability: "the open dialog follows community availability after loading", fxRequest: "the accepted analysis is exactly the fx request and context", analysisLink: "the task result opens its analysis report", reportGrade: "the report displays its artifact's grade and score", reportStandard: "the report exposes the fx standard and sharing", analysisHistory: "deleting the card and task leaves immutable analysis history", reportFrozen: "the shared build keeps the submitted rolls after the owned card is deleted", analysisShare: "sharing includes the dated grade, ruler and frozen build", optimizerRecord: "the same artifact appears in optimizer history", analysisBuild: "opening the winner creates a build with the original submitted riven", realSearch: "the original optimizer executes through the task queue", fixedSearch: "optimization keeps its captured scope and owner", editor: "search completion leaves another weapon's editor unchanged", reportLink: "the search report has a durable result link", openReport: "the saved search report opens without changing the original editor", frozenRiven: "captured riven rolls and rank survive later edits to the owned card", artifact: "every ranked result retains its original replay request", typed: "typed cards use the shared reader and become editable saved rivens", batchControls: "saved rivens expose selection and batch appraisal", appraisalCapture: "appraisal captures the card, ruler and batch before execution", grade: "a real search is replayed and graded through the shared appraisal pipeline", reference: "the appraisal report retains its reference board and measurement time", transient: "a replaced automatic scan cannot report into its successor", reset: "resetting the calculator clears its transient running item" }))
    app.check(label, result[key], JSON.stringify(result));
} finally { await app.close(); }
process.exitCode = app.failures ? 1 : 0;
console.log(app.failures ? `${app.failures} failed` : "task executors preserve their inputs and results");
