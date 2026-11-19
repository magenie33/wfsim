// SPDX-License-Identifier: AGPL-3.0-or-later
// A populated legacy reader can upgrade, mix both representations, restart and recover the exact same headers.
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import postgres from './pg/node_modules/postgres/src/index.js';
import {openInventoryOperator} from './inventory_operator.mjs';
import {createRankPartition,prepareRankSources,applyRankSources} from './board_rank_index.mjs';
import {cachedRankInputs,registeredRankInputs} from './board_rank_inputs.mjs';
import {boardRankCompactUpgrade,boardRankCompactRollback} from './board_rank_compact_schema.mjs';

const url=process.env.WFSIM_BOARD_RANK_TEST_URL;assert(url&&['127.0.0.1','localhost'].includes(new URL(url).hostname));
const schema='rank_inputs_'+randomUUID().replaceAll('-','').slice(0,12);
const connect=()=>postgres(url,{max:3,fetch_types:false,onnotice(){},connection:{search_path:schema,statement_timeout:6000}});
let sql=connect();
const native=await openInventoryOperator(resolve(process.env.WFSIM_INVENTORY_BOARD_BIN||'target/debug/wfsim-board'));
const [base]=JSON.parse(readFileSync('tests/gas/rows.json')).slice(1,2),record=base.record,weapon=record.weapon;
const [d]=await native.call('describe',[{id:'fixture',record}]),identity=d.candidate;assert(identity);
const rulers=['standard_single_target','standard_multi_target','standard_single_target__aimed_half'];
const mode=d.publication_modes[0].id,version='9007199254740997',quoted=(r,m)=>'['+[identity,r,m].map(JSON.stringify).join(', ')+']';
const deps=r=>[{scope:'build',key:identity,version:null},{scope:'measurement',key:quoted(r,mode),version},
 {scope:'measurement',key:quoted(r,'missing'),version:null},{scope:'review',key:'removed/review',version:'7'}];
