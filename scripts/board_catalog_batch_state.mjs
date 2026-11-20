// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {readFileSync} from 'node:fs';
import {inflateSync} from 'node:zlib';
import {coverage} from './rank_catalog_coverage.mjs';
import {sha256,scopeKey} from './board_catalog_readback.mjs';

export function transaction(db,action){
 db.exec('BEGIN IMMEDIATE');try{const result=action();db.exec('COMMIT');return result;}
 catch(e){db.exec('ROLLBACK');throw e;}
}
export const jsonFile=path=>JSON.parse(readFileSync(path,'utf8'));
export function initializeJournal(db){
 db.exec(`PRAGMA synchronous=FULL;
 CREATE TABLE IF NOT EXISTS run(id INTEGER PRIMARY KEY CHECK(id=1),contract TEXT NOT NULL,closed INTEGER NOT NULL DEFAULT 0);
 CREATE TABLE IF NOT EXISTS scopes(weapon TEXT,ruler TEXT,priority INTEGER NOT NULL,receipt TEXT NOT NULL,ordered TEXT NOT NULL,
  request_position TEXT,request_version TEXT,original_cut TEXT,phase TEXT NOT NULL DEFAULT 'ready',manifest TEXT,revision TEXT,disposition TEXT,
  pages_sha256 TEXT,verified_at TEXT,PRIMARY KEY(weapon,ruler)) WITHOUT ROWID;
 CREATE INDEX IF NOT EXISTS batch_next ON scopes(phase,priority,weapon,ruler);
 CREATE TABLE IF NOT EXISTS pins(weapon TEXT,ruler TEXT,revision TEXT,rows INTEGER,tag TEXT,PRIMARY KEY(weapon,ruler,revision,tag)) WITHOUT ROWID;
 CREATE TABLE IF NOT EXISTS initial_heads(weapon TEXT,ruler TEXT,revision TEXT,rows INTEGER,ordered TEXT,pages_sha256 TEXT,
  PRIMARY KEY(weapon,ruler)) WITHOUT ROWID;
 CREATE TABLE IF NOT EXISTS pages(weapon TEXT,ruler TEXT,revision TEXT,offset INTEGER,last INTEGER,digest TEXT,
  PRIMARY KEY(weapon,ruler,revision,offset)) WITHOUT ROWID;
 CREATE TABLE IF NOT EXISTS verified(weapon TEXT,ruler TEXT,revision TEXT,disposition TEXT,pages_sha256 TEXT,verified_at TEXT,
  PRIMARY KEY(weapon,ruler,revision)) WITHOUT ROWID;
 CREATE TABLE IF NOT EXISTS high_water(id INTEGER PRIMARY KEY CHECK(id=1),first_data REAL,first_root REAL,
  min_data REAL,min_root REAL,max_wal REAL,samples INTEGER);
 `);
}

export function openProtectedSets(config,final,native){
 const declarations=[...(final.current_pins?[final.current_pins]:[]),...(final.additional_heads||[])];
 const expected=new Map(declarations.map(p=>[p.sha256||p.pins,p]));
 assert(expected.size===declarations.length,'duplicate protected set declaration');
 const sets=new Map();
 try{
  for(const {members,proof} of config.pins){
   const db=new DatabaseSync(members,{readOnly:true});
   const proofs=new DatabaseSync(proof,{readOnly:true});
   try{
    const binding=db.prepare('SELECT pins,native FROM source WHERE singleton=1').get(),tag=binding?.pins;
    const spec=expected.get(tag);
    assert(spec&&binding.native===native&&!sets.has(tag),'protected inventory or Native changed');
    const scopes=db.prepare('SELECT weapon,ruler,revision,total,loaded FROM scopes ORDER BY weapon,ruler').all();
    assert(scopes.length===spec.scopes&&scopes.reduce((n,s)=>n+s.total,0)===spec.members,'protected set membership differs');
    const membersCount=db.prepare('SELECT count(*) n,min(ordinal) lo,max(ordinal) hi FROM members WHERE weapon=? AND ruler=?');
    const outcomes=proofs.prepare(`SELECT ordinal,id,mode,fact,destination FROM live_audit WHERE weapon=? AND ruler=? AND revision=? ORDER BY ordinal`);
    const original=db.prepare('SELECT ordinal,id,mode FROM members WHERE weapon=? AND ruler=? ORDER BY ordinal');
    const sealed=proofs.prepare('SELECT total,pins FROM live_proof WHERE weapon=? AND ruler=? AND revision=?');
    for(const x of scopes){
     const keys=[x.weapon,x.ruler],m=membersCount.get(...keys),p=sealed.get(...keys,x.revision);
     assert(m.n===x.total&&x.loaded===x.total&&(!x.total||(m.lo===1&&m.hi===x.total))&&p?.total===x.total&&p.pins===tag,
      'unaccounted protected rows in '+scopeKey(...keys));
     const identities=original.iterate(...keys);
     for(const resolved of outcomes.iterate(...keys,x.revision)){
      const o=identities.next().value;
      assert(o&&o.ordinal===resolved.ordinal&&o.id===resolved.id&&o.mode===resolved.mode&&
       ['same','verified_replacement','retired','deleted'].includes(resolved.fact)&&
       ['visible','shape_winner','below_floor','withdrawn'].includes(resolved.destination),'protected identity differs from its disposition');
     }
     assert(identities.next().done,'protected identities remain without dispositions');
    }
    sets.set(tag,{db,scopes});
   }catch(e){db.close();throw e;}finally{proofs.close();}
  }
  assert(sets.size===expected.size,'a protected set is missing');
  return sets;
 }catch(e){for(const {db} of sets.values())db.close();throw e;}
}

