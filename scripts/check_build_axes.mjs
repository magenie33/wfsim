// WHAT A BUILD CONSISTS OF IS DECLARED ONCE, AND EVERY SURFACE ANSWERS TO IT.
//
// The THIRTY-FIRST check, and the cheap half of a pair. Its partner,
// `check_opt_replay`, asserts the ANSWER — a ranked row re-run in the simulator
// comes back with its own number — and that one can never go stale because it
// holds no list. This one holds the list, and exists for the surfaces an answer
// cannot reach: a share link nobody has clicked yet, a board record nobody has
// submitted yet.
//
// `engine::builds::BUILD_AXES` is the single declaration and arrives in
// `/api/meta.build_axes`. Three JS surfaces carry their own SPELLINGS of those
// axes, because each protocol names them differently and renaming would migrate
// every stored preset:
//
//   * the page's build state   (`BUILD_STATE_KEYS` in app.js)
//   * the share tuple          (`SHARE_AXES` in app.js)
//   * the board record         (`AXES` in worker/index.js, `axis:` per row)
//
// Each of those had, at some point, an axis the others had — and the one that
// did not dropped it in silence, because a missing field and a defaulted field
// are the same absence on the wire. Four times: `mode` from the board
// submission, `valence` from the worker's table, both
// from the share tuple, `valence` from the optimizer's "+ add".
//
// So the assertion is COVERAGE, in one direction only: every axis the engine
// declares must be claimed by every surface that is supposed to carry it. It
// runs in plain node against the served meta and the two source files — no
// browser — so it can sit in CI beside the parity and board-submit checks
// rather than being something to remember.
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { openApp } from "./cdp.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => readFileSync(resolve(ROOT, p), "utf8");

// THE ENGINE'S OWN LIST, taken from the SHIPPING build's `/api/meta` rather
// than by reading the Rust — the page and the worker are held against what
// actually reaches a browser, which is the only copy that can be wrong in a way
// a user sees.
const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const meta = await evaluate(`(async () => {
  const m = await api('/api/meta', {});
  return { axes: m.build_axes || null };
})()`);

check("the engine declares what a build consists of, and serves it",
  Array.isArray(meta.axes) && meta.axes.length >= 5,
  JSON.stringify(meta.axes));
if (!Array.isArray(meta.axes) || !meta.axes.length) {
  await app.finish("a build's axes are declared once");
}

const ALL = meta.axes.map((a) => a.id);
const ON_BOARD = meta.axes.filter((a) => a.on_board).map((a) => a.id);
// Two facts worth stating out loud rather than leaving to a reader: the list is
// not empty of either kind. A table where nothing is `on_board` would make the
// worker's coverage assertion below pass vacuously.
check("...with axes on both sides of the board line",
  ON_BOARD.length >= 3 && ON_BOARD.length < ALL.length,
  `${ON_BOARD.length} of ${ALL.length} on the board: ${ON_BOARD.join(",")}`);

/// The `axis:`/`{ axis: "…" }` ids a source file claims, by literal text. Read
/// out of the file the same way `check_board_submit` reads `boardPayload()` —
/// a claim in the source is what the next person edits, so the source is what
/// this measures.
const claimed = (src, re) => [...src.matchAll(re)].map((m) => m[1]);

const appJs = read("web/src/static/app.js");
const workerJs = read("worker/index.js");

// ---- 1. THE PAGE'S BUILD STATE ----------------------------------------
const stateBlock = appJs.match(/const BUILD_STATE_KEYS = \[([\s\S]*?)\n\];/);
check("the page's build state declares which axis each of its keys carries",
  !!stateBlock, "BUILD_STATE_KEYS not found in app.js");
if (stateBlock) {
  const have = claimed(stateBlock[1], /axis:\s*"([a-z_]+)"/g);
  const missing = ALL.filter((a) => !have.includes(a));
  const unknown = have.filter((a) => !ALL.includes(a));
  check("...and it covers every axis the engine declares",
    missing.length === 0, `missing: ${missing.join(", ")}`);
  // The other direction too: an id here that the engine has never heard of is
  // a rename that happened on one side, which reads as coverage and is not.
  check("...and claims none the engine has never heard of",
    unknown.length === 0, `unknown: ${unknown.join(", ")}`);
}

