// SPDX-License-Identifier: AGPL-3.0-or-later
// THE LIVE BOARD'S TWO HOPS TO THE SITE — docs/BOARD.md §"The live board". Run
// by `live_board.sh`.
//
//   node live_publish.mjs delta <work-dir>                verified facts since the last read
//   node live_publish.mjs cursor <work-dir>               the facts file just read whole: the delta starts at its newest
//   node live_publish.mjs subset <work-dir> <ids> <out> <facts>...   the weapons <ids> touch: library, facts
//   node live_publish.mjs open <work-dir> <results> <out>   the client results no fact answers
//   node live_publish.mjs push <work-dir> <dir>           the files that moved, to R2
//
// `delta` appends to `facts-known.ndjson` every `scores` row finished in the
// last two hours of what it has seen that moved: a scorer ships a row minutes
// after it finishes, so a cursor on the clock alone would step over it. Exit 3
// means nothing moved. `push` sends a file only when its bytes differ from the
// last push (`pushed.json`), through the worker's `/api/board/live/`.
//
// THE FACTS FILE IS NEVER HELD WHOLE: it is every row of `scores`, past what
// one string can be and past what the board server's memory can hold as a map.
// It is streamed a line at a time, and appended to rather than rewritten —
// `wfsim-board` keeps the newest line of a key (`load_facts`), and the hourly
// read writes the file afresh.
import { readFileSync, writeFileSync, appendFileSync, existsSync, readdirSync, createReadStream } from "node:fs";
import { createInterface } from "node:readline";
import { createHash } from "node:crypto";
import { join } from "node:path";
import { query, close } from "./db.mjs";

const OVERLAP_SECONDS = 2 * 3600;
const SITE = process.env.WFSIM_SITE || "https://wfsim.app";
const [mode, work, dir] = process.argv.slice(2);

const key = (f) => `${f.identity}|${f.ruler}|${f.mode}`;

/// Each non-empty line of a file, read a line at a time; none when it is missing.
async function* eachLine(p) {
  if (!existsSync(p)) return;
  for await (const l of createInterface({ input: createReadStream(p), crlfDelay: Infinity })) if (l) yield l;
}
/// A FACT LINE'S KEY without parsing the line: every writer of the file puts
/// identity, ruler and mode first, in that order (`fetch_facts.sh`, `delta`).
const HEAD = /^\{"identity":"([^"]*)","ruler":"([^"]*)","mode":"([^"]*)"/;
const factHead = (l) => { const m = HEAD.exec(l); if (m) return { identity: m[1], ruler: m[2], mode: m[3] }; const f = JSON.parse(l); return f; };

async function delta() {
  const factsPath = join(work, "facts-known.ndjson");
  const cursorPath = join(work, "facts-cursor.txt");
  const recentPath = join(work, "facts-recent.json");
  let seen = existsSync(cursorPath) ? readFileSync(cursorPath, "utf8").trim() : "";
  if (!seen) for await (const l of eachLine(factsPath)) { const at = JSON.parse(l).finished_at; if (at > seen) seen = at; }
  const since = new Date(Date.parse(seen || "1970-01-01T00:00:00Z") - OVERLAP_SECONDS * 1000).toISOString();
  const rows = await query(`SELECT identity, ruler, mode, measured_by, score, cost_seconds, started_at, finished_at
                         FROM scores WHERE finished_at > ? ORDER BY finished_at`, [since]);
  await close();
  // A ROW MOVED IS ONE THE LAST WINDOW DID NOT HOLD AS IT IS NOW — the window
  // overlaps the next, so a row unchanged since is not a move. Compared as
  // values: two spellings of one fact are not a change.
  const recent = existsSync(recentPath) ? JSON.parse(readFileSync(recentPath, "utf8")) : {};
  const same = (a, r) => a && a[0] === r.score && a[1] === r.measured_by && a[2] === r.finished_at;
  const window = {}, moved = [], movedIds = [];
  for (const r of rows) {
    if (!same(recent[key(r)], r)) { moved.push(JSON.stringify(r)); movedIds.push(r.identity); }
    window[key(r)] = [r.score, r.measured_by, r.finished_at];
    if (r.finished_at > seen) seen = r.finished_at;
  }
  if (moved.length) appendFileSync(factsPath, moved.join("\n") + "\n");
  writeFileSync(recentPath, JSON.stringify(window));
  writeFileSync(cursorPath, seen + "\n");
  writeFileSync(join(work, "moved-ids.txt"), movedIds.map((id) => id + "\n").join(""));
  if (!moved.length) process.exit(3);
  console.error(`live: ${moved.length} fact(s) moved since ${since}`);
}

/// THE CURSOR MOVED TO THE NEWEST ROW OF A FILE JUST READ WHOLE, and the last
/// window forgotten: the store holds everything before it, so a delta asks for
/// hours rather than every row since a cursor that stopped — a cursor days
/// old asked for millions of rows, more than the server's heap holds.
async function cursor() {
  let seen = "";
  for await (const l of eachLine(join(work, "facts-known.ndjson"))) {
    const at = /"finished_at":"([^"]*)"/.exec(l)?.[1] ?? "";
    if (at > seen) seen = at;
  }
  if (!seen) return;
  writeFileSync(join(work, "facts-cursor.txt"), seen + "\n");
  writeFileSync(join(work, "facts-recent.json"), "{}");
}

