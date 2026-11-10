// SPDX-License-Identifier: AGPL-3.0-or-later
// THE HEADS OF A PROJECTED BOARD — docs/BOARD.md §"The heads". For each weapon
// of `<board-dir>`, its rows in every scene (`<board-dir>/<id>.json` and each
// `<board-dir>/<scene>/<id>.json`) cut to `boardHeads`, the page's own code,
// into `<board-dir>/heads/<id>.json`. A weapon's heads are written again only
// when one of its files is newer, and dropped once it has none.
//
//   node scripts/board_heads.mjs <board-dir>
import { readdirSync, statSync, mkdirSync, rmSync, existsSync, mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { DatabaseSync } from "node:sqlite";
import { rankBoard, BOARD_HEADS_PLACES } from "../mcp/headless.js";
import { jsonArrayRows } from "./json_array_stream.mjs";
import { outputWriter } from "./inventory_publication_io.mjs";

/// THE DIRECTORY'S NAME, and never a scene's: `board_meta.py` `HEADS` skips it.
const HEADS = "heads";

const json = (dir) => (existsSync(dir) ? readdirSync(dir).filter((f) => f.endsWith(".json") && f !== "index.json" && f !== "meta.json") : []);

async function writeHeads(dir) {
  const out = join(dir, HEADS);
  mkdirSync(out, { recursive: true });
  const scenes = readdirSync(dir, { withFileTypes: true }).filter((e) => e.isDirectory() && e.name !== HEADS).map((e) => e.name);
  const files = new Map();
  for (const sub of ["", ...scenes]) {
    for (const f of json(join(dir, sub))) {
      const id = f.slice(0, -5);
      if (!files.has(id)) files.set(id, []);
      files.get(id).push(join(dir, sub, f));
    }
  }
  let wrote = 0, dropped = 0;
  const scratch = mkdtempSync(join(tmpdir(), 'board-heads-')), db = new DatabaseSync(join(scratch, 'rows.sqlite'));
  db.exec(`PRAGMA cache_size=-16384;PRAGMA temp_store=FILE;PRAGMA mmap_size=0;
    CREATE TABLE rows(seq INTEGER PRIMARY KEY,g TEXT NOT NULL,benchmark TEXT NOT NULL,score REAL NOT NULL,at TEXT,page TEXT NOT NULL);
    CREATE INDEX groups ON rows(g,score DESC,seq);
    CREATE INDEX newest ON rows(benchmark,at DESC,seq);
    CREATE TABLE kept(seq INTEGER PRIMARY KEY);`);
  const context = {};
  const put = db.prepare('INSERT INTO rows VALUES(?,?,?,?,?,?)');
  try {
  for (const [id, paths] of files) {
    const at = join(out, `${id}.json`);
    const was = existsSync(at) ? statSync(at).mtimeMs : -1;
    if (paths.every((p) => statSync(p).mtimeMs <= was)) continue;
    db.exec('DELETE FROM rows;DELETE FROM kept;BEGIN');
    let count = 0;
    for (const path of paths) for await (const raw of jsonArrayRows(path)) {
      const row = JSON.parse(raw), ranked = rankBoard([row], [], [])[0];
      const group = ranked.key.slice(0, ranked.key.lastIndexOf('#'));
      put.run(++count, group, JSON.stringify([row.benchmark]),
        row.score || 0, row.measured_at || null, JSON.stringify(row));
      if (count % 256 === 0) db.exec('COMMIT;BEGIN');
    }
    db.exec('COMMIT');
    for (const { g } of db.prepare('SELECT DISTINCT g FROM rows ORDER BY g').iterate()) {
      const samples = db.prepare(`SELECT page FROM rows WHERE g=? GROUP BY score ORDER BY score DESC LIMIT ?`)
        .all(g, BOARD_HEADS_PLACES + 1).map(row => JSON.parse(row.page));
      const first = rankBoard(samples, [], []).filter(row => row.rank <= BOARD_HEADS_PLACES);
      if (first.length) db.prepare('INSERT OR IGNORE INTO kept SELECT seq FROM rows WHERE g=? AND score>=?')
        .run(g, Math.min(...first.map(row => row.row.score || 0)));
    }
    for (const { benchmark } of db.prepare('SELECT DISTINCT benchmark FROM rows ORDER BY benchmark').iterate()) {
      const newest = db.prepare(`SELECT seq FROM rows WHERE benchmark=? AND at IS NOT NULL ORDER BY at DESC,seq LIMIT 1`).get(benchmark);
      if (newest) db.prepare('INSERT OR IGNORE INTO kept VALUES(?)').run(newest.seq);
    }
    const writer = await outputWriter(context, at); writer.append('['); let n = 0;
    for (const row of db.prepare('SELECT r.page FROM rows r JOIN kept k ON k.seq=r.seq ORDER BY r.seq').iterate())
      writer.append((n++ ? ',' : '') + row.page);
    writer.append(']'); await writer.finish();
    wrote++;
  }
  for (const f of json(out)) {
    if (!files.has(f.slice(0, -5))) { rmSync(join(out, f)); dropped++; }
  }
  } finally { context.writer?.close(); db.close(); rmSync(scratch, { recursive: true, force: true }); }
  return { wrote, dropped, weapons: files.size };
}

const dir = process.argv[2];
if (!dir) { console.error("usage: board_heads.mjs <board-dir>"); process.exit(2); }
const t = Date.now();
const r = await writeHeads(dir);
console.error(`heads: ${r.wrote} written, ${r.dropped} dropped, ${r.weapons} weapons, ${Date.now() - t} ms`);
