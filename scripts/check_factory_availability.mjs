// SPDX-License-Identifier: AGPL-3.0-or-later
// Cached availability must survive paused statistics without changing task eligibility.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import {PGlite} from './pg/node_modules/@electric-sql/pglite/dist/index.js';
import {pgDb,placeholders} from '../worker/db.js';
import {seedComputeWork} from './pg/fixtures.mjs';
import {signalTick} from '../worker/settler.js';
import {forgetDials} from '../worker/dials.js';
import {openEnv,run} from './factoryd.mjs';

const canonical=readFileSync(new URL('../worker/pg/sync.sql',import.meta.url),'utf8');
const functionSql=source=>source.match(/CREATE OR REPLACE FUNCTION factory_open\(text, bigint, text, bigint\)[\s\S]*?\$\$;/)?.[0];
assert(functionSql(canonical));
if(process.argv.includes('--sql')){process.stdout.write(functionSql(canonical)+'\n');process.exit(0);}
const module=process.env.WFSIM_AVAILABILITY_MODULE;
const {availabilityWriter,cacheAvailability,publishAvailability,AVAILABILITY_KEY}=await import(module?pathToFileURL(module):'../worker/factory_availability.js');
const pg=new PGlite({parsers:{20:Number,1700:Number}});await pg.waitReady;
for(const file of ['schema.sql','library.sql','sync.sql','inventory.sql','build_workflows.sql','board_admission.sql'])
  await pg.exec(readFileSync(new URL('../worker/pg/'+file,import.meta.url),'utf8'));
