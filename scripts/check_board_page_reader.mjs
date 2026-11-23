// SPDX-License-Identifier: AGPL-3.0-or-later
// Drive the real page loader with immutable partitions, stale files and broken pages.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import { boardPagesRoute } from "../worker/board_pages.js";

const source = readFileSync(process.env.WFSIM_BOARD_READER_SOURCE || new URL("../web/src/static/app/40-official-builds.js", import.meta.url), "utf8");
const shared = readFileSync(new URL("../web/src/static/app/83-headless-card.js", import.meta.url), "utf8");
const cut = (text, first, last) => text.slice(text.indexOf(first), text.indexOf(last));
const current = source.includes("const pageBoardReader");
const loader = [
  current ? cut(shared, "const publishedBoardRevisions", "function headlessNonaHelp(say") : "",
  cut(source, current ? "const pageBoardReader" : "async function readBoardPartitionCatalog", "/// `res.ok`"),
  cut(source, "const BOARD_HAVE", "/// EVERY WEAPON'S GROUP LEADERS"),
  cut(source, "async function loadWeaponBoard", "function ensureBoardCell"),
].join("\n");

const weapon = "furis", ruler = "standard_single_target", revision = "a".repeat(64), step = 7, total = step * 64 - 3;
const previous = [{ benchmark: ruler, score: -1 }, { benchmark: "retained_other_ruler", score: 123 }];

async function run(options = {}) {
  const rows = Array.from({ length: options.empty ? 0 : total }, (_, score) => ({ benchmark: ruler, score }));
  const extra = ruler + "_extra", remote = ruler + "_remote";
  const benchmarks = [{ id: ruler, cell_dir: "" }, { id: extra, cell_dir: "" }, { id: remote, cell_dir: "sp200" }];
  const opened = options.overlap ? [extra, remote] : options.opened ? [extra] : [];
  const state = { active: 0, maximum: 0, pages: 0, oldReads: 0 };
  const wait = (duration, signal) => new Promise((resolve, reject) => {
    const cancel = () => { clearTimeout(timer); reject(signal.reason || Error("aborted")); };
    const timer = setTimeout(() => { signal.removeEventListener("abort", cancel); resolve(); }, duration);
    if (signal.aborted) cancel();
    else signal.addEventListener("abort", cancel, { once: true });
  });
  const timeout = ms => AbortSignal.timeout(ms === 60000 ? (options.overall || 3000) : 200);
  const context = vm.createContext({
    URLSearchParams, AbortController, AbortSignal: { any: AbortSignal.any, timeout }, timeoutSignal: timeout,
    window: {}, LIVE_ORIGIN: "https://wfsim.invalid", BOARD_ORIGIN: "https://board.invalid", boardOffOrigin: false,
    BOARD: {}, BOARD_META: null, location: { pathname: "/weapons/Furis/build-library", search: "?bench=" + ruler },
    finder: { weapon, b: ruler }, raRuler: null, benchList: () => benchmarks, benchSceneOf: b => b?.cell_dir || "",
    libraryDefaultBenchmark: () => ruler, modSuffix: () => "build-library",
    localStorage: { getItem: k => k === "opened" ? JSON.stringify(opened.map(b => ({ b }))) : null },
    boardActiveKey: () => "active", openedBoardKey: () => "opened",
    async fetchJsonPatient(path, _tries, signal) {
      const url = new URL(path, "https://wfsim.invalid");
      if (url.pathname === "/board/" + weapon + ".json") {
        state.oldReads++;
        return previous;
      }
      if (url.pathname === "/api/board/pages/catalog") {
        const scene = url.searchParams.get("scene");
        const rulers = (options.overlap ? benchmarks : options.extra ? benchmarks.slice(0, 2) : benchmarks.slice(0, 1))
          .filter(b => !options.absent && b.cell_dir === scene);
        return { ok: true, partitions: rulers.map(b => ({ weapon, ruler: b.id, scene, revision, rows: rows.length })), next: null };
      }
      assert.equal(url.pathname, "/api/board/pages");
      assert.equal(url.searchParams.get("revision"), revision);
      const after = Number(url.searchParams.get("after"));
      assert(Number.isSafeInteger(after) && after >= 0 && after % step === 0);
      state.pages++;
      state.maximum = Math.max(state.maximum, ++state.active);
      try {
        await wait(options.hang && after === step ? 500 : options.delay || 20 + (after / step % 3) * 2, signal);
        const ruler = url.searchParams.get("ruler");
        const values = rows.slice(after, after + step).map(row => ({ ...row, benchmark: ruler }));
        const end = after + values.length;
        const page = { ok: true, weapon, ruler, revision, after, count: rows.length, rows: values, next: end < rows.length ? end : null };
        if (after === step * 3) {
          if (options.wrongRevision || options.failRuler === ruler) page.revision = "b".repeat(64);
          if (options.wrongCount) page.count++;
          if (options.gap) page.next++;
          if (options.short) page.rows = page.rows.slice(1);
        }
        return page;
      } finally { state.active--; }
    },
  });
  vm.runInContext(loader, context);
  const timer = options.cancel && setTimeout(() => vm.runInContext("boardFileState('furis').controller.abort()", context), 35);
  const started = performance.now();
  try {
    if (options.overlap) {
      const load = (...rulers) => vm.runInContext("loadBoardRulers('furis'," + JSON.stringify(rulers) + ")", context);
      await Promise.all([load(ruler), load(ruler, extra), load(remote)]);
      if (options.failRuler) {
        const surviving = JSON.parse(vm.runInContext("JSON.stringify(BOARD.furis.map(row => row.benchmark))", context));
        assert.equal(surviving.length, 2 * total);
        assert.deepEqual(new Set(surviving), new Set([extra, remote]));
        options.failRuler = null;
        await vm.runInContext("loadBoardRulers('furis',['standard_single_target'],true)", context);
      }
    } else await vm.runInContext("loadWeaponBoard('furis')", context);
    const result = JSON.parse(vm.runInContext("JSON.stringify(BOARD.furis ?? null)", context));
    const cell = vm.runInContext("Boolean(boardHasCell('furis','standard_single_target'))", context);
    return { result, state, elapsed: performance.now() - started, cell };
  } finally { clearTimeout(timer); }
}

