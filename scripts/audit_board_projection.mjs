// SPDX-License-Identifier: AGPL-3.0-or-later
// Reconstruct measured rows from canonical bodies independently of publisher selection and checkpoints.
import {createHash} from 'node:crypto';
import {existsSync,mkdirSync,copyFileSync,readFileSync,readdirSync,statSync,statfsSync,openSync,closeSync,fsyncSync,renameSync} from 'node:fs';
import {join} from 'node:path';
import {canon} from '../worker/task_contract.js';
import {fileHash,syncDirectory} from './inventory_publication_io.mjs';
import {jsonArrayRows} from './json_array_stream.mjs';

const schema=`CREATE TABLE IF NOT EXISTS state_board_audit_baselines(dir TEXT,weapon TEXT,path TEXT,sha TEXT,PRIMARY KEY(dir,weapon)) WITHOUT ROWID;
 CREATE TABLE IF NOT EXISTS state_board_audit_rows(id INTEGER PRIMARY KEY,dir TEXT,weapon TEXT,ruler INTEGER,identity TEXT,mode TEXT,
  fact_mode TEXT,source INTEGER,mode_order INTEGER,shape TEXT,floor TEXT,def INTEGER,score REAL,riven INTEGER,page TEXT,
  UNIQUE(ruler,identity,mode));
 CREATE INDEX IF NOT EXISTS board_audit_shapes ON state_board_audit_rows(dir,weapon,ruler,shape,score DESC,def DESC,source,mode_order);
 CREATE TABLE IF NOT EXISTS state_board_audit_selection(id INTEGER PRIMARY KEY,leader REAL,keep INTEGER);
 CREATE TABLE IF NOT EXISTS state_board_audit_leaders(dir TEXT,weapon TEXT,benchmark TEXT,mode TEXT,riven INTEGER,score REAL,page TEXT,
  PRIMARY KEY(dir,weapon,benchmark,mode,riven)) WITHOUT ROWID;
 CREATE TABLE IF NOT EXISTS state_board_audit_files(dir TEXT,weapon TEXT,sha TEXT,stamp TEXT,PRIMARY KEY(dir,weapon)) WITHOUT ROWID;
 CREATE TABLE IF NOT EXISTS state_board_audit_counts(ruler INTEGER,weapon TEXT,n INTEGER,PRIMARY KEY(ruler,weapon)) WITHOUT ROWID;
 CREATE TABLE IF NOT EXISTS state_board_audit_dirty(weapon TEXT PRIMARY KEY) WITHOUT ROWID;
 CREATE TABLE IF NOT EXISTS state_board_audit_source_keys(id TEXT PRIMARY KEY) WITHOUT ROWID;
 CREATE TABLE IF NOT EXISTS state_board_audit_round(id INTEGER PRIMARY KEY CHECK(id=1),body TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS state_board_audit_setup(id INTEGER PRIMARY KEY CHECK(id=1));`;
const allowed=(hex,n)=>(BigInt('0x'+hex)&(1n<<BigInt(n)))!==0n;
const fail=message=>{throw Error('independent board audit: '+message);};
const stamp=path=>{const s=statSync(path,{bigint:true});return [s.dev,s.ino,s.size,s.mtimeNs,s.ctimeNs].map(String).join(':');};

