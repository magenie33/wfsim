// A SUBMISSION SURVIVES THE WHOLE HOP, and every hop here is the real one.
//
//   the PAGE's own `boardPayload()`, in a browser
//     -> `worker/index.js`, the real door, against a stub database
//       -> the inbox row it stores, verbatim
//         -> `wfsim-intake`, the real binary
//           -> the build that would land in the library
//
// Only the network and D1 are stubbed. Nothing about what a build IS is: three
// separate programs in two languages have to agree, and the failures this
// refuses are the ones where they quietly stop agreeing.
//
// WHY A CHAIN RATHER THAN THREE UNIT TESTS. Each end has its own — the page's
// payload against the door's axes (`check_board_submit`), the door's shape
// rules, intake's own six — and all of them passed on the day `mode` was in the
// door's key, because no test looked at what came out the far end. What a
// player places and what the library holds is one question, and this is the
// only place it is asked.
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import worker from "../worker/index.js";
import { openApp } from "./cdp.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const INTAKE = ["release", "debug"]
  .map((p) => resolve(ROOT, "target", p, process.platform === "win32" ? "wfsim-intake.exe" : "wfsim-intake"))
  .find(existsSync);

// A MISSING BINARY IS A FAILURE, not a skip. A check that quietly passes when
// the thing it checks is absent is the shape this file exists to refuse.
if (!INTAKE) {
  console.log("FAIL  wfsim-intake is not built — `cargo build --release --bin wfsim-intake`");
  process.exit(1);
}

// THE ELEMENTS ARE SCATTERED AMONG THE PLAIN CARDS, which is the whole point: a
// player places them where the slots are, and what makes two arrangements one
// build is the PAIRING they pool into, not where the cards sit.
const PLACED = [
  "serration", "hellfire", "split_chamber", "malignant_force",
  "point_strike", "primed_cryo_rounds", "vital_sense", "stormbringer",
];

const app = await openApp({ boot: 20000 });
const { check } = app;

const seen = await app.evaluate(`(async () => {
  const s = (ms) => new Promise(r => setTimeout(r, ms));
  history.pushState({}, '', '/weapons/Braton_Prime'); route(); await s(4000);
  const want = ${JSON.stringify(PLACED)};
  for (let i = 0; i < want.length; i++) {
    const slot = mainSlots()[i];
    if (slot) slot.mod = want[i];
  }
  try { refreshPanel(); } catch (_) {}
  return { placed: mainSlots().filter(x => x.mod).map(x => x.mod), payload: boardPayload() };
})()`);

check("the page places the build a player asked for",
  JSON.stringify(seen.placed) === JSON.stringify(PLACED), seen.placed.join(","));
check("...and sends the mods in that order, untouched",
  JSON.stringify(seen.payload.mods) === JSON.stringify(PLACED), seen.payload.mods.join(","));
// THE SHAPE AND NOT THE NUMBERS. A player's own rolls never leave the browser,
// which is why the board does not rank luck and why intake has to resolve the
// corner itself.
check("...and a riven travels as a shape, never as rolls",
  !("riven_rolls" in seen.payload), JSON.stringify(Object.keys(seen.payload)));

const stored = new Map();
const db = {
  prepare: (sql) => ({
    bind: (...args) => ({
      run: async () => {
        const into = sql.match(/INTO ([a-z_]+)/);
        if (into) stored.set(args[0], { table: into[1], at: args[1], record: args[2] });
        return { success: true };
      },
    }),
    first: async () => ({ n: stored.size }),
  }),
};
const res = await worker.fetch(
  new Request("https://wfsim.app/api/board/submit", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(seen.payload),
  }),
  { LIBRARY: db, ASSETS: { fetch: async () => new Response("site") } },
);
const [queueId, row] = [...stored.entries()][0] || [];
check("the door takes it", res.status === 200, String(res.status));
check("...into the queue, under an id that says nothing about the build",
  row && row.table === "inbox" && /^[0-9a-f-]{36}$/i.test(queueId), `${row && row.table} ${queueId}`);
// VERBATIM. The door has no game data, so anything it decided about the build
// would be a second answer to a question it cannot see the evidence for.
check("...storing what arrived, unchanged",
  row && JSON.stringify(JSON.parse(row.record).mods) === JSON.stringify(PLACED),
  row && row.record);

const line = JSON.stringify({ id: queueId, at: row.at, record: JSON.parse(row.record) });
const built = execFileSync(INTAKE, { input: `${line}\n`, encoding: "utf8" })
  .trim().split("\n").filter(Boolean).map((l) => JSON.parse(l));
check("intake makes one build of it", built.length === 1, String(built.length));

const b = built[0] || { record: {} };
check("...with an id derived from the build, not allocated",
  /^[0-9a-f]{32}$/.test(b.id || ""), String(b.id));
// THE MODE DOES NOT SURVIVE, and it must not: mods are equipped on the WEAPON
// and a mode is how it is fired, so one submission is already scored in every
// mode the weapon sustains. It was in the door's key once, and the same cards
// sent from two modes were two rows in a table whose promise is one per build.
check("...and no mode, because a mode is not part of a build",
  !("mode" in b.record), JSON.stringify(b.record.mode));
check("...naming the queue row it spends", (b.from || []).includes(queueId), (b.from || []).join(","));

// THE PAIRING IS WHAT SURVIVES. The order changes — that is what
// canonicalisation is — and what may not change is which elements combine with
// which, because moving one across a boundary re-pairs everything after it.
const ELEMENT = { hellfire: "heat", malignant_force: "toxin", primed_cryo_rounds: "cold", stormbringer: "electricity" };
const pairsOf = (mods) => {
  const seq = [];
  for (const m of mods) {
    const e = ELEMENT[m];
    if (e && !seq.includes(e)) seq.push(e);
  }
  const pairs = [];
  for (let i = 0; i + 1 < seq.length; i += 2) pairs.push([seq[i], seq[i + 1]].sort().join("+"));
  return pairs.sort().join("  ");
};
check("...and the pairing the player built is the pairing that is stored",
  pairsOf(b.record.mods || []) === pairsOf(PLACED),
  `${pairsOf(PLACED)}  ->  ${pairsOf(b.record.mods || [])}`);
check("...while the order is the canonical one, not the one placed",
  JSON.stringify(b.record.mods) !== JSON.stringify(PLACED), (b.record.mods || []).join(","));

await app.finish("a submission survives the hop into the library");