const full = await run();
assert.equal(full.state.oldReads, 0, "current ranking must never read an older board file");
assert.equal(full.cell, true);
assert.deepEqual(full.result.map(row => row.score), Array.from({ length: total }, (_, i) => i));
assert(full.elapsed > 200 && full.state.maximum > 1 && full.state.maximum <= 4);
assert.equal(full.state.pages, 64); assert.equal(full.state.active, 0);
console.log("  ok  64 pages, one revision, ordered rows, no legacy request");

const focused = await run({ extra: true });
assert.equal(focused.result.length, total, "opening a ruler must not download unrelated ranks in its scene");
assert.equal(focused.state.pages, 64);
const concurrent = await run({ extra: true, opened: true });
assert.equal(concurrent.result.length, total * 2);
assert.equal(concurrent.state.pages, 128);
assert(concurrent.state.maximum <= 4);
console.log("  ok  multiple partitions share one request limit");
const overlap = await run({ overlap: true });
assert.equal(overlap.result.length, 3 * total);
assert.equal(overlap.state.pages, 3 * 64);
assert.equal(overlap.state.maximum, 4);
const retried = await run({ overlap: true, failRuler: ruler });
assert.equal(retried.result.length, 3 * total);
assert.equal(retried.cell, true);
assert.equal(retried.state.maximum, 4);
console.log("  ok  overlapping requests share pages and retain other rulers through failure and retry");

