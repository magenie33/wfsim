// SPDX-License-Identifier: AGPL-3.0-or-later
// THE HEADS OF A PROJECTED BOARD — docs/BOARD.md §"The heads". For each weapon
// of `<board-dir>`, its rows in every scene (`<board-dir>/<id>.json` and each
// `<board-dir>/<scene>/<id>.json`) cut to `boardHeads`, the page's own code,
// into `<board-dir>/heads/<id>.json`. A weapon's heads are written again only
// when one of its files is newer, and dropped once it has none.
//
//   node scripts/board_heads.mjs <board-dir>
import { readFileSync, writeFileSync, readdirSync, statSync, mkdirSync, rmSync, existsSync } from "node:fs";
import { join } from "node:path";
import { boardHeads } from "../mcp/headless.js";

/// THE DIRECTORY'S NAME, and never a scene's: `board_meta.py` `HEADS` skips it.
const HEADS = "heads";

const json = (dir) => (existsSync(dir) ? readdirSync(dir).filter((f) => f.endsWith(".json") && f !== "index.json" && f !== "meta.json") : []);

function writeHeads(dir) {
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
  for (const [id, paths] of files) {
    const at = join(out, `${id}.json`);
    const was = existsSync(at) ? statSync(at).mtimeMs : -1;
    if (paths.every((p) => statSync(p).mtimeMs <= was)) continue;
    const rows = paths.flatMap((p) => JSON.parse(readFileSync(p, "utf8")));
    writeFileSync(at, JSON.stringify(boardHeads(rows)));
    wrote++;
  }
  for (const f of json(out)) {
    if (!files.has(f.slice(0, -5))) { rmSync(join(out, f)); dropped++; }
  }
  return { wrote, dropped, weapons: files.size };
}

const dir = process.argv[2];
if (!dir) { console.error("usage: board_heads.mjs <board-dir>"); process.exit(2); }
const t = Date.now();
const r = writeHeads(dir);
console.error(`heads: ${r.wrote} written, ${r.dropped} dropped, ${r.weapons} weapons, ${Date.now() - t} ms`);
