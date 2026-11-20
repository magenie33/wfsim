// SPDX-License-Identifier: AGPL-3.0-or-later
// Check the transition from an archived public revision to a verified native publication.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {mkdir, mkdtemp, rm, writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {boardPublicHeadSchema} from './board_public_head_schema.mjs';
import {importPublicBoard} from './import_board_publication.mjs';

const digest=bytes=>createHash('sha256').update(bytes).digest('hex');

export async function checkPublicHeads({sql,web,options,run}) {
 const root=await mkdtemp(join(tmpdir(),'board-public-heads-'));
 const path=join(root,'inventory.json'),ruler='target',scene='scene';
 const wire=score=>({benchmark:ruler,score});
 const records=Array.from({length:129},(_,i)=>wire(200-i));
 async function file(name,value){
  const bytes=JSON.stringify(value);
  await writeFile(join(root,name),bytes);
  return digest(bytes);
 }
 const page=async(weapon,revision=null,after=0)=>(await web.unsafe('SELECT board_read_page($1,$2,$3,$4,128) page',[weapon,ruler,revision,after]))[0].page;
 const catalog=async weapon=>(await web.unsafe('SELECT board_read_catalog($1,$2,NULL,128) page',[weapon,scene]))[0].page.partitions;
 const native=async(tx,weapon,score)=>{
  const revision=digest(weapon+score);
  await tx.unsafe(`INSERT INTO record_board_read_snapshots VALUES($1,$2,$3,'native','engine','checksum',1,$4::text::jsonb,'date','date')`,
   [weapon,ruler,revision,JSON.stringify([wire(score)])]);
  await tx.unsafe('INSERT INTO state_board_read_rows VALUES($1,$2,$3,1,$4)',[weapon,ruler,revision,JSON.stringify(wire(score))]);
  await tx.unsafe(`INSERT INTO state_board_read_partitions VALUES($1,$2,$3,true,true,$4)
   ON CONFLICT(weapon,ruler) DO UPDATE SET enabled=true,canonical_sources=true,revision=EXCLUDED.revision`,[weapon,ruler,scene,revision]);
 };
 try{
  await mkdir(join(root,'board',scene),{recursive:true});
  const names=['cold','corrected','retry'];
  const inventory={v:1,stamp:await file('board.meta.json',{scored_at:1}),digest:digest('historical manifest'),files:[]};
  const index=Object.fromEntries(names.map(name=>[name,[records[0]]]));
  for(const name of [...names,'index']){
   const relative=scene+'/'+name+'.json';
   inventory.files.push({name,scene,file:relative,digest:await file('board/'+relative,name==='index'?index:records)});
  }
  const expected=await file('inventory.json',inventory);
  const importAll=()=>importPublicBoard(sql,root,path,expected);
  await sql.unsafe('DROP TRIGGER board_select_native_head ON state_board_read_partitions; DROP TABLE state_board_public_heads,state_board_public_import');
  await sql.begin(tx=>native(tx,'corrected',23));
  await sql.unsafe("INSERT INTO state_board_read_partitions VALUES('cold','target','scene',false,false,NULL)");
  await run(boardPublicHeadSchema(options));
  assert.deepEqual((await page('corrected')).rows,[wire(23)],'upgrade adopts the existing verified correction');
  for(const table of ['state_board_public_heads','state_board_public_import','state_board_public_withdrawn_revisions'])
   await assert.rejects(web.unsafe('SELECT * FROM '+table),error=>error.code==='42501');
  await sql.unsafe(`CREATE FUNCTION fail_public_page() RETURNS trigger LANGUAGE plpgsql AS $$
   BEGIN IF NEW.weapon='retry' AND NEW.ordinal=129 THEN RAISE EXCEPTION 'interrupted page'; END IF; RETURN NEW; END $$;
   CREATE TRIGGER fail_public_page BEFORE INSERT ON state_board_read_rows FOR EACH ROW EXECUTE FUNCTION fail_public_page()`);
  await assert.rejects(importAll(),/interrupted page/);
  assert.equal((await page('retry')).error,'not_ready','interrupted import publishes no partial rows');
  assert.equal((await sql.unsafe("SELECT count(*)::int n FROM record_board_read_snapshots WHERE weapon='retry'"))[0].n,0);
  await sql.unsafe('DROP TRIGGER fail_public_page ON state_board_read_rows; DROP FUNCTION fail_public_page()');
  const imported=await importAll();
  assert.deepEqual(imported,{archive:expected,inserted:1,existing:2,rows:129});
  assert.deepEqual((await page('corrected')).rows,[wire(23)]);
  const [initial]=await catalog('cold');
  assert.equal(initial.rows,129);
  assert.deepEqual(initial.leaders,index.cold);
  assert.deepEqual((await page('cold')).rows,records.slice(0,128));
  assert.deepEqual((await page('cold',initial.revision,128)).rows,records.slice(128));
  await sql.begin(async tx=>{
   await native(tx,'cold',17);
   assert.equal((await catalog('cold'))[0].revision,initial.revision);
   assert.equal((await page('cold')).count,129);
  });
  const [current]=await catalog('cold');
  assert.notEqual(current.revision,initial.revision);
  assert.deepEqual(current.leaders,[wire(17)]);
  assert.deepEqual((await page('cold')).rows,[wire(17)]);
  assert.deepEqual((await page('cold',initial.revision,128)).rows,records.slice(128));
  await assert.rejects(web.unsafe('INSERT INTO state_board_public_withdrawn_revisions VALUES($1,$2,$3)',['cold',ruler,initial.revision]),error=>error.code==='42501');
  await sql.unsafe('INSERT INTO state_board_public_withdrawn_revisions VALUES($1,$2,$3)',['cold',ruler,initial.revision]);
  assert.equal((await page('cold',initial.revision,128)).error,'revision_unavailable');
  assert.deepEqual((await page('cold')).rows,[wire(17)]);
  assert.deepEqual((await page('retry')).rows,records.slice(0,128),'unaffected snapshot history stays readable');
  const [{body}]=await sql.unsafe('SELECT body FROM state_board_read_rows WHERE (weapon,ruler,revision,ordinal)=($1,$2,$3,129)',['cold',ruler,initial.revision]);
  assert.deepEqual(JSON.parse(body),records[128]);
  assert.equal((await importAll()).inserted,0,'replay preserves the verified lower score');
  await sql.unsafe("UPDATE state_board_read_partitions SET enabled=false WHERE weapon='cold'");
  await sql.unsafe("DELETE FROM state_board_read_partitions WHERE weapon='corrected'");
  await run(boardPublicHeadSchema(options));
  assert.equal((await importAll()).inserted,0);
  assert.deepEqual(await catalog('cold'),[]);
  assert.deepEqual(await catalog('corrected'),[]);
  const altered={...inventory,digest:digest('different release')};
  const other=await file('different.json',altered);
  await assert.rejects(importPublicBoard(sql,root,join(root,'different.json'),other),/already selected/);
  await file('board/'+scene+'/cold.json',[wire(1397)]);
  await assert.rejects(importAll(),/published file changed/);
  console.log('ok — existing public heads, complete baseline pages, durable import retry, atomic verified replacement, pinned readers and withdrawals');
 }finally{await rm(root,{recursive:true,force:true});}
}
