// SPDX-License-Identifier: AGPL-3.0-or-later
// Compare durable changed-identity ranking with the unchanged complete Native projector.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,mkdtempSync,rmSync,mkdirSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {execFileSync} from 'node:child_process';
import {createHash,randomBytes} from 'node:crypto';
import postgres from './pg/node_modules/postgres/src/index.js';
import {lockRankInputs} from './board_rank_keys.mjs';
import {openInventoryOperator} from './inventory_operator.mjs';
import {cachedRankInputs} from './board_rank_inputs.mjs';
import {prepareRankSources,createRankPartition,applyRankSources,prepareRankSnapshot,BoardRankConflict} from './board_rank_index.mjs';
const url=process.env.WFSIM_BOARD_RANK_TEST_URL;
assert(url&&['127.0.0.1','localhost','[::1]'].includes(new URL(url).hostname),'use an isolated local PostgreSQL');
const binary=resolve(process.env.WFSIM_INVENTORY_BOARD_BIN||'target/debug/wfsim-board');
const schema='rank_index_'+randomBytes(6).toString('hex');
const sql=postgres(url,{max:3,fetch_types:false,onnotice(){},connection:{search_path:schema,statement_timeout:10000}});
const work=mkdtempSync(join(tmpdir(),'rank-index-')),source='00000000-0000-4000-8000-000000000042';
const native=await openInventoryOperator(binary),counts={};
const operate={...native,call:async(op,items)=>{counts[op]=(counts[op]||0)+items.length;
 const result=await native.call(op,items);return op==='floor'&&process.argv.includes('--break-floor')?result.map(()=>({keep:true})):result;}};
const ruler='standard_single_target',base=JSON.parse(readFileSync(new URL('../tests/gas/rows.json',import.meta.url)))[1].record;
const records=Array.from({length:32},(_,i)=>({...base,mods:[i%11===10?'serration':'serration@'+(i%11),
 'vital_sense@'+Math.floor(i/11),'hellfire','speed_trigger','galvanized_aptitude','hammer_shot','vigilante_fervor','magnetic_capacity']}));
const described=await native.call('describe',records.map((record,i)=>({id:'fixture-'+i,record})));
assert(described.every(row=>row.candidate));
const inputs=records.map((record,i)=>({identity:described[i].candidate,record,facts:described[i].modes.map((mode,n)=>({identity:described[i].candidate,ruler,mode,
 score:i===3?500:i===4?100:i===5?99.999999:150+i+n*0.25,metric:'kpm',measured_by:'verified:8ce96e275dc2',started_at:'2026-11-16T00:00:00.000Z',finished_at:'2026-11-16T00:00:01.123Z'}))}));
