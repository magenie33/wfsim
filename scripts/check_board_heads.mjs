// SPDX-License-Identifier: AGPL-3.0-or-later
// THE HEADS ANSWER AS THE WHOLE BOARD DOES — docs/BOARD.md §"The heads". Over
// the committed snapshot (`site/board/`, every scene), each weapon's rows and
// their `boardHeads` are asked the same `builder.board.read` at its deepest
// limit — every ruler, mode or modes pooled, riven-ness, distinct or not — and
// the card's "updated" time of each ruler, and must answer alike. Three cuts
// that are wrong on purpose must each be caught, or this check could not fail.
//
//   node scripts/check_board_heads.mjs [weapon ids]
import { readFileSync, readdirSync, existsSync, statSync } from "node:fs";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { availableParallelism } from "node:os";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
import { HEADLESS_QUERIES, boardHeads, rankBoard, boardMeasuredAt, BOARD_HEADS_PLACES } from "../mcp/headless.js";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const BOARD = join(ROOT, "site", "board");
const read = HEADLESS_QUERIES.find((q) => q.id === "builder.board.read");

const scenes = readdirSync(BOARD, { withFileTypes: true }).filter((e) => e.isDirectory() && e.name !== "heads").map((e) => e.name);
const ids = isMainThread ? (process.argv.slice(2).length ? process.argv.slice(2)
  : readdirSync(BOARD).filter((f) => f.endsWith(".json") && f !== "index.json" && f !== "meta.json").map((f) => f.slice(0, -5))) : [];
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

const empty = () => ({ visited: [], bad: 0, rows: 0, kept: 0, errors: [],
  caught: Object.fromEntries(Object.keys(broken).map((k) => [k, 0])) });
async function inspect(assignment) {
  const result = empty();
  for (const { id, index } of assignment) {
    result.visited.push({ id, index });
    const all = rowsOf(id);
    if (!all.length) continue;
    const heads = boardHeads(all);
    result.rows += all.length; result.kept += heads.length;
    const off = await differs(id, all, heads);
    if (off.length) {
      result.bad++;
      result.errors.push({ index, text: `FAIL  ${id}: the heads answer ${off.length} question(s) otherwise, e.g. ${off[0]}` });
    }
    for (const [k, cut] of Object.entries(broken)) if (!result.caught[k] && (await differs(id, all, cut(all))).length) result.caught[k] = 1;
  }
  return result;
}

function assignments(count) {
  const all = ids.map((id, index) => ({ id, index }));
  if (count === 1) return [all];
  const groups = Array.from({ length: count }, () => ({ load: 0, items: [] }));
  const weights = all.map(item => ({ ...item, size: ["", ...scenes].reduce((n, scene) => {
    const path = join(BOARD, scene, `${item.id}.json`);
    return n + (existsSync(path) ? statSync(path).size : 0);
  }, 1) })).sort((a, b) => b.size - a.size || a.index - b.index);
  for (const { size, ...item } of weights) {
    const group = groups.reduce((a, b) => a.load <= b.load ? a : b);
    group.items.push(item); group.load += size;
  }
  return groups.map(group => group.items.sort((a, b) => a.index - b.index));
}

async function parallel(groups) {
  const workers = groups.map(assignment => new Worker(new URL(import.meta.url), { workerData: { assignment },
    resourceLimits: { maxOldGenerationSizeMb: 512, maxYoungGenerationSizeMb: 32 } }));
  try {
    return await Promise.all(workers.map(worker => new Promise((accept, reject) => {
      let result, messages = 0;
      worker.on("message", value => {
        if (++messages !== 1) reject(Error("duplicate board-head worker result"));
        result = value;
      });
      worker.once("error", reject);
      worker.once("exit", code => {
        if (code !== 0 || messages !== 1) reject(Error(`board-head worker exited ${code} with ${messages} result(s)`));
        else accept(result);
      });
    })));
  } catch (error) {
    await Promise.allSettled(workers.map(worker => worker.terminate()));
    throw error;
  }
}

function combine(groups, results) {
  if (results.length !== groups.length) throw Error("missing board-head worker result");
  const whole = empty(), visited = new Set();
  for (const [i, result] of results.entries()) {
    const expected = new Map(groups[i].map(item => [item.index, item.id]));
    if (result.visited.length !== expected.size) throw Error("missing or duplicate board-head weapon visit");
    for (const { id, index } of result.visited) {
      if (visited.has(index)) throw Error("duplicate board-head weapon visit");
      if (!expected.has(index) || expected.get(index) !== id || ids[index] !== id) throw Error("unexpected board-head weapon visit");
      visited.add(index);
    }
    for (const key of ["bad", "rows", "kept"]) {
      if (!Number.isSafeInteger(result[key]) || result[key] < 0) throw Error(`invalid board-head worker ${key}`);
      whole[key] += result[key];
    }
    if (result.bad !== result.errors.length || result.kept > result.rows) throw Error("invalid board-head worker counts");
    whole.errors.push(...result.errors);
    for (const key of Object.keys(broken)) {
      if (![0, 1].includes(result.caught[key])) throw Error("missing board-head negative control");
      whole.caught[key] |= result.caught[key];
    }
  }
  if (visited.size !== ids.length) throw Error("missing board-head weapon visit");
  return whole;
}

if (!isMainThread) {
  parentPort.postMessage(await inspect(workerData.assignment));
} else {
  const requested = Number(process.env.WFSIM_BOARD_HEADS_WORKERS ?? Math.min(4, availableParallelism()));
  if (!Number.isInteger(requested) || requested < 1 || requested > 4) throw Error("board-head workers must be 1 through 4");
  const groups = assignments(Math.max(1, Math.min(requested, ids.length)));
  const result = combine(groups, groups.length === 1 ? [await inspect(groups[0])] : await parallel(groups));
  for (const error of result.errors.sort((a, b) => a.index - b.index)) console.log(error.text);
  for (const [k, n] of Object.entries(result.caught)) {
    if (!n) { result.bad++; console.log(`FAIL  a cut with the ${k} passed: this check cannot see that`); }
    else console.log(`ok    a cut with the ${k} is caught`);
  }
  console.log(result.bad ? `\n${result.bad} failed` : `\nthe heads answer as the board does: ${ids.length} weapons, ${result.kept} of ${result.rows} rows kept`);
  process.exitCode = result.bad ? 1 : 0;
}
