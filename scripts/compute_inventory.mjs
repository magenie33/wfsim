// SPDX-License-Identifier: AGPL-3.0-or-later
// Union the engine's complete missing rows with distinct queued rows before pacing.
import { DatabaseSync } from "node:sqlite";
import { createReadStream, mkdtempSync, readFileSync, rmSync, statSync } from "node:fs";
import { createInterface } from "node:readline";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { execFileSync } from "node:child_process";
import { BOARD_KINDS } from "../worker/board_kinds.js";

export async function publishInventory(plan, { origin = process.env.FACTORY_ORIGIN || "https://wfsim.app", token = process.env.FACTORY_TOKEN,
  send = fetch } = {}) {
  if (!token) throw new Error("complete inventory needs FACTORY_TOKEN");
  const response = await send(`${origin}/api/board/inventory`, { method: "POST", headers: {
    authorization: `Bearer ${token}`, "content-type": "application/json", "user-agent": "wfsim-queue" },
    body: JSON.stringify(plan), signal: AbortSignal.timeout(180000) });
  if (!response.ok || !(await response.json()).ok) throw new Error(`complete inventory refused (${response.status})`);
}

export async function completeInventory(queue, missing, orphans, startedAt, rulers = null) {
  const roster = rulers && new Set(rulers);
  if (roster && (!roster.size || roster.size !== rulers.length)) throw new Error("incomplete inventory roster");
  const dir = mkdtempSync(join(tmpdir(), "compute-inventory-")), db = new DatabaseSync(join(dir, "rows.db"));
  try {
    db.exec("PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; PRAGMA cache_size=-32768; CREATE TABLE rows(k TEXT PRIMARY KEY,queued INTEGER NOT NULL) WITHOUT ROWID");
    const queued = db.prepare("INSERT OR IGNORE INTO rows(k,queued) VALUES (?,1)"), missingRow = db.prepare("INSERT OR IGNORE INTO rows(k,queued) VALUES (?,0)"), remove = db.prepare("DELETE FROM rows WHERE k=?");
    let pending = 0;
    db.exec("BEGIN");
    for (const [path, statement] of [[queue, queued], [missing, missingRow], [orphans, remove]]) {
      let lineNumber = 0;
      for await (const line of createInterface({ input: createReadStream(path), crlfDelay: Infinity })) {
        lineNumber++;
        if (!line.trim()) continue;
        const row = JSON.parse(line);
        if (!row || !["build_id", "ruler", "mode"].every(k => typeof row[k] === "string" && row[k]))
          throw new Error(`invalid task row ${path}:${lineNumber}`);
        if (roster && !roster.has(row.ruler)) {
          if (path === missing) throw new Error(`unknown inventory ruler ${row.ruler}`);
          continue;
        }
        statement.run(JSON.stringify([row.build_id, row.ruler, row.mode]));
        if (++pending === 10000) { db.exec("COMMIT; BEGIN"); pending = 0; }
      }
    }
    db.exec("COMMIT");
    const count = db.prepare("SELECT count(*) n,coalesce(sum(queued),0) queued FROM rows").get();
    return { rows: count.n, queued: count.queued, kind: "board", verb: "simulate", covers: [...BOARD_KINDS], started_at: startedAt };
  } finally { db.close(); rmSync(dir, { recursive: true, force: true }); }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const [queue, missing, orphans, facts, roster] = process.argv.slice(2);
  if (!roster) throw new Error("usage: compute_inventory.mjs queue missing orphans facts roster");
  const started = process.env.WFSIM_SCAN_STARTED_AT || new Date(statSync(facts).birthtimeMs || statSync(facts).mtimeMs).toISOString();
  const plan = await completeInventory(queue, missing, orphans, started, readFileSync(roster, "utf8").trim().split(/\s+/));
  plan.source = execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim();
  await publishInventory(plan);
  console.log(`inventory: ${plan.rows} distinct board tasks, including work not yet admitted`);
}
