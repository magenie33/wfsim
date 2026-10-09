// SPDX-License-Identifier: AGPL-3.0-or-later
// THE LIVE BOARD'S TWO HOPS TO THE SITE — docs/BOARD.md §"The live board". Run
// by `live_board.sh`.
//
//   node live_publish.mjs delta <work-dir>                verified facts since the last read
//   node live_publish.mjs subset <work-dir> <ids> <out> <facts>   the weapons <ids> touch: library, facts
//   node live_publish.mjs push <work-dir> <dir>           the files that moved, to R2
//
// `delta` folds into `facts-known.ndjson` every `scores` row finished in the
// last two hours of what it has seen: a scorer ships a row minutes after it
// finishes, so a cursor on the clock alone would step over it. Exit 3 means
// nothing moved. `push` sends a file only when its bytes differ from the last
// push (`pushed.json`), through the worker's `/api/board/live/`.
import { readFileSync, writeFileSync, existsSync, readdirSync } from "node:fs";
import { createHash } from "node:crypto";
import { join } from "node:path";

const OVERLAP_SECONDS = 2 * 3600;
const SITE = process.env.WFSIM_SITE || "https://wfsim.app";
const [mode, work, dir] = process.argv.slice(2);

async function d1(sql, params = []) {
  const { CF_ACCOUNT, CF_TOKEN, CF_D1_DATABASE } = process.env;
  const r = await fetch(`https://api.cloudflare.com/client/v4/accounts/${CF_ACCOUNT}/d1/database/${CF_D1_DATABASE}/query`, {
    method: "POST",
    headers: { authorization: `Bearer ${CF_TOKEN}`, "content-type": "application/json" },
    body: JSON.stringify({ sql, params }),
  });
  const j = await r.json().catch(() => null);
  if (!r.ok || !j || !j.success) throw new Error(`d1 ${r.status}: ${JSON.stringify(j && j.errors)}`);
  return j.result[0].results;
}

const key = (f) => `${f.identity}|${f.ruler}|${f.mode}`;

async function delta() {
  const factsPath = join(work, "facts-known.ndjson");
  const cursorPath = join(work, "facts-cursor.txt");
  const lines = readFileSync(factsPath, "utf8").split("\n").filter(Boolean);
  let seen = existsSync(cursorPath) ? readFileSync(cursorPath, "utf8").trim() : "";
  if (!seen) for (const l of lines) { const f = JSON.parse(l); if (f.finished_at > seen) seen = f.finished_at; }
  const since = new Date(Date.parse(seen || "1970-01-01T00:00:00Z") - OVERLAP_SECONDS * 1000).toISOString();
  const rows = await d1(`SELECT identity, ruler, mode, measured_by, score, cost_seconds, started_at, finished_at
                         FROM scores WHERE finished_at > ? ORDER BY finished_at`, [since]);
  const byKey = new Map(lines.map((l) => [key(JSON.parse(l)), l]));
  // COMPARED AS VALUES: the hourly read writes its lines through jq and this
  // through JSON.stringify, and two spellings of one fact are not a change.
  const same = (a, b) => a && a.score === b.score && a.measured_by === b.measured_by && a.finished_at === b.finished_at;
  let moved = 0;
  const movedIds = [];
  for (const r of rows) {
    const had = byKey.get(key(r));
    if (!same(had && JSON.parse(had), r)) { byKey.set(key(r), JSON.stringify(r)); moved += 1; movedIds.push(r.identity); }
    if (r.finished_at > seen) seen = r.finished_at;
  }
  writeFileSync(cursorPath, seen + "\n");
  writeFileSync(join(work, "moved-ids.txt"), movedIds.map((id) => id + "\n").join(""));
  if (!moved) process.exit(3);
  writeFileSync(factsPath, [...byKey.values()].join("\n") + "\n");
  console.error(`live: ${moved} fact(s) moved since ${since}`);
}

/// THE BUILDS OF EVERY WEAPON A SET OF BUILD IDS TOUCHES — a weapon is ranked
/// whole or not at all (`wfsim-board --subset`). Exit 3 when it names none.
function subset(idsPath, out, factsIn) {
  const lines = (p) => (existsSync(p) ? readFileSync(p, "utf8").split("\n").filter(Boolean) : []);
  const want = new Set(lines(idsPath));
  const weaponOf = new Map();
  for (const l of lines(join(work, "library-ids.ndjson"))) {
    const x = JSON.parse(l);
    if (want.has(x.k)) weaponOf.set(x.k, x.v.weapon);
  }
  for (const l of lines(join(work, "new-builds.ndjson"))) {
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
  for (const l of lines(join(work, "library-ids.ndjson"))) { const x = JSON.parse(l); if (weapons.has(x.v.weapon)) ids.add(x.k); }
  for (const l of lines(join(work, "new-builds.ndjson"))) { const x = JSON.parse(l); if (weapons.has(x.record.weapon)) ids.add(x.id); }
  writeFileSync(`${out}.facts`, lines(factsIn).filter((l) => ids.has(JSON.parse(l).identity)).map((l) => l + "\n").join(""));
  console.error(`live: re-ranking ${weapons.size} weapon(s), ${lib.length} build(s)`);
}

async function push() {
  const ledgerPath = join(work, "pushed.json");
  const ledger = existsSync(ledgerPath) ? JSON.parse(readFileSync(ledgerPath, "utf8")) : {};
  let sent = 0;
  // THE STAMP GOES LAST, so a reader who sees a new `meta.json` finds every file
  // it names already there — and the worker opens the live board only once one
  // exists (`worker/live_board.js`).
  const names = readdirSync(dir).filter((n) => n.endsWith(".json") && n !== "meta.json").sort();
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
else if (mode === "push") await push();
else if (mode === "subset") subset(dir, process.argv[5], process.argv[6]);
else { console.error("usage: live_publish.mjs delta|subset|push <work-dir> ..."); process.exit(2); }