if(process.env.WFSIM_AVAILABILITY_SYNC_SQL)await pg.exec(functionSql(readFileSync(process.env.WFSIM_AVAILABILITY_SYNC_SQL,'utf8')));
const result=rows=>({rows:rows.rows,affected:rows.affectedRows??0});
const client={query:async(sql,args)=>result(await pg.query(sql,args)),transaction:fn=>pg.transaction(tx=>fn({query:async(sql,args)=>result(await tx.query(sql,args))}))};
const db=Object.assign(pgDb(client),{
  run:async(sql,...args)=>(await client.query(placeholders(sql),args)).affected,
  get:async(sql,...args)=>(await client.query(placeholders(sql),args)).rows[0]??null,
});
let cases=0;
const check=(label,value)=>{assert(value,label);cases++;console.log('ok '+label);};
const now=Date.now()-10000,iso=offset=>new Date(now+offset).toISOString();
const opened=async(at=iso(0),pack=200)=>db.get('SELECT * FROM factory_open(?1::text,NULL,?2::text,?3::bigint)',at,iso(-120_000),pack);
const body=async()=>db.get('SELECT body,at FROM state_factory_findings WHERE kind=?',AVAILABILITY_KEY);
const sample=(offset=0,extra={})=>({at:iso(offset),open:5000,asking:7,pack_max:200,epoch:null,...extra});
const write=availabilityWriter(client);
try{
  await seedComputeWork(db,{id:'available',verb:'simulate',state:'open',request:'{}',opened_at:iso(-1000)});
  await db.run(`INSERT INTO state_factory_snapshot(id,at,open,urgent,asking,computing,queue_rows,settle_queue_rows,daemon_at,daemon_commit)
    VALUES(1,'',99,0,9,0,123,0,'1970-01-01T00:00:00.000Z','old')`);
  const before=await db.get('SELECT * FROM state_factory_snapshot');
  assert.equal((await opened()).open,1);
  await cacheAvailability(db,sample(),{write});
  check('a fresh independent availability row is used while complete statistics stay paused',(await opened()).open===5000&&(await opened()).asking===7);
  const plan=(await client.query('EXPLAIN(ANALYZE,BUFFERS,FORMAT JSON)SELECT * FROM factory_open($1::text,NULL,$2::text,$3::bigint)',[iso(0),iso(-120000),200])).rows[0]['QUERY PLAN'];
  if(process.env.WFSIM_AVAILABILITY_PLAN_FILE)writeFileSync(process.env.WFSIM_AVAILABILITY_PLAN_FILE,JSON.stringify(plan,null,2)+'\n');
  const nodes=[];const walk=value=>{if(!value||typeof value!=='object')return;if(value['Relation Name'])nodes.push(value);for(const child of Object.values(value))if(typeof child==='object')Array.isArray(child)?child.forEach(walk):walk(child);};walk(plan);
  check('the fresh cache plan reads no question, consumer, device or snapshot heap',nodes.some(node=>node['Relation Name']==='state_factory_findings'&&node['Actual Loops']>0)
    &&nodes.every(node=>['state_factory_findings','record_compute_order_cut_activations'].includes(node['Relation Name'])||node['Actual Loops']===0));
  check('a cache write never restamps the complete snapshot',JSON.stringify(await db.get('SELECT * FROM state_factory_snapshot'))===JSON.stringify(before));
  check('a pack above the sampled ceiling retains canonical fallback',(await opened(iso(0),201)).open===1);
  check('an expired sample retains canonical fallback',(await opened(iso(60_001))).open===1);
  check('a future sample cannot appear fresh',(await opened(iso(-1))).open===1);
  await db.run('UPDATE state_factory_findings SET at=? WHERE kind=?',iso(-30000).replace('Z','Y'),AVAILABILITY_KEY);
  check('a malformed clock inside the lexical freshness window is ignored',(await opened()).open===1);
  await db.run('UPDATE state_factory_findings SET at=? WHERE kind=?',iso(0),AVAILABILITY_KEY);
  for(const malformed of ['bad json','[]','{}','{"v":1,"epoch":null,"open":1e1000000,"asking":7,"pack_max":200}',
    JSON.stringify({v:1,epoch:null,open:5000,asking:7,pack_max:200,extra:String.fromCharCode(0)}),JSON.stringify({v:1,epoch:null,open:'5000',asking:7,pack_max:200}),
    JSON.stringify({v:1,epoch:null,open:1e100,asking:7,pack_max:200}),JSON.stringify({v:1,open:5000,asking:7,pack_max:200})]){
    await db.run('UPDATE state_factory_findings SET body=? WHERE kind=?',malformed,AVAILABILITY_KEY);
    check('malformed or incomplete availability falls back: '+malformed.slice(0,35),(await opened()).open===1);
  }
  await db.run('DELETE FROM state_factory_findings WHERE kind=?',AVAILABILITY_KEY);
  await db.run('UPDATE state_factory_snapshot SET at=? WHERE id=1',iso(0));
  check('the original fresh snapshot remains a compatibility fallback',(await opened()).open===99);
  await cacheAvailability(db,sample(),{write});
  await db.run("INSERT INTO entity_compute_order_cuts VALUES('availability-cut',?,1,99,999999999,'z',?,'{}')",iso(1000),iso(1000));
  await db.run("INSERT INTO record_compute_order_cut_activations VALUES('availability-cut',?,0,'',0,'')",iso(1000));
  check('an active cut invalidates both the old availability epoch and a pre-cut full snapshot',(await opened(iso(2000))).open===1);
  check('a sample taken before the active cut cannot overwrite availability',!await cacheAvailability(db,sample(2000),{write}));
  await db.run('UPDATE state_factory_snapshot SET at=? WHERE id=1',iso(1500));
  check('a complete snapshot sampled after the cut retains its original fallback',(await opened(iso(2000))).open===99);
  await db.run("UPDATE state_factory_snapshot SET at='' WHERE id=1");
  const current=sample(3000,{epoch:iso(1000)});
  await cacheAvailability(db,current,{write});
  check('a current-epoch cache is usable after the cut',(await opened(iso(3000))).open===5000);
  check('a delayed older sample cannot overwrite a newer sample',!await cacheAvailability(db,{...current,at:iso(2000),open:42},{write})&&JSON.parse((await body()).body).open===5000);
  for(const stopAfter of [0,1,2]){
    let allowed=true,queries=0;const original=await body();
    const interrupted={transaction:fn=>client.transaction(tx=>fn({query:async(sql,args)=>{const rows=await tx.query(sql,args);if(++queries===stopAfter)allowed=false;return rows;}}))};
    if(stopAfter===0)allowed=false;
    const saved=await cacheAvailability(db,{...current,at:iso(4000),open:42},{write:availabilityWriter(interrupted),allowed:()=>allowed});
    check('owner stop rolls back at transaction boundary '+stopAfter,saved===false&&JSON.stringify(await body())===JSON.stringify(original));
  }
  await assert.rejects(publishAvailability(db,{...current,at:iso(5000),open:6},{write,publish:async()=>{throw Error('R2 unavailable');}}),/R2 unavailable/);
  check('an R2 failure leaves independently committed availability usable',(await opened(iso(5000))).open===6);
  let published=0;
  await assert.rejects(publishAvailability(db,{...current,at:iso(6000),open:8},{write:async()=>{throw Error('cache unavailable');},publish:async()=>{published++;return 'ok';}}),/cache unavailable/);
  check('a cache failure does not suppress the independent R2 attempt',published===1&&(await opened(iso(5000))).open===6);
  const state=await body();
  await publishAvailability(db,current,{write,allowed:()=>false,publish:async()=>{throw Error('late publish');}});
  check('an already stopped owner neither writes nor publishes',JSON.stringify(await body())===JSON.stringify(state));
  forgetDials();const snap=await signalTick({LIBRARY:db},{engine:'fixture',release:'fixture'},now+7000);
  check('the real fast sample reads the active cut epoch and its actual pack ceiling',snap.epoch===iso(1000)&&Number.isSafeInteger(snap.pack_max)&&snap.pack_max>=200);
  check('the cache reuses the existing findings role grant',/GRANT SELECT, INSERT, UPDATE, DELETE ON state_factory_findings,/.test(readFileSync(new URL('../deploy/factoryd_grants.sql',import.meta.url),'utf8')));
  await db.run("INSERT INTO state_task_pipeline_controls VALUES('statistics',true,?,'check')",iso(0));
  const connections=[];
  const native=async(target,sql,args=[])=>{const out=await target.query(sql,args);return Object.assign(out.rows,{count:out.affectedRows});};
  const openedEnv=await openEnv({PG_URL:'fixture'},(_url,options)=>{
    connections.push(options);
    return {unsafe:(sql,args)=>native(pg,sql,args),begin:fn=>pg.transaction(tx=>fn({unsafe:(sql,args)=>native(tx,sql,args)})),end:async()=>{}};
  });
  check('the cache writer uses the existing signal connection without another pool',connections.length===7&&connections.every(value=>value.max===1)&&typeof openedEnv.writeAvailability==='function');
  await db.run('DELETE FROM state_factory_findings WHERE kind=?',AVAILABILITY_KEY);
  const fullBefore=await db.get('SELECT * FROM state_factory_snapshot');let attempted=0;
  const code=await run({once:true,opened:openedEnv,servedNow:async()=>({engine:null,release:null}),target:{bucket:'fixture'},
    put:async(_target,object)=>{if(object.key==='work.json'){attempted++;throw Error('R2 integration failure');}}});
  check('the actual daemon commits availability even when its R2 attempt fails',code===1&&attempted===1&&JSON.parse((await body()).body).epoch===iso(1000));
  check('the actual paused daemon preserves complete statistics',JSON.stringify(await db.get('SELECT * FROM state_factory_snapshot'))===JSON.stringify(fullBefore));
  console.log(cases+' availability checks passed');
}finally{forgetDials();await pg.close();}
