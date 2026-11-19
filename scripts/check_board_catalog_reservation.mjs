// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {stat,utimes} from 'node:fs/promises';
import {dirname} from 'node:path';
import {captureRankBaseline} from './board_rank_baseline.mjs';
import {prepareRankSources} from './board_rank_index.mjs';
import {registerCatalogScope} from './adopt_rank_catalog.mjs';
import {collectRankOrphan} from './board_rank_gc.mjs';
const sha=body=>createHash('sha256').update(body).digest('hex');

async function inventory(sql,weapon,ruler){
 const keys=[weapon,ruler];
 const get=async(name)=>(await sql.unsafe('SELECT * FROM '+name+' WHERE weapon=$1 AND ruler=$2',keys))[0];
 const rank=await get('state_board_rank_partitions');
 const [header]=await sql.unsafe("SELECT version,applied_version AS applied FROM state_board_input_mailbox WHERE scope='partition' AND key=$1",[JSON.stringify(keys)]);
 const [counts]=await sql.unsafe(`SELECT (SELECT count(*) FROM state_board_rank_sources WHERE weapon=$1 AND ruler=$2) sources,
  (SELECT count(*) FROM state_board_rank_dirty WHERE weapon=$1 AND ruler=$2) dirty`,keys);
 return {weapon,ruler,rank,request:await get('state_board_requested_partitions'),partition_mail:header,source_count:Number(counts.sources),
  dirty:{all:Number(counts.dirty)},cut_file:{sha256:rank?.cut}};
}
export async function requestCatalogFixture(sql,native,weapon,ruler,source){
 await sql.unsafe('INSERT INTO state_board_requested_partitions(weapon,ruler) VALUES($1,$2)',[weapon,ruler]);
 await sql.unsafe('SELECT board_requested_partition_enqueue($1,$2::uuid)',[JSON.stringify(native.contract.rulers),source]);
 return inventory(sql,weapon,ruler);
}