const weapon=base.weapon;let part;
const current=new Map(inputs.map(row=>[row.identity,structuredClone(row)]));
async function apply(batch,options={}){
 const cached=await cachedRankInputs(sql,part,batch);
 const prepared=await prepareRankSources(operate,{weapon,ruler,inputs:batch,cached});
 part=await sql.begin(tx=>applyRankSources(tx,part,prepared,options));
 return prepared;
}
async function oracle(label){
 const directory=join(work,'oracle-'+label);mkdirSync(directory);
 const all=[...current.values()].filter(row=>row.record).sort((a,b)=>a.identity<b.identity?-1:1);
 const facts=join(directory,'facts.ndjson');writeFileSync(facts,all.flatMap(row=>row.facts).map(row=>JSON.stringify(row)).join('\n')+'\n');
 writeFileSync(join(directory,'state.yaml'),'boards: {}\n');
 execFileSync(binary,['--project',directory,'--facts-in',facts,'--state',join(directory,'state.yaml')],{input:JSON.stringify(all.map(row=>row.record)),stdio:['pipe','ignore','pipe'],timeout:60000});
 const scene=native.contract.rulers.find(row=>row.id===ruler).scene;
 const expected=JSON.parse(readFileSync(join(directory,scene,weapon+'.json'))).filter(row=>row.benchmark===ruler);
 const index=JSON.parse(readFileSync(join(directory,scene,'index.json')));
 const got=await prepareRankSnapshot(sql,operate,{weapon,ruler,source,work});
 assert.deepEqual(got.rows,expected,label+' complete rows/order/ranked mods/measurement clocks');
 assert.deepEqual(got.leaders,(index[weapon]||[]).filter(row=>row.benchmark===ruler),label+' complete leaders');
 console.log('  ok  '+label+' complete Native differential: '+got.rows.length+' rows');
 return got;
}
try{
 await sql.unsafe('CREATE SCHEMA '+schema);
 const ids=inputs.map(x=>x.identity).reverse(),reviews=['z','a','a'];
 const keys=[...new Set([...ids.map(x=>'build/'+x),...reviews.map(x=>'review/'+x)])].map(k=>source+'/board-rank/'+k);
 const tryKeys=tx=>tx.unsafe('SELECT pg_try_advisory_xact_lock(hashtextextended(key,0)) free FROM jsonb_array_elements_text($1::text::jsonb) a(key)',[JSON.stringify(keys)]);
 await sql.begin(async tx=>{
  let calls=0;
  await lockRankInputs({unsafe:(query,params)=>{calls++;return tx.unsafe(query,params);}},source,{identities:ids,reviews});
  assert.equal(calls,1,'all dependency locks must use one database round trip');
  assert((await sql.begin(tryKeys)).every(r=>!r.free),'routing and baseline writers lock the exact same identity and review keys');
 });
 assert((await sql.begin(tryKeys)).every(r=>r.free),'transaction end releases every dependency key');
 await sql.unsafe(readFileSync(new URL('../worker/pg/board_mailbox.sql',import.meta.url),'utf8'));
 await sql.unsafe(readFileSync(new URL('../worker/pg/board_pages.sql',import.meta.url),'utf8'));
 for(const file of ['board_rank','board_rank_compact'])await sql.unsafe(readFileSync(new URL('../worker/pg/'+file+'.sql',import.meta.url),'utf8'));
 await sql.unsafe('UPDATE state_board_input_source SET source=$1',[source]);
 part=await createRankPartition(sql,{weapon,ruler,source,native:native.adapter,cut:'fixture-cut'});
 await apply(inputs,{cursor:32,loaded:32});
 await sql.unsafe('UPDATE state_board_rank_partitions SET complete=true WHERE weapon=$1 AND ruler=$2',[weapon,ruler]);
 part=(await sql.unsafe('SELECT * FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2',[weapon,ruler]))[0];
 await oracle('baseline');
 const probe=await prepareRankSources(operate,{weapon,ruler,inputs});
 await sql.unsafe('ANALYZE state_board_rank_rows');
 part=await sql.begin(async tx=>{
  await tx.unsafe('SET LOCAL enable_seqscan=off');
  let checked=false;
  const traced={unsafe:async(query,params)=>{
   if(query.startsWith('INSERT INTO state_board_rank_winners')){
    const [{ 'QUERY PLAN': plan }]=await tx.unsafe('EXPLAIN (FORMAT JSON) '+query,params);
    function bounded(node){
     return (node['Node Type']==='Limit'&&node.Plans?.some(p=>p['Index Name']==='board_rank_shape_order'))||
      (node.Plans||[]).some(bounded);
    }
    assert(bounded(plan[0].Plan),'each affected shape must read its first row from the ordering index');
    checked=true;
   }
   return tx.unsafe(query,params);
  }};
  const updated=await applyRankSources(traced,part,probe);assert(checked);return updated;
 });
 const changed=structuredClone(inputs[3]);changed.facts=changed.facts.map(row=>({...row,score:10,finished_at:'2026-11-16T01:00:00.000Z'}));
 counts.describe=0;counts.render=0;await apply([changed],{live:true});current.set(changed.identity,changed);
 assert.equal(counts.describe,0,'a fact-only update reuses its Native descriptor');
 assert.equal(counts.render,changed.facts.length,'only the changed identity is rendered');
 await oracle('changed-fact');
 await apply([inputs[3]],{cursor:33,loaded:33});await oracle('late-baseline-cannot-overwrite-live');
 const removed={identity:inputs[0].identity,record:null,facts:[]};await apply([removed],{live:true});current.delete(removed.identity);
 await oracle('removed-source');
 const stale=part,prepared=await prepareRankSources(operate,{weapon,ruler,inputs:[inputs[1]]});
 await apply([inputs[2]],{live:true});
 await assert.rejects(sql.begin(tx=>applyRankSources(tx,stale,prepared,{live:true})),error=>error instanceof BoardRankConflict);
 await assert.rejects(createRankPartition(sql,{weapon,ruler,source:'different',native:native.adapter,cut:'fixture-cut'}),/boundary changed/);
 await assert.rejects(prepareRankSources(operate,{weapon,ruler,inputs:[{...inputs[1],identity:'unknown-alias'}]}),/identities require qualification/);
 await oracle('stale-source-and-alias-refusal');
 const [leader]=await sql.unsafe('SELECT identity FROM state_board_rank_winners WHERE weapon=$1 AND ruler=$2 ORDER BY score DESC LIMIT 1',[weapon,ruler]);
 const winner=structuredClone(current.get(leader.identity));await apply([winner],{live:true});
 assert.equal(part.winnersChanged,false,'identical Native winners preserve the published projection');
 winner.facts=winner.facts.map(row=>({...row,finished_at:'2026-11-18T02:00:00.000Z'}));
 await apply([winner],{live:true});current.set(winner.identity,winner);
 assert.equal(part.winnersChanged,true,'a changed public measurement time with the same score still changes the winner');
 await oracle('same-score-updated-wire');
 const riven={weapon:'coda_catabolyst',mods:['frostbite','riven','galvanized_diffusion','galvanized_shot',
  'lethal_torrent','pistol_elementalist','pressurized_magazine','critical_mutation'],
  arcanes:['secondary_enervate'],exilus:'ruinous_extension',valence:'heat',
  riven_pos:['critical_damage','electricity','viral'],riven_neg:'zoom'};
 const rivenRecords=[[1.02,0.99,1.08,1.03],[1.1,1.1,1.1,0.9]].map(riven_rolls=>({...riven,riven_rolls}));
 const rivenDescriptors=await native.call('describe',rivenRecords.map((record,i)=>({id:`riven-${i}`,record})));
 assert.notEqual(rivenDescriptors[0].admit,rivenDescriptors[1].admit,'current Native never admits the actual-roll input');
 const rivenInputs=rivenDescriptors.map(({candidate:identity},i)=>({identity,record:rivenRecords[i],facts:[
  {...inputs[0].facts[0],identity,mode:'base',score:90-i*10}]}));
 const cached=rivenInputs.map(({identity,record},i)=>({identity,record_sha256:createHash('sha256').update(JSON.stringify(record)).digest('hex'),
  descriptor:{...rivenDescriptors[i],admit:rivenDescriptors[1].admit}}));
 const scope={weapon:riven.weapon,ruler,inputs:rivenInputs,cached};
 const original={...native,call:(op,items)=>op==='standard_riven'?items.map(()=>({standard:true})):native.call(op,items)};
 assert.deepEqual((await prepareRankSources(original,scope)).rows.map(row=>row.score),[90,80],'the old cached mask would admit the actual-roll score');
 counts.describe=0;
 assert.deepEqual((await prepareRankSources(operate,scope)).rows.map(row=>row.score),[80]);
 assert.equal(counts.describe,0,'riven eligibility applies without changing pinned Native descriptions');
 console.log('  ok  real PostgreSQL Native index, live precedence, stale preparation and recovery-boundary checks');
}finally{
 await native.close();await sql.unsafe('DROP SCHEMA IF EXISTS '+schema+' CASCADE');await sql.end();rmSync(work,{recursive:true,force:true});
}
