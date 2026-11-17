// Arrange valid task/result fixtures through the same storage boundaries as producers.
import {dialsOf} from '../../worker/dials.js';
import {askingIds} from '../../worker/tasks.js';
import {COMPUTING_MS} from '../../worker/contribution.js';
import {readInventoryState} from '../../worker/task_inventory.js';
import {readPipelineControl} from '../../worker/task_pipeline.js';
import {demandOf} from '../../worker/contribution.js';

// Frozen full-read oracle for formatter fixtures; production requires a sealed provider bundle.
export async function statisticsCounts(env,now=Date.now(),served={}){
 const db=env.LIBRARY,at=new Date(now).toISOString(),d=await dialsOf(env,now);
 const first=sql=>db.prepare(sql).first(),all=sql=>db.prepare(sql).all().then(x=>x.results);
 const sample=await db.prepare(`WITH a AS(SELECT count(*) AS n FROM ${askingIds(3,1)} x) SELECT
  (SELECT count(*) FROM(SELECT 1 FROM state_compute_effective_work WHERE state='open' AND waits=0
   AND(lease_until IS NULL OR lease_until<?1::text) LIMIT greatest(1,(SELECT n FROM a))*?2::bigint)o) AS open,
  (SELECT count(*) FROM state_compute_effective_work WHERE state='open' AND waits>0) AS urgent,
  (SELECT n FROM a) AS asking,(SELECT count(*) FROM entity_compute_devices WHERE banned=0 AND last_at>=?4::text) AS computing,
  (SELECT count(*) FROM state_board_effective_pending_work) AS queue_rows,
  (SELECT count(*) FROM state_pending_verifications) AS settle_queue_rows`)
  .bind(at,Math.max(d.pack_max,d.pack8_max),new Date(now-2*d.asked_every_ms).toISOString(),new Date(now-COMPUTING_MS).toISOString()).first();
 const source=`CASE WHEN q.initiator='system' THEN 'system' WHEN q.initiator='anonymous' OR q.initiator LIKE 'user:%' OR q.initiator LIKE 'bot:%' THEN 'requested' ELSE 'unattributed' END`;
 const owed=await all(`SELECT CASE WHEN EXISTS(SELECT 1 FROM relation_task_consumers d WHERE d.producer='new_build' AND d.state='live' AND d.ref=q.build_id||'|'||q.ruler||'|'||q.mode) THEN 0
  WHEN q.batch LIKE 'rescore%' THEN 1 WHEN q.batch LIKE 'refresh-%' THEN 3 ELSE 2 END k,${source} source,count(*) n
  FROM state_board_effective_pending_work q JOIN entity_compute_batches b ON b.id=q.batch GROUP BY k,source`);
 const openOf=(await all(`SELECT verb,producer,${source} source,count(*) n,count(*) FILTER(WHERE leased_at IS NOT NULL OR lease IS NOT NULL) dispatched FROM state_compute_effective_work q WHERE state='open' GROUP BY verb,producer,source`))
  .map(row=>({...row,n:Number(row.n),dispatched:Number(row.dispatched)}));
 const taken=await all(`SELECT k,count(*) n FROM(SELECT q.id,min(CASE WHEN d.producer='new_build' THEN 0 WHEN d.payload::jsonb->>'batch' LIKE 'rescore%' THEN 1 WHEN d.producer='refresh' THEN 3 ELSE 2 END)k
  FROM state_compute_effective_work q JOIN relation_task_consumers d ON d.question=q.id AND d.state='live' AND d.producer IN('new_build','survey_build','refresh','sweep')
  WHERE q.state='open' AND q.verb='simulate' AND(q.leased_at IS NOT NULL OR q.lease IS NOT NULL) GROUP BY q.id)x GROUP BY k`);
 const by={},sources={};for(const row of owed){by[row.k]=(by[row.k]??0)+Number(row.n);(sources[row.k]??={requested:0,system:0,unattributed:0})[row.source]+=Number(row.n);}
 const epoch=(await first('SELECT compute_order_cut_epoch() epoch')).epoch;
 const inventory=await readInventoryState(db,now),engine=served.engine??(inventory.fresh||inventory.lastComplete)?.engine??(await first('SELECT engine FROM state_factory_snapshot WHERE id=1'))?.engine??'fixture';
 return{countsAt:at,key:{engine,epoch},by,sources,openOf,takenBy:Object.fromEntries(taken.map(row=>[row.k,Number(row.n)])),
  snapshot:{at,engine,release:served.release??null,...Object.fromEntries(Object.entries(sample).map(([k,v])=>[k,Number(v)]))}};
}

export const fixtureSnapshotWrite=env=>async statement=>{await env.LIBRARY.batch([statement]);return true;};

// Daemon scheduling fixtures supply the frozen reader; provider qualification uses its real RR transport.
export function fixtureDaemon(run,options){
 const opened=options.opened,env=opened.env;
 const writeSnapshot=async(statement,allowed)=>{
  const tx={query:async(sql,args)=>({rows:(await env.LIBRARY.prepare(sql).bind(...args).all()).results})};
  if(!await allowed(tx))return false;await fixtureSnapshotWrite(env)(statement);return allowed(tx);
 };
 return run({...options,opened:{...opened,writeSnapshot},statisticsFactory:({env,controlEnv,release,stopped})=>{
  let closed=false,bundle=null;
  const allowed=async()=>!closed&&!stopped()&&!(await readPipelineControl(controlEnv,'statistics')).paused;
  return{allowed,state:()=>({state:bundle?'sealed':'unknown'}),stop(){closed=true;},changed(){bundle=null;},
   async assert(value){if(!await allowed()||value.key.engine!==release().engine)throw Error('statistics paused or changed');return true;},
   async advance(){if(!await allowed())return{steps:0};const served=release(),counts=await statisticsCounts(env,Date.now(),served);
    if(!await allowed())return{steps:1};bundle={...counts,demand:await demandOf(env,Date.parse(counts.countsAt),undefined,{counts})};return{steps:1,sealed:true};},
   async get(){return await allowed()?bundle:null;}};
 }});
}