export async function checkOccupiedReservation({sql,native,work,source,weapon,inputs,until,insert,floor,epoch}){
 const ruler='standard_multi_target',keys=[weapon,ruler],mode=inputs[0].facts[0].mode;
 for(const row of inputs.slice(0,70))await insert({...row.facts[0],ruler});
 await requestCatalogFixture(sql,native,weapon,ruler,source);
 await until(async row=>row.status==='baseline-page'&&(await inventory(sql,weapon,ruler)).rank?.loaded>0,'original player-requested partial cut');
 await sql.unsafe('UPDATE state_board_rank_partitions SET available_at=NULL,error=$3 WHERE weapon=$1 AND ruler=$2',[...keys,'interrupted']);
 await sql.unsafe(`INSERT INTO state_board_rank_dirty(weapon,ruler,identity,version,applied_version,available_at)
  VALUES($1,$2,$3,101,101,NULL) ON CONFLICT DO NOTHING`,[...keys,inputs[0].identity]);
 const saved=await inventory(sql,weapon,ruler);assert(Number(saved.rank.loaded)<Number(saved.rank.total));assert(saved.source_count>0);
 const schema='board_cut_'+saved.rank.cut.slice(0,40);assert(/^board_cut_[0-9a-f]{40}$/.test(schema));
 try{
  const [before]=await sql.unsafe(`SELECT count(*) n,md5(string_agg(md5(to_jsonb(r)::text),'' ORDER BY identity,mode)) digest
   FROM state_board_rank_rows r WHERE weapon=$1 AND ruler=$2`,keys);
  await sql.unsafe('DELETE FROM board_facts WHERE ruler=$1',[ruler]);
  const input={...inputs[0],facts:[{...inputs[0].facts[0],ruler,mode,score:777}]};await insert(input.facts[0]);
  const manifest=await captureRankBaseline(sql,{source,weapon,ruler,native:native.adapter,work,mailboxFloor:floor});
  const {rows}=await prepareRankSources(native,{weapon,ruler,inputs:[input]});assert.equal(rows.length,1);
  const receipt={source,weapon,ruler,snapshot:manifest.snapshot,native:native.adapter,archive:'c'.repeat(64),eligible:true,unresolved:0,members:1,
   rows:1,pages_sha256:sha('1 1 '+sha(rows[0].page+'\n')+'\n')};
  const args={epoch,manifest,receipt,contract:native.contract,work,reservation:saved};
  const archivedRevision='a'.repeat(64);
  await sql.begin(async tx=>{
   await tx.unsafe('INSERT INTO state_board_public_import VALUES(1,$1)',[receipt.archive]);
   await tx.unsafe("INSERT INTO record_board_read_snapshots VALUES($1,$2,$3,$4,'',$5,1,'[]','before','before')",
    [...keys,archivedRevision,'archive:'+receipt.archive,receipt.archive]);
   await tx.unsafe('INSERT INTO state_board_read_rows VALUES($1,$2,$3,1,$4)',[...keys,archivedRevision,rows[0].page]);
   await tx.unsafe('INSERT INTO state_board_public_heads SELECT weapon,ruler,scene,$3 FROM state_board_read_partitions WHERE weapon=$1 AND ruler=$2',[...keys,archivedRevision]);
  });
  await assert.rejects(registerCatalogScope(sql,{...args,receipt:{...receipt,archive:'b'.repeat(64)}}),/outside the pinned archive/);
  await sql.unsafe("INSERT INTO record_board_read_snapshots VALUES($1,$2,'unknown','unknown','','other',0,'[]','before','before')",keys);
  await assert.rejects(registerCatalogScope(sql,args),/private catalog preparation changed/);
  await sql.unsafe("DELETE FROM record_board_read_snapshots WHERE weapon=$1 AND ruler=$2 AND revision='unknown'",keys);
  await assert.rejects(registerCatalogScope(sql,{...args,reservation:{...saved,rank:{...saved.rank,cursor:'0'}}}),/predecessor changed/);
  await assert.rejects(registerCatalogScope(sql,{...args,reservation:{...saved,source_count:saved.source_count+1}}),/coverage changed/);
  assert.equal((await inventory(sql,weapon,ruler)).rank.cut,saved.rank.cut);
  assert.equal((await sql.unsafe('SELECT to_regnamespace($1) ns',[schema]))[0].ns,null);
  assert.equal((await registerCatalogScope(sql,args)).status,'frozen');
  const [archived]=await sql.unsafe('SELECT count(*) n,md5(string_agg(md5(to_jsonb(r)::text),\'\' ORDER BY identity,mode)) digest FROM '+schema+'.state_board_rank_rows r');
  assert.deepEqual({...archived},{...before});
  const [old]=await sql.unsafe('SELECT cursor,loaded,cut,input_sha256 FROM '+schema+'.state_board_rank_partitions');
  for(const [k,v] of Object.entries(old))assert.equal(v,saved.rank[k]);
  await utimes(dirname(saved.rank.path),new Date(0),new Date(0));
  for(let i=0;i<8;i++)await collectRankOrphan(sql,{work,catalog:true});
  assert((await stat(saved.rank.path)).isFile());
  assert.equal((await registerCatalogScope(sql,args)).status,'frozen');
  await until(async()=>!!(await sql.unsafe('SELECT revision FROM state_board_read_partitions WHERE weapon=$1 AND ruler=$2',keys))[0].revision,'restored player reservation current');
  const [result]=await sql.unsafe('SELECT board_read_page($1,$2,NULL,0,128) p',keys);assert.equal(result.p.rows[0].score,777);
  const current=await inventory(sql,weapon,ruler);assert.equal(current.request.position,saved.request.position);assert(current.request.activated_at);
  assert.equal(current.partition_mail.version,saved.partition_mail.version);
  console.log('ok — pinned player requests activate in position, occupied typed indices and original cut survive replacement, mismatch rolls back');
 }finally{await sql.unsafe('DROP SCHEMA IF EXISTS '+schema+' CASCADE');}
}
