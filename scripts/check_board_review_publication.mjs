// A verified review keeps observing publication when its first CDN read is old.
import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";

const source = fs.readFileSync(new URL("../web/src/static/app/59-simulation-tasks.js", import.meta.url), "utf8");
const headlessSource = fs.readFileSync(new URL("../web/src/static/app/83-headless-card.js", import.meta.url), "utf8");
const marker = "// A review observes one public verification round";
assert.ok(source.includes(marker), "the production review observer must be present");

const remote = "826757a6-febe-425e-bdfd-a9fcac6d6e64";
const track = {
  id: "board-review:" + remote, review: remote, identity: "a6704721c922dab755938d5563f72d92",
  weapon: "kuva_nukor", ruler: "standard_multi_target", mode: "base", engine: "9e4647c46c76",
  source: "retest", sources: ["retest"], state: "waiting",
};

function observer(outcomes = [false, true]) {
  const scheduled = new Map(), host = { checks: 0, outcomes, requests: [] };
  let timer = 0;
  const context = vm.createContext({
    USER_TASK_TYPES: new Map(), userTaskSources: new Map(),
    META: { board_entry_share: 0.2 }, taskClone: structuredClone, rowHasRiven: (row) => !!row?.riven,
    window: { addEventListener() {} }, document: { addEventListener() {} },
    localStorage: { getItem() { return null; } },
    setTimeout(callback, ms) { scheduled.set(++timer, { callback, ms }); return timer; },
    clearTimeout(id) { scheduled.delete(id); }, timeoutSignal() {},
    async fetch(path) {
      host.requests.push(path);
      const body = path === "/api/board/review" ? { ok: true, accepted: false, id: remote }
        : { ok: true, reviews: [{ ...track, id: remote, state: "succeeded",
          current_score: 10.76757631, current_by: "verified:9e4647c46c76" }] };
      return { ok: true, async json() { return body; } };
    },
    host, track,
  });
  vm.runInContext(block(headlessSource, "function headlessBoardRecord(", "async function headlessMeasurementWitnesses("), context);
  vm.runInContext(source.slice(source.indexOf(marker)), context);
  vm.runInContext(`
    saveBoardReviewTrackers = async () => {};
    renderBoardReviewNote = () => {};
    scheduleUserTasks = () => {};
    refreshReviewedBoard = async () => {
      host.checks++;
      return host.outcomes[Math.min(host.checks - 1, host.outcomes.length - 1)];
    };
    boardReviewTrackers.set(track.id, { ...track });
    globalThis.observed = () => boardReviewTrackers.get(track.id);
  `, context);
  return {
    context, host, scheduled,
    async poll() { await vm.runInContext("pollBoardReviews()", context); },
    async tick() {
      const entry = scheduled.entries().next().value;
      assert.ok(entry, "publication observation must have a scheduled retry");
      const [id, job] = entry;
      scheduled.delete(id);
      job.callback();
      await new Promise(setImmediate);
    },
  };
}

const delayed = observer();
await delayed.poll();
assert.equal(delayed.host.checks, 1, "a successful review checks the public board once");
assert.ok(delayed.scheduled.size > 0, "an old first CDN response must schedule another publication check");
assert.equal(delayed.scheduled.values().next().value.ms, 10_000, "publication follows the existing polling interval");
await delayed.tick();
assert.equal(delayed.host.checks, 2, "the next publication check must read the board again");
assert.equal(delayed.context.observed().publication, "published");
assert.equal(delayed.scheduled.size, 0, "confirmed publication stops fetching");
await delayed.poll();
assert.equal(delayed.host.checks, 2);
console.log("  ok  a review waits through an old CDN response until publication");

