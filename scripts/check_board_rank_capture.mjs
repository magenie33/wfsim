// SPDX-License-Identifier: AGPL-3.0-or-later
// Empty measurement gaps must not turn one cursor fetch into an unbounded source scan.
import assert from 'node:assert/strict';
import {mkdtempSync,readFileSync,rmSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {randomBytes,createHash} from 'node:crypto';
import postgres from './pg/node_modules/postgres/src/index.js';
import {captureRankBaseline} from './board_rank_baseline.mjs';
const url=process.env.WFSIM_BOARD_RANK_TEST_URL;
assert(url&&['localhost','127.0.0.1'].includes(new URL(url).hostname));
const schema='rank_capture_'+randomBytes(6).toString('hex'),source='00000000-0000-4000-8000-000000000045';
const sql=postgres(url,{max:1,fetch_types:false,onnotice(){},connection:{search_path:schema,statement_timeout:10000}});
const work=mkdtempSync(join(tmpdir(),'rank-capture-')),weapon='capture_fixture',ruler='standard_single_target';
try{
 await sql.unsafe('CREATE SCHEMA '+schema);
 await sql.unsafe(`CREATE TABLE state_board_input_source(id integer PRIMARY KEY,source text);
  CREATE TABLE capture_builds(id text PRIMARY KEY,weapon text,at text,record text);
  CREATE TABLE board_facts(identity text,ruler text,mode text,score float8,review_id text);
  CREATE INDEX ON board_facts(identity,ruler);
  CREATE TABLE state_board_input_mailbox(scope text,key text,version bigint,PRIMARY KEY(scope,key));
  CREATE FUNCTION candidate_delay(text) RETURNS boolean LANGUAGE plpgsql VOLATILE AS $$
   BEGIN IF $1 LIKE 'empty-%' THEN PERFORM pg_sleep(0.001);END IF;RETURN true;END $$;
  CREATE VIEW entity_weapon_builds AS SELECT * FROM capture_builds WHERE candidate_delay(id)`);
 await sql.unsafe('INSERT INTO state_board_input_source VALUES(1,$1)',[source]);
 await sql.unsafe(`INSERT INTO capture_builds VALUES('first',$1,'2026-11-17T00:00:00Z','{"weapon":"capture_fixture"}')`,[weapon]);
 await sql.unsafe(`INSERT INTO capture_builds SELECT 'empty-'||n,$1,'2026-11-17T00:00:00Z','{"weapon":"capture_fixture"}' FROM generate_series(1,2048)n`,[weapon]);
 await sql.unsafe(`INSERT INTO capture_builds VALUES('last',$1,'2026-11-17T00:00:00Z','{"weapon":"capture_fixture"}')`,[weapon]);
 await sql.unsafe(`INSERT INTO board_facts VALUES('first',$1,'base',11,NULL),('last',$1,'base',22,'selected-review')`,[ruler]);
 await sql.unsafe(`INSERT INTO state_board_input_mailbox VALUES('build','first',1),('build','last',2),('review','selected-review',5);
  INSERT INTO state_board_input_mailbox SELECT 'measurement',jsonb_build_array(identity,ruler,mode)::text,3 FROM board_facts`);
 await sql.unsafe("SET statement_timeout='2s'");
 const manifest=await captureRankBaseline(sql,{weapon,ruler,source,native:'fixture-native',work});
 const body=readFileSync(manifest.path),rows=body.toString().trimEnd().split('\n').map(JSON.parse);
 assert.equal(createHash('sha256').update(body).digest('hex'),manifest.sha256);
 assert.equal(body.length,manifest.bytes);
 assert.equal(manifest.total,2);assert.equal(manifest.facts,2);
 assert.deepEqual(rows.filter(row=>row.kind==='source').map(row=>row.source.id),['first','last']);
 assert.equal(rows[0].snapshot,rows.at(-1).snapshot);
 assert.equal(rows.at(-1).kind,'complete');assert.equal(rows.at(-1).total,2);
 const last=rows.find(row=>row.kind==='source'&&row.source.id==='last');
 assert.equal(last.facts[0].score,22);
 assert(last.headers.some(row=>row.scope==='review'&&row.key==='selected-review'&&row.version==='5'));
 assert(last.headers.some(row=>row.scope==='measurement'&&row.key==='["last", "standard_single_target", "base"]'&&row.version==='3'));
 await sql.unsafe("SET statement_timeout='10s'");
 await sql.unsafe('DELETE FROM board_facts');
 const empty=await captureRankBaseline(sql,{weapon,ruler,source,native:'fixture-native',work});
 assert.equal(empty.total,0);assert.equal(empty.facts,0);
 assert.deepEqual(readFileSync(empty.path,'utf8').trimEnd().split('\n').map(JSON.parse).map(row=>row.kind),['meta','complete']);
 console.log('board rank capture: candidate-page timeout, empty gaps, coherent facts and headers PASS');
}finally{
 await sql.unsafe('SET statement_timeout=10000').catch(()=>{});
 await sql.unsafe('DROP SCHEMA IF EXISTS '+schema+' CASCADE');await sql.end();
 rmSync(work,{recursive:true,force:true});
}
