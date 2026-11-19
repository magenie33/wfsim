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

function observer(outcomes = [false, true], missing = false) {
  const scheduled = new Map(), host = { checks: 0, outcomes, requests: [], missing, submitFailure: false };
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
      const body = path === "/api/board/submit" ? { ok: !host.submitFailure, request: remote }
        : path === "/api/board/requests/status" ? { ok: true, requests: [{ id: remote, phase: "admitted", workflow: "new", workflow_state: "succeeded", current_score: 12, current_by: "verified:e" }] }
        : path === "/api/board/review" ? { ok: !host.missing, accepted: false, id: remote }
        : { ok: true, reviews: [{ ...track, id: remote, state: "succeeded",
          current_score: 10.76757631, current_by: "verified:9e4647c46c76" }] };
      const status = path === "/api/board/review" && host.missing ? 404 : path === "/api/board/submit" && host.submitFailure ? 503 : 200;
      return { ok: status === 200, status, async json() { return body; } };
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

const missing = observer([true], true);
vm.runInContext(`
  boardReviewTrackers.clear();
  globalThis.pending = { ...track, id: 'board-review-request:missing', review: undefined,
    record: { weapon: track.weapon, mods: ['serration'] }, state: 'submitting' };
  boardReviewTrackers.set(pending.id, pending);
`, missing.context);
missing.host.submitFailure = true;
await vm.runInContext("acceptBoardReview(pending)", missing.context);
assert.equal(vm.runInContext("boardReviewTrackers.get(pending.id).state", missing.context), "submitting", "an unavailable missing-build entry retains its durable retry");
missing.host.submitFailure = false;
await vm.runInContext("acceptBoardReview(pending)", missing.context);
assert.equal(vm.runInContext("boardReviewTrackers.get(pending.id).board_request", missing.context), remote);
await missing.poll();
assert.equal(vm.runInContext("boardReviewTrackers.get(pending.id).publication", missing.context), "published");
assert(missing.host.requests.includes("/api/board/requests/status"));
console.log("  ok  an absent database baseline uses requested coverage, retries failure and observes publication");

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
  block(headlessSource, 'const publishedBoardRevisions', 'function headlessNonaHelp(say'),
  block(boardSource, 'const pageBoardReader', '/// `res.ok`'),
  block(boardSource, 'const BOARD_HAVE', "/// EVERY WEAPON'S GROUP LEADERS"),
  block(boardSource, 'let BOARD_INDEX', '/// A scene is visible'),
  block(boardSource, 'async function loadWeaponBoard', 'function ensureBoardCell'),
  block(boardSource, 'let boardIndexAsk', '/// THE LEADERS A RULER'),
  block(boardSource, 'async function reloadReviewedBoard', "/// Redraw the current weapon's"),
].join('\n');

const weapon = "kuva_nukor", baseRuler = "standard_multi_target", sceneRuler = "night_multi_target";
const oldRevision = "a".repeat(64), newRevision = "b".repeat(64);
const old = deferred(), fresh = deferred(), attempts = [];
let revision = oldRevision;
const row = (ruler, score) => ({ benchmark: ruler, mode: "base", score });
const freshBase = [row(baseRuler, 10)], freshScene = [row(sceneRuler, 12)];
const cacheContext = vm.createContext({
  AbortController, AbortSignal, URLSearchParams, BOARD: {}, window: {},
  location: { pathname: "/weapons/Kuva_Nukor/simulator", search: "" },
  finder: {}, raRuler: null, benchList: () => [{ id: baseRuler, cell_dir: "" }, { id: sceneRuler, cell_dir: "night" }],
  benchSceneOf: (bench) => bench?.cell_dir || "", libraryDefaultBenchmark: () => baseRuler, modSuffix: () => "simulator",
  boardActiveKey: () => "active", openedBoardKey: () => "opened",
  localStorage: { getItem: (key) => key === "active" ? JSON.stringify({ b: sceneRuler }) : null },
  async fetchJsonPatient(path) {
    const url = new URL(path, "https://wfsim.invalid"), params = url.searchParams;
    if (url.pathname.endsWith("/catalog")) {
      const scene = params.get("scene"), ruler = scene ? sceneRuler : baseRuler, pinned = revision;
      if (pinned === oldRevision && !params.has("weapon")) await old.promise;
      const leaders = pinned === newRevision ? (scene ? freshScene : freshBase) : [row(ruler, 1411)];
      return { ok: true, partitions: [{ weapon, ruler, scene, revision: pinned, rows: 1, leaders }], next: null };
    }
    assert.equal(url.pathname, "/api/board/pages");
    const ruler = params.get("ruler"), pinned = params.get("revision");
    attempts.push([ruler, pinned]);
    if (ruler === baseRuler) await (pinned === oldRevision ? old.promise : fresh.promise);
    const rows = pinned === newRevision ? (ruler === baseRuler ? freshBase : freshScene) : [row(ruler, 1411)];
    return { ok: true, weapon, ruler, revision: pinned, count: 1, rows, after: 0, next: null };
  },
});
vm.runInContext(loaderSource, cacheContext);
vm.runInContext(`
  globalThis.boardSnapshot = () => ({ rows: BOARD.kuva_nukor, index: BOARD_INDEX, scene: SCENE_INDEX.night,
    loaded: [...boardFileState('kuva_nukor').loaded.keys()], failed: [...boardFileState('kuva_nukor').failed] });
  globalThis.originalState = boardFileState('kuva_nukor');
  originalState.failed.add('previous-failure');
`, cacheContext);
const initial = ["loadWeaponBoard(\x27kuva_nukor\x27)", "loadFullBoard()", "loadSceneIndex(\x27night\x27)"]
  .map(call => vm.runInContext(call, cacheContext));
await new Promise(setImmediate);
revision = newRevision;
const reloading = vm.runInContext("reloadReviewedBoard(\x27kuva_nukor\x27,\x27night_multi_target\x27)", cacheContext);
const parallelReader = vm.runInContext("loadWeaponBoard(\x27kuva_nukor\x27)", cacheContext);
await new Promise(setImmediate);
assert.equal(cacheContext.originalState.controller.signal.aborted, true);
assert.equal(attempts.filter(([ruler]) => ruler === baseRuler).length, 2, "a concurrent reader shares the new revision");
fresh.resolve();
const reloaded = await reloading;
await parallelReader;
assert.equal(JSON.stringify(reloaded.indexRows), JSON.stringify(freshScene));
old.resolve();
await Promise.all(initial);
const snapshot = cacheContext.boardSnapshot();
assert.equal(JSON.stringify([...snapshot.rows].sort((a,b)=>a.score-b.score)), JSON.stringify([...freshBase, ...freshScene]));
assert.equal(JSON.stringify(snapshot.scene), JSON.stringify({ [weapon]: freshScene }));
assert.equal(snapshot.index, null, "an old index request cannot win after a new publication");
assert.equal(snapshot.failed.length, 0);
assert.ok(snapshot.loaded.includes("") && snapshot.loaded.includes("night"));
await vm.runInContext("loadFullBoard()", cacheContext);
assert.equal(JSON.stringify(cacheContext.boardSnapshot().index), JSON.stringify({ [weapon]: freshBase }));
console.log("  ok  publication reload ignores late replies from the former revision");

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
