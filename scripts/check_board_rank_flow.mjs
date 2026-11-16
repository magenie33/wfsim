// SPDX-License-Identifier: AGPL-3.0-or-later
// Exercise the complete canonical-mailbox to staged-page path through actual Native and PostgreSQL.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,chmodSync,mkdtempSync,rmSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {randomBytes} from 'node:crypto';
import {setTimeout as pause} from 'node:timers/promises';
import postgres from './pg/node_modules/postgres/src/index.js';
import {pgDb,postgresClient} from '../worker/db.js';
import worker from '../worker/index.js';
import {openInventoryOperator} from './inventory_operator.mjs';
import {openBoardRankFlow} from './board_rank_flow.mjs';
import {prepareRankSnapshot,applyRankSources,BoardRankConflict,BoardRankQualificationError} from './board_rank_index.mjs';
import {prepareRankIdentity,routeRankInput,putRankPublication} from './board_rank_work.mjs';
import {claimRankPublication} from './board_rank_publication.mjs';
import {collectRankStorage} from './board_rank_gc.mjs';
import {prepareRankBaselinePage} from './board_rank_baseline.mjs';
import {boardRankSchema} from './board_rank_schema.mjs';
const url=process.env.WFSIM_BOARD_RANK_TEST_URL;assert(url&&['localhost','127.0.0.1'].includes(new URL(url).hostname));
const schema='rank_flow_'+randomBytes(6).toString('hex'),role=schema+'_reader',source='00000000-0000-4000-8000-000000000044';
const sql=postgres(url,{max:4,fetch_types:false,onnotice(){},connection:{search_path:schema,statement_timeout:10000}});
const binary=process.env.WFSIM_INVENTORY_BOARD_BIN||'target/debug/wfsim-board',native=await openInventoryOperator(binary);
const work=mkdtempSync(join(tmpdir(),'rank-flow-')),engine='8ce96e275dc2',ruler='standard_single_target';
const base=JSON.parse(readFileSync(new URL('../tests/gas/rows.json',import.meta.url)))[1].record,weapon=base.weapon;
const records=Array.from({length:90},(_,i)=>({...base,mods:['serration@'+i%10,'vital_sense@'+Math.floor(i/10)%5,'hellfire@'+Math.floor(i/50),
 'speed_trigger','galvanized_aptitude','hammer_shot','vigilante_fervor','magnetic_capacity']}));
const described=await native.call('describe',records.map((record,i)=>({id:'fixture-'+i,record})));
const inputs=records.map((record,i)=>({identity:described[i].candidate,record,facts:described[i].modes.map(mode=>({identity:described[i].candidate,ruler,mode,
 score:100+i,metric:'kpm',measured_by:'verified:'+engine,finished_at:'2026-11-16T00:00:01.123Z',retired_at:null,review_id:i===80?'review-before-registration':null}))}));
