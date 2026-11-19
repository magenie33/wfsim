// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {createHash} from 'node:crypto';
import {mkdirSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {deflateSync} from 'node:zlib';
import {qualify} from './qualify_rank_catalog.mjs';
import {coverage} from './rank_catalog_coverage.mjs';

const wire=row=>deflateSync(JSON.stringify(row));
const sha=s=>createHash('sha256').update(s).digest('hex');

export async function batchFixture({sql,native,work,source,weapon,ruler,second,inputs,retained,request,protectedHead}){
 const stage=join(work,'capture'),state=join(work,'controller');
 mkdirSync(stage);mkdirSync(state);
 const db=name=>new DatabaseSync(join(stage,name+'.sqlite')),c=db('catalog'),a=db('archive'),l=db('qualification');
 const write=(name,value)=>{const path=join(state,name+'.json');writeFileSync(path,JSON.stringify(value));return path;};
 const [pg]=await sql.unsafe('SELECT system_identifier::text cluster,pg_current_snapshot()::text snapshot FROM pg_control_system()');
 const [{floor}]=await sql.unsafe('SELECT last_value::text floor FROM board_input_revision');
 const archive=sha('immutable archive fixture');
 c.exec(`CREATE TABLE progress(k TEXT PRIMARY KEY,v TEXT);
 CREATE TABLE builds(id TEXT PRIMARY KEY,weapon TEXT,body BLOB,version TEXT);
 CREATE TABLE facts(id TEXT,ruler TEXT,mode TEXT,weapon TEXT,review TEXT,body BLOB,version TEXT,review_version TEXT,verified INTEGER,PRIMARY KEY(id,ruler,mode));
 CREATE INDEX by_scope ON facts(weapon,ruler,id,mode);
 CREATE TABLE removals(id TEXT,ruler TEXT,mode TEXT,version TEXT)`);
 a.exec(`CREATE TABLE contract(id INTEGER PRIMARY KEY,archive TEXT,native TEXT);CREATE TABLE sealed(id INTEGER PRIMARY KEY,archive TEXT);
 CREATE TABLE scopes(weapon TEXT,ruler TEXT);CREATE TABLE members(weapon TEXT,ruler TEXT,id TEXT,mode TEXT,file TEXT,ordinal INTEGER,wire BLOB)`);
 a.prepare('INSERT INTO contract VALUES(1,?,?)').run(archive,native.adapter);a.prepare('INSERT INTO sealed VALUES(1,?)').run(archive);
 const at=new Date().toISOString(),spec={format:1,source,cluster:pg.cluster,native:native.adapter,archive};
 for(const [k,v] of Object.entries({spec,replay:{snapshot:pg.snapshot,captured_at:at},floor})){
  c.prepare('INSERT INTO progress VALUES(?,?)').run(k,JSON.stringify(v));
 }
 const builds=await sql.unsafe('SELECT id,record,at FROM entity_weapon_builds ORDER BY id');
 for(const b of builds)c.prepare('INSERT INTO builds VALUES(?,?,?,NULL)').run(b.id,weapon,wire(b));
 const facts=await sql.unsafe('SELECT * FROM board_facts ORDER BY identity,ruler,mode');
 function addFact({identity,ruler,mode,...f}){
  c.prepare('INSERT INTO facts VALUES(?,?,?,?,?,?,NULL,NULL,0)').run(identity,ruler,mode,weapon,f.review_id||null,wire(f));
 }
 for(const f of facts)addFact(f);
 const retired='retired_fixture_ruler';addFact({...inputs[0].facts[0],ruler:retired});
 a.prepare('INSERT INTO scopes VALUES(?,?)').run(weapon,ruler);
 a.prepare('INSERT INTO scopes VALUES(?,?)').run(weapon,'demolisher');
 a.prepare('INSERT INTO members VALUES(?,?,?,?,?,?,?)').run(weapon,ruler,protectedHead[0].id,protectedHead[0].mode,'archive',1,wire(protectedHead[0].body));
 l.exec(`CREATE TABLE verified_outputs(weapon TEXT,ruler TEXT,rows INTEGER,bytes INTEGER,ordered TEXT,PRIMARY KEY(weapon,ruler));
 CREATE TABLE live_audit(weapon TEXT,ruler TEXT,revision TEXT,ordinal INTEGER,id TEXT,mode TEXT,fact TEXT,destination TEXT,detail TEXT);
 CREATE TABLE live_proof(weapon TEXT,ruler TEXT,revision TEXT,pins TEXT,total INTEGER)`);
 const cache=new DatabaseSync(':memory:');
 for(const id of [ruler,second,'demolisher',retired]){
  const q=new DatabaseSync(':memory:');
  let r;do{r=await qualify(c,a,q,cache,native,weapon,id,5);}while(!r.complete);
  assert(r.eligible,'independent qualifier '+id);
  coverage(c,a,l,q);
  const values=q.prepare('SELECT page FROM published JOIN rows USING(id,mode) ORDER BY ordinal').all().map(x=>x.page+'\n').join('');
  l.prepare('INSERT INTO verified_outputs VALUES(?,?,?,?,?)').run(weapon,id,r.rows,Buffer.byteLength(values),sha(values));
  q.close();
 }
 cache.close();
 const total=protectedHead.length,tag=sha(JSON.stringify([[weapon,ruler,retained,total]])),p=db('pinned-members');
 p.exec(`CREATE TABLE source(singleton INTEGER,pins TEXT,native TEXT);
 CREATE TABLE scopes(weapon TEXT,ruler TEXT,revision TEXT,total INTEGER,loaded INTEGER);
 CREATE TABLE members(weapon TEXT,ruler TEXT,id TEXT,mode TEXT,ordinal INTEGER,wire BLOB)`);
 p.prepare('INSERT INTO source VALUES(1,?,?)').run(tag,native.adapter);
 p.prepare('INSERT INTO scopes VALUES(?,?,?,?,?)').run(weapon,ruler,retained,total,total);
 for(const [i,row] of protectedHead.entries()){
  p.prepare('INSERT INTO members VALUES(?,?,?,?,?,?)').run(weapon,ruler,row.id,row.mode,i+1,wire(row.body));
  l.prepare('INSERT INTO live_audit VALUES(?,?,?,?,?,?,?,?,NULL)').run(weapon,ruler,retained,i+1,row.id,row.mode,'same','visible');
 }
 l.prepare('INSERT INTO live_proof VALUES(?,?,?,?,?)').run(weapon,ruler,retained,tag,total);
 const final={...coverage(c,a,l),current_pins:{scopes:1,members:total,sha256:tag},additional_heads:[]};
 assert(final.eligible&&final.original===1);for(const x of [c,a,l,p])x.close();
 return {stage,state,work,coverage:write('coverage',{final}),budget:write('budget',{preliminary_capacity_pass:true,
  source_cut:{snapshot:pg.snapshot},scope_count:final.scopes,budget_total:4*1024**3,root_required:8*1024**3,wal_observed:0}),
  reservations:write('reservations',{reservations:[request]}),pins:[{members:join(stage,'pinned-members.sqlite'),proof:join(stage,'qualification.sqlite')}]};
}