const bounded = observer([false]);
const limit = vm.runInContext("BOARD_PUBLICATION_MAX_ATTEMPTS", bounded.context);
assert.ok(Number.isInteger(limit) && limit > 1);
await bounded.poll();
for (let i = 1; i < limit; i++) await bounded.tick();
assert.equal(bounded.host.checks, limit);
assert.equal(bounded.context.observed().publication, "unavailable");
assert.equal(bounded.scheduled.size, 0, "an unavailable publication does not fetch forever");
await bounded.poll();
assert.equal(bounded.host.checks, limit);
console.log("  ok  an unavailable publication stops after its bounded attempts");

const repeated = observer([true]);
await repeated.poll();
assert.equal(repeated.context.observed().publication, "published");
vm.runInContext(`
  globalThis.pending = { ...track, id: 'board-review-request:another-run', review: undefined,
    source: 'another-run', sources: ['another-run'], state: 'submitting' };
  boardReviewTrackers.set(pending.id, pending);
`, repeated.context);
await vm.runInContext("acceptBoardReview(pending)", repeated.context);
assert.equal(repeated.context.observed().publication, "waiting", "the same review UUID must reactivate publication observation");
assert.equal(repeated.context.observed().publication_attempts, 0);
assert.ok(repeated.context.observed().sources.includes("retest"));
assert.ok(repeated.context.observed().sources.includes("another-run"));
await repeated.poll();
assert.equal(repeated.host.checks, 2, "a repeated completed round must check the board again");
assert.equal(repeated.context.observed().publication, "published");
console.log("  ok  retrying the same completed round observes publication again");

function block(text, from, until) {
  const first = text.indexOf(from), last = text.indexOf(until, first + from.length);
  assert.ok(first >= 0 && last > first, "the production board loader block must be present: " + from);
  return text.slice(first, last);
}

function deferred() {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
}

const boardSource = fs.readFileSync(new URL("../web/src/static/app/40-official-builds.js", import.meta.url), "utf8");
const loaderSource = [
  block(boardSource, "const BOARD_HAVE", "/// EVERY WEAPON'S GROUP LEADERS"),
  block(boardSource, "let BOARD_INDEX", "/// ONE WEAPON'S ROWS"),
  block(boardSource, "async function loadWeaponBoard", "function ensureBoardCell"),
  block(boardSource, "let boardIndexAsk", "/// THE LEADERS A RULER'S RANKING READS"),
  block(boardSource, "async function reloadReviewedBoard", "/// THIS ORIGIN HOLDS NO BOARD"),
].join("\n");