/// THE BUILDS OF EVERY WEAPON A SET OF BUILD IDS TOUCHES — a weapon is ranked
/// whole or not at all (`wfsim-board --subset`). Exit 3 when it names none.
async function subset(idsPath, out, factsIn) {
  const want = new Set();
  for await (const l of eachLine(idsPath)) want.add(l);
  const weaponOf = new Map();
  for await (const l of eachLine(join(work, "library-ids.ndjson"))) {
    const x = JSON.parse(l);
    if (want.has(x.k)) weaponOf.set(x.k, x.v.weapon);
  }
  for await (const l of eachLine(join(work, "new-builds.ndjson"))) {
    const x = JSON.parse(l);
    if (want.has(x.id)) weaponOf.set(x.id, x.record.weapon);
  }
  const weapons = new Set(weaponOf.values());
  if (!weapons.size) process.exit(3);
  const lib = JSON.parse(readFileSync(join(work, "library-live.json"), "utf8")).filter((r) => weapons.has(r.weapon));
  writeFileSync(out, JSON.stringify(lib));
  // …AND ONLY THOSE WEAPONS' FACTS: reading every row of `scores` three times
  // is most of what a subset pass costs.
  const ids = new Set();
  for await (const l of eachLine(join(work, "library-ids.ndjson"))) { const x = JSON.parse(l); if (weapons.has(x.v.weapon)) ids.add(x.k); }
  for await (const l of eachLine(join(work, "new-builds.ndjson"))) { const x = JSON.parse(l); if (weapons.has(x.record.weapon)) ids.add(x.id); }
  writeFileSync(`${out}.facts`, "");
  let kept = [];
  for (const p of factsIn) {
    for await (const l of eachLine(p)) {
      if (!ids.has(factHead(l).identity)) continue;
      kept.push(l);
      if (kept.length >= 10_000) { appendFileSync(`${out}.facts`, kept.join("\n") + "\n"); kept = []; }
    }
  }
  if (kept.length) appendFileSync(`${out}.facts`, kept.join("\n") + "\n");
  console.error(`live: re-ranking ${weapons.size} weapon(s), ${lib.length} build(s)`);
}

/// THE CLIENT RESULTS NO FACT HAS ANSWERED YET, out of `results` — a few
/// hundred lines, held — against the facts file streamed once.
async function open(resultsPath, out) {
  const waiting = new Map();
  for await (const l of eachLine(resultsPath)) waiting.set(key(JSON.parse(l)), l);
  for await (const l of eachLine(join(work, "facts-known.ndjson"))) {
    if (!waiting.size) break;
    waiting.delete(key(factHead(l)));
  }
  writeFileSync(out, [...waiting.values()].map((l) => l + "\n").join(""));
}

async function push() {
  const ledgerPath = join(work, "pushed.json");
  const ledger = existsSync(ledgerPath) ? JSON.parse(readFileSync(ledgerPath, "utf8")) : {};
  let sent = 0;
  // THE STAMP GOES LAST, so a reader who sees a new `meta.json` finds every file
  // it names already there — and the worker opens the live board only once one
  // exists (`worker/live_board.js`).
  // A SCENE'S DIRECTORY IS PUSHED BENEATH ITS NAME (`Benchmark::cell_dir`).
  const json = (sub) => readdirSync(join(dir, sub), { withFileTypes: true }).filter((e) => e.isFile() && e.name.endsWith(".json")).map((e) => sub ? `${sub}/${e.name}` : e.name);
  const scenes = readdirSync(dir, { withFileTypes: true }).filter((e) => e.isDirectory()).map((e) => e.name);
  const names = [...json(""), ...scenes.flatMap(json)].filter((n) => n !== "meta.json").sort();
  for (const name of [...names, "meta.json"].filter((n) => existsSync(join(dir, n)))) {
    const bytes = readFileSync(join(dir, name));
    const sha = createHash("sha256").update(bytes).digest("hex");
    if (ledger[name] === sha) continue;
    const r = await fetch(`${SITE}/api/board/live/${name}`, {
      method: "PUT",
      headers: { authorization: `Bearer ${process.env.BOARD_PUSH_TOKEN}`, "content-type": "application/json" },
      body: bytes,
    });
    if (!r.ok) throw new Error(`push ${name}: ${r.status} ${await r.text()}`);
    ledger[name] = sha;
    sent += 1;
    // THE LEDGER IS WRITTEN PER FILE, so a push cut off halfway resumes where
    // it stopped instead of sending the board again.
    writeFileSync(ledgerPath, JSON.stringify(ledger));
  }
  console.error(`live: pushed ${sent} file(s)`);
}

if (mode === "delta") await delta();
else if (mode === "cursor") await cursor();
else if (mode === "push") await push();
else if (mode === "subset") await subset(dir, process.argv[5], process.argv.slice(6));
else if (mode === "open") await open(dir, process.argv[5]);
else { console.error("usage: live_publish.mjs delta|cursor|subset|open|push <work-dir> ..."); process.exit(2); }
