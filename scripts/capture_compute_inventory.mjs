// SPDX-License-Identifier: AGPL-3.0-or-later
// Capture all inventory inputs in one read-only database snapshot.
import postgres from './pg/node_modules/postgres/src/index.js';
import { createWriteStream, renameSync, rmSync, writeFileSync } from 'node:fs';
import { once } from 'node:events';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { BOARD_KINDS } from '../worker/board_kinds.js';

export async function captureInventory(sql, paths, engine, covers = BOARD_KINDS) {
  const outputs = Object.fromEntries(Object.entries(paths).map(([k, p]) => [k, `${p}.partial-${process.pid}`]));
  const counts = {};
  try {
    const meta = await sql.begin('isolation level repeatable read read only', async tx => {
      await tx.unsafe("SET LOCAL statement_timeout='180s'");
      await tx.unsafe("SET LOCAL idle_in_transaction_session_timeout='60s'");
      const [stamp] = await tx.unsafe("SELECT to_char(transaction_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"') started_at,pg_current_snapshot()::text snapshot");
      const scans = {
        library: ['SELECT id,at,record FROM builds', []],
        facts: ['SELECT identity,ruler,mode,measured_by,score,cost_seconds,started_at,finished_at FROM scores', []],
        queue: ['SELECT q.batch,q.build_id,q.ruler,q.mode FROM queue q WHERE EXISTS(SELECT 1 FROM batches b WHERE b.id=q.batch)', []],
        tasks: ['SELECT * FROM factory_inventory_rows($1::text,ARRAY(SELECT jsonb_array_elements_text(($2::text)::jsonb)))', [engine, JSON.stringify(covers)]],
      };
      for (const [name, [query, args]] of Object.entries(scans)) {
        const out = createWriteStream(outputs[name]); let failed = null, n = 0;
        const builds = name === 'library' ? createWriteStream(outputs.builds) : null;
        out.on('error', error => { failed = error; });
        builds?.on('error', error => { failed = error; });
        const write = async text => { if (failed) throw failed; if (!out.write(text)) await once(out, 'drain'); };
        try {
          if (name === 'library') await write('[');
          await tx.unsafe(query, args).cursor(5000, async rows => {
            for (const row of rows) {
              if (name === 'library') {
                const record = typeof row.record === 'string' ? JSON.parse(row.record) : row.record;
                if (!record || typeof record !== 'object') throw new Error('invalid inventory build');
                await write((n ? ',' : '') + JSON.stringify(record));
                if (!builds.write(JSON.stringify({ id: row.id, at: row.at, record }) + '\n')) await once(builds, 'drain');
              } else await write(JSON.stringify(row) + '\n');
              n++;
            }
          });
          if (name === 'library') await write(']');
          out.end(); await once(out, 'finish'); if (failed) throw failed;
          if (builds) { builds.end(); await once(builds, 'finish'); if (failed) throw failed; }
        } catch (error) { out.destroy(); builds?.destroy(); throw error; }
        counts[name] = n;
      }
      return { ...stamp, engine, covers, counts };
    });
    for (const name of ['library', 'builds', 'facts', 'queue', 'tasks']) renameSync(outputs[name], paths[name]);
    writeFileSync(outputs.meta, JSON.stringify(meta)); renameSync(outputs.meta, paths.meta);
    return meta;
  } finally { for (const p of Object.values(outputs)) rmSync(p, { force: true }); }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const [library, facts, queue, prefix] = process.argv.slice(2);
  if (!prefix || !process.env.PG_URL) throw new Error('usage: capture_compute_inventory.mjs library facts queue prefix (PG_URL required)');
  const engine = process.env.WFSIM_INVENTORY_ENGINE || execFileSync('python3', ['-c', 'import sys;sys.path.insert(0,"scripts");from build_site_app import engine_id;print(engine_id())'], { encoding: 'utf8' }).trim();
  if (!/^[a-f0-9]{12}$/.test(engine)) throw new Error('invalid inventory engine');
  const sql = postgres(process.env.PG_URL, { max: 1, fetch_types: false, connect_timeout: 15, onnotice: () => {},
    connection: { application_name: 'wfsim-inventory' } });
  try { console.log(JSON.stringify(await captureInventory(sql, { library, facts, queue, builds: `${prefix}.builds`, tasks: `${prefix}.tasks`, meta: `${prefix}.json` }, engine))); }
  finally { await sql.end({ timeout: 5 }); }
}