const context={work,source,engine,native:native.adapter},open=()=>openBoardRankFlow({sql:reader,...context,binary,nativeSha:native.adapter,requestedPartitions:true});
let flow,reader;
const read=async(revision=null,after=0)=>{
 const params=new URLSearchParams({weapon,ruler,after:String(after)});if(revision)params.set('revision',revision);
 const response=await worker.fetch(new Request('https://wfsim.invalid/api/board/pages?'+params),{LIBRARY:pgDb(postgresClient(sql))},{waitUntil(){}});
 const body=await response.json();assert(response.status===200||body.error==='not_ready');return body;
};
async function until(predicate,label){
 const started=Date.now();let last;
 while(Date.now()-started<30000){last=await flow.step();if(await predicate(last))return last;await pause(1);}
 throw Error('timed out '+label+' '+JSON.stringify(last));
}
async function put(scope,key,body){return (await sql.unsafe('SELECT board_input_put($1,$2,$3) v',[scope,key,body===null?null:JSON.stringify(body)]))[0].v;}
async function exact(label){
 const reference=await prepareRankSnapshot(sql,native,{weapon,ruler,source,work}),rows=[];let after=0,revision;
 do{const page=await read(revision,after);assert(page.ok);revision=page.revision;rows.push(...page.rows);after=page.next;}while(after!==null);
 assert.deepEqual(rows,reference.rows,label);return {rows,revision};
}
try{
 await sql.unsafe('CREATE SCHEMA '+schema);
 for(const name of ['board_mailbox','board_pages'])await sql.unsafe(readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8'));
 await sql.unsafe('UPDATE state_board_input_source SET source=$1',[source]);
 await sql.unsafe(`CREATE SEQUENCE compute_queue_position;
  CREATE TABLE state_board_requests(id text,phase text,intent text);
  CREATE TABLE state_board_request_control(id integer,enabled boolean);
  INSERT INTO state_board_request_control VALUES(1,true)`);
 await sql.unsafe(readFileSync(new URL('../worker/pg/board_requested_partitions.sql',import.meta.url),'utf8'));
 await sql.unsafe(`CREATE TABLE entity_weapon_builds(id text PRIMARY KEY,weapon text,at text,record text);
  CREATE TABLE board_facts(identity text,ruler text,mode text,score float8,metric text,measured_by text,finished_at text,retired_at text,review_id text);
  CREATE TABLE entity_measurement_reviews(id text);
  CREATE INDEX ON board_facts(identity,ruler,mode)`);
 const database=new URL(url).pathname.slice(1);assert(/^[a-z0-9_]+$/.test(database));
 await sql.unsafe(readFileSync(new URL('../deploy/board_read_grants.sql',import.meta.url),'utf8').replaceAll('wfsim_board_read',role).replace('ON DATABASE wfsim','ON DATABASE '+database));
 const [{current_user:owner}]=await sql.unsafe('SELECT current_user');
 const installer=await sql.reserve();
 try{await installer.unsafe(boardRankSchema({schema,readerRole:role,httpRole:owner,tablespace:'pg_default'}));}finally{installer.release();}
 await sql.unsafe('GRANT USAGE ON SCHEMA '+schema+' TO '+role);
 await sql.unsafe('GRANT EXECUTE ON FUNCTION board_requested_partition_enqueue(text,uuid),board_requested_partition_selected(text,text,boolean),board_requested_partition_activate(text,text,text,uuid) TO '+role);
 reader=postgres(url,{max:4,fetch_types:false,onnotice(){},connection:{search_path:schema,role,statement_timeout:10000,lock_timeout:2000}});
 await sql.unsafe(`INSERT INTO entity_weapon_builds SELECT x.identity,$2,'2026-11-16T00:00:00.000Z',x.record::text FROM jsonb_to_recordset(($1::text)::jsonb)x(identity text,record jsonb)`,[JSON.stringify(inputs),weapon]);
 await sql.unsafe(`INSERT INTO board_facts SELECT x.* FROM jsonb_to_recordset(($1::text)::jsonb)x(identity text,ruler text,mode text,score float8,metric text,measured_by text,finished_at text,retired_at text,review_id text)`,[JSON.stringify(inputs.flatMap(row=>row.facts))]);
 await sql.unsafe(`SELECT board_input_seed('build',id,jsonb_build_object('identity',id,'weapon',weapon)::text) FROM entity_weapon_builds`);
 await sql.unsafe(`SELECT board_input_seed('measurement',jsonb_build_array(identity,ruler,mode)::text,jsonb_build_object('identity',identity,'ruler',ruler,'mode',mode)::text) FROM board_facts`);
 await sql.unsafe('INSERT INTO state_board_read_partitions VALUES($1,$2,$3,true,true,NULL)',[weapon,ruler,native.contract.rulers.find(row=>row.id===ruler).scene]);
 flow=await open();
 await until(row=>row.status==='baseline-page','first baseline page');
 assert.equal((await read()).error,'not_ready','partial baseline is not public');
 const [partial]=await sql.unsafe('SELECT * FROM state_board_rank_partitions');assert(Number(partial.loaded)>0&&Number(partial.loaded)<90);
 const original=readFileSync(partial.path),corrupt=Buffer.from(original);corrupt[1]^=1;
 chmodSync(partial.path,0o640);writeFileSync(partial.path,corrupt);
 await assert.rejects(prepareRankBaselinePage(sql,native,partial,{work}),/digest changed/);
 writeFileSync(partial.path,original);chmodSync(partial.path,0o440);
 await flow.close();flow=await open();
 const changed=inputs[0];await sql.unsafe('UPDATE board_facts SET score=500,finished_at=$2 WHERE identity=$1',[changed.identity,'2026-11-17T00:01:02.123Z']);
 for(const fact of changed.facts)await put('measurement','['+[changed.identity,ruler,fact.mode].map(JSON.stringify).join(', ')+']',{identity:changed.identity,ruler,mode:fact.mode,score:500});
 // The deleted review routes before its captured identity has registered any dependencies.
 await sql.unsafe('DELETE FROM board_facts WHERE review_id=$1',['review-before-registration']);
 const version=await put('review','review-before-registration',null);
 await sql.begin(tx=>routeRankInput(tx,{scope:'review',key:'review-before-registration',version,body:null,deleted:true},source));
 await until(row=>row.status==='published','resumed complete publication');
 const first=await exact('resumed baseline plus dirty closure retains every exact row');
 assert(first.rows.some(row=>row.score===500));
 assert.equal(first.rows.length,178,'both modes of the deleted review are removed');
 assert(!first.rows.some(row=>row.score===180),'a review deletion before dependency registration cannot reappear');
 assert.equal((await sql.unsafe('SELECT count(*) n FROM state_board_rank_baseline_obligations o JOIN state_board_rank_dirty d USING(weapon,ruler,identity) WHERE d.applied_version<o.version'))[0].n,'0');
 assert(Number((await sql.unsafe('SELECT count(*) n FROM state_board_rank_baseline_obligations'))[0].n)<10,'coherent baseline headers avoid re-normalizing the complete history');
 const [part]=await sql.unsafe('SELECT * FROM state_board_rank_partitions');
 // A selected review can disappear after Native preparation, before dependency registration.
 const target=inputs[1];await sql.unsafe("UPDATE board_facts SET review_id='late-selected-review' WHERE identity=$1",[target.identity]);
 const stale=await prepareRankIdentity(sql,native,part,target.identity);
 await sql.unsafe('DELETE FROM board_facts WHERE identity=$1',[target.identity]);
 const reviewVersion=await put('review','late-selected-review',null);
 await sql.begin(tx=>routeRankInput(tx,{scope:'review',key:'late-selected-review',version:reviewVersion,body:null,deleted:true},source));
 await assert.rejects(sql.begin(tx=>applyRankSources(tx,part,stale,{live:true})),error=>error instanceof BoardRankConflict,
  'a routed review tombstone cannot be acknowledged by stale Native work');
 const fresh=await prepareRankIdentity(sql,native,part,target.identity);
 await sql.begin(tx=>applyRankSources(tx,part,fresh,{live:true}));
 const before=await prepareRankIdentity(sql,native,part,changed.identity);
 await sql.unsafe('DELETE FROM entity_weapon_builds WHERE id=$1',[changed.identity]);await sql.unsafe('DELETE FROM board_facts WHERE identity=$1',[changed.identity]);
 await put('build',changed.identity,null);
 await until(row=>row.status==='published','source removal publication');
 const second=await exact('canonical deletion produces a complete replacement');assert.notEqual(second.revision,first.revision);
 assert.deepEqual((await read(first.revision)).rows,first.rows.slice(0,128),'pinned older version remains readable');
 assert(before.sources[0].dependencies.some(row=>row.scope==='measurement'),'dependencies include valid measurement keys');
 // The source epoch is checked at write time, including a previously prepared input.
 await sql.unsafe("UPDATE state_board_input_source SET source='changed-source'");
 await assert.rejects(sql.unsafe('SELECT board_rank_source_lock($1::text)',[source]),/board source changed/);
 await sql.unsafe('UPDATE state_board_input_source SET source=$1',[source]);
 // Cold files are reclaimed only after the completed cache no longer needs them.
 for(let i=0;i<10;i++)await collectRankStorage(sql,context);
 assert.equal((await sql.unsafe('SELECT path FROM state_board_rank_partitions'))[0].path,'');
 await flow.close();flow=await open();assert.deepEqual((await read()).rows,second.rows.slice(0,128));
 // A missing incomplete cut is durably blocked instead of rescanning every loop.
 await sql.unsafe("UPDATE state_board_rank_partitions SET complete=false,seeded=false,path=$1,available_at=board_input_timestamp(clock_timestamp())",[join(work,'rank-cut-00000000-0000-4000-8000-000000000000','inputs.ndjson')]);
 await until(row=>row.status==='baseline_error','missing cut refusal');
 assert.equal((await sql.unsafe('SELECT available_at FROM state_board_rank_partitions'))[0].available_at,null);
 for(let i=0;i<15;i++)assert.notEqual((await flow.step()).status,'baseline_error');
 const other='demolisher',unknown=native.contract.rulers.find(row=>![ruler,other].includes(row.id)).id;
 await sql.unsafe('INSERT INTO board_facts SELECT identity,$2,mode,333,metric,measured_by,finished_at,NULL,NULL FROM board_facts WHERE identity=$1',[inputs[2].identity,other]);
 await sql.unsafe('INSERT INTO state_board_requested_partitions(weapon,ruler) VALUES($1,$2)',[weapon,other]);
 await until(async()=>!!(await sql.unsafe('SELECT enabled FROM state_board_read_partitions WHERE weapon=$1 AND ruler=$2',[weapon,other]))[0]?.enabled,'automatic requested-partition activation');
 const auto=(await sql.unsafe('SELECT board_read_page($1,$2,NULL,0,128) page',[weapon,other]))[0].page;
 assert(auto.ok&&auto.rows.some(row=>row.score===333),'a demanded partition is selected only after complete staging');
 assert((await sql.unsafe('SELECT activated_at FROM state_board_requested_partitions WHERE weapon=$1 AND ruler=$2',[weapon,other]))[0].activated_at);
 await sql.unsafe('UPDATE state_board_read_partitions SET enabled=false WHERE weapon=$1 AND ruler=$2',[weapon,other]);
 await sql.begin(tx=>putRankPublication(tx,{weapon,ruler:other}));
 assert.equal(await claimRankPublication(sql,{source,native:native.adapter}),null,'an operator-disabled partition cannot start another snapshot');
 await sql.unsafe('INSERT INTO entity_weapon_builds VALUES($1,$2,$3,$4)',['unknown-alias',weapon,'2026-11-16',JSON.stringify(base)]);
 await sql.unsafe('INSERT INTO board_facts VALUES($1,$2,$3,10,\'kpm\',$4,$5,NULL,NULL)',['unknown-alias',unknown,'base','verified:'+engine,'2026-11-16T00:00:00.000Z']);
 await sql.unsafe('INSERT INTO state_board_requested_partitions(weapon,ruler) VALUES($1,$2)',[weapon,unknown]);
 await until(row=>row.status==='baseline_error'&&row.blocked,'unknown alias refusal');
 const [refused]=await sql.unsafe('SELECT * FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2',[weapon,unknown]);
 assert.equal(refused.available_at,null);assert.match(refused.error,/identities require qualification/);
 assert.equal((await sql.unsafe('SELECT enabled FROM state_board_read_partitions WHERE weapon=$1 AND ruler=$2',[weapon,unknown]))[0].enabled,false);
 for(let i=0;i<15;i++)assert.notEqual((await flow.step()).status,'baseline_error');
 await flow.close();flow=null;
 await sql.unsafe("UPDATE state_board_rank_partitions SET native='changed-native'");
 await assert.rejects(open(),error=>error instanceof BoardRankQualificationError);
 console.log('ok — actual Native/PG complete flow, resumable baseline, late facts/deleted reviews, exact immutable pages, source deletion, epoch guards and durable missing-cut refusal');
}finally{await flow?.close();await native.close();await reader?.end();await sql.unsafe('DROP SCHEMA IF EXISTS '+schema+' CASCADE');await sql.unsafe('DROP OWNED BY '+role).catch(()=>{});await sql.unsafe('DROP ROLE IF EXISTS '+role);await sql.end();rmSync(work,{recursive:true,force:true});}
