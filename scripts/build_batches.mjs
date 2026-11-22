// SPDX-License-Identifier: AGPL-3.0-or-later
// Canonical entities and captured intent commit before inbox acknowledgement.
import { createReadStream } from "node:fs";
import { createInterface } from "node:readline";
import { once } from "node:events";
import { pathToFileURL } from "node:url";
import { computationId } from "../worker/computation_input.js";
import { DEADLINE_MS } from "../worker/deadlines.js";
import { close, transaction } from "./db.mjs";

export const BUILD_BATCH_ROWS = 1000, BUILD_BATCH_BYTES = 1024 * 1024;
const statement = (table, deleting) => deleting
  ? `WITH acknowledged AS MATERIALIZED (
       SELECT build_ack_submissions(input), input FROM (VALUES (?::text)) AS captured(input)
     ) SELECT build_inbox_assert_ack(input) FROM acknowledged`
  : table === 'entity_weapon_builds' ? `SELECT build_intake_batch(?::text)` : `INSERT INTO ${table} (id, at, record)
     SELECT DISTINCT ON (id) id, at, record FROM jsonb_to_recordset((?::text)::jsonb)
       AS rows(id text, at text, record text, position bigint) ORDER BY id, position DESC
     ON CONFLICT (id) DO UPDATE SET at = excluded.at, record = excluded.record`;

export async function prepareIntakeBuild(build) {
  const now=Date.now();
  build.deadlines ||= {intake:new Date(now+DEADLINE_MS.new_build).toISOString(),correction:new Date(now+DEADLINE_MS.board_review).toISOString(),system:new Date(now).toISOString()};
  for(const flow of [build.flow,build.family_flow].filter(Boolean))for(const node of flow.nodes || []) {
    let request=node.request;
    if(node.kind==='measure') {
      request={...flow.templates[node.template],...flow.patches[node.patch].set};
      for(const key of flow.patches[node.patch].remove)delete request[key];
    }
    node.computation=await computationId(node.kind==='measure'?'simulate':'optimize_report',request);
  }
  return build;
}

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
      const {id,at,record,initiator,family_source,observation,rejected} = JSON.parse(line);
      if (typeof id !== 'string' || !id || typeof at !== 'string' || record === undefined)
        throw new Error('inbox acknowledgement requires a captured row');
      table = 'record_build_submissions';
      value = {id,at,record:typeof record === 'string' ? record : JSON.stringify(record),
        initiator,family_source,observation:typeof observation === 'object' && observation !== null ? JSON.stringify(observation) : observation};
      if(rejected===true)value.rejected=true;
    } else {
      let build;
      try { build = JSON.parse(line); } catch { throw new Error(`invalid intake build on line ${position + 1}`); }
      table = ["warframe", "companion"].includes(build.record?.kind) ? "entity_equipment_builds" : "entity_weapon_builds";
      if(table==='entity_weapon_builds') {
        await prepareIntakeBuild(build);
        value={...build,position:++position};
        delete value.family;
      } else value = { id: build.id, at: build.at, record: JSON.stringify(build.record ?? null), position: ++position };
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
    const [mode, file, captured] = process.argv.slice(2);
    if (mode === "handoff" && file && captured) {
      if (process.env.PG_URL) {
        const statements = [];
        for await (const batch of buildBatches(file)) statements.push(batch);
        for await (const batch of buildBatches(captured, { deleting: true })) statements.push(batch);
        try { await transaction(statements); } finally { await close(); }
      }
    } else {
      if (!["builds", "deletes"].includes(mode) || !file) throw new Error("usage: build_batches.mjs builds|deletes <file> or handoff <builds> <captured>");
      for await (const batch of buildBatches(file, { deleting: mode === "deletes" })) {
        if (!process.stdout.write(`${JSON.stringify(batch)}\n`)) await once(process.stdout, "drain");
      }
    }
  } catch (error) { console.error(`builds: ${error.message}`); process.exitCode = 1; }
}
