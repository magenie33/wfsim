// SPDX-License-Identifier: AGPL-3.0-or-later
// THE HEADS ANSWER AS THE WHOLE BOARD DOES — docs/BOARD.md §"The heads". Over
// the committed snapshot (`site/board/`, every scene), each weapon's rows and
// their `boardHeads` are asked the same `builder.board.read` at its deepest
// limit — every ruler, mode or modes pooled, riven-ness, distinct or not — and
// the card's "updated" time of each ruler, and must answer alike. Three cuts
// that are wrong on purpose must each be caught, or this check could not fail.
//
//   node scripts/check_board_heads.mjs [weapon ids]
import { readFileSync, readdirSync, existsSync } from "node:fs";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { HEADLESS_QUERIES, boardHeads, rankBoard, boardMeasuredAt, BOARD_HEADS_PLACES } from "../mcp/headless.js";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const BOARD = join(ROOT, "site", "board");
const read = HEADLESS_QUERIES.find((q) => q.id === "builder.board.read");

const scenes = readdirSync(BOARD, { withFileTypes: true }).filter((e) => e.isDirectory() && e.name !== "heads").map((e) => e.name);
const ids = process.argv.slice(2).length ? process.argv.slice(2)
  : readdirSync(BOARD).filter((f) => f.endsWith(".json") && f !== "index.json" && f !== "meta.json").map((f) => f.slice(0, -5));
const rowsOf = (id) => ["", ...scenes].flatMap((s) => {
  const p = join(BOARD, s, `${id}.json`);
  return existsSync(p) ? JSON.parse(readFileSync(p, "utf8")) : [];
});

/// What a reader can ask of `rows`, as one string per question.
async function answers(id, rows) {
  const benches = [...new Set(rows.map((r) => r.benchmark))];
  const modes = [...new Set(rows.map((r) => r.mode || "base"))];
  const w = { id, name: id, modes, evolutions: [] };
  const host = { meta: () => ({ weapons: [w], benchmarks: benches.map((b) => ({ id: b, name: b })) }), board: async () => rows,
    tr: (s) => s, origin: "", screen_weapon: () => null };
  const out = [];
  for (const riven of ["without", "with", "any"]) for (const mode of [undefined, ...modes])
    for (const pooled of [false, true]) for (const distinct of [false, true]) {
      const args = { weapon: id, riven, limit: BOARD_HEADS_PLACES, pooled, distinct, ...(mode ? { mode } : {}) };
      out.push([JSON.stringify(args), JSON.stringify(await read.run(args, host))]);
    }
  for (const b of benches) out.push([`updated ${b}`, boardMeasuredAt(rows.filter((r) => r.benchmark === b))]);
  return out;
}

async function differs(id, rows, cut) {
  const want = await answers(id, rows), got = await answers(id, cut);
  return want.filter(([, a], i) => a !== got[i][1]).map(([q]) => q);
}

/// WRONG ON PURPOSE: one place short; one build of every tie left out; the
/// newest row left out.
const top = (rows) => rankBoard(rows, [], []).filter((x) => x.rank <= BOARD_HEADS_PLACES);
const broken = {
  "one place short": (rows) => boardHeads(rows, BOARD_HEADS_PLACES - 1),
  "tie cut": (rows) => {
    const seen = new Set(), drop = new Set();
    for (const x of top(rows)) {
      const place = `${x.key.slice(0, x.key.lastIndexOf("#"))}#${x.rank}`;
      if (seen.has(place)) drop.add(x.row); else seen.add(place);
    }
    return boardHeads(rows).filter((r) => !drop.has(r));
  },
  "newest left out": (rows) => {
    const kept = new Set(top(rows).map((x) => x.row));
    return boardHeads(rows).filter((r) => kept.has(r));
  },
};

let bad = 0, rows = 0, kept = 0;
const caught = Object.fromEntries(Object.keys(broken).map((k) => [k, 0]));
for (const id of ids) {
  const all = rowsOf(id);
  if (!all.length) continue;
  const heads = boardHeads(all);
  rows += all.length; kept += heads.length;
  const off = await differs(id, all, heads);
  if (off.length) { bad++; console.log(`FAIL  ${id}: the heads answer ${off.length} question(s) otherwise, e.g. ${off[0]}`); }
  for (const [k, cut] of Object.entries(broken)) if (!caught[k] && (await differs(id, all, cut(all))).length) caught[k] = 1;
}
for (const [k, n] of Object.entries(caught)) {
  if (!n) { bad++; console.log(`FAIL  a cut with the ${k} passed: this check cannot see that`); }
  else console.log(`ok    a cut with the ${k} is caught`);
}
console.log(bad ? `\n${bad} failed` : `\nthe heads answer as the board does: ${ids.length} weapons, ${kept} of ${rows} rows kept`);
process.exit(bad ? 1 : 0);
