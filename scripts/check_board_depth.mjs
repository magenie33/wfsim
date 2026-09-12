// HOW DEEP THE BOARD IS READ is the READER's choice, and this is what holds it.
//
// The publisher no longer holds rows back: every scored row is published and
// the page shows builds within half their group's leader by default
// (docs/BOARD.md). That moves a rule from a place with unit tests to a place
// without, so it is checked here, on the real page:
//
//   * the default is half, and it is INCLUSIVE — a row exactly on the line
//     shows, because that is the row a reader goes looking for;
//   * "All" shows every row the board holds;
//   * the line is drawn PER GROUP — one ruler, one mode, one riven-ness — so a
//     strong group cannot empty a weak one;
//   * a group whose leader scored ZERO is never emptied;
//   * `#1` still means `#1`: filtering takes a prefix, so the ranks a reader
//     sees do not move when the depth does;
//   * the choice survives a reload.
//
// ROWS ARE INJECTED rather than read off the published board, because the
// published board is whatever the scoring bot last wrote — a fixture is the
// only way to ask about a row at exactly half, or a group that scored nothing.
//
// Usage:
//   node scripts/check_board_depth.mjs                  serves site/
//   node scripts/check_board_depth.mjs http://host:port  against a running server
import { openApp } from "./cdp.mjs";

const where = process.argv[2];
const app = await openApp(where ? { base: where, boot: 13000 } : { boot: 15000 });
const { evaluate, check, finish, sleep } = app;

// One weapon, four groups, chosen so every rule above has a row that proves it.
const FIXTURE = `(() => {
  const W = "braton_prime";
  const row = (bench, mode, riven, score, id) => ({
    benchmark: bench, mode, score, id,
    mods: [], arcanes: [], evolutions: [], exilus: "", valence: "",
    riven: riven ? { bonuses: ["damage"], malus: null, rolls: [1.1] } : null,
  });
  BOARD[W] = [
    // plain / single_target / base — leader 100, one row exactly on the line
    row("single_target", "base", false, 100, "p100"),
    row("single_target", "base", false, 50, "p50"),    // exactly half
    row("single_target", "base", false, 49.9, "p49"),  // a hair under
    row("single_target", "base", false, 10, "p10"),
    // riven rows of the SAME ruler and mode: a group of their own
    row("single_target", "base", true, 400, "r400"),
    row("single_target", "base", true, 210, "r210"),   // 52% of its own leader
    // …and a group that scored nothing at all
    row("group_clear", "base", false, 0, "z1"),
    row("group_clear", "base", false, 0, "z2"),
  ];
  builtinMemo = null;
  return BOARD[W].length;
})()`;

await evaluate(`switchWeapon("braton_prime")`);
await sleep(2500);
const n = await evaluate(FIXTURE);
check("the fixture is in place", n === 8, `got ${n} rows`);

// Which rows survive, by score, at each depth.
const at = (d) => evaluate(`(() => {
  setBoardDepth(${d});
  builtinMemo = null;
  return builtinBuilds().map((b) => b.board.score).sort((a, b) => b - a).join(",");
})()`);

const half = await at(0.5);
check("half is the default line, inclusive of a row exactly on it",
  half === "400,210,100,50,0,0", half);
check("…so the row a hair under is the one that goes",
  !half.split(",").includes("49.9"), half);
check("a group whose leader scored nothing keeps every row",
  half.split(",").filter((x) => x === "0").length === 2, half);

const all = await at(0);
check("All shows every row the board holds", all === "400,210,100,50,49.9,10,0,0", all);

const quarter = await at(0.25);
check("a quarter keeps the 49.9 and still drops the 10",
  quarter === "400,210,100,50,49.9,0,0", quarter);

// PER GROUP: the riven leader is 400 and the plain one 100. Under a shared
// reference every plain row would be gone at half.
check("the line is per group, so the riven leader does not empty the plain list",
  half.includes("100") && half.includes("50"), half);

// The ranks a reader sees must not move with the depth.
const ranksAt = (d) => evaluate(`(() => {
  setBoardDepth(${d});
  builtinMemo = null;
  return builtinBuilds().filter((b) => !b.riven && b.benchmark === "single_target")
    .map((b) => b.rank + ":" + b.board.score).join(",");
})()`);
const rHalf = await ranksAt(0.5);
const rAll = await ranksAt(0);
check("#1 still means #1 — the shown ranks do not move with the depth",
  rAll.startsWith(rHalf), `half=${rHalf} all=${rAll}`);

// …and the choice is the reader's, so it outlives the page.
await evaluate(`setBoardDepth(0)`);
await app.load("/weapons/Braton_Prime");
await sleep(3000);
const kept = await evaluate(`String(boardDepth)`);
check("the depth survives a reload", kept === "0", kept);

await finish();
