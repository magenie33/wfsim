// A complete inventory capture commits all files from one read-only snapshot.
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, existsSync, rmSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { PGlite } from './pg/node_modules/@electric-sql/pglite/dist/index.js';
import { captureInventory } from './capture_compute_inventory.mjs';
const dir = mkdtempSync(join(tmpdir(), 'compute-capture-'));
const pg = new PGlite();
try {
  await pg.exec(['schema.sql', 'library.sql', 'sync.sql'].map(name => readFileSync(new URL(`../worker/pg/${name}`, import.meta.url), 'utf8')).join('\n'));
  await pg.query("INSERT INTO builds(id,at,record) VALUES ('a','x','{\"weapon\":\"torid\"}')");
  await pg.query("INSERT INTO builds(id,at,record) SELECT 'b'||i,'x','{\"weapon\":\"torid\"}' FROM generate_series(1,5000) i");
  await pg.query("INSERT INTO batches(id,at,why,total) VALUES ('b','x','test',1)");
  await pg.query("INSERT INTO queue(batch,build_id,ruler,mode) VALUES ('b','a','r','base')");
  const paths = Object.fromEntries(['library', 'builds', 'facts', 'queue', 'tasks', 'meta'].map(k => [k, join(dir, k)]));
  let reads = 0, fail = false;
  const sql = { begin: async (options, fn) => {
    assert.equal(options, 'isolation level repeatable read read only');
    return pg.transaction(async tx => {
      await tx.exec('SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY');
      const [settings] = (await tx.query("SELECT current_setting('transaction_read_only') ro,current_setting('transaction_isolation') isolation")).rows;
      assert.equal(settings.ro, 'on'); assert.equal(settings.isolation, 'repeatable read');
      return fn({ unsafe: (query, params = []) => {
        assert(params.every(value => !Array.isArray(value)), 'wire parameters must be scalar values');
        if (query.startsWith('FETCH')) {
          assert.equal(query, 'FETCH FORWARD 5000 FROM inventory_scan');
          if (fail && ++reads === 2) return Promise.reject(new Error('connection lost'));
        }
        return tx.query(query, params).then(r => r.rows);
      } });
    });
  } };
  const meta = await captureInventory(sql, paths, '4393e1730621');
  assert.deepEqual(meta.counts, { library: 5001, facts: 0, queue: 1, tasks: 0 });
  assert.equal(JSON.parse(readFileSync(paths.library)).length, 5001);
  assert.equal(JSON.parse(readFileSync(paths.library))[0].weapon, 'torid');
  assert.equal(JSON.parse(readFileSync(paths.builds, 'utf8').split('\n')[0]).id, 'a');
  assert.equal(JSON.parse(readFileSync(paths.queue)).build_id, 'a');
  assert(JSON.parse(readFileSync(paths.meta)).snapshot);
  const prior = readFileSync(paths.meta, 'utf8'); fail = true;
  await assert.rejects(captureInventory(sql, paths, '4393e1730621'), /connection lost/);
  assert.equal(readFileSync(paths.meta, 'utf8'), prior);
  assert(!readdirSync(dir).some(name => name.includes('.partial-')));
  assert(existsSync(paths.tasks));
  console.log('ok: bounded SQL fetches retain one read-only snapshot across pages; a cut rolls back and preserves the previous manifest without partial files');
} finally { await pg.close(); rmSync(dir, { recursive: true, force: true }); }
