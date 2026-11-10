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
  await pg.query("INSERT INTO builds(id,at,record) SELECT 'b'||i,'x','{\"weapon\":\"torid\"}' FROM generate_series(1,25000) i");
  await pg.query("INSERT INTO batches(id,at,why,total) VALUES ('b','x','test',1)");
  await pg.query("INSERT INTO queue(batch,build_id,ruler,mode) VALUES ('b','a','r','base')");
  const paths = Object.fromEntries(['library', 'builds', 'facts', 'queue', 'tasks', 'meta'].map(k => [k, join(dir, k)]));
  let reads = 0, fail = false, libraryFetches = 0, libraryScan = false;
  const sql = { begin: async (options, fn) => {
    assert.equal(options, 'isolation level repeatable read read only');
    return pg.transaction(async tx => {
      await tx.exec('SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY');
      const [settings] = (await tx.query("SELECT current_setting('transaction_read_only') ro,current_setting('transaction_isolation') isolation")).rows;
      assert.equal(settings.ro, 'on'); assert.equal(settings.isolation, 'repeatable read');
      return fn({ unsafe: (query, params = []) => {
        assert(params.every(value => !Array.isArray(value)), 'wire parameters must be scalar values');
        if (query.startsWith('DECLARE')) libraryScan = query.includes('SELECT id,at,record FROM builds');
        if (query.startsWith('FETCH')) {
          const page = /^FETCH FORWARD ([0-9]+) FROM inventory_scan$/.exec(query);
          assert(page && Number(page[1]) > 0 && Number(page[1]) <= 20000, 'one result page stays bounded');
          if (libraryScan) libraryFetches++;
          if (fail && ++reads === 2) return Promise.reject(new Error('connection lost'));
        }
        return tx.query(query, params).then(r => r.rows);
      } });
    });
  } };
  const meta = await captureInventory(sql, paths, '4393e1730621');
  assert.equal(libraryFetches, 2, 'a complete 25001-row library needs only two bounded network round trips');
  assert.deepEqual(meta.counts, { library: 25001, facts: 0, queue: 1, tasks: 0 });
  assert.equal(JSON.parse(readFileSync(paths.library)).length, 25001);
  assert.equal(JSON.parse(readFileSync(paths.library))[0].weapon, 'torid');
  assert.equal(JSON.parse(readFileSync(paths.builds, 'utf8').split('\n')[0]).id, 'a');
  assert.equal(JSON.parse(readFileSync(paths.queue)).build_id, 'a');
  assert(JSON.parse(readFileSync(paths.meta)).snapshot);
  const compact = Object.fromEntries(Object.entries(paths).filter(([key]) => key !== 'library')
    .map(([key, path]) => [key, path + '-compact']));
  const compactMeta = await captureInventory(sql, compact, '4393e1730621');
  assert.deepEqual(compactMeta.counts, meta.counts);
  assert.equal(readFileSync(compact.builds, 'utf8'), readFileSync(paths.builds, 'utf8'),
    'omitting the derived JSON array preserves every raw build, identity and clock');
  assert(!readdirSync(dir).some(name => name === 'library-compact' || name.startsWith('undefined')));
  await pg.query("INSERT INTO scores(identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at) VALUES ('a','r','base','old',99,'kpm',0,'2026-10-18T00:00:00.000Z','2026-10-18T00:00:00.000Z')");
  await pg.query(`INSERT INTO board_reviews(ref,id,round_at,identity,ruler,mode,engine,question,record,state,
    baseline_score,baseline_metric,baseline_cost_seconds,baseline_by,baseline_started_at,baseline_finished_at,created_at,invalidated_at)
    VALUES ('a|r|base','capture-retired','2026-11-06T00:00:00.000Z','a','r','base','new','capture-question','{}','waiting',
      99,'kpm',0,'old','2026-10-18T00:00:00.000Z','2026-10-18T00:00:00.000Z','2026-11-06T00:00:00.000Z','2026-11-06T00:00:00.000Z')`);
  await pg.query("UPDATE scores SET review_id='capture-retired' WHERE identity='a'");
  await captureInventory(sql,paths,'4393e1730621');
  assert.equal(JSON.parse(readFileSync(paths.facts)).retired_at,'2026-11-06T00:00:00.000Z',
    'a complete snapshot cannot turn a retired measurement back into an active entry-line fact');
  await pg.query("UPDATE board_reviews SET state='succeeded' WHERE id='capture-retired'");
  await pg.query("UPDATE scores SET score=25,finished_at='2026-11-06T00:01:00.000Z' WHERE identity='a'");
  await captureInventory(sql,paths,'4393e1730621');
  assert.equal(JSON.parse(readFileSync(paths.facts)).retired_at,null);
  assert.equal(JSON.parse(readFileSync(paths.facts)).score,25);
  const prior = readFileSync(paths.meta, 'utf8'); fail = true;
  await assert.rejects(captureInventory(sql, paths, '4393e1730621'), /connection lost/);
  assert.equal(readFileSync(paths.meta, 'utf8'), prior);
  assert(!readdirSync(dir).some(name => name.includes('.partial-')));
  assert(existsSync(paths.tasks));
  console.log('ok: bounded SQL fetches retain one read-only snapshot across pages; a cut rolls back and preserves the previous manifest without partial files');
} finally { await pg.close(); rmSync(dir, { recursive: true, force: true }); }
