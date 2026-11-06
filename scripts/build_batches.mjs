// SPDX-License-Identifier: AGPL-3.0-or-later
// Bounded intake writes; duplicate build ids keep their last input record.
import { createReadStream } from "node:fs";
import { createInterface } from "node:readline";
import { once } from "node:events";
import { pathToFileURL } from "node:url";

export const BUILD_BATCH_ROWS = 1000, BUILD_BATCH_BYTES = 1024 * 1024;
const statement = (table, deleting) => deleting
  ? `DELETE FROM ${table} WHERE id IN (SELECT value FROM jsonb_array_elements_text((?::text)::jsonb))`
  : `INSERT INTO ${table} (id, at, record)
     SELECT DISTINCT ON (id) id, at, record FROM jsonb_to_recordset((?::text)::jsonb)
       AS rows(id text, at text, record text, position bigint) ORDER BY id, position DESC
     ON CONFLICT (id) DO UPDATE SET at = excluded.at, record = excluded.record`;

export async function* buildBatches(file, { deleting = false, maxRows = BUILD_BATCH_ROWS, maxBytes = BUILD_BATCH_BYTES } = {}) {
  if (!Number.isSafeInteger(maxRows) || maxRows < 1 || !Number.isSafeInteger(maxBytes) || maxBytes < 1)
    throw new Error("invalid intake batch bounds");
  const batches = new Map();
  let position = 0;
  const body = batch => ({ sql: batch.sql, params: [`[${batch.rows.join(",")}]`] });
  for await (const line of createInterface({ input: createReadStream(file), crlfDelay: Infinity })) {
    if (!line) continue;
    let table, value;
    if (deleting) {
      table = line.startsWith("equipment:") ? "equipment_inbox" : "inbox";
      value = line;
    } else {
      let build;
      try { build = JSON.parse(line); } catch { throw new Error(`invalid intake build on line ${position + 1}`); }
      table = ["warframe", "companion"].includes(build.record?.kind) ? "equipment_builds" : "builds";
      value = { id: build.id, at: build.at, record: JSON.stringify(build.record ?? null), position: ++position };
    }
    if (!batches.has(table)) {
      const sql = statement(table, deleting);
      batches.set(table, { sql, rows: [], bytes: 0, base: Buffer.byteLength(JSON.stringify({ sql, params: ["[]"] })) });
    }
    const batch = batches.get(table), row = JSON.stringify(value);
    const bytes = Buffer.byteLength(JSON.stringify(row)) - 2;
    if (batch.base + bytes > maxBytes) throw new Error("one intake row exceeds the batch byte limit");
    if (batch.rows.length && (batch.rows.length >= maxRows || batch.base + batch.bytes + bytes + 1 > maxBytes)) {
      yield body(batch);
      batch.rows = []; batch.bytes = 0;
    }
    batch.bytes += bytes + (batch.rows.length ? 1 : 0);
    batch.rows.push(row);
  }
  for (const batch of batches.values()) if (batch.rows.length) yield body(batch);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.stdout.on("error", error => { if (error.code === "EPIPE") process.exit(0); else throw error; });
  try {
    if (!["builds", "deletes"].includes(process.argv[2]) || !process.argv[3]) throw new Error("usage: build_batches.mjs builds|deletes <file>");
    for await (const batch of buildBatches(process.argv[3], { deleting: process.argv[2] === "deletes" })) {
      if (!process.stdout.write(`${JSON.stringify(batch)}\n`)) await once(process.stdout, "drain");
    }
  } catch (error) { console.error(`builds: ${error.message}`); process.exitCode = 1; }
}