for (const [name, options] of [["revision changed", { wrongRevision: true }], ["count changed", { wrongCount: true }],
  ["cursor gap", { gap: true }], ["short page", { short: true }], ["page timeout", { hang: true }],
  ["weapon cancellation", { cancel: true }]]) {
  const value = await run(options);
  assert.equal(value.result, null, name); assert.equal(value.cell, false, name);
  assert.equal(value.state.oldReads, 0, name); assert(value.state.maximum <= 4, name);
  await new Promise(resolve => setTimeout(resolve, 1));
  assert.equal(value.state.active, 0, name);
  console.log("  ok  " + name + " withholds an unavailable ranking");
}

const progressing = await run({ overall: 110, delay: 20 });
assert(progressing.elapsed > 110);
assert.equal(progressing.result.length, total, "completed pages must survive an overall read longer than one minute");
const absent = await run({ absent: true });
assert.deepEqual(absent.result, []); assert.equal(absent.state.pages, 0); assert.equal(absent.cell, false);
const empty = await run({ empty: true });
assert.deepEqual(empty.result, []); assert.equal(empty.state.pages, 1); assert.equal(empty.cell, true);
console.log("  ok  absent and complete-empty partitions remain distinct");

const narrowCatalog = vm.runInNewContext(cut(shared, "const publishedBoardRevisions", "function headlessNonaHelp(say")
  + "\npublishedBoardReader", { URLSearchParams, AbortSignal, AbortController });
const expectedCatalog = Array.from({ length: 65 }, (_, i) => ({ weapon: `weapon${String(i).padStart(2, "0")}`,
  ruler, revision, scene: "", rows: 1 }));
const asks = [];
const reader = narrowCatalog(async path => {
  const params = new URL(path, "https://wfsim.invalid").searchParams;
  const limit = params.get("limit"), after = params.get("after");
  asks.push([limit, after]);
  if (!limit) throw new DOMException("board transfer timed out", "TimeoutError");
  const page = expectedCatalog.filter(p => p.weapon + "/" + ruler > after).slice(0, Number(limit));
  return { ok: true, partitions: page, next: page.length === Number(limit) ? page.at(-1).weapon + "/" + ruler : null };
});
assert.deepEqual(JSON.parse(JSON.stringify(await reader.catalog("", null, undefined, ruler))), expectedCatalog);
assert.deepEqual(asks, [[null, ""], ["64", ""], ["64", expectedCatalog[63].weapon + "/" + ruler]]);
const env = { LIBRARY: { prepare(statement) {
  assert.match(statement, /\?::bigint/);
  return { bind(...args) {
    assert.deepEqual(args, [null, "", "", 64, ruler]);
    return { first: async () => ({ page: { ok: true, partitions: expectedCatalog } }) };
  } };
} } };
const limited = await boardPagesRoute(new Request(`https://wfsim.invalid/api/board/pages/catalog?ruler=${ruler}&limit=64`), env);
const limitedPage = await limited.json();
assert.equal(limited.status, 200);
assert.deepEqual(limitedPage.partitions, expectedCatalog.slice(0,64));
assert.equal(limitedPage.next, expectedCatalog[63].weapon + "/" + ruler);
assert.equal((await boardPagesRoute(new Request(`https://wfsim.invalid/api/board/pages/catalog?ruler=${ruler}&limit=513`), env)).status, 400);
console.log("  ok  a failed wide ranking transfer resumes at the same cursor with bounded pages");