const weapon = "kuva_nukor", baseRuler = "standard_multi_target", sceneRuler = "night_multi_target";
const oldBase = [{ benchmark: baseRuler, mode: "base", score: 1411 }];
const oldScene = [{ benchmark: sceneRuler, mode: "base", score: 1500 }];
const freshBase = [{ benchmark: baseRuler, mode: "base", score: 10 }];
const freshScene = [{ benchmark: sceneRuler, mode: "base", score: 12 }];
const oldWeapon = deferred(), oldIndex = deferred(), oldSceneIndex = deferred(), freshWeapon = deferred();
const asks = [];
let publicationReload = false;
const cacheContext = vm.createContext({
  AbortController, AbortSignal, URLSearchParams,
  BOARD: {}, BOARD_META: { scene_weapons: { night: [weapon] } }, BOARD_ORIGIN: "https://board.invalid", boardOffOrigin: false,
  window: {}, location: { pathname: "/weapons/Kuva_Nukor/simulator", search: "" },
  finder: {}, raRuler: null,
  benchList: () => [{ id: baseRuler, cell_dir: "" }, { id: sceneRuler, cell_dir: "night" }],
  benchSceneOf: (bench) => bench?.cell_dir || "",
  libraryDefaultBenchmark: () => baseRuler, modSuffix: () => "simulator",
  boardActiveKey: () => "active", openedBoardKey: () => "opened",
  localStorage: { getItem: (key) => key === "active" ? JSON.stringify({ b: sceneRuler }) : null },
  timeoutSignal: () => new AbortController().signal,
  async fetchJsonPatient(url) {
    asks.push(url);
    if (url === "/board.meta.json") return { scene_weapons: { night: [weapon] } };
    if (url === "/board/" + weapon + ".json") return publicationReload ? freshWeapon.promise : oldWeapon.promise;
    if (url === "/board/night/" + weapon + ".json") return publicationReload ? freshScene : oldScene;
    if (url === "/board/index.json") return publicationReload ? { [weapon]: freshBase } : oldIndex.promise;
    if (url === "/board/night/index.json") return publicationReload ? { [weapon]: freshScene } : oldSceneIndex.promise;
    throw new Error("unexpected board fetch: " + url);
  },
  async fetch(url) {
    const rows = url.includes("/night/") ? freshScene : freshBase;
    return { ok: true, status: 200, async json() { return rows; } };
  },
});
vm.runInContext(loaderSource, cacheContext);
vm.runInContext(`
  globalThis.boardSnapshot = () => ({ rows: BOARD.kuva_nukor, index: BOARD_INDEX,
    scene: SCENE_INDEX.night, loaded: [...boardFileState('kuva_nukor').loaded],
    failed: [...boardFileState('kuva_nukor').failed], have: BOARD_HAVE.has('kuva_nukor') });
  globalThis.originalState = boardFileState('kuva_nukor');
  originalState.failed.add('previous-failure');
`, cacheContext);
const initialWeapon = vm.runInContext("loadWeaponBoard('kuva_nukor')", cacheContext);
const initialIndex = vm.runInContext("loadFullBoard()", cacheContext);
const initialSceneIndex = vm.runInContext("loadSceneIndex('night')", cacheContext);
publicationReload = true;
const reloading = vm.runInContext("reloadReviewedBoard('kuva_nukor', 'night_multi_target')", cacheContext);
await new Promise(setImmediate);
assert.ok(cacheContext.originalState.controller.signal.aborted, "publication invalidates the old weapon request generation");
const parallelReader = vm.runInContext("loadWeaponBoard('kuva_nukor')", cacheContext);
await new Promise(setImmediate);
assert.equal(asks.filter((url) => url === "/board/kuva_nukor.json").length, 2,
  "a parallel reader shares the publication reload instead of starting another weapon fetch");
freshWeapon.resolve(freshBase);
const reloaded = await reloading;
await parallelReader;
assert.equal(JSON.stringify(reloaded.rows), JSON.stringify([...freshBase, ...freshScene]));
assert.equal(JSON.stringify(reloaded.indexRows), JSON.stringify(freshScene));
let snapshot = cacheContext.boardSnapshot();
assert.equal(snapshot.have, true);
assert.equal(snapshot.failed.length, 0, "the old failed-scene markers do not survive publication reload");
assert.ok(snapshot.loaded.includes("") && snapshot.loaded.includes("night"), "all opened weapon scenes are reloaded");
oldWeapon.resolve(oldBase);
oldIndex.resolve({ [weapon]: oldBase });
oldSceneIndex.resolve({ [weapon]: oldScene });
await Promise.all([initialWeapon, initialIndex, initialSceneIndex]);
snapshot = cacheContext.boardSnapshot();
assert.equal(JSON.stringify(snapshot.rows), JSON.stringify([...freshBase, ...freshScene]), "a late old weapon response cannot overwrite the reloaded rows");
assert.equal(JSON.stringify(snapshot.scene), JSON.stringify({ [weapon]: freshScene }), "a late old scene-index response cannot overwrite publication");
assert.ok(snapshot.index === null || JSON.stringify(snapshot.index) === JSON.stringify({ [weapon]: freshBase }),
  "a late old main-index response cannot restore the stale global leaderboard");
await vm.runInContext("loadFullBoard()", cacheContext);
assert.equal(JSON.stringify(cacheContext.boardSnapshot().index), JSON.stringify({ [weapon]: freshBase }));
console.log("  ok  publication reload invalidates all weapon scenes and ignores late old responses");

