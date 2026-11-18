// SPDX-License-Identifier: AGPL-3.0-or-later
// node scripts/check_riven_branch_analyst.mjs [baseline git ref]
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
const app = "web/src/static/app/";
const file = app + "21-riven-analyst.js";
const baseline = process.argv[2] ? execFileSync("git", ["show", `${process.argv[2]}:${file}`], {
  cwd: new URL("..", import.meta.url), encoding: "utf8",
}) : read(file);
const leaders = read(app + "40-official-builds.js").split("function boardGroupLeaders(rows) {")[1].split("\n}")[0];
const metaFile = read("site/index.html").match(/meta\.[a-f0-9]+\.json/)[0];
const meta = JSON.parse(read(`site/pkg/${metaFile}`));
const rowsOf = (id, dir = "") => JSON.parse(read(`site/board/${dir}${id}.json`));

function surface(source, id, forceCold = false) {
  let links;
  const box = { querySelectorAll(selector) {
    if (selector !== "[data-ra]") return [];
    links = [...this.innerHTML.matchAll(/data-ra="([^"]+)"/g)].map(([, ra]) => ({
      dataset: { ra }, addEventListener(_, fn) { this.click = fn; },
    }));
    return links;
  } };
  const weapon = { value: id };
  const board = { [id]: rowsOf(id) };
  const calls = { depth: 0, rank: 0 };
  const ctx = {
    META: meta, BOARD: board, boardDepth: 0,
    $: (key) => ({ "riven-published": box, weapon })[key],
    weaponInfo: (key) => meta.weapons.find((w) => w.id === key),
    libraryDefaultBenchmark: () => meta.benchmarks[0].id,
    benchList: () => meta.benchmarks,
    ensureBoardCell: () => true,
    benchSelectorHtml: (id) => `selector:${id}`,
    wireBenchSelector: () => {},
    escHtml: String, tr: String, modeLabel: (_, mode) => mode, gainPct: String,
    measuredRecordHtml: (r) => r.measured_at || "", measuredNote: () => "measured",
    weaponPath: (key) => `/weapons/${key}`, benchmarkName: String,
    cardOfState: JSON.stringify, boardRowState: (_, row) => row,
    rowHasRiven: (r) => !!r?.riven,
    rivenPoolAll: () => meta.riven_stats[ctx.weaponInfo(weapon.value).riven_class] || [],
    rivenStat: (stat) => ctx.rivenPoolAll().find((s) => s.id === stat), rivenStatName: (s) => s.id,
    countDepth: () => calls.depth++, countRank: () => calls.rank++,
  };
  const render = runInNewContext([
    read(app + "87-headless.js").split("const HEADLESS_QUERIES =")[0], read(app + "92-riven-input.js"),
    `function boardGroupLeaders(rows) { countDepth(); ${leaders}\n}`,
    "const originalRank = rankBoard; rankBoard = (...args) => { countRank(); return originalRank(...args); };",
    source,
    forceCold ? "const draw = renderPublishedRivens; renderPublishedRivens = () => { raRankMemo = null; return draw(); };" : "",
    "renderPublishedRivens",
  ].join("\n"), ctx);
  return { box, board, weapon, calls, ctx, render, click: (i) => links[i].click(), count: () => links.length };
}

let checks = 0, saved = 0;
for (const id of ["torid", "laetum", "furis"]) {
  const a = surface(baseline, id, !process.argv[2]), b = surface(read(file), id);
  const same = (act) => {
    for (const s of [a, b]) act(s);
    assert.equal(b.box.innerHTML, a.box.innerHTML, id);
    checks++;
  };
  same((s) => s.render());
  assert.ok(a.count() >= 2, `${id} must have different rows to select`);
  for (const i of [1, 0, 1, 0, 1]) same((s) => s.click(i));
  assert.deepEqual(a.calls, { depth: 6, rank: 6 });
  assert.deepEqual(b.calls, { depth: 1, rank: 1 });
  saved += a.calls.rank - b.calls.rank;
  same((s) => { s.ctx.boardDepth = 1; s.render(); });
  same((s) => { s.board[id] = rowsOf(id).slice(1); s.render(); });
  same((s) => { s.ctx.META = { ...meta }; s.render(); });
  same((s) => { s.weapon.value = "torid" === id ? "laetum" : "torid"; s.render(); });
  assert.deepEqual(b.calls, { depth: 5, rank: 5 }, `${id}: each changed input must rerank`);
}
console.log(`${checks} identical Analyst renders; saved ${saved} depth scans and ${saved} full rank calls`);
