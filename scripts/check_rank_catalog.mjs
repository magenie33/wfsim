// SPDX-License-Identifier: AGPL-3.0-or-later
// Real capture triggers, immutable archive membership and Native rendering cross multiple partitions.
import assert from 'node:assert/strict';
import {createHash,randomUUID} from 'node:crypto';
import {mkdirSync,mkdtempSync,readFileSync,rmSync,writeFileSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {tmpdir} from 'node:os';
import {execFileSync,spawnSync} from 'node:child_process';
import {DatabaseSync} from 'node:sqlite';
import postgres from './pg/node_modules/postgres/src/index.js';
import {openInventoryOperator} from './inventory_operator.mjs';
import {prepareRankSources} from './board_rank_index.mjs';
import {archiveInventory,importArchive} from './board_catalog_archive.mjs';
import {limitRankScratch} from './board_rank_storage.mjs';
import {coverage} from './rank_catalog_coverage.mjs';
import {qualify} from './qualify_rank_catalog.mjs';

const url=new URL(process.env.WFSIM_BOARD_RANK_TEST_URL);
assert(['localhost','127.0.0.1'].includes(url.hostname),'isolated PostgreSQL required');
const schema='catalog_'+randomUUID().replaceAll('-',''),sql=postgres(url.href,{max:3,fetch_types:false,onnotice(){},connection:{search_path:schema}});
const dir=mkdtempSync(join(tmpdir(),'rank-catalog-')),binary=resolve(process.env.WFSIM_INVENTORY_BOARD_BIN||'target/debug/wfsim-board');
const native=await openInventoryOperator(binary),archive=new DatabaseSync(join(dir,'archive.sqlite'));
const rulers=['standard_single_target','standard_multi_target'],date='2026-11-01T00:00:00.000Z',engine='fixture';
const records=JSON.parse(readFileSync(new URL('../tests/gas/rows.json',import.meta.url))).slice(0,2).map(r=>r.record);
const real=process.env.WFSIM_RANK_CORRECTION_FIXTURE&&JSON.parse(readFileSync(process.env.WFSIM_RANK_CORRECTION_FIXTURE))[0];
if(real)records[0]=real.record;
const sha=s=>createHash('sha256').update(s).digest('hex');
const described=await native.call('describe',records.map((record,i)=>({id:String(i),record})));
const pg={...process.env,PGHOST:url.hostname,PGPORT:url.port||'5432',PGDATABASE:url.pathname.slice(1),PGUSER:decodeURIComponent(url.username),PGPASSWORD:decodeURIComponent(url.password),PGOPTIONS:'-c search_path='+schema+',pg_catalog'};
if(!pg.PGUSER)delete pg.PGUSER;
const py=process.env.PYTHON||'python3';
const run=(body,...args)=>spawnSync(py,['-c',body,...args],{env:pg,encoding:'utf8'});
let capture,spec;
const cmd=(code,...args)=>run("import sys,sqlite3;sys.path.insert(0,'scripts');import stage_rank_catalog as c;"+code,dir,native.adapter,spec,...args);
const success=result=>{assert.equal(result.status,0,result.stderr);return result.stdout;};
const initialize="s=sqlite3.connect(sys.argv[1]+'/catalog.sqlite');d=c.Database();a=c.initialise(d,s,sys.argv[2],sys.argv[3]);";
const insert=async f=>sql.unsafe(`INSERT INTO state_board_measurement_selection SELECT * FROM jsonb_populate_record(NULL::state_board_measurement_selection,$1::text::jsonb)
 ON CONFLICT(identity,ruler,mode) DO UPDATE SET score=excluded.score,finished_at=excluded.finished_at,measured_by=excluded.measured_by,review_id=excluded.review_id`,[JSON.stringify(f)]);
const archiveRows=new Map();
try{
 await sql.unsafe('CREATE SCHEMA '+schema);
 await sql.unsafe(readFileSync('worker/pg/board_mailbox.sql','utf8'));
 await sql.unsafe(`CREATE TABLE entity_weapon_builds(id text COLLATE "C" PRIMARY KEY,weapon text,at text,record text);
 CREATE TABLE state_board_measurement_selection(identity text COLLATE "C",ruler text COLLATE "C",mode text COLLATE "C",score float8,
 metric text,measured_by text,started_at text,finished_at text,review_id text,cost_seconds float8,PRIMARY KEY(identity,ruler,mode));
 CREATE TABLE entity_measurement_reviews(id text PRIMARY KEY,identity text,ruler text,mode text,state text,invalidated_at text,result_engine text,engine text,completed_at text);`);
 const view=readFileSync('worker/pg/library.sql','utf8').match(/CREATE OR REPLACE VIEW board_facts AS[\s\S]*?;/)[0];
 await sql.unsafe(view);await sql.unsafe(readFileSync('worker/pg/board_capture.sql','utf8'));
 const fs=[];
 for(const [i,d] of described.entries()){
  assert(d.candidate);
  await sql.unsafe('INSERT INTO entity_weapon_builds VALUES($1,$2,$3,$4)',[d.candidate,records[i].weapon,date,JSON.stringify(records[i])]);
  for(const ruler of rulers){
   const mode=i===0&&real?real.mode:d.publication_modes[0].id;
   const f={identity:d.candidate,ruler,mode,score:20+i,metric:'kpm',measured_by:'verified:'+engine,started_at:date,finished_at:date,cost_seconds:0,review_id:null};
   if(!i&&real&&ruler===rulers[0])Object.assign(f,real.prior);
   if(i===1&&ruler===rulers[0])f.review_id='retiring';
   await insert(f);fs.push(f);
   const out=await prepareRankSources(native,{weapon:records[i].weapon,ruler,inputs:[{identity:d.candidate,record:records[i],facts:[f]}]});
   assert.equal(out.rows.length,1);
   const row=JSON.parse(out.rows[0].page);
   if(!i&&real&&ruler===rulers[0])assert.deepEqual(row,real.old_page);
   const rows=archiveRows.get(records[i].weapon)||[];rows.push(row);archiveRows.set(records[i].weapon,rows);
  }
 }
 const rankWeapon=records[1].weapon,rankRuler='standard_single_target__icebind1000';
 const scene=native.contract.rulers.find(r=>r.id===rankRuler).scene;
 const rankRecords=Array.from({length:12},(_,i)=>({...records[1],mods:[
   i%3===2?'serration':'serration@'+(i%3),'vital_sense@'+Math.floor(i/3),'hellfire','speed_trigger',
   'galvanized_aptitude','hammer_shot','vigilante_fervor','magnetic_capacity']}));
 for(const record of rankRecords.slice(-4)){
  record.mods[7]='riven';record.riven_pos=['damage','multishot'];record.riven_neg='zoom';record.riven_rolls=[1.1,1.1,0.9];
 }
 const rankDesc=await native.call('describe',rankRecords.map((record,i)=>({id:String(i),record})));
 const rankFacts=[],rankArchive=[];
 for(const [i,d] of rankDesc.entries()){
  const record=rankRecords[i],id=d.candidate;assert(id);
  await sql.unsafe('INSERT INTO entity_weapon_builds VALUES($1,$2,$3,$4)',[id,rankWeapon,date,JSON.stringify(record)]);
  const scored=d.publication_modes.map((m,j)=>({identity:id,ruler:rankRuler,mode:m.id,score:i===0?500:i===1?100:5+j,
   metric:'kpm',measured_by:'verified:'+engine,finished_at:date,started_at:date,cost_seconds:0}));
  for(const f of scored){await insert(f);rankFacts.push(f);}
  const out=await prepareRankSources(native,{weapon:rankWeapon,ruler:rankRuler,inputs:[{identity:id,record,facts:scored}]});
  rankArchive.push(...out.rows.map(row=>JSON.parse(row.page)));
 }
 const oracle=join(dir,'oracle');mkdirSync(oracle);writeFileSync(join(oracle,'facts.ndjson'),rankFacts.map(x=>JSON.stringify(x)).join('\n')+'\n');
 writeFileSync(join(oracle,'state.yaml'),'boards: {}\n');
 execFileSync(binary,['--project',oracle,'--facts-in',join(oracle,'facts.ndjson'),'--state',join(oracle,'state.yaml')],
   {input:JSON.stringify(rankRecords.map((record,i)=>({record,id:rankDesc[i].candidate})).sort((a,b)=>a.id.localeCompare(b.id)).map(x=>x.record)),stdio:['pipe','ignore','pipe'],timeout:30000});
 const expected=JSON.parse(readFileSync(join(oracle,scene,rankWeapon+'.json'))).filter(row=>row.benchmark===rankRuler);
 const review=(tx,f,id,status,at)=>tx.unsafe('INSERT INTO entity_measurement_reviews VALUES($1,$2,$3,$4,$5,$6,NULL,$7,$8)',
  [id,f.identity,f.ruler,f.mode,status,at,f.measured_by.replace(/^verified:/,''),f.finished_at]);
 await review(sql,fs[2],'retiring','succeeded',null);
 mkdirSync(join(dir,'board'));
 const files={};
 for(const [weapon,rows] of archiveRows){const body=JSON.stringify(rows);files[weapon]=sha(body);writeFileSync(join(dir,'board',weapon+'.json'),body);}
 const index=JSON.stringify(Object.fromEntries(archiveRows));files.index=sha(index);writeFileSync(join(dir,'board/index.json'),index);
 mkdirSync(join(dir,'board',scene));writeFileSync(join(dir,'board',scene,'index.json'),JSON.stringify({[rankWeapon]:[rankArchive[0]]}));
 writeFileSync(join(dir,'board',scene,rankWeapon+'.json'),JSON.stringify(rankArchive));
 const digest=sha(Object.keys(files).sort().map(k=>k+' '+files[k]+'\n').join(''));
 writeFileSync(join(dir,'board.meta.json'),JSON.stringify({files,digest,scene_weapons:{[scene]:[rankWeapon]}}));
 const inventory=archiveInventory(dir);
 const imported=await importArchive(archive,native,inventory,dir,15);
 assert(imported.complete);spec=imported.archive;
 const writer=await sql.reserve();
 try{
  await writer.unsafe("BEGIN");await writer.unsafe("UPDATE entity_weapon_builds SET at='pre-fence' WHERE id=$1",[described[0].candidate]);
  const blocked=cmd(initialize);assert.notEqual(blocked.status,0);assert.match(blocked.stderr,/existing writers/);
  await sql.unsafe("UPDATE entity_weapon_builds SET at='interleaved' WHERE id=$1",[described[1].candidate]);
  await writer.unsafe('COMMIT');
 }finally{writer.release();}
 success(cmd(initialize+'c.scan_page(d,s,a,1);d.close()'));
 const newer=real?real.new_fact:{...fs[0],score:31.048715222248482,measured_by:'verified:new',finished_at:'2026-11-19T08:48:12.975Z',review_id:'correction'};
 await review(sql,newer,newer.review_id,'succeeded',null);await insert(newer);
 await sql.unsafe("UPDATE entity_measurement_reviews SET state='retired',invalidated_at=$1 WHERE id='retiring'",[newer.finished_at]);
 await sql.unsafe('DELETE FROM state_board_measurement_selection WHERE (identity,ruler,mode)=($1,$2,$3)',[fs[3].identity,fs[3].ruler,fs[3].mode]);
 await sql.unsafe("INSERT INTO entity_weapon_builds VALUES('moved','before','day','{}')");
 await insert({...fs[0],identity:'moved'});
 success(cmd(initialize+"\nwhile not c.receipt(s,'scanned_all'): c.scan_page(d,s,a,1)\nd.close()"));
 await sql.unsafe("UPDATE entity_weapon_builds SET weapon='after' WHERE id='moved'");
 await sql.unsafe("DELETE FROM entity_measurement_reviews WHERE id=$1",[newer.review_id]);
 success(cmd(initialize+'assert not c.catchup_page(d,s,a,2,lambda:None);d.close()'));
 const openWave=cmd(initialize+'c.replay(d,s,a,2,10);d.close()');assert.notEqual(openWave.status,0);assert.match(openWave.stderr,/finish the checkpointed catchup/);
 success(cmd(initialize+"\nwhile not c.catchup_page(d,s,a,2,lambda:None):pass\nd.close()"));
 success(cmd(initialize+'c.replay(d,s,a,2,10);d.close()'));
 await review(sql,newer,newer.review_id,'succeeded',null);
 success(cmd(initialize+'c.replay(d,s,a,2,10);d.close()'));
 capture=new DatabaseSync(join(dir,'catalog.sqlite'),{readOnly:true});
 assert.deepEqual(capture.prepare("SELECT weapon FROM facts WHERE id='moved'").get().weapon,'after');
 const cache=new DatabaseSync(join(dir,'descriptors.sqlite'));
 const seen=new Set();const cachedNative={...native,call:(op,items)=>{
  if(op==='describe')for(const {id} of items){assert(!seen.has(id),'canonical build described repeatedly across scopes');seen.add(id);}
  return native.call(op,items);
 }};
 const bounded=new DatabaseSync(':memory:');bounded.exec('CREATE TABLE bounded(b BLOB)');
 limitRankScratch(bounded,32768);
 assert.throws(()=>bounded.exec('INSERT INTO bounded VALUES(zeroblob(65536))'),/full/i);
 assert.equal(bounded.prepare('SELECT count(*) n FROM bounded').get().n,0);
 bounded.close();
 const ledger=new DatabaseSync(join(dir,'receipts.sqlite'));
 let checked=0;
 for(const [i,record] of records.entries())for(const [j,ruler] of rulers.entries()){
  const q=new DatabaseSync(join(dir,`scope-${i}-${j}.sqlite`));
  try{
   const r=await qualify(capture,archive,q,cache,cachedNative,record.weapon,ruler,10);
   assert(r.complete&&r.eligible);assert.equal(r.members,1);
   const got=q.prepare('SELECT fact,destination FROM dispositions').get();
   assert.equal(got.fact,[['verified_replacement','same'],['retired','deleted']][i][j]);
   assert.equal(got.destination,i?'withdrawn':'visible');
   if(!i&&!j)assert.equal(q.prepare('SELECT score FROM rows').get().score,newer.score);
   assert.deepEqual(await qualify(capture,archive,q,cache,cachedNative,record.weapon,ruler,10),r);
   assert.equal(coverage(capture,archive,ledger,q).pages_sha256,r.pages_sha256);
   checked+=r.members;
  }finally{q.close();}
 }
 assert.equal(checked,4);
 const ranks=new DatabaseSync(join(dir,'ranks.sqlite'));
 const ranked=await qualify(capture,archive,ranks,cache,cachedNative,rankWeapon,rankRuler,10);
 assert(ranked.eligible);assert.equal(ranked.members,rankArchive.length);
 const published=ranks.prepare('SELECT page FROM published JOIN rows USING(id,mode) ORDER BY ordinal').all().map(r=>JSON.parse(r.page));
 assert.deepEqual(published,expected);
 const statuses=ranks.prepare('SELECT DISTINCT destination FROM dispositions ORDER BY destination').all().map(x=>x.destination);
 assert.deepEqual(statuses,['below_floor','shape_winner','visible']);
 coverage(capture,archive,ledger,ranks);
 assert.equal(ledger.prepare('SELECT count(*) n FROM outcomes WHERE weapon=? AND ruler=?').get(rankWeapon,rankRuler).n,rankArchive.length);
 assert.equal(ledger.prepare('SELECT sha FROM page_hashes WHERE weapon=? AND ruler=? AND first=1').get(rankWeapon,rankRuler).sha,
   ranks.prepare('SELECT sha FROM pages WHERE first=1').get().sha);
 ranks.close();
 const missing=coverage(capture,archive,ledger);assert.equal(missing.eligible,false);assert.equal(missing.qualified,5);assert.equal(missing.missing,1);
 const currentOnly=new DatabaseSync(join(dir,'current-only.sqlite'));
 const extra=await qualify(capture,archive,currentOnly,cache,cachedNative,'after',rulers[0],10);
 assert(extra.complete&&extra.eligible);coverage(capture,archive,ledger,currentOnly);currentOnly.close();
 assert.equal(coverage(capture,archive,ledger).eligible,true);
 const stale=new DatabaseSync(':memory:');stale.exec('CREATE TABLE progress(k TEXT PRIMARY KEY,v TEXT)');
 stale.prepare('INSERT INTO progress VALUES(?,?)').run('receipt',JSON.stringify({...ranked,snapshot:'other'}));
 assert.throws(()=>coverage(capture,archive,ledger,stale),/stale scope/);stale.close();
 const bad=new DatabaseSync(join(dir,'negative.sqlite'));
 archive.prepare('UPDATE members SET id=NULL WHERE weapon=? AND ruler=?').run(records[0].weapon,rulers[0]);
 const negative=await qualify(capture,archive,bad,cache,cachedNative,records[0].weapon,rulers[0],10);
 assert.equal(negative.eligible,false);assert.equal(negative.unresolved,1);bad.close();ledger.close();cache.close();
 const step=cmd(initialize+"from pathlib import Path;import json;print(json.dumps(c.manifest(s,Path(sys.argv[1]),sys.argv[4],sys.argv[5])));d.close()",records[0].weapon,rulers[0]);
 const result=JSON.parse(success(step));assert.equal(result.facts,1);assert.equal(result.sha256,sha(readFileSync(result.path)));
 console.log(JSON.stringify({archive_members:checked+rankArchive.length,scopes:5,ranked_destinations:statuses,score:newer.score,prior:fs[0].score,real_case:!!real,
  restart:true,writer_fence:true,build_move:true,review_retirement:true,deletion:true,manifest_verified:true}));
}finally{
 capture?.close();archive.close();await native.close();await sql.unsafe('DROP SCHEMA IF EXISTS '+schema+' CASCADE');
 await sql.end();rmSync(dir,{recursive:true,force:true});
}
