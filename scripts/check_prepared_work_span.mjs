// SPDX-License-Identifier: AGPL-3.0-or-later
// Continuous input spans retain lifecycle obligations and match the independent full native walk.
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,readFileSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {execFileSync} from 'node:child_process';
import {openCatalog} from './incremental_inventory.mjs';
import {PreparedBoardInputs} from './prepared_board_inputs.mjs';
import {PreparedWorkInventory} from './prepared_work_inventory.mjs';
import {PreparedWorkAudit} from './audit_prepared_work.mjs';
import {prepareBoardCut} from './prepare_board_cut.mjs';
import {validateWorkSpan} from './prepared_work_consumer.mjs';
import {openInventoryOperator} from './inventory_operator.mjs';
import {classifyInventory} from './classify_compute_inventory.mjs';
import {questionId} from '../worker/task_contract.js';

const binary=resolve(process.env.WFSIM_INVENTORY_BOARD_BIN||'target/debug/wfsim-board'),root=mkdtempSync(join(tmpdir(),'work-span-'));
const native=await openInventoryOperator(binary),engine='0123456789ab',contract={v:1,record_storage:'interned-deflate-v1'};
const source=join(root,'inputs.sqlite'),records=openCatalog(source,{engine,contract,create:true}),inputs=new PreparedBoardInputs(records.db,{engine,normalizer:{v:1}});
const workPath=join(root,'work.sqlite'),auditPath=join(root,'audit.sqlite'),sources=new Map(),facts=new Map(),queue=new Map(),tasks=new Map();
let inventory,audit,serial=0,checked=0;
const file=(name,rows)=>{const path=join(root,name+'.ndjson');writeFileSync(path,rows.map(row=>JSON.stringify(row)+'\n').join(''));return path;};
const receipt=()=>({epoch:String(++serial),snapshot:'snapshot-'+serial,engine,started_at:'2026-11-14T00:00:00.000Z'});
function update(rows) {
 for(const {kind,row}of rows) {
  if(kind==='builds'){if(row.deleted)sources.delete(row.id);else sources.set(row.id,row);}
  else if(kind==='facts')facts.set(JSON.stringify([row.identity,row.ruler,row.mode]),row);
  else if(kind==='reset'){for(const [key,item]of queue)if(item.build_id===row.id)queue.delete(key);for(const [key,item]of facts)if(item.identity===row.id)facts.delete(key);}
  else if(kind==='queue')queue.set(JSON.stringify([row.build_id,row.ruler,row.mode]),row);
  else if(kind==='tasks')tasks.set(row.id,row);
 }
}
async function seal(rows) {
 update(rows);const cut={bootstrap:false,delta:file('delta-'+serial,rows),receipt:receipt()};let result;
 do{result=await prepareBoardCut(inputs,records,cut,native.call);}while(result.pending);return inputs.cut(result.cut);
}
const reopen=()=>{inventory?.close();audit?.close();inventory=new PreparedWorkInventory(workPath,source,contract,native.contract,{engine,create:!inventory});audit=new PreparedWorkAudit(auditPath);};
const step=options=>inventory.step(native.call,{budgetMs:0,audit:async(index,reader,cut,settings)=>{
 checked++;return audit.run(index,reader,cut,{...settings,scratchRoot:root});
},...options});
async function finish(target) {for(let n=0;inventory.consumer.state().acknowledged<target;n++){assert(n<10000);await step();}}
async function oracle(label) {
 const dir=join(root,'oracle-'+serial);mkdirSync(dir,{recursive:true});
 const raw=Object.fromEntries(Object.entries({builds:[...sources.values()],facts:[...facts.values()],queue:[...queue.values()],tasks:[...tasks.values()]})
  .map(([kind,rows])=>[kind,file('oracle-'+serial+'-'+kind,rows)]));
 const rulers=native.contract.rulers.map(row=>row.id);
 execFileSync(binary,['--each-ruler','--dry-run','--gate','--facts-in',raw.facts,'--queue-in',raw.queue,'--inventory-builds',join(dir,'canonical'),
  '--queue-missing',join(dir,'{id}.missing'),'--queue-orphans',join(dir,'{id}.orphans')],
 {input:JSON.stringify([...sources.values()].map(row=>row.record)),stdio:['pipe','ignore','pipe'],timeout:60000});
 for(const suffix of ['missing','orphans'])writeFileSync(join(dir,suffix),rulers.map(id=>readFileSync(join(dir,id+'.'+suffix),'utf8')).join(''));
 const expected=await classifyInventory({...raw,rulers,canonical:join(dir,'canonical'),missing:join(dir,'missing'),orphans:join(dir,'orphans'),
  questions:true,scratchRoot:root,meta:{engine,snapshot:'oracle',started_at:'2026-11-14T00:00:00Z',counts:{library:sources.size,queue:queue.size,tasks:tasks.size}}});
 assert.equal(inventory.catalog.digest(),expected.question_digest,label);
 for(const key of ['rows','queued','orders','verbs'])assert.deepEqual(JSON.parse(JSON.stringify(inventory.catalog.summary()[key])),JSON.parse(JSON.stringify(expected[key])),label+': '+key);
}
try {
 const raw=['magnetic_capacity','primed_magazine_warp','fast_hands'].map(last=>({weapon:'braton_prime',
  mods:['vital_sense','hellfire','speed_trigger','primary_acuity','galvanized_aptitude','hammer_shot','vigilante_fervor',last],
  evolutions:['braton_prime_evo1_incarnon_form','braton_prime_daring_reverie','braton_prime_voids_guidance','braton_prime_prelude_of_might'],arcanes:['primary_deadhead']}));
 const described=await native.call('describe',raw.map((record,n)=>({id:'seed-'+n,record}))),builds=described.map(row=>({id:row.candidate,record:row.record}));
 const [a,b,c]=builds,ruler=native.contract.rulers.find((_,target)=>described.every(row=>(BigInt('0x'+row.admit)&(1n<<BigInt(target)))!==0n)).id;
 const scored=(id,score,extra={})=>({identity:id,ruler,mode:'base',score,measured_by:'fixture',cost_seconds:1,
  started_at:'2026-11-14T00:00:00Z',finished_at:'2026-11-14T00:00:01Z',...extra});
 const initialFacts=[scored(a.id,100),scored(b.id,20),scored(c.id,19.9)],rawPaths={};
 for(const [kind,rows]of Object.entries({builds,facts:initialFacts,queue:[],tasks:[]}))rawPaths[kind]=file(kind,rows);
 update([...builds.map(row=>({kind:'builds',row})),...initialFacts.map(row=>({kind:'facts',row}))]);
 const bootstrap={bootstrap:true,raw:rawPaths,meta:{engine,snapshot:'snapshot-1',started_at:'2026-11-14T00:00:00.000Z',counts:{library:3,facts:3,queue:0,tasks:0}},receipt:receipt()};
 let first;do{first=await prepareBoardCut(inputs,records,bootstrap,native.call);}while(first.pending);
 reopen();await finish(first.cut);await oracle('cold inclusive entry floor');assert.equal(checked,1);
 const child={weapon:'torid',arcanes:['primary_deadhead'],exilus:'vigilante_supplies',
  evolutions:['torid_evo1_incarnon_form','torid_final_fusillade','torid_swift_deliverance','torid_survivors_edge'],
  mods:['primed_cryo_rounds','malignant_force','hellfire','primary_acuity','galvanized_scope','vital_sense','hunter_track@0','riven'],
  riven_neg:'status_duration',riven_pos:['critical_chance','critical_damage','damage'],riven_rolls:[1.1,1.1,1.1,.9]};
 const family=await native.call('describe',[child,{...child,mods:child.mods.map(id=>id.split('@')[0])}].map((record,n)=>({id:'family-'+n,record})));
 assert.equal(family[0].default,family[1].candidate);
 const dependent={id:family[0].candidate,record:family[0].record},parent={id:family[1].candidate,record:family[1].record};
 const transient={id:'transient',record:{...raw[0],mods:raw[0].mods.map((id,n)=>n?id:'serration')}};
 const obligation=await questionId('simulate',{record:transient.record,ruler,mode:'base'});
 await seal([{kind:'builds',row:{id:'alias',record:{...a.record,note:'alias'}}},{kind:'builds',row:dependent},{kind:'builds',row:transient},
  {kind:'tasks',row:{id:obligation,state:'open',kind:'board',verb:'simulate'}},{kind:'facts',row:scored(a.id,120,{finished_at:'2026-11-14T00:00:02Z'})}]);
 await seal([{kind:'builds',row:parent},{kind:'queue',row:{build_id:c.id,ruler,mode:'base'}},
  {kind:'facts',row:scored(parent.id,100)}]);
 const fourth=await seal([{kind:'builds',row:{id:transient.id,deleted:true}},{kind:'reset',row:{id:c.id}},
  {kind:'facts',row:scored(parent.id,100,{retired_at:'2026-11-14T00:00:03Z'})}]);
 await assert.rejects(step({fault:async stage=>{if(stage==='after-inventory-input-commit')throw Error('lost span input reply');}}),/lost span input reply/);
 assert.equal(inventory.consumer.span().last,fourth.id);assert.equal(inventory.consumer.state().acknowledged,first.cut);
 reopen();
 for(let n=0;;n++) {
  assert(n<10000);
  try{await step({fault:async stage=>{if(stage==='after-inventory-audit')throw Error('crash before cumulative ack');}});}
  catch(error){assert.match(error.message,/crash before cumulative ack/);break;}
 }
 assert.equal(inventory.consumer.state().acknowledged,first.cut);await oracle('all sealed changes before the cumulative acknowledgement');
 assert(inventory.catalog.statement('SELECT 1 FROM state_inventory_computations WHERE id=? AND state=?').get(Buffer.from(obligation,'hex'),'open'),
  'registered obligation vanished with its transient source');
 const fifth=await seal([{kind:'facts',row:scored(b.id,200,{finished_at:'2026-11-14T00:00:04Z'})}]);
 reopen();await finish(fourth.id);assert.equal(inventory.consumer.state().cut,fourth.id,'new arrivals changed the pinned audit snapshot');
 const ack=inventory.consumer.acknowledgement();assert.equal(ack.input_span.cuts,3);validateWorkSpan(inputs,ack.input_span,fourth.id,{after:first.cut});
 assert.throws(()=>validateWorkSpan(inputs,{...ack.input_span,inputs:ack.input_span.inputs-1},fourth.id),/differs/);
 records.db.prepare("UPDATE record_prepared_board_cuts SET state='preparing' WHERE id=3").run();
 assert.throws(()=>validateWorkSpan(inputs,ack.input_span,fourth.id),/missing cut/);
 records.db.prepare("UPDATE record_prepared_board_cuts SET state='sealed' WHERE id=3").run();
 const held=inputs.cut(3);records.db.prepare('UPDATE record_prepared_board_cuts SET sealed_digest=? WHERE id=3').run('0'.repeat(64));
 assert.throws(()=>validateWorkSpan(inputs,ack.input_span,fourth.id),/differs/);records.db.prepare('UPDATE record_prepared_board_cuts SET sealed_digest=? WHERE id=3').run(held.sealed_digest);
 inputs.observeAcknowledgement('inventory',ack);assert.equal(inputs.collect().floor,0,'work retired the paused board input');
 await finish(fifth.id);await oracle('continued arrivals, default retirement, aliases and threshold changes');
 const member=inventory.catalog.db.prepare('SELECT * FROM relation_inventory_owner_members LIMIT 1').get();assert(member);
 inventory.catalog.db.prepare('DELETE FROM relation_inventory_owner_members WHERE owner=? AND candidate=? AND ruler=? AND mode=?')
  .run(member.owner,member.candidate,member.ruler,member.mode);
 await assert.rejects(audit.members.run(inventory.index,inventory.reader,fifth,native.call,()=>{}),/expected member differs/);
 inventory.catalog.db.prepare('INSERT INTO relation_inventory_owner_members VALUES(?,?,?,?,?,?)').run(...Object.values(member));
 const spurious=inventory.catalog.symbol('mode','wrong-mode');inventory.catalog.db.prepare('UPDATE relation_inventory_owner_members SET mode=? WHERE owner=? AND candidate=? AND ruler=? AND mode=?')
  .run(spurious,member.owner,member.candidate,member.ruler,member.mode);
 await assert.rejects(audit.members.run(inventory.index,inventory.reader,fifth,native.call,()=>{}),/expected member differs/);
 inventory.catalog.db.prepare('UPDATE relation_inventory_owner_members SET mode=? WHERE owner=? AND candidate=? AND ruler=? AND mode=?')
  .run(member.mode,member.owner,member.candidate,member.ruler,spurious);
 const sixth=await seal([{kind:'builds',row:{id:dependent.id,deleted:true}}]),invalidate=inventory.invalidate;
 inventory.invalidate=function(items,...args){return invalidate.call(this,items.filter(item=>item.kind!=='source'),...args);};
 await assert.rejects(finish(sixth.id),/membership audit/);assert.equal(inventory.consumer.state().acknowledged,fifth.id,'missing invalidation advanced ACK');
 inventory.invalidate=invalidate;inventory.index.candidate(dependent.id);await finish(sixth.id);await oracle('repaired missing invalidation');
 const warm=[];
 for(let round=0;round<3;round++) {
  for(let n=0;n<4;n++)await seal([{kind:'facts',row:scored(b.id,150+round*4+n,{finished_at:`2026-11-14T00:01:${String(round*4+n).padStart(2,'0')}Z`})}]);
  const target=serial,before=checked,start=performance.now();let arrival=true;
  await step({fault:async stage=>{if(stage==='after-inventory-input-commit'&&arrival){arrival=false;
   await seal([{kind:'facts',row:scored(a.id,125+round,{finished_at:`2026-11-14T00:02:0${round}Z`})}]);
  }}});
  await finish(target);assert.equal(inventory.consumer.state().acknowledged,target);
  assert.equal(checked-before,1,'work performed a full audit for each obsolete cut');
  const next=serial;assert.equal(next,target+1);await finish(next);
  warm.push({initial_span_cuts:4,arrivals_during_processing:1,audits:checked-before,milliseconds:performance.now()-start});await oracle('continuous catch-up round '+round);
 }
 const previous=serial;for(let n=0;n<260;n++)await seal([]);
 assert.equal((await step()).pending,true);assert.equal(inventory.consumer.state().acknowledged,previous,'empty cuts bypassed the bounded span audit');
 await finish(serial);assert.equal(inventory.consumer.acknowledgement().input_span.cuts,260);
 assert.equal(inventory.consumer.acknowledgement().plan.started_at,'2026-11-14T00:00:00.000Z','catch-up fabricated a fresh clock');
 console.log(JSON.stringify({warm}));
 console.log('ok: contiguous pinned spans retain transient registered obligations, full potential QIDs and native floor/default rules; independent expected memberships reject missing and same-count wrong members plus lost invalidations; crash/restart/new arrivals preserve audited ACK and slowest retention; continuous catch-up audits once per span');
}finally{inventory?.close();audit?.close();records.close();await native.close();rmSync(root,{recursive:true,force:true});}
