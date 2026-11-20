// SPDX-License-Identifier: AGPL-3.0-or-later
// Seal an independent receipt for each archived identity and each visible output row.
import {createHash} from 'node:crypto';
import {deflateSync,inflateSync} from 'node:zlib';
import {isDeepStrictEqual} from 'node:util';
import {dirname,join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {DatabaseSync} from 'node:sqlite';
import {guardCatalogFile,limitRankScratch} from './board_rank_storage.mjs';
import {openRankInventoryOperator} from './board_operator.mjs';
import {prepareRankSources} from './board_rank_index.mjs';

const unpack=bytes=>JSON.parse(inflateSync(bytes));
const state=(db,k)=>JSON.parse(db.prepare('SELECT v FROM progress WHERE k=?').get(k)?.v||'null');
function tx(db,write){
 db.exec('BEGIN');
 try{write();db.exec('COMMIT');}catch(e){db.exec('ROLLBACK');throw e;}
}
function sameBuild(a,b){
 const omit=({score,shown,measured_by,measured_at,...record})=>record;
 return isDeepStrictEqual(omit(a),omit(b));
}

export async function qualify(capture,archive,q,cache,native,weapon,ruler,seconds){
 const spec=state(capture,'spec'),cut=state(capture,'replay');
 const historic=archive.prepare('SELECT * FROM contract JOIN sealed USING(id,archive)').get();
 if(!cut||!historic||spec.archive!==historic.archive||spec.native!==native.adapter||historic.native!==native.adapter)
  throw Error('incomplete or mismatched capture, archive or native');
 const contract=JSON.stringify({source:spec.source,archive:spec.archive,native:spec.native,policy:native.policyAdapter,cut,weapon,ruler});
 q.exec(`CREATE TABLE IF NOT EXISTS progress(k TEXT PRIMARY KEY,v TEXT);
 CREATE TABLE IF NOT EXISTS rows(id TEXT,mode TEXT,ord INTEGER,shape TEXT,floor TEXT,def INTEGER,score REAL,page TEXT,verified INTEGER,
  PRIMARY KEY(id,mode)) WITHOUT ROWID;
 CREATE INDEX IF NOT EXISTS shape_order ON rows(shape,score DESC,def DESC,id,ord);
 CREATE TABLE IF NOT EXISTS winners(shape TEXT PRIMARY KEY,id TEXT,mode TEXT,ord INTEGER,floor TEXT,def INTEGER,score REAL) WITHOUT ROWID;
 CREATE INDEX IF NOT EXISTS leaders ON winners(floor,score DESC);
 CREATE INDEX IF NOT EXISTS ranked ON winners(score DESC,def DESC,id,ord);
 CREATE TABLE IF NOT EXISTS published(ordinal INTEGER PRIMARY KEY,id TEXT,mode TEXT,UNIQUE(id,mode));
 CREATE TABLE IF NOT EXISTS pages(first INTEGER PRIMARY KEY,last INTEGER,sha TEXT);
 CREATE TABLE IF NOT EXISTS dispositions(file TEXT,ordinal INTEGER,id TEXT,mode TEXT,fact TEXT,destination TEXT,detail TEXT,
   PRIMARY KEY(file,ordinal)) WITHOUT ROWID`);
 const get=k=>state(q,k),set=(k,v)=>q.prepare('INSERT OR REPLACE INTO progress VALUES(?,?)').run(k,JSON.stringify(v));
 if(get('contract')===null)set('contract',contract);
 if(get('contract')!==contract)throw Error('scope qualification cut changed; use a new receipt');
 if(!native.contract.rulers.some(r=>r.id===ruler)){
  const members=archive.prepare('SELECT count(*) n FROM members WHERE weapon=? AND ruler=?').get(weapon,ruler).n;
  const retained=capture.prepare('SELECT count(*) n FROM facts WHERE weapon=? AND ruler=?').get(weapon,ruler).n;
  const r={weapon,ruler,source:spec.source,snapshot:cut.snapshot,native:spec.native,archive:spec.archive,
   members,unresolved:members,rows:0,retained,reason:'ruler absent from pinned Native',eligible:members===0&&retained>0};
  set('receipt',r);return {complete:true,...r};
 }
 const end=Date.now()+seconds*1000;
 const ids=capture.prepare('SELECT DISTINCT id FROM facts WHERE weapon=? AND ruler=? AND id>? ORDER BY id LIMIT 16');
 const facts=capture.prepare('SELECT mode,body,verified FROM facts WHERE weapon=? AND ruler=? AND id=? ORDER BY mode');
 const build=capture.prepare('SELECT body FROM builds WHERE id=?');
 cache.exec('CREATE TABLE IF NOT EXISTS descriptors(native TEXT,id TEXT,sha TEXT,body BLOB,checksum TEXT,PRIMARY KEY(native,id)) WITHOUT ROWID');
 const descriptor=cache.prepare('SELECT sha,body,checksum FROM descriptors WHERE native=? AND id=?');
 const remember=cache.prepare('INSERT OR REPLACE INTO descriptors VALUES(?,?,?,?,?)');
 const checksum=(id,sha,body)=>createHash('sha256').update(native.adapter+id+sha).update(body).digest('hex');
 const add=q.prepare('INSERT INTO rows VALUES(?,?,?,?,?,?,?,?,?)');
 while(!get('rendered')&&Date.now()<end){
  const keys=ids.all(weapon,ruler,get('id')||'');
  if(!keys.length){set('rendered',true);break;}
  const inputs=[],cached=[],reviews=new Map();
  for(const {id} of keys){
   const fs=facts.all(weapon,ruler,id).map(f=>({...unpack(f.body),identity:id,ruler,mode:f.mode,verified:f.verified}));
   reviews.set(id,new Map(fs.map(f=>[f.mode,f.verified])));
   inputs.push({identity:id,record:unpack(build.get(id).body).record,facts:fs});
   const prev=descriptor.get(native.adapter,id);
   if(prev){
    if(checksum(id,prev.sha,prev.body)!==prev.checksum)throw Error('corrupt shared Native descriptor');
    cached.push({identity:id,record_sha256:prev.sha,descriptor:unpack(prev.body)});
   }
  }
  const out=await prepareRankSources(native,{weapon,ruler,inputs,cached});
  tx(cache,()=>{
   for(const s of out.sources){
    if(cached.some(v=>v.identity===s.identity&&v.record_sha256===s.record_sha256))continue;
    const body=deflateSync(JSON.stringify(s.descriptor));
    remember.run(native.adapter,s.identity,s.record_sha256,body,checksum(s.identity,s.record_sha256,body));
   }
  });
  tx(q,()=>{
   const descriptors=new Map(out.sources.map(s=>[s.identity,s.descriptor]));
   for(const row of out.rows){
    const sourceMode=descriptors.get(row.identity).publication_modes.find(m=>m.mode===row.mode).id;
    add.run(row.identity,row.mode,row.mode_order,row.shape,row.floor,Number(row.default),row.score,row.page,reviews.get(row.identity).get(sourceMode));
   }
   set('id',keys.at(-1).id);
  });
 }
 const shapes=q.prepare('SELECT DISTINCT shape FROM rows WHERE shape>? ORDER BY shape LIMIT 256');
 const winning=q.prepare('INSERT INTO winners SELECT shape,id,mode,ord,floor,def,score FROM rows WHERE shape=? ORDER BY score DESC,def DESC,id,ord LIMIT 1');
 while(get('rendered')&&!get('winners')&&Date.now()<end){
  const keys=shapes.all(get('shape')??'');
  if(!keys.length){set('winners',true);break;}
  tx(q,()=>{for(const {shape} of keys)winning.run(shape);set('shape',keys.at(-1).shape);});
 }
 const order=' ORDER BY score DESC,def DESC,id,ord LIMIT 256';
 const select='SELECT w.*,(SELECT l.score FROM winners l WHERE l.floor=w.floor ORDER BY l.score DESC LIMIT 1) leader FROM winners w';
 const publish=q.prepare('INSERT INTO published(id,mode) VALUES(?,?)');
 while(get('winners')&&!get('floored')&&Date.now()<end){
  const cursor=get('rank');
  // One bounded index seek per equality prefix preserves mixed-direction ordering.
  const group=cursor?[
   ['score=? AND def=? AND id=? AND ord>?',cursor],
   ['score=? AND def=? AND id>?',cursor.slice(0,3)],
   ['score=? AND def<?',cursor.slice(0,2)],
   ['score<?',cursor.slice(0,1)]
  ]:[['1',[]]];
  const page=[];
  for(const [where,args] of group){
   page.push(...q.prepare(select+' WHERE '+where+order).all(...args).slice(0,256-page.length));
   if(page.length===256)break;
  }
  if(!page.length){set('floored',true);break;}
  const masks=await native.call('floor',page.map(({score,leader})=>({score,leader})));
  tx(q,()=>{
   for(const [i,row] of page.entries())if(masks[i].keep)publish.run(row.id,row.mode);
   const {score,def,id,ord}=page.at(-1);set('rank',[score,def,id,ord]);
  });
 }
 const match=q.prepare('SELECT * FROM rows WHERE id=? AND mode=?'),visible=q.prepare('SELECT ordinal FROM published WHERE id=? AND mode=?');
 const winner=q.prepare('SELECT * FROM winners WHERE shape=?'),leader=q.prepare('SELECT score FROM winners WHERE floor=? ORDER BY score DESC LIMIT 1');
 const retired=capture.prepare('SELECT body FROM facts WHERE id=? AND ruler=? AND mode=?');
 const removed=capture.prepare('SELECT version FROM removals WHERE (id,ruler,mode)=(?,?,?)');
 const old=archive.prepare('SELECT * FROM members WHERE weapon=? AND ruler=? AND ordinal>? ORDER BY ordinal LIMIT 256');
 const unique=archive.prepare('SELECT count(*) n FROM members WHERE weapon=? AND ruler=? AND id=? AND mode=?');
 const record=q.prepare('INSERT INTO dispositions VALUES(?,?,?,?,?,?,?)');
 while(get('floored')&&!get('audited')&&Date.now()<end){
  const page=old.all(weapon,ruler,get('ordinal')??-1);
  if(!page.length){set('audited',true);break;}
  tx(q,()=>{
   for(const {file,ordinal,id,mode,wire} of page){
    let fact='unresolved',destination='unresolved',detail=null;
    if(id&&unique.get(weapon,ruler,id,mode).n===1){
     const r=match.get(id,mode);
     if(r){
      const was=unpack(wire),now=JSON.parse(r.page);
      if(isDeepStrictEqual(was,now))fact='same';
      else if(r.verified&&now.measured_at>=was.measured_at&&sameBuild(was,now))fact='verified_replacement';
      const v=visible.get(id,mode);
      if(v){destination='visible';detail=v;}
      else{
       const w=winner.get(r.shape);
       if(w&&(w.id!==id||w.mode!==mode)){destination='shape_winner';detail={id:w.id,mode:w.mode};}
       else if(w){destination='below_floor';detail={score:r.score,leader:leader.get(r.floor).score};}
      }
     }else{
      const f=retired.get(id,ruler,mode),t=removed.get(id,ruler,mode);
      if(f&&unpack(f.body).retired_at){fact='retired';destination='withdrawn';detail={at:unpack(f.body).retired_at};}
      else if(!f&&t){fact='deleted';destination='withdrawn';detail=t;}
     }
    }
    record.run(file,ordinal,id,mode,fact,destination,JSON.stringify(detail));
   }
   set('ordinal',page.at(-1).ordinal);
  });
 }
 if(!get('audited'))return {complete:false};
 const n=q.prepare('SELECT count(*) n FROM dispositions').get().n;
 const expected=archive.prepare('SELECT count(*) n FROM members WHERE weapon=? AND ruler=?').get(weapon,ruler).n;
 const unresolved=q.prepare("SELECT count(*) n FROM dispositions WHERE fact='unresolved' OR destination='unresolved'").get().n;
 if(n!==expected)throw Error('archived member coverage incomplete');
 while(!get('hashed')&&Date.now()<end){
  const last=get('last')||0,hash=createHash('sha256');
  const page=q.prepare('SELECT p.ordinal,r.page FROM published p JOIN rows r USING(id,mode) WHERE ordinal>? ORDER BY ordinal LIMIT 128').all(last);
  if(!page.length){set('hashed',true);break;}
  let bytes=get('bytes')||0;
  for(const [i,r] of page.entries()){
   if(r.ordinal!==last+i+1)throw Error('incomplete ranked page');
   const line=r.page+'\n';hash.update(line);bytes+=Buffer.byteLength(line);
  }
  if(bytes>256*1024*1024)throw Error('ranking output exceeds partition budget');
  tx(q,()=>{q.prepare('INSERT INTO pages VALUES(?,?,?)').run(last+1,page.at(-1).ordinal,hash.digest('hex'));
   set('last',page.at(-1).ordinal);set('bytes',bytes);});
 }
 if(!get('hashed'))return {complete:false};
 if(!get('receipt')){
  const hash=createHash('sha256');
  for(const {first,last,sha} of q.prepare('SELECT * FROM pages ORDER BY first').iterate())hash.update(first+' '+last+' '+sha+'\n');
  set('receipt',{weapon,ruler,source:spec.source,snapshot:cut.snapshot,native:spec.native,archive:spec.archive,policy:native.policyAdapter,
   members:n,unresolved,rows:get('last')||0,pages_sha256:hash.digest('hex'),eligible:unresolved===0});
 }
 return {complete:true,...get('receipt')};
}

if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href){
 const [capturePath,archivePath,result,binary,weapon,ruler]=process.argv.slice(2);
 const c=new DatabaseSync(capturePath,{readOnly:true}),a=new DatabaseSync(archivePath,{readOnly:true}),q=new DatabaseSync(guardCatalogFile(result));
 const native=await openRankInventoryOperator(binary,{policyBinary:process.env.WFSIM_BOARD_RIVEN_POLICY_BIN,policySha:process.env.WFSIM_BOARD_RIVEN_POLICY_SHA256});
 const cache=new DatabaseSync(guardCatalogFile(join(dirname(capturePath),'descriptors.sqlite')));
 try{limitRankScratch(q);c.exec('BEGIN');a.exec('BEGIN');console.log(JSON.stringify(await qualify(c,a,q,cache,native,weapon,ruler,30)));}
 finally{c.close();a.close();q.close();cache.close();await native.close();}
}
