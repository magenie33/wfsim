// SPDX-License-Identifier: AGPL-3.0-or-later
// Reconstruct measured rows from canonical bodies independently of publisher selection and checkpoints.
import {createHash} from 'node:crypto';
import {existsSync,mkdirSync,copyFileSync,readFileSync} from 'node:fs';
import {join} from 'node:path';
import {canon} from '../worker/task_contract.js';
import {fileHash} from './inventory_publication_io.mjs';
import {jsonArrayRows} from './json_array_stream.mjs';

const schema=`CREATE TABLE IF NOT EXISTS state_board_audit_baselines(dir TEXT,weapon TEXT,path TEXT,sha TEXT,PRIMARY KEY(dir,weapon)) WITHOUT ROWID;
 CREATE TABLE IF NOT EXISTS state_board_audit_rows(id INTEGER PRIMARY KEY,dir TEXT,weapon TEXT,ruler INTEGER,identity TEXT,mode TEXT,
  fact_mode TEXT,source INTEGER,mode_order INTEGER,shape TEXT,floor TEXT,def INTEGER,score REAL,riven INTEGER,page TEXT,
  UNIQUE(ruler,identity,mode));
 CREATE INDEX IF NOT EXISTS board_audit_shapes ON state_board_audit_rows(dir,weapon,ruler,shape,score DESC,def DESC,source,mode_order);
 CREATE TABLE IF NOT EXISTS state_board_audit_selection(id INTEGER PRIMARY KEY,leader REAL,keep INTEGER);
 CREATE TABLE IF NOT EXISTS state_board_audit_leaders(dir TEXT,weapon TEXT,benchmark TEXT,mode TEXT,riven INTEGER,score REAL,page TEXT,
  PRIMARY KEY(dir,weapon,benchmark,mode,riven)) WITHOUT ROWID;`;
const allowed=(hex,n)=>(BigInt('0x'+hex)&(1n<<BigInt(n)))!==0n;
const fail=message=>{throw Error('independent board audit: '+message);};

export class BoardProjectionAudit {
 constructor(db,reader,work) {this.db=db;this.reader=reader;this.work=work;db.exec(schema);}
 async freezeBaseline({dir,weapon,path,original}) {
  const held=this.db.prepare('SELECT * FROM state_board_audit_baselines WHERE dir=? AND weapon=?').get(dir,weapon);
  if(held)return;
  let frozen=null;
  if(original) {
   const root=join(this.work,'board-baselines');mkdirSync(root,{recursive:true});frozen=join(root,original.sha+'.json');
   if(!existsSync(frozen))copyFileSync(path,frozen);
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
 async run(cut,operate,contract,{check=()=>{}}={}) {
  this.db.exec('DELETE FROM state_board_audit_rows;DELETE FROM state_board_audit_selection;DELETE FROM state_board_audit_leaders');
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
  for(const source of this.reader.rows('source',cut.id)) {
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
  const totals=new Map(),manifest=[];
  const weapons=this.db.prepare(`SELECT dir,weapon FROM state_board_audit_baselines UNION SELECT dir,weapon FROM state_board_audit_rows ORDER BY dir,weapon`).all();
  for(const {dir,weapon} of weapons) {
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
   if(!count&&!baseline?.path) {if(existsSync(path))fail('unexpected empty weapon artifact');}
   else {if(!existsSync(path)||await fileHash(path)!==expected)fail('weapon rows differ: '+dir+'/'+weapon);manifest.push({name:(dir?dir+'/':'')+weapon+'.json',sha:expected});}
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
  return {complete:true,cut:cut.id,snapshot:cut.receipt.snapshot,engine:cut.receipt.engine,started_at:cut.receipt.started_at,
   sources,rendered,files:manifest.length,digest:createHash('sha256').update(canon(manifest)).digest('hex')};
 }
}
