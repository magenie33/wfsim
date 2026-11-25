// SPDX-License-Identifier: AGPL-3.0-or-later
// Installation and rollback preserve source identity, restrict the reader and pin privileged namespaces.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {randomBytes} from 'node:crypto';
import postgres from './pg/node_modules/postgres/src/index.js';
import {boardReadSchema} from './board_read_schema.mjs';
import {boardRankSchema,boardRankRollbackSchema} from './board_rank_schema.mjs';
import {checkPublicHeads} from './check_board_public_heads.mjs';
import {guardRankSpace} from './board_rank_storage.mjs';
const url=process.env.WFSIM_BOARD_RANK_TEST_URL;assert(url&&['localhost','127.0.0.1'].includes(new URL(url).hostname));
const schema='rank_schema_'+randomBytes(6).toString('hex'),role=schema+'_reader',http=schema+'_http';
const database=new URL(url).pathname.slice(1);assert(/^[a-z0-9_]+$/.test(database));
const sql=postgres(url,{max:1,fetch_types:false,onnotice(){},connection:{search_path:schema,statement_timeout:10000}});
const options={schema,readerRole:role,httpRole:http,tablespace:'pg_default'};
const run=async text=>{try{return await sql.unsafe(text);}catch(error){await sql.unsafe('ROLLBACK');throw error;}};
let reader,web;
try{
 const space=(data,root)=>({env:{WFSIM_BOARD_STORAGE_REQUIRED:'1'},storage:()=>'/data',filesystem:path=>({bavail:BigInt(path==='/'?root:data),bsize:1024n**3n})});
 assert.equal(guardRankSpace('/data',space(33,2)),'/data');
 assert.throws(()=>guardRankSpace('/data',space(31,2)),/headroom/);
 assert.throws(()=>guardRankSpace('/data',space(33,0)),/headroom/);
 const configured=(data,value)=>{const options=space(data,2);options.env.WFSIM_BOARD_DATA_RESERVE_BYTES=value;return options;};
 assert.equal(guardRankSpace('/data',configured(16,String(16*1024**3))),'/data');
 assert.throws(()=>guardRankSpace('/data',configured(15,String(16*1024**3))),/headroom/);
 for(const invalid of ['','0','16GiB',String(15*1024**3),'9007199254740992'])
  assert.throws(()=>guardRankSpace('/data',configured(40,invalid)),/invalid board Data reserve/);
 await sql.unsafe('CREATE SCHEMA '+schema);
 for(const name of ['schema','library'])await sql.unsafe(readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8'));
 await sql.unsafe('CREATE ROLE '+http+' NOLOGIN');
 await run(boardReadSchema(http).replaceAll('wfsim_board_read',role).replace("default_tablespace='wfsim_data'","default_tablespace='pg_default'").replace('ON DATABASE wfsim','ON DATABASE '+database));
 await sql.unsafe('GRANT USAGE ON SCHEMA '+schema+' TO '+role+','+http);
 const [{source}]=await sql.unsafe('SELECT source FROM state_board_input_source');
 await assert.rejects(run(boardRankSchema({...options,httpRole:schema+'_absent'})),error=>error.code==='42704');
 assert.equal((await sql.unsafe("SELECT to_regclass('state_board_rank_partitions') name"))[0].name,null,'grant failure rolls back the complete install');
 await run(boardRankSchema(options));
 assert.equal((await sql.unsafe('SELECT source FROM state_board_input_source'))[0].source,source);
 await assert.rejects(run(boardRankSchema(options)),/explicit upgrade required/);
 reader=postgres(url,{max:1,fetch_types:false,onnotice(){},connection:{search_path:schema,role,statement_timeout:10000}});
 web=postgres(url,{max:1,fetch_types:false,onnotice(){},connection:{search_path:schema,role:http,statement_timeout:10000}});
 await checkPublicHeads({sql,web,options,run});
 for(const statement of ["UPDATE entity_weapon_builds SET at='2000-01-01' WHERE false",'UPDATE state_board_input_source SET source=source WHERE false',
  'UPDATE state_board_input_mailbox SET version=version WHERE false','SELECT body FROM state_board_input_mailbox','UPDATE state_board_read_partitions SET enabled=false WHERE false'])
  await assert.rejects(reader.unsafe(statement),error=>error.code==='42501');
 await reader.unsafe('SELECT board_rank_source_lock($1::text)',[source]);
 await assert.rejects(reader.unsafe('SELECT board_rank_source_lock($1::text)',['different-source']),/board source changed/);
 await reader.unsafe(`CREATE TEMP TABLE state_board_input_source(id integer,source text);INSERT INTO pg_temp.state_board_input_source VALUES(1,'fake-source')`);
 await assert.rejects(reader.unsafe('SELECT board_rank_source_lock($1::text)',['fake-source']),/board source changed/);
 await reader.unsafe('SELECT board_rank_source_lock($1::text)',[source]);
 await reader.unsafe("INSERT INTO state_board_rank_partitions(weapon,ruler,source,native,cut,input_sha256) VALUES('fixture','ruler',$1,'native','cut','sha')",[source]);
 await sql.unsafe(`INSERT INTO state_board_read_partitions VALUES('fixture','ruler','scene',true,true,'revision');
  INSERT INTO record_board_read_snapshots VALUES('fixture','ruler','revision','${source}','engine','sha',0,'[]','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z');
  INSERT INTO state_board_rank_generations(token,weapon,ruler,source,native,cut,engine,index_revision,ready_version,phase,directory,winners_sha256,output_sha256,captured_at,input_sha256)
   VALUES('00000000-0000-4000-8000-000000000001','fixture','ruler','${source}','native','cut','engine',1,1,'published','unused','sha','sha','2026-01-01T00:00:00.000Z','sha');
  INSERT INTO record_board_rank_snapshot_refs VALUES('fixture','ruler','revision','00000000-0000-4000-8000-000000000001')`);
 await assert.rejects(run(boardRankRollbackSchema({schema,httpRole:http})),/revisions still retained/);
 assert.equal((await web.unsafe("SELECT board_read_page('fixture','ruler',NULL,0,128) page"))[0].page.ok,true);
 await sql.unsafe('DELETE FROM record_board_rank_snapshot_refs');
 await assert.rejects(run(boardRankRollbackSchema({schema,httpRole:http})),/recovery boundary/);
 await sql.unsafe('DELETE FROM state_board_rank_generations;DELETE FROM state_board_rank_partitions');
 await run(boardRankRollbackSchema({schema,httpRole:http}));
 assert.equal((await sql.unsafe("SELECT to_regclass('state_board_rank_partitions') name"))[0].name,null);
 assert.equal((await sql.unsafe('SELECT source FROM state_board_input_source'))[0].source,source);
 assert.equal((await web.unsafe("SELECT board_read_page('fixture','ruler',NULL,0,128) page"))[0].page.ok,true,'legacy complete publication survives rollback');
 console.log('ok — atomic owned install/reinstall refusal, restricted reader, namespace/source guards, retained-ref rollback refusal and legacy rollback');
}finally{await web?.end();await reader?.end();await sql.unsafe('DROP SCHEMA IF EXISTS '+schema+' CASCADE');for(const name of [role,http]){await sql.unsafe('DROP OWNED BY '+name).catch(()=>{});await sql.unsafe('DROP ROLE IF EXISTS '+name);}await sql.end();}