// ---- 2. THE SHARE TUPLE -----------------------------------------------
const shareBlock = appJs.match(/const SHARE_AXES = \[([\s\S]*?)\];/);
check("the share tuple declares the axes it carries", !!shareBlock,
  "SHARE_AXES not found in app.js");
if (shareBlock) {
  const have = claimed(shareBlock[1], /"([a-z_]+)"/g);
  const missing = ALL.filter((a) => !have.includes(a));
  check("...and a link carries every axis a build has",
    missing.length === 0, `missing: ${missing.join(", ")}`);
}

// ---- 3. THE BOARD RECORD ----------------------------------------------
const axesBlock = workerJs.match(/export const AXES = \[([\s\S]*?)\n\];/);
check("the worker's record table names the axes its rows carry", !!axesBlock,
  "AXES not found in worker/index.js");
if (axesBlock) {
  const have = claimed(axesBlock[1], /axis:\s*"([a-z_]+)"/g);
  const missing = ON_BOARD.filter((a) => !have.includes(a));
  const unknown = have.filter((a) => !ALL.includes(a));
  // ONLY the ones a ruler records. Mod ranks and rivens are deliberately not
  // on a board row — every row is scored at full investment, and a riven is an
  // item that exists on one machine — so demanding them here would be wrong.
  check("...and covers every axis a board row is supposed to keep",
    missing.length === 0, `missing: ${missing.join(", ")}`);
  check("...and claims none the engine has never heard of",
    unknown.length === 0, `unknown: ${unknown.join(", ")}`);
  // AND THE STORED RECORD IS DERIVED FROM THAT TABLE, which is what makes the
  // coverage above worth anything: a row named here and copied nowhere is a
  // field that validates and is never written, which is exactly how `mode` and
  // `valence` were lost. `check_board_submit` proves the derivation end to end
  // against a KV stub; this asserts the shape it depends on has not been
  // unpicked into hand-written lists again.
  // ASSERTED AS A PROPERTY, NOT AS A SPELLING. This used to require the literal
  // `AXES.map((a) =>`, and the day `benchmark` became provenance rather than
  // identity — `AXES.filter((a) => a.identity !== false).map(…)` — the check
  // went red on code that was still entirely derived. A check that
  // fails on a refactor it should not care about is a check people learn to
  // edit rather than to read.
  //
  // What actually matters is two things: the record loop walks `AXES`, and the
  // identity key is built FROM `AXES` rather than from a list of names typed
  // out again. The second half is what was lost twice.
  const idAt = workerJs.indexOf("const identity =");
  const idBody = idAt < 0 ? "" : workerJs.slice(idAt, workerJs.indexOf("async function", idAt));
  const loops = /for \(const a of AXES\)/.test(workerJs);
  const fromAxes = /AXES/.test(idBody);
  const noList = !/["'](benchmark|weapon|mods|evolutions|arcanes)["']\s*,/.test(idBody);
  check("...and the stored record and identity key are still derived from it",
    loops && fromAxes && noList,
    idAt < 0
      ? "no `identity` in worker/index.js"
      : `record loop ${loops}, identity reads AXES ${fromAxes}, no name list ${noList}`);
}

// ---- 4. THE ONE THAT CANNOT GO STALE, NAMED ----------------------------
// A list is only ever as good as the person who remembers it, so the product
// does not rest on this check — it rests on `check_opt_replay`, which compares
// the simulator's answer with the search's and needs no list at all. Asserting
// the file exists is not ceremony: this check would otherwise read as the whole
// guarantee, and it is the weaker half.
check("...and the answer-side guard it leans on is still here",
  /check_opt_replay/.test(read("scripts/check_opt_replay.mjs").slice(0, 200))
    || read("scripts/check_opt_replay.mjs").length > 0,
  "scripts/check_opt_replay.mjs is missing — the list is not the guarantee");

await app.finish("a build's axes are declared once, and every surface answers to it");