export function fixtureDemand(render){
 const kept=new WeakMap();
 return async(env,now=Date.now(),scan=undefined,options={})=>{
  let counts=options.counts;
  if(!counts){
   const inventory=await readInventoryState(env.LIBRARY,now),engine=(inventory.fresh||inventory.lastComplete)?.engine??null;
   const epoch=(await env.LIBRARY.prepare('SELECT compute_order_cut_epoch() epoch').first()).epoch;
   const old=kept.get(env);
   if(old&&old.engine===engine&&old.epoch===epoch&&now>=Date.parse(old.counts.countsAt)&&now-Date.parse(old.counts.countsAt)<600000)counts=old.counts;
   else{counts=await statisticsCounts(env,now);kept.set(env,{engine,epoch,counts});}
  }
  return render(env,now,scan,{...options,counts});
 };
}
export async function seedComputeWork(db,row) {
  const at=row.opened_at || "2026-01-01T00:00:00.000Z", engine=row.fact_engine || "fixture";
  const spec={id:row.id,verb:row.verb || "simulate",request:typeof row.request==="string" ? row.request : JSON.stringify(row.request || {}),
    due_at:row.due_at || at,lanes:row.lanes || null,waits:row.waits || 0,producer:row.producer || "",initiator:row.initiator || null};
  await db.run("SELECT compute_register_tasks(?1::text,?2::text,?3::text)",JSON.stringify([spec]),engine,at);
  let task=await db.get("SELECT task_id FROM state_compute_work WHERE id=?",row.id);
  if (["fact","spot"].includes(row.state) || row.canon!=null || row.fact_at!=null) {
    await db.run("SELECT compute_accept_result(?1::text,?2::bigint,'open',?3::text,?4::text,?5::text,?6::text,?7::bigint,?8::text)",
      row.id,task.task_id,["fact","spot"].includes(row.state) ? row.state : "fact",row.canon || "{}",engine,row.fact_at || at,
      row.odd || 0,typeof row.witnesses==="string" ? row.witnesses : JSON.stringify(row.witnesses || []));
    if (row.state==="open") {
      await db.run("SELECT compute_open_task(?1::text,'fixture_recheck',?2::text,?3::text)",row.id,engine,at);
      task=await db.get("SELECT task_id FROM state_compute_work WHERE id=?",row.id);
    }
  }
  const columns=new Set(["state","due_at","waits","opened_at","lanes","lease","lease_until","leased_to","leased_at",
    "lease_prev","leased_to_prev","lease_prev_until","needs","odd","producer","initiator","withdrawn_at"]);
  const entries=Object.entries(row).filter(([key])=>columns.has(key));
  if (entries.length) await db.run(`UPDATE state_compute_work SET ${entries.map(([key],i)=>`${key}=?${i+1}`).join(",")}
    WHERE id=?${entries.length+1}`, ...entries.map(([,value])=>value),row.id);
  return task.task_id;
}

export async function replaceComputeResult(db,id,fields={}) {
  const old=await db.get("SELECT * FROM state_compute_work WHERE id=?",id);
  if (!old) throw new Error("missing computation fixture: "+id);
  const row={...old,...fields};
  const {task}=await db.get("SELECT compute_open_task(?1::text,'fixture_correction',?2::text,?3::text,true) AS task",
    id,row.fact_engine,row.fact_at);
  await db.run("SELECT compute_accept_result(?1::text,?2::bigint,'open',?3::text,?4::text,?5::text,?6::text,?7::bigint,?8::text)",
    id,task,row.state,row.canon,row.fact_engine,row.fact_at,row.odd,row.witnesses);
}

export async function seedComputeWorkMany(db,rows) {
  for(let from=0;from<rows.length;from+=512) {
    const page=rows.slice(from,from+512),at=page[0].opened_at,engine="fixture";
    const specs=page.map(row=>({id:row.id,verb:row.verb || "simulate",request:typeof row.request==="string" ? row.request : JSON.stringify(row.request || {}),
      due_at:row.due_at,lanes:null,waits:0,producer:"",initiator:null}));
    await db.run("SELECT compute_register_tasks(?1::text,?2::text,?3::text)",JSON.stringify(specs),engine,at);
    await db.run(`SELECT compute_accept_result(s.id,s.task_id,'open',x.state,'{}',?2::text,?3::text,0,'[]')
      FROM jsonb_to_recordset((?1::text)::jsonb) x(id text,state text) JOIN state_compute_scheduling s ON s.id=x.id
      WHERE x.state IN('fact','spot')`,JSON.stringify(page),engine,at);
    await db.run(`UPDATE state_compute_scheduling s SET state=x.state,opened_at=x.opened_at
      FROM jsonb_to_recordset((?1::text)::jsonb) x(id text,state text,opened_at text) WHERE s.id=x.id`,JSON.stringify(page));
  }
}