const publicationContext = observer().context;
vm.runInContext(block(boardSource, "function boardGroupLeaders", "/// How many rows"), publicationContext);
const record = {
  weapon, mods: ["hornet_strike", "riven"], evolutions: [], arcanes: ["secondary_encumber"],
  valence: "magnetic", exilus: "ruinous_extension", riven_pos: ["critical_damage", "damage"],
  riven_neg: "recoil", riven_rolls: [1.05, 1.05, 1.05],
};
const reviewed = { ...track, record, current_score: 15, current_by: "verified:9e4647c46c76",
  completed_at: "2026-11-05T04:29:45Z" };
const publicRow = {
  benchmark: track.ruler, mode: track.mode, score: 15, mods: record.mods, evolutions: [], arcanes: record.arcanes,
  valence: record.valence, exilus: record.exilus,
  riven: { bonuses: record.riven_pos, malus: record.riven_neg, rolls: record.riven_rolls },
  measured_by: "9e4647c46c76", measured_at: "2026-11-05T04:29Z",
};
function published(task = reviewed, rows = [publicRow], indexRows = rows) {
  Object.assign(publicationContext, { task, rows, indexRows });
  return vm.runInContext("boardReviewPublication(task, rows, indexRows)", publicationContext);
}
assert.equal(published(), true, "the exact frozen record and current fact confirm publication");
assert.equal(published(reviewed, [publicRow], [{ ...publicRow, score: 1411 }]), false,
  "a stale index still advertising the old score must keep publication pending");
assert.equal(published(reviewed, [{ ...publicRow, score: 1411 }], [publicRow]), false);
assert.equal(published(reviewed, [{ ...publicRow, measured_by: "dac6ecbaf97e" }]), false);
assert.equal(published(reviewed, [{ ...publicRow, measured_at: "2026-11-05T04:28Z" }]), false);
assert.equal(published(reviewed, [{ ...publicRow, riven: { ...publicRow.riven, rolls: [1.05, 1.05, 0.95] } }]), false,
  "a different riven corner does not confirm the frozen build");
assert.equal(published(reviewed, [{ ...publicRow, mods: ["hornet_strike@0", "riven"] }]), false,
  "a different mod rank does not confirm the frozen build");
assert.equal(published({ ...reviewed, record: { ...record, weapon: "other_weapon" } }), false);
const plainRecord = { weapon, mods: ["hornet_strike"], arcanes: [], evolutions: [] };
const plainRow = { benchmark: track.ruler, mode: track.mode, score: 15,
  mods: plainRecord.mods, arcanes: [], evolutions: [], measured_by: publicRow.measured_by, measured_at: publicRow.measured_at };
assert.equal(published({ ...reviewed, record: plainRecord }, [plainRow]), true,
  "omitted optional record fields and empty public fields identify the same plain build");
console.log("  ok  publication matches exact build, rank, rolls, engine, score and measurement minute");

const leader = { ...publicRow, mods: ["hornet_strike", "lethal_torrent", "riven"], score: 20 };
assert.equal(published({ ...reviewed, current_score: 1 }, [leader]), true,
  "an absent row below its own group's entry line is a legitimate published removal");
assert.equal(published({ ...reviewed, current_score: 4 }, [leader]), false,
  "a row exactly on the inclusive entry line cannot be declared trimmed");
assert.equal(published({ ...reviewed, current_score: 1 }, [{ ...leader, riven: undefined }]), false,
  "a plain leader cannot justify removing a riven build");
assert.equal(published({ ...reviewed, current_score: 1 }, [], [leader]), false);
assert.equal(published({ ...reviewed, current_score: 1 }, [leader], []), false);
assert.equal(published({ ...reviewed, current_score: 1 }, [leader], [{ ...publicRow, score: 1411 }]), false,
  "a trimmed weapon row is still pending while its old index row survives");
console.log("  ok  a missing row confirms only a supported entry-line removal");