const run=async script=>{const pin=await sql.reserve();try{return await pin.unsafe(script);}catch(e){await pin.unsafe('ROLLBACK');throw e;}finally{pin.release();}};
const header=async()=>sql.unsafe('SELECT * FROM state_board_rank_dependencies ORDER BY weapon,ruler,identity,scope,key');
const call=async q=>(await sql.unsafe(q))[0];
try{
 await sql.unsafe('CREATE SCHEMA '+schema);
 for(const f of ['board_mailbox','board_pages','board_rank'])await sql.unsafe(readFileSync('worker/pg/'+f+'.sql','utf8'));
 await sql.unsafe('CREATE TABLE entity_weapon_builds(id text PRIMARY KEY)');
 const {role}=await call('SELECT current_user role'),{source}=await call('SELECT source FROM state_board_input_source');
 const options={schema,readerRole:role,tablespace:'pg_default'};
 const parts=[];
 for(const ruler of rulers)parts.push(await createRankPartition(sql,{weapon,ruler,source,native:native.adapter,cut:'fixed'}));
 const input=(i,body=record)=>({identity,record:body,facts:[],dependencies:deps(rulers[i])});
 let described=0;
 const operator={...native,call:async(op,items)=>{if(op==='describe')described+=items.length;return native.call(op,items);}};
 const prepare=async(i,body=record)=>{const inputs=[input(i,body)],cached=await cachedRankInputs(sql,parts[i],inputs);
  return prepareRankSources(operator,{weapon,ruler:rulers[i],inputs,cached});};
 const put=async(i,p)=>{parts[i]=await sql.begin(tx=>applyRankSources(tx,parts[i],p,{live:true}));};
 const original=await prepareRankSources(native,{weapon,ruler:rulers[0],inputs:[input(0)]});
 const s=original.sources[0];
 await sql.unsafe('INSERT INTO state_board_rank_sources VALUES($1,$2,$3,$4,$5::text::jsonb,$6,true)',
  [weapon,rulers[0],identity,s.record_sha256,JSON.stringify(s.descriptor),s.input_sha256]);
 for(const dep of s.dependencies)await sql.unsafe('INSERT INTO state_board_rank_dependencies VALUES($1,$2,$3,$4,$5,$6)',
  [weapon,rulers[0],identity,dep.scope,dep.key,dep.version]);
 const before=await header(),oid=(await call("SELECT 'state_board_rank_dependencies'::regclass::oid::text oid")).oid;
 await run(boardRankCompactUpgrade(options));
 assert.deepEqual(await header(),before);
 assert.equal((await call("SELECT 'state_board_rank_legacy_dependencies'::regclass::oid::text oid")).oid,oid);
 assert.equal((await registeredRankInputs(sql,{scope:'measurement',key:quoted(rulers[0],mode)}))[0].version,version);
 await assert.rejects(run(boardRankCompactUpgrade(options)));
 await put(1,await prepare(1));assert.equal(described,1);
 await put(2,await prepare(2));assert.equal(described,1);
 await put(0,await prepare(0));assert.equal(described,1);
 assert.equal(Number((await call('SELECT count(*) n FROM state_board_rank_descriptions')).n),1);
 assert.equal(Number((await call('SELECT count(*) n FROM state_board_rank_legacy_dependencies')).n),0);
 const all=await header();assert.equal(all.length,12);
 assert.equal((await registeredRankInputs(sql,{scope:'review',key:'removed/review'})).length,3);
 assert.equal((await registeredRankInputs(sql,{scope:'measurement',key:JSON.stringify([identity,rulers[0],mode])})).length,0);
 assert.equal((await registeredRankInputs(sql,{scope:'build',key:identity})).length,3);
 assert.equal((await registeredRankInputs(sql,{scope:'measurement',key:quoted(rulers[1],'missing')}))[0].version,null);
 const [oldWriter]=await sql.unsafe("SELECT pg_relation_is_updatable('state_board_rank_dependencies'::regclass,false) mask");
 assert.equal(Number(oldWriter.mask),0);
 const changedRecord={...record,unused_capture_note:'same canonical build'};
 await put(2,await prepare(2,changedRecord));assert.equal(Number((await call('SELECT count(*) n FROM state_board_rank_descriptions')).n),2);
 await put(2,await prepare(2));assert.equal(Number((await call('SELECT count(*) n FROM state_board_rank_descriptions')).n),1);
 const repeated=await prepare(1);
 repeated.sources[0].dependencies.push(repeated.sources[0].dependencies[0]);
 await assert.rejects(put(1,repeated),/duplicate or misplaced/);
 const collision=await prepare(1);collision.sources[0].descriptor={...collision.sources[0].descriptor,corrupt:true};
 await assert.rejects(put(1,collision),/immutable Native description/);
 await sql.end();sql=connect();assert.deepEqual(await header(),all);
 await assert.rejects(run(boardRankCompactRollback(options)),/expand compact inputs/);
 const frozen=await sql.unsafe('SELECT weapon,ruler,revision,input_sha256 FROM state_board_rank_partitions ORDER BY weapon,ruler');
 assert.equal((await call('SELECT board_rank_expand_inputs(1) n')).n,1);
 assert.equal(Number((await call('SELECT count(*) n FROM state_board_rank_descriptions')).n),1);
 assert.deepEqual(await header(),all);
 assert.equal((await call('SELECT board_rank_expand_inputs(64) n')).n,2);
 assert.equal(Number((await call('SELECT count(*) n FROM state_board_rank_descriptions')).n),0);
 await run(boardRankCompactRollback(options));assert.deepEqual(await header(),all);
 assert.equal((await call("SELECT 'state_board_rank_dependencies'::regclass::oid::text oid")).oid,oid);
 for(const row of await sql.unsafe('SELECT descriptor FROM state_board_rank_sources'))assert.deepEqual(row.descriptor,s.descriptor);
 assert.deepEqual(await sql.unsafe('SELECT weapon,ruler,revision,input_sha256 FROM state_board_rank_partitions ORDER BY weapon,ruler'),frozen);
 console.log('ok — mixed storage, exact large and missing versions, shared Native cache, recovery, disagreement rollback, old reader and original table identity');
}finally{await native.close();await sql.unsafe('DROP SCHEMA IF EXISTS '+schema+' CASCADE');await sql.end();}
