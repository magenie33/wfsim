// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {join} from 'node:path';
import {mkdirSync,readFileSync,writeFileSync} from 'node:fs';
import {runBatch} from './adopt_rank_catalog_batch.mjs';
import {freshDirectory} from './board_catalog_fresh.mjs';

export async function checkFreshScope({sql,config,weapon,ruler,second,inputs,step,put,native}){
 const fresh='heavy_gunner__aimed_body',keys=[weapon,fresh],fact=inputs[0].facts[0],old='a'.repeat(64);
 const opts={available:()=>128*1024**3,pageBudget:1};
 const scope={weapon,ruler:fresh,fresh:true,members:0,rows:8,identities:8,facts:16,manifest_bytes:65536};
 const base=JSON.parse(readFileSync(config.budget));
 const make=(name,forecast=scope)=>{
  const state=join(config.state,`fresh-scope-${name}`);mkdirSync(state);
  const budget=join(state,'budget.json');writeFileSync(budget,JSON.stringify({...base,scope_count:1,scope:forecast}));
  return {...config,state,budget,riven_policy_binary:config.native,riven_policy_sha256:native.adapter,priority_scope:{weapon:forecast.weapon,ruler:forecast.ruler}};
 };
 const ledgerPath=join(config.stage,'qualification.sqlite');
 const original=readFileSync(ledgerPath);
 const retained=make('retained',{...scope,ruler:second});
 await assert.rejects(runBatch(retained,'priority-step',opts),/retained scope requires its original qualification/);
 await sql.unsafe('INSERT INTO board_facts SELECT identity,$2,mode,321,metric,measured_by,finished_at,retired_at,review_id FROM board_facts WHERE identity=$1 AND ruler=$3 AND mode=$4',[fact.identity,fresh,ruler,fact.mode]);
 await put('measurement',JSON.stringify([fact.identity,fresh,fact.mode]).replaceAll(',',', '),{identity:fact.identity,ruler:fresh,mode:fact.mode,score:321,publication_zone:0});
 await sql.unsafe(`INSERT INTO state_board_rank_new_scopes(weapon,ruler,zone,zone_position,queued_at) VALUES($1,$2,0,1,'2000-01-01T00:00:00Z')
  ON CONFLICT(weapon,ruler) DO UPDATE SET zone=0,zone_position=1,queued_at='2000-01-01T00:00:00Z'`,keys);
 const published=async()=>(await sql.unsafe('SELECT revision FROM state_board_public_heads WHERE weapon=$1 AND ruler=$2',keys))[0]?.revision;
 const registered=async()=>(await sql.unsafe('SELECT catalog_phase FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2',keys))[0]?.catalog_phase;
 const good=make('good');
 const archived=new DatabaseSync(join(config.stage,'archive.sqlite'));
 archived.prepare('INSERT INTO scopes VALUES(?,?)').run(...keys);
 try{await assert.rejects(runBatch(make('archive-only'),'priority-step',opts),/retained scope requires its original qualification/);}
 finally{archived.prepare('DELETE FROM scopes WHERE weapon=? AND ruler=?').run(...keys);archived.close();}
 await assert.rejects(runBatch({...good,priority_scope:{weapon,ruler:second}},'priority-step',opts),/one-scope capacity forecast/);
 await assert.rejects(runBatch(good,'priority-step',{...opts,available:()=>1024**3}),/reserve exhausted/);
 assert.equal(await registered(),undefined);
 const tooSmall=make('small',{...scope,manifest_bytes:16});
 const capped=await runBatch(tooSmall,'priority-step',opts);assert.equal(capped.status,'held');assert.match(capped.reason,/disk budget/);
 assert.equal(await registered(),undefined);assert.equal(await published(),undefined);
 const noRows=await runBatch(make('rows',{...scope,rows:0}),'priority-step',opts);
 assert.equal(noRows.status,'held');assert.match(noRows.reason,/fresh scope qualification differs/);
 assert.equal(await registered(),undefined);assert.equal(await published(),undefined);
 const [archive]=await sql.unsafe('SELECT archive FROM state_board_public_import WHERE id=1');
 await sql.begin(async tx=>{
  await tx.unsafe("INSERT INTO record_board_read_snapshots VALUES($1,$2,$3,$4,'',$5,0,'[]','old','old')",[...keys,old,'archive:'+archive.archive,archive.archive]);
  const [source]=await tx.unsafe('SELECT scene FROM state_board_read_partitions WHERE weapon=$1 AND ruler=$2',[weapon,ruler]);
  await tx.unsafe('INSERT INTO state_board_public_heads VALUES($1,$2,$3,$4)',[...keys,source.scene,old]);
 });
 const held=await runBatch(make('old-head'),'priority-step',opts);assert.equal(held.status,'held');assert.match(held.reason,/already has a public head/);
 assert.equal(await published(),old);assert.equal(await registered(),undefined);
 await sql.begin(async tx=>{
  await tx.unsafe('DELETE FROM state_board_public_heads WHERE weapon=$1 AND ruler=$2',keys);
  await tx.unsafe('DELETE FROM record_board_read_snapshots WHERE weapon=$1 AND ruler=$2',keys);
 });
 // A depleted capacity sample never becomes the real run's first high-water value.
 const selected=make('selected');let frozen;
 for(let i=0;i<10;i++){frozen=await runBatch(selected,'priority-step',opts);if(frozen.status==='frozen')break;}
 assert.equal(frozen.status,'frozen');assert.equal(await published(),undefined);
 const deadline=Date.now()+30000;while(!await published()&&Date.now()<deadline)await step();
 const revision=await published();assert(revision&&revision!==old);assert.equal(await registered(),'active');
 const waiting=new DatabaseSync(join(selected.state,'priority-scope.sqlite'));
 try{waiting.prepare('UPDATE priority_work SET due=0').run();}finally{waiting.close();}
 assert.equal((await runBatch(selected,'priority-step',opts)).status,'awaiting-completion');
 const completion={...selected,priority_completion_revision:revision};
 const sealedBudget=readFileSync(selected.budget,'utf8'),changedBudget=JSON.parse(sealedBudget);changedBudget.scope.facts++;
 writeFileSync(selected.budget,JSON.stringify(changedBudget));
 try{await assert.rejects(runBatch(completion,'priority-complete',opts),/priority publication boundary changed/);}
 finally{writeFileSync(selected.budget,sealedBudget);}
 const capacityDrift={...opts,available:()=>1024**3};
 await assert.rejects(runBatch({...completion,priority_completion_revision:'0'.repeat(64)},'priority-complete',capacityDrift),/pinned Native publication changed/);
 const directory=freshDirectory(selected.state,...keys),cut=join(directory,'cut.json');const raw=readFileSync(cut,'utf8');const corrupt=JSON.parse(raw);corrupt.snapshot='not-the-captured-snapshot';
 writeFileSync(cut,JSON.stringify(corrupt));
 try{await assert.rejects(runBatch(completion,'priority-complete',capacityDrift),/fresh scope qualification differs/);}finally{writeFileSync(cut,raw);}
 const stats=async()=>(await sql.unsafe(`SELECT (SELECT count(*) FROM state_board_rank_generations) generations,
  (SELECT count(*) FROM state_board_read_partitions) partitions,(SELECT count(*) FROM board_facts) facts`))[0];
 const start=await stats();let done;
 for(let i=0;i<10;i++){done=await runBatch(completion,'priority-complete',capacityDrift);if(done.status==='selected')break;}
 assert.equal(done.status,'selected');assert.equal(done.revision,revision);assert(done.rows>0);
 assert.deepEqual(await stats(),start);assert.deepEqual(readFileSync(ledgerPath),original,'historical ledger is unchanged');
 const journal=new DatabaseSync(join(selected.state,'priority-scope.sqlite'),{readOnly:true});
 try{assert.equal(journal.prepare('SELECT phase FROM priority_work').get().phase,'selected');}finally{journal.close();}
 assert.equal((await runBatch(completion,'priority-complete',capacityDrift)).status,'selected');
 console.log('ok — bounded first publication, fresh receipt and fenced completion, retained and changed head refusal');
}