const live = { partitions: new Map(), reads: 0, pages: [], draws: 0, now: 0, bench: false, failPage: false };
const document = { hidden: false, addEventListener(type, handler) { if (type === "visibilitychange") live.visible = handler; } };
const tab = vm.createContext({
  URLSearchParams, AbortController, AbortSignal, document, window: {}, META: {}, BOARD: {}, LIVE_ORIGIN: "",
  Date: { now: () => live.now }, setInterval(handler, ms) { live.tick = handler; live.interval = ms; },
  location: { pathname: "/weapons/Furis", search: "" }, finder: { weapon, b: ruler },
  benchList: () => [ruler, "other"].map(id => ({ id })), benchSceneOf: () => "", libraryDefaultBenchmark: () => ruler,
  localStorage: { getItem: k => k === "opened" ? JSON.stringify([{ b: "other" }]) : null }, boardActiveKey: () => "active", openedBoardKey: () => "opened",
  modSuffix: () => "build-library", routeWeaponId: () => weapon, boardRowsLanded: () => live.draws++,
  $: () => ({ hidden: !live.bench }), benchCurrent: () => ({ id: ruler }), renderBenchBoard: () => live.draws++,
  async fetchJsonPatient(path) {
    const url = new URL(path, "https://wfsim.invalid");
    if (url.pathname.endsWith("/catalog")) {
      live.reads++;
      const ruler = url.searchParams.get("ruler");
      if (live.bench) live.rulers.push(ruler);
      return { ok: true, next: null, partitions: [...live.partitions.values()].filter(p => !ruler || p.ruler === ruler).map(p => ({
        weapon, scene: "", ruler: p.ruler, revision: p.revision, rows: p.rows.length, leaders: p.rows.slice(0, 1)
      })) };
    }
    const p = live.partitions.get(url.searchParams.get("ruler"));
    assert.equal(url.searchParams.get("revision"), p.revision);
    live.pages.push(p.ruler);
    if (live.failPage) throw Error("temporary page failure");
    return { ok: true, weapon, ruler: p.ruler, revision: p.revision, after: 0, next: null, rows: p.rows, count: p.rows.length };
  },
});
vm.runInContext([loader, cut(source, "let BOARD_INDEX", "/// A scene is visible"),
  cut(source, "let boardIndexAsk", "/// The benchmark page:"),
  cut(source, "/// Visible public views check", "/// THE BOARD'S ROWS,")].join("\n"), tab);
assert.equal(typeof live.tick, "function", "an open library observes later publications");
const update = (r, rev, scores) => live.partitions.set(r, { ruler: r, revision: rev.repeat(64),
  rows: scores.map(score => ({ benchmark: r, score })) });
const scores = () => JSON.parse(vm.runInContext("JSON.stringify(BOARD.furis.map(r => r.score).sort((a,b)=>a-b))", tab));
const tick = async () => { live.now += live.interval; await live.tick(); };
update("other", "a", [5]);
await vm.runInContext("loadWeaponBoard('furis')", tab);
document.hidden = true;
await tick();
assert.equal(live.reads, 1, "a hidden library does not poll");
document.hidden = false;
await live.visible();
assert.equal(live.draws, 0);
assert.deepEqual(live.pages, ["other"]);
update(ruler, "b", [10]);
await tick();
assert.deepEqual(scores(), [5, 10], "a first qualified build appears in the open library");
update(ruler, "c", [31]);
live.failPage = true;
await tick();
assert.deepEqual(scores(), [5, 10], "a partial refresh preserves the prior complete partition");
live.failPage = false;
await tick();
assert.deepEqual(scores(), [5, 31], "a correction replaces the previous score");
const pages = live.pages.length, draws = live.draws;
await tick();
assert.equal(live.draws, draws);
assert.equal(live.pages.length, pages);
assert.equal(live.pages.filter(r => r === "other").length, 1, "unchanged partitions stay local");
live.bench = true;
live.rulers = [];
await tick();
assert.deepEqual(live.rulers, [ruler], "a visible ranking requests only the selected ruler");
const leader = () => vm.runInContext("benchIndexOf(benchCurrent()).index.furis.find(r => r.benchmark === 'standard_single_target').score", tab);
assert.equal(leader(), 31);
update(ruler, "d", [34]);
await tick();
assert.equal(leader(), 34, "the open ranking observes the same catalog");
vm.runInContext("BOARD_INDEX = { furis: [{ benchmark: 'standard_single_target', score: 34 }] }", tab);
live.rulers = [];
update(ruler, "e", [36]);
await tick();
assert.deepEqual(live.rulers, [ruler], "a previously loaded scene still refreshes only the selected ruler");
assert.equal(leader(), 36);
assert.equal(live.pages.length, pages, "leaders need only the catalog");
console.log("  ok  visible views follow new and updated partitions, retain complete rows on failure, and skip unchanged pages");