export class BoardProjectionAudit {
 constructor(db,reader,work) {this.db=db;this.reader=reader;this.work=work;db.exec(schema);}
 async initialize(index,{check=()=>{}}={}) {
  if(this.db.prepare('SELECT 1 FROM state_board_audit_setup').get())return;
  for(const dir of [...new Set(index.contract.rulers.map(row=>row.scene))]) {
   const root=join(this.work,'verified',dir);if(!existsSync(root))continue;
   for(const file of readdirSync(root,{withFileTypes:true}))if(file.isFile()&&file.name.endsWith('.json')&&!['index.json','meta.json'].includes(file.name)) {
    check();const weapon=file.name.slice(0,-5),path=join(root,file.name);
    if(!/^[a-z0-9_]{1,80}$/.test(weapon))fail('invalid legacy weapon artifact');
    await this.freezeBaseline({dir,weapon,path,original:{size:statSync(path).size,sha:await fileHash(path)}});
   }
  }
  for(const row of this.db.prepare('SELECT DISTINCT weapon FROM state_board_audit_baselines').all())index.publicationDirty(row.weapon);
  this.db.prepare('INSERT INTO state_board_audit_setup VALUES(1)').run();
 }
 async freezeBaseline({dir,weapon,path,original}) {
  const held=this.db.prepare('SELECT * FROM state_board_audit_baselines WHERE dir=? AND weapon=?').get(dir,weapon);
  if(held)return;
  let frozen=null;
  if(original) {
   const root=join(this.work,'board-baselines');mkdirSync(root,{recursive:true});frozen=join(root,original.sha+'.json');
   if(!existsSync(frozen)) {
    const fs=statfsSync(this.work);if(fs.bavail*fs.bsize-original.size<8*1024**3)fail('legacy baseline reached the disk reserve');
    const partial=frozen+'.partial';copyFileSync(path,partial);
    if(await fileHash(partial)!==original.sha)fail('legacy baseline changed while freezing');
    const fd=openSync(partial,'r');try{fsyncSync(fd);}finally{closeSync(fd);}renameSync(partial,frozen);syncDirectory(frozen);
   }
   if(await fileHash(frozen)!==original.sha)fail('legacy baseline changed');
  }
  this.db.prepare('INSERT INTO state_board_audit_baselines VALUES(?,?,?,?)').run(dir,weapon,frozen,original?.sha??null);
 }
 leader(dir,weapon,page,metadata) {
  if(!metadata)return;
  this.db.prepare(`INSERT INTO state_board_audit_leaders VALUES(?,?,?,?,?,?,?) ON CONFLICT(dir,weapon,benchmark,mode,riven)
   DO UPDATE SET score=excluded.score,page=excluded.page WHERE excluded.score>state_board_audit_leaders.score`)
   .run(dir,weapon,metadata.benchmark,metadata.mode,Number(metadata.riven),Number(metadata.score),page);
 }
 saveRound(round){this.db.prepare('INSERT OR REPLACE INTO state_board_audit_round VALUES(1,?)').run(canon(round));}
 dirty(weapon){if(weapon)this.db.prepare('INSERT OR IGNORE INTO state_board_audit_dirty VALUES(?)').run(weapon);}
 async run(cut,operate,contract,{check=()=>{}}={}) {
  this.reader.bindViews(cut.id);
  let round=JSON.parse(this.db.prepare('SELECT body FROM state_board_audit_round WHERE id=1').get()?.body||'null');
  if(round?.cut>cut.id)fail('audit cut cannot move backwards');
  if(round&&round.cut!==cut.id&&!round.complete)fail('previous audit cut is unfinished');
  if(!round||round.cut!==cut.id||round.complete) {
   const prior=round?.cut===cut.id?round.prior:round?.cut||0;
   const priorCount=round?.cut===cut.id?round.prior_count:Number(round?.proof?.sources||0);
   round={cut:cut.id,prior,prior_count:priorCount,cursor:0,complete:false,rendered:0,checked_versions:0};
   this.db.exec('DELETE FROM state_board_audit_dirty;DELETE FROM state_board_audit_source_keys');
   if(!prior)this.db.exec('INSERT OR IGNORE INTO state_board_audit_dirty SELECT DISTINCT weapon FROM state_board_audit_baselines');
   this.saveRound(round);
  }
  while(round.cursor<cut.input_count) {
   check();const page=this.reader.readPage(cut.id,round.cursor),facts=[];
   for(const item of page) {
    const value=item.value;
    if(item.kind==='source') {
     if(value) {
      if(item.key!==value.id||item.candidate!==(value.candidate??null)||item.weapon!==(value.bucket?.[0]??null)
       ||item.admit!==(value.admit??null)||item.modes!==(value.publication_modes?canon(value.publication_modes):null)
       ||item.publication!==Number(!!value.publication))fail('source lookup differs from canonical body');
      this.reader.recordBody(value.raw_record_ref);if(value.record_ref)this.reader.recordBody(value.record_ref);
     }
    } else if(item.kind==='fact'&&value) {
     if(item.identity!==value.identity||item.ruler!==value.ruler||item.mode!==value.mode||Number(item.score)!==Number(value.score)
      ||item.key!==canon([value.identity,value.ruler,value.mode])||item.at!==(value.retired_at??value.finished_at??'')
      ||item.retired!==Number(value.retired_at!=null)||item.gate_ruler!==(value.gate_ruler??null)||item.scene!==(value.scene??null))
      fail('measurement lookup differs from canonical body');
     facts.push(value);
    }
   }
   if(facts.length&&canon(await operate('facts',facts))!==canon(facts))fail('measurement normalization changed');
   this.db.exec('BEGIN IMMEDIATE');
   try {
    for(const item of page) {
     if(item.kind==='source') {
      if(round.prior)this.db.prepare('INSERT OR IGNORE INTO state_board_audit_source_keys VALUES(?)').run(item.key);
      this.dirty(this.reader.value('source',item.key,round.prior)?.value?.bucket?.[0]);this.dirty(item.value?.bucket?.[0]);
     } else if(['fact','reset'].includes(item.kind)) {
      const identity=item.value?.identity||item.value?.id;
      for(const row of this.reader.raw.prepare('SELECT DISTINCT weapon FROM state_catalog_publication_sources WHERE candidate=?').all(identity))this.dirty(row.weapon);
     }
    }
    round={...round,cursor:page.at(-1).ordinal+1,checked_versions:round.checked_versions+page.length};this.saveRound(round);this.db.exec('COMMIT');
   }catch(error){this.db.exec('ROLLBACK');throw error;}
   await new Promise(resolve=>setImmediate(resolve));
  }
  for(;;) {
   const next=this.db.prepare('SELECT weapon FROM state_board_audit_dirty ORDER BY weapon LIMIT 1').get();if(!next)break;
   const measured=await this.reconstruct(next.weapon,cut,operate,contract,{check});
   round={...round,rendered:round.rendered+measured.rendered};
   this.db.exec('BEGIN IMMEDIATE');try {
    this.db.prepare('DELETE FROM state_board_audit_dirty WHERE weapon=?').run(next.weapon);this.saveRound(round);this.db.exec('COMMIT');
   }catch(error){this.db.exec('ROLLBACK');throw error;}
  }
  let sources=round.prior?round.prior_count:this.reader.raw.prepare('SELECT count(*) n FROM entity_catalog_sources').get().n;
  for(let after='';;) {
   const page=this.db.prepare('SELECT id FROM state_board_audit_source_keys WHERE id>? ORDER BY id LIMIT 256').all(after);if(!page.length)break;
   for(const row of page)sources+=Number(this.reader.value('source',row.id,cut.id)?.value!=null)-Number(this.reader.value('source',row.id,round.prior)?.value!=null);
   after=page.at(-1).id;check();await new Promise(resolve=>setImmediate(resolve));
  }
  if(sources!==this.reader.sourceCount())fail('sealed source count differs from independent source changes');
  const proof=await this.finish(cut,contract,{check,sources,rendered:round.rendered});
  this.saveRound({...round,complete:true,proof});return {...proof,checked_versions:round.checked_versions};
 }
 async reconstruct(weapon,cut,operate,contract,{check=()=>{}}={}) {
  this.db.exec('DELETE FROM state_board_audit_rows;DELETE FROM state_board_audit_selection');
  this.db.prepare('DELETE FROM state_board_audit_leaders WHERE weapon=?').run(weapon);
  this.db.prepare('DELETE FROM state_board_audit_counts WHERE weapon=?').run(weapon);
  let sources=0,rendered=0,requests=[];
  const flush=async()=>{
   if(!requests.length)return;const values=await operate('render',requests.map(item=>item.request));
   if(values.length!==requests.length)fail('incomplete native render audit');
   this.db.exec('BEGIN IMMEDIATE');
   try {
    for(let n=0;n<requests.length;n++) {
     const row=requests[n],value=values[n];
     this.db.prepare(`INSERT INTO state_board_audit_rows(dir,weapon,ruler,identity,mode,fact_mode,source,mode_order,shape,floor,def,score,riven,page)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(ruler,identity,mode) DO UPDATE SET fact_mode=excluded.fact_mode,source=excluded.source,
      mode_order=excluded.mode_order,shape=excluded.shape,floor=excluded.floor,def=excluded.def,score=excluded.score,riven=excluded.riven,page=excluded.page
      WHERE (excluded.fact_mode,excluded.source,excluded.mode_order)<(state_board_audit_rows.fact_mode,state_board_audit_rows.source,state_board_audit_rows.mode_order)`)
      .run(row.dir,row.weapon,row.ruler,row.identity,row.mode,row.fact_mode,row.source,row.mode_order,value.shape,value.floor,
       Number(value.default),value.score,Number(value.riven),value.page);
    }
    this.db.exec('COMMIT');
   }catch(error){this.db.exec('ROLLBACK');throw error;}
   rendered+=requests.length;requests=[];check();
  };
  for(const source of this.reader.weaponRows(weapon,cut.id)) {
   sources++;if(sources%256===0)check();const d=source.value;
   const raw=JSON.parse(this.reader.recordBody(d.raw_record_ref));
   // Only measured candidates need native row work; every body and its lookup facets are still checked.
   if(source.candidate!==(d.candidate??null)||source.weapon!==(d.bucket?.[0]??null)
    ||source.admit!==(d.admit??null)||source.modes!==(d.publication_modes?canon(d.publication_modes):null)
    ||source.publication!==Number(!!d.publication))fail('source lookup differs from canonical body');
   if(!d.candidate||!d.publication)continue;
   let after=['',''];
   for(;;) {
    const facts=this.reader.db.prepare(`SELECT key FROM entity_prepared_board_versions WHERE kind='fact' AND identity=?
      AND body IS NOT NULL AND first_cut<=? AND (last_cut IS NULL OR last_cut>?) AND (ruler,mode)>(?,?)
      ORDER BY ruler,mode LIMIT 256`).all(d.candidate,cut.id,cut.id,...after);
    if(!facts.length)break;
    const [native]=await operate('describe',[{id:source.key,record:raw}]);
    const expected={...d,record:JSON.parse(this.reader.recordBody(d.record_ref))};delete expected.raw_record_ref;delete expected.record_ref;
    if(canon(native)!==canon(expected))fail('source no longer matches native normalization');
    for(const item of facts) {
     const fact=this.reader.value('fact',item.key,cut.id),f=fact.value;after=[fact.ruler,fact.mode];
     if(fact.identity!==f.identity||fact.ruler!==f.ruler||fact.mode!==f.mode||Number(fact.score)!==Number(f.score)
      ||fact.key!==canon([f.identity,f.ruler,f.mode])||fact.at!==(f.retired_at??f.finished_at??'')
      ||fact.retired!==Number(f.retired_at!=null))fail('measurement lookup differs from canonical body');
     if(f.retired_at!=null)continue;
     const ruler=contract.rulers.findIndex(row=>row.id===f.ruler);if(ruler<0||!allowed(d.admit,ruler))continue;
     for(let n=0;n<d.publication_modes.length;n++) {
      const mode=d.publication_modes[n];if(mode.id!==f.mode)continue;
      requests.push({dir:contract.rulers[ruler].scene,weapon:d.bucket[0],ruler,identity:d.candidate,mode:mode.mode,
       fact_mode:f.mode,source:source.source_order,mode_order:n,request:{benchmark:f.ruler,mode:mode.mode,row:{...native.record,...native.publication},fact:f}});
      if(requests.length===256)await flush();
     }
    }
   }
  }
  await flush();
  // A separate window calculation reconstructs both shape winners and their comparison maxima.
  this.db.exec(`INSERT INTO state_board_audit_selection(id,leader,keep)
   SELECT id,max(score) OVER(PARTITION BY ruler,floor),NULL FROM (
    SELECT *,row_number() OVER(PARTITION BY ruler,shape ORDER BY score DESC,def DESC,source,mode_order,id) place
    FROM state_board_audit_rows) WHERE place=1`);
  for(let after=0;;) {
   const page=this.db.prepare(`SELECT s.id,s.leader,r.score FROM state_board_audit_selection s JOIN state_board_audit_rows r ON r.id=s.id
    WHERE s.id>? ORDER BY s.id LIMIT 256`).all(after);
   if(!page.length)break;const decisions=await operate('floor',page.map(row=>({score:row.score,leader:row.leader})));
   if(decisions.length!==page.length)fail('incomplete native floor audit');
   for(let n=0;n<page.length;n++)this.db.prepare('UPDATE state_board_audit_selection SET keep=? WHERE id=?').run(Number(decisions[n].keep),page[n].id);
   after=page.at(-1).id;check();
  }
  const totals=new Map();
  const files=this.db.prepare(`SELECT dir,weapon FROM (SELECT dir,weapon FROM state_board_audit_baselines UNION SELECT dir,weapon FROM state_board_audit_rows)
   WHERE weapon=? ORDER BY dir,weapon`).all(weapon);
  for(const {dir,weapon} of files) {
   check();const digest=createHash('sha256').update('[');let count=0;
   const append=page=>{digest.update((count++?',':'')+page);};
   const baseline=this.db.prepare('SELECT * FROM state_board_audit_baselines WHERE dir=? AND weapon=?').get(dir,weapon);
   if(baseline?.path) {
    if(await fileHash(baseline.path)!==baseline.sha)fail('legacy audit baseline changed');
    let batch=[];
    const carry=async()=>{const choices=await operate('carry',batch.map(row=>({dir,raw:row.raw})));
     for(let n=0;n<batch.length;n++)if(choices[n].keep){append(batch[n].raw);this.leader(dir,weapon,batch[n].raw,choices[n].index);}batch=[];};
    for await(const row of jsonArrayRows(baseline.path,{withOffsets:true})){batch.push(row);if(batch.length===256)await carry();}
    if(batch.length)await carry();
   }
   let cursor=[-1,0,0,0,0,0];
   for(;;) {
    const page=this.db.prepare(`SELECT r.* FROM state_board_audit_rows r JOIN state_board_audit_selection s ON s.id=r.id AND s.keep=1
     WHERE dir=? AND weapon=? AND (r.ruler,-r.score,-r.def,r.source,r.mode_order,r.id)>(?,?,?,?,?,?)
     ORDER BY r.ruler,-r.score,-r.def,r.source,r.mode_order,r.id LIMIT 256`).all(dir,weapon,...cursor);
    if(!page.length)break;
    for(const row of page){append(row.page);this.leader(dir,weapon,row.page,{benchmark:contract.rulers[row.ruler].id,...row});totals.set(row.ruler,(totals.get(row.ruler)||0)+1);}
    const last=page.at(-1);cursor=[last.ruler,-last.score,-last.def,last.source,last.mode_order,last.id];
   }
   const path=join(this.work,'verified',dir,weapon+'.json'),expected=digest.update(']').digest('hex');
   if(!count&&!baseline?.path) {
    if(existsSync(path))fail('unexpected empty weapon artifact');
    this.db.prepare('INSERT OR REPLACE INTO state_board_audit_files VALUES(?,?,NULL,NULL)').run(dir,weapon);
   } else {
    if(!existsSync(path)||await fileHash(path)!==expected)fail('weapon rows differ: '+dir+'/'+weapon);
    this.db.prepare('INSERT OR REPLACE INTO state_board_audit_files VALUES(?,?,?,?)').run(dir,weapon,expected,stamp(path));
   }
  }
  for(let n=0;n<contract.rulers.length;n++)this.db.prepare('INSERT OR REPLACE INTO state_board_audit_counts VALUES(?,?,?)').run(n,weapon,totals.get(n)||0);
  return {sources,rendered};
 }
 async finish(cut,contract,{check=()=>{},sources,rendered}={}) {
  const manifest=[],totals=new Map(this.db.prepare('SELECT ruler,sum(n) n FROM state_board_audit_counts GROUP BY ruler').all().map(row=>[row.ruler,row.n]));
  for(const file of this.db.prepare('SELECT * FROM state_board_audit_files ORDER BY dir,weapon').all()) {
   check();const path=join(this.work,'verified',file.dir,file.weapon+'.json');
   if(file.sha==null){if(existsSync(path))fail('unexpected weapon artifact');continue;}
   if(!existsSync(path))fail('audited weapon artifact is missing');
   const current=stamp(path);
   if(current!==file.stamp) {
    if(await fileHash(path)!==file.sha)fail('audited weapon artifact changed');
    this.db.prepare('UPDATE state_board_audit_files SET stamp=? WHERE dir=? AND weapon=?').run(current,file.dir,file.weapon);
   }
   manifest.push({name:(file.dir?file.dir+'/':'')+file.weapon+'.json',sha:file.sha});
  }
  for(const dir of [...new Set(contract.rulers.map(row=>row.scene))].sort()) {
   const digest=createHash('sha256').update('{');let weapon='',count=0,started=false,after=['','','',-1];
   for(;;) {
    const rows=this.db.prepare(`SELECT * FROM state_board_audit_leaders WHERE dir=? AND (weapon,benchmark,mode,riven)>(?,?,?,?)
     ORDER BY weapon,benchmark,mode,riven LIMIT 256`).all(dir,...after);if(!rows.length)break;
    for(const row of rows){if(row.weapon!==weapon){if(started)digest.update(']');digest.update((started?',':'')+JSON.stringify(row.weapon)+':[');weapon=row.weapon;count=0;started=true;}
     digest.update((count++?',':'')+row.page);}
    const last=rows.at(-1);after=[last.weapon,last.benchmark,last.mode,last.riven];
   }
   if(started)digest.update(']');const expected=digest.update('}').digest('hex'),name=(dir?dir+'/':'')+'index.json';
   if(await fileHash(join(this.work,'verified',name))!==expected)fail('index differs: '+name);manifest.push({name,sha:expected});
  }
  const state=readFileSync(join(this.work,'pub/data/board_state.yaml'),'utf8');
  for(let n=0;n<contract.rulers.length;n++) {
   const key=contract.rulers[n].id,block=state.match(new RegExp('^  '+key+':\\n((?:    .*\\n)*)','m'))?.[1];
   if(!block||Number(block.match(/submissions: (\d+)/)?.[1])!==sources||Number(block.match(/listed: (\d+)/)?.[1])!==(totals.get(n)||0))fail('state counts differ: '+key);
  }
  manifest.sort((a,b)=>a.name<b.name?-1:a.name>b.name?1:0);
  return {complete:true,artifact_scope:'native-board-v1',cut:cut.id,snapshot:cut.receipt.snapshot,engine:cut.receipt.engine,started_at:cut.receipt.started_at,
   sources,rendered,files:manifest.length,digest:createHash('sha256').update(canon(manifest)).digest('hex')};
 }
}
