import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 13000, lang: "en" });
const { evaluate, check, sleep } = app;
try {
  await app.load("/weapons/Torid/simulator", 7000);
  const out = await evaluate(`(async () => {
    const observer = window.open('about:blank', 'task-queue-observer');
    if (!observer) throw new Error('The check could not open its observer tab');
    const sleep = (ms) => new Promise(resolve => setTimeout(resolve, ms));
    const until = async (condition) => {
      for (let index = 0; index < 160; index++) { if (condition()) return; await sleep(100); }
      throw new Error('Task center condition timed out');
    };
    await initUserTasks();
    gainPrefs = { ...gainPrefs, on: false };
    restartCalc();
    await loadBoard(presetWeapon());
    await sleep(SCENE_ROWS_WITHIN_MS + 500);
    await userTasks.store.change(state => { state.tasks = []; });
    await userTasks.refresh();
    const out = {};
    activePreset = '';
    setSimRuns(2); sim.duration = 1;
    await runSim();
    out.unnamedResult = !!document.querySelector('#sim-results table, #sim-results .fold')
      && userTasks.list().some(task => task.state === 'succeeded' && !task.origin.context.preset);
    await userTasks.store.change(state => { state.tasks = []; });
    await userTasks.refresh();
    const addBuild = async () => {
      if (!activePreset) { document.querySelector('#preset-bar-builder-builds .pchip.add').click(); await sleep(250); }
    };
    await addBuild();
    setSimRuns(3); sim.duration = 2;
    const a = { weapon: presetWeapon(), preset: activePreset, scenario: activeScenario, key: simKey() };
    const actualFleet = simulateFleet;
    const held = [];
    simulateFleet = (body, progress, options) => {
      const actual = actualFleet(body, progress, options);
      actual.catch(() => {});
      return new Promise((resolve, reject) => held.push({ body, progress, options, actual, resolve, reject }));
    };
    const first = runSim();
    await until(() => held.length === 1);
    const taskA = userTasks.list().find(task => task.state === 'running');
    history.pushState(null, '', '/weapons/Phenmor/simulator');
    await route(); await sleep(600); await addBuild();
    setSimRuns(4); sim.duration = 3;
    const b = { weapon: presetWeapon(), preset: activePreset, scenario: activeScenario, key: simKey() };
    const second = runSim();
    await until(() => userTasks.list().some(task => task.state === 'queued'));
    const taskB = userTasks.list().find(task => task.state === 'queued');
    out.oneHeavy = held.length === 1 && userTasks.list().filter(task => task.state === 'running').length === 1;
    observer.location.href = '/';
    await until(() => observer?.__wfsimReady && observer.eval('!!userTasks'));
    await until(() => observer.eval('userTasks.view().some(task => task.id === ' + JSON.stringify(taskA.id) + ')'));
    const observed = observer.eval('userTasks.view()');
    out.oneRunner = observed.filter(task => task.state === 'running').length === 1
      && observed.find(task => task.state === 'running').id === taskA.id
      && userTasks.list().find(task => task.id === taskA.id).state === 'running';
    observer.close();
    out.frozen = taskA.request.weapon === a.weapon && taskA.request.runs === 3 && taskA.request.duration === 2
      && taskB.request.weapon === b.weapon && taskB.request.runs === 4;
    document.getElementById('user-tasks-toggle').click();
    renderUserTasks();
    out.visible = !document.getElementById('user-tasks-panel').hidden && document.querySelectorAll('.task-row').length >= 3;
    out.noForm = !document.querySelector('#user-tasks-panel form, #user-tasks-panel input, #user-tasks-panel select');
    out.quick = userTaskItems().filter(task => task.id === 'quick').length === 1;
    held[0].resolve(await held[0].actual);
    await first;
    await until(() => held.length === 2);
    const runningB = userTasks.list().find(task => task.id === taskB.id);
    const recordA = results.find(record => record.id === userTasks.list().find(task => task.id === taskA.id).result_ref);
    out.resultOrigin = recordA.weapon === a.weapon && recordA.preset === a.preset && recordA.scenario === a.scenario && recordA.key === a.key && recordA.fight.duration === 2;
    out.editorPreserved = presetWeapon() === b.weapon && activePreset === b.preset && simKey() === b.key;
    out.pageProgress = document.querySelector('.simrun')?.dataset.userTask === taskB.id;
    await userTasks.cancel(taskB.id);
    held[1].progress?.(4, 4);
    held[1].resolve(await held[1].actual);
    await second; await sleep(150);
    out.cancelled = userTasks.list().find(task => task.id === taskB.id).state === 'cancelled';
    out.noLateRecord = !!runningB.attempt && !results.some(record => record.id === runningB.attempt);
    out.stopFreed = !document.getElementById('run-sim').disabled;
    const current = userTasks.list().find(task => task.id === taskA.id);
    window.taskCenterResultUrl = USER_TASK_TYPES.get('simulation').resultLink(current);
    await userTasks.remove(taskA.id);
    out.keepsResult = results.some(record => record.id === recordA.id);
    simulateFleet = actualFleet;
    window.taskCenterEditor = JSON.stringify(snapshotState());
    window.taskCenterFight = JSON.stringify(snapshotScenario());
    return out;
  })()`, { userGesture: true });
  for (const [name, key] of [["only one heavy calculation runs while another waits", "oneHeavy"],
    ["an unnamed build keeps and displays its task-owned result", "unnamedResult"],
    ["another tab observes the same running attempt without claiming it", "oneRunner"],
    ["requests keep their submitted weapon, fight and run count", "frozen"], ["the task list is visible from the second page", "visible"],
    ["the center does not duplicate any configuration form", "noForm"], ["quick calculation has one transient list item", "quick"],
    ["a completed result belongs to the original preset and fight", "resultOrigin"], ["completion preserves the current editor", "editorPreserved"],
    ["the original simulator progress component follows its own task", "pageProgress"], ["cancellation remains terminal", "cancelled"],
    ["late completion creates no result record", "noLateRecord"], ["cancelling frees the original Run button", "stopFreed"],
    ["removing task history preserves the saved result", "keepsResult"]]) check(name, out[key] === true, JSON.stringify(out));

  await evaluate("window.taskReportTab = window.open(taskCenterResultUrl, 'task-report-check')", { userGesture: true });
  await sleep(12000);
  const report = await evaluate(`({ ready: taskReportTab?.__wfsimReady, report: taskReportTab?.eval('!!simulationReport'),
    preserved: JSON.stringify(snapshotState()) === taskCenterEditor && JSON.stringify(snapshotScenario()) === taskCenterFight })`);
  check("the existing simulator opens the saved report in another tab", report.ready && report.report, JSON.stringify(report));
  check("opening a report leaves the originating editor unchanged", report.preserved, JSON.stringify(report));
  await evaluate("taskReportTab.close()");
  await app.load("/weapons/Phenmor/simulator", 7000);
  check("task history survives reload", await evaluate("userTasks.list().some(task => task.state === 'cancelled')"));
  await app.send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
  await evaluate("userTaskPanelOpen = true; renderUserTasks()");
  check("the phone keeps the entry and list inside its viewport", await evaluate("document.documentElement.scrollWidth <= innerWidth && document.querySelector('.task-center').getBoundingClientRect().right <= innerWidth && document.querySelector('#user-tasks-toggle').getBoundingClientRect().right <= innerWidth"));
  process.exitCode = app.failures ? 1 : 0;
  console.log(app.failures ? `\n${app.failures} failed` : "\nthe task center uses real calculations and preserves their owners");
} finally { await app.close(); }