export function pinRows(sets,tag,weapon,ruler,after,last){
 return sets.get(tag).db.prepare('SELECT wire FROM members WHERE weapon=? AND ruler=? AND ordinal>? AND ordinal<=? ORDER BY ordinal')
 .all(weapon,ruler,after,last).map(r=>JSON.parse(inflateSync(r.wire)));
}

export function planBatch({journal,capture,archive,ledger,sets,final,budget,contract,reservations,selected,scenes}){
 const actual=coverage(capture,archive,ledger,null,{readOnly:true});
 assert(final.eligible&&actual.eligible&&['scopes','qualified','held','missing','orphans','members','original'].every(k=>actual[k]===final[k]),
  'complete qualified catalog required');
 assert(budget.preliminary_capacity_pass===true&&budget.scope_count===actual.scopes,'qualified capacity forecast required');
 const plan=JSON.stringify(contract),existing=journal.prepare('SELECT contract FROM run WHERE id=1').get();
 if(existing){assert.equal(existing.contract,plan,'batch boundary changed');return;}
 const requests=new Map(reservations.map(x=>[scopeKey(x.weapon,x.ruler),x]));
 assert(requests.size===reservations.length,'duplicate reservation in inventory');
 transaction(journal,()=>{
  journal.prepare('INSERT INTO run(id,contract) VALUES(1,?)').run(plan);
  const savePin=journal.prepare('INSERT INTO pins VALUES(?,?,?,?,?)');
  for(const [tag,{scopes}] of sets)for(const s of scopes)savePin.run(s.weapon,s.ruler,s.revision,s.total,tag);
  const put=journal.prepare('INSERT INTO scopes(weapon,ruler,priority,receipt,ordered,request_position,request_version,original_cut) VALUES(?,?,?,?,?,?,?,?)');
  const pin=journal.prepare('SELECT 1 FROM pins WHERE weapon=? AND ruler=? LIMIT 1');
  const rows=ledger.prepare(`SELECT e.weapon,e.ruler,r.body,v.rows,v.ordered FROM temp.expected e JOIN evidenced USING(weapon,ruler) JOIN receipts r USING(weapon,ruler)
   JOIN verified_outputs v USING(weapon,ruler) ORDER BY weapon,ruler`);
  let n=0;
  for(const x of rows.iterate()){
   const r=JSON.parse(x.body),keys=[x.weapon,x.ruler],w=requests.get(scopeKey(...keys));
   assert(r.eligible&&r.unresolved===0&&r.weapon===x.weapon&&r.ruler===x.ruler&&r.rows===x.rows&&
    r.source===contract.source&&r.native===contract.native&&r.archive===contract.archive&&r.snapshot===contract.snapshot&&
    /^[a-f0-9]{64}$/.test(x.ordered),'unqualified catalog receipt');
   assert(scenes.has(r.ruler)||(r.reason==='ruler absent from pinned Native'&&r.members===0&&r.rows===0&&r.retained>0),'unknown ruler');
   const position=w?.request?.position;
   const requested=w?.read?.enabled===false&&!w.read.canonical_sources&&w.request?.enqueued;
   if(requested)assert(position!=null&&w.partition_mail?.version!=null,'requested scope lacks an original reservation');
   put.run(...keys,pin.get(...keys)?0:requested?1:2,x.body,x.ordered,requested?String(position):null,
    requested?String(w.partition_mail.version):null,requested?(w.rank?.cut??null):null);
   n++;
  }
  assert(n===actual.scopes,'qualified outputs do not cover every scope');
  const absent=journal.prepare('SELECT 1 FROM pins LEFT JOIN scopes USING(weapon,ruler) WHERE scopes.weapon IS NULL LIMIT 1').get();
  assert(!absent,'a protected scope is absent from the verified catalog');
  const exact=journal.prepare('SELECT rows FROM pins WHERE weapon=? AND ruler=? AND revision=? LIMIT 1');
  const remember=journal.prepare('INSERT INTO initial_heads VALUES(?,?,?,?,?,?)');
  for(const s of selected){
   assert(exact.get(s.weapon,s.ruler,s.revision)?.rows===Number(s.rows)||/^[a-f0-9]{64}$/.test(s.pages_sha256),
    'initial selection lacks its protected membership or Native page proof');
   remember.run(s.weapon,s.ruler,s.revision,Number(s.rows),s.output_sha256??null,s.pages_sha256??null);
  }
 });
}

export function pageSummary(journal,row,count){
 const hash=[];let after=0;
 for(const p of journal.prepare('SELECT offset,last,digest FROM pages WHERE weapon=? AND ruler=? AND revision=? ORDER BY offset')
  .iterate(row.weapon,row.ruler,row.revision)){
  assert(p.offset===after&&p.last===Math.min(after+128,count),'invalid readback checkpoint');
  if(count)hash.push((after+1)+' '+p.last+' '+p.digest+'\n');
  after=p.last;
 }
 assert(after===count,'public page verification incomplete');
 return sha256(hash.join(''));
}
