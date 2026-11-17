// SPDX-License-Identifier: AGPL-3.0-or-later
// One bounded consumer and one sealed source for snapshot and demand publication.
import {createStatisticsProjection,statisticsProjectionIdentity,STATISTICS_PROJECTION_CONTRACT} from './statistics_projection.js';
import {readPipelineControl} from './task_pipeline.js';
import {dialsOf} from './dials.js';
import {demandOf} from './contribution.js';
import {statisticsSnapshot} from './settler.js';
import {placeholders} from './placeholders.js';
import {iso} from './instant.js';
import {currentStatistics,statisticsExpiresAt} from './statistics_clock.js';

export const STATISTICS_PUMP_MS=1000,STATISTICS_REFRESH_MS=10000,STATISTICS_SLICE_MS=250,STATISTICS_SLICE_STEPS=128;
export const STATISTICS_BUNDLE_FORMAT='statistics-provider-1';
const ordered=value=>Array.isArray(value)?value.map(ordered):value&&typeof value==='object'?Object.fromEntries(Object.keys(value).sort().map(key=>[key,ordered(value[key])])):value;
const identical=(a,b)=>JSON.stringify(ordered(a))===JSON.stringify(ordered(b));
const completeBundle=value=>value?.format===STATISTICS_BUNDLE_FORMAT&&value.key&&value.countsAt===value.snapshot?.at
 &&value.countsAt===value.demand?.counts_at&&value.demand?.at===value.countsAt
 &&value.demand?.expires_at===statisticsExpiresAt(value.countsAt)&&value.demand?.order_epoch===value.key.epoch;
export const statisticsQuery=db=>async(sql,args=[])=>(await db.prepare(sql).bind(...args).all()).results;

export function statisticsQueryDb(query){
 const statement=(sql,args=[])=>({sql,args,bind:(...values)=>statement(sql,values),
  first:async()=>(await query(placeholders(sql),args))[0]??null,
  all:async()=>({results:await query(placeholders(sql),args)}),
  run:async()=>{await query(placeholders(sql),args);return{meta:{changes:0}};}});
 return {prepare:sql=>statement(sql),batch:async statements=>{
  const rows=[];for(const s of statements)rows.push({results:await query(placeholders(s.sql),s.args),meta:{changes:0}});return rows;
 }};
}

export async function statisticsDependencies(query,role=null){
 const [fn]=await query(`SELECT to_regprocedure('statistics_preflight(text)')::text installed,
  coalesce(has_function_privilege(current_user,to_regprocedure('statistics_preflight(text)'),'EXECUTE'),false) allowed`,[]);
 if(!fn.installed)return{ready:false,state:'not_installed'};
 if(!fn.allowed)return{ready:false,state:'missing_grants'};
 let value;try{[{value}]=await query('SELECT statistics_preflight(coalesce($1::text,current_user::text)) value',[role]);}
 catch(error){if(['42501','42883','42P01'].includes(error.code))return{ready:false,state:error.code==='42501'?'missing_grants':'missing_schema'};throw error;}
 if(value.contract!==STATISTICS_PROJECTION_CONTRACT)return{...value,ready:false,state:'unsupported_contract'};
 return {...value,state:value.ready?'ready':!value.schema?'missing_schema':!value.capture?'missing_capture':!value.index?'missing_index':'missing_grants'};
}

export async function statisticsStatus(env,now=Date.now(),role='wfsim_factoryd'){
 const query=statisticsQuery(env.LIBRARY),control=await readPipelineControl(env,'statistics');
 const snapshot=await env.LIBRARY.prepare('SELECT at,engine FROM state_factory_snapshot WHERE id=1').first();
 const progress=await env.LIBRARY.prepare("SELECT * FROM state_task_pipeline_progress WHERE name='statistics'").first();
 const dependencies=await statisticsDependencies(query,role);
 let projection=null;
 if(dependencies.schema&&dependencies.grants)[projection]=await query(`SELECT h.namespace,n.phase::text,n.cursor,n.counts_at,
  coalesce(g.blocked,(SELECT reason FROM state_statistics_invalidations LIMIT 1)) blocked,
  EXISTS(SELECT 1 FROM state_statistics_journal LIMIT 1) captured_changes,
  EXISTS(SELECT 1 FROM state_statistics_obligations o WHERE o.namespace=h.namespace LIMIT 1) work
  FROM state_statistics_generation g LEFT JOIN state_statistics_head h ON h.id=1 LEFT JOIN state_statistics_namespaces n ON n.id=h.namespace WHERE g.id=1`,[]);
 return{name:'statistics',paused:control.paused,control,snapshot:snapshot??null,pending:null,progress:progress??null,
  dependencies,projection:projection??null,state:control.paused?'paused':!dependencies.ready?dependencies.state:projection?.blocked?'halted':!projection?.namespace?'not_started':projection.phase!=='2'?'baseline':projection.work||projection.captured_changes?'catchup':projection.counts_at?'sealed':'unsealed',observed_at:iso(now)};
}

export function createStatisticsProvider({env,controlEnv=env,transaction,query=statisticsQuery(env.LIBRARY),controlQuery=statisticsQuery(controlEnv.LIBRARY),release,stopped=()=>false,
 now=()=>Date.now(),clock=()=>performance.now(),limits={},projectionFactory=createStatisticsProjection,dependenciesOf=statisticsDependencies}){
 let active=null,flight=null,bundle=null,indexOid=null,version=0,closed=false,sealQuery=null,lastState=null;
 const sampleEnv={LIBRARY:statisticsQueryDb(async(sql,args)=>{if(!sealQuery)throw Error('statistics sample is not active');return sealQuery(sql,args);})};
 const stopping=()=>closed||stopped()||!!active?.cancelled;
 const allowed=async({ignoreIndex=false,lookup=controlQuery,signal,remainingMs}={})=>{
  if(stopping())return false;
  const [row]=await lookup(`SELECT coalesce(c.paused,false) paused,
   CASE WHEN $1::oid IS NULL THEN true ELSE EXISTS(SELECT 1 FROM pg_index WHERE indexrelid=$1::oid AND indisvalid AND indisready) END index_ready
   FROM(SELECT 1)one LEFT JOIN state_task_pipeline_controls c ON c.name='statistics'`,[ignoreIndex?null:indexOid],{signal,remainingMs});
  return !stopping()&&!row.paused&&row.index_ready;
 };
 const keyOf=async options=>{
  if(!await allowed(options))throw Error('statistics paused or stopped');
  return statisticsProjectionIdentity((sql,args)=>query(sql,args,options),release().engine);
 };
 const projection=transaction?projectionFactory({transaction,currentIdentity:keyOf,allowed,stopped:stopping,now,clock,limits,
  complete:async(q,counts,{at,key})=>{
   const served=release();if(served.engine!==key.engine)throw Error('statistics source changed');
   sealQuery=q;
   try{
    const sampled=Date.parse(at),dials=await dialsOf(sampleEnv,sampled);
    const snapshot=await statisticsSnapshot(q,counts.snapshot,served,dials,sampled);
    const demand=await demandOf(sampleEnv,sampled,undefined,{counts});
    return{...counts,format:STATISTICS_BUNDLE_FORMAT,snapshot,demand};
   }finally{sealQuery=null;}
  }}):null;
 const state=async value=>{
  if(identical(value,lastState))return;lastState=value;
  await env.LIBRARY.prepare(`INSERT INTO state_factory_findings(kind,body,at) VALUES('statistics_provider',?1::text,?2::text)
   ON CONFLICT(kind) DO UPDATE SET body=excluded.body,at=excluded.at`).bind(JSON.stringify(value),iso(now())).run();
 };
 const ready=async()=>{
  if(stopping())return false;
  if(!await allowed({ignoreIndex:true})){if(!stopping())await state({state:'paused'});return false;}
  const deps=await dependenciesOf(query);if(!deps.ready||!projection){await state({state:!projection?'missing_provider':deps.state});return false;}
  indexOid=deps.index_oid;return allowed();
 };
 const assert=async(value,{lookup=query}={})=>{
  if(!currentStatistics(value.demand,now()))throw Error('statistics sample expired');
  const observedVersion=version,engine=release().engine;
  if(!await allowed({lookup}))throw Error('statistics paused or stopped');
  const deps=await dependenciesOf(lookup);if(!deps.ready)throw Error('statistics dependencies changed');
  const key=await statisticsProjectionIdentity(lookup,engine);
  const [head]=await lookup('SELECT h.namespace,n.counts_at FROM state_statistics_head h JOIN state_statistics_namespaces n ON n.id=h.namespace WHERE h.id=1',[]);
  if(key.blocked||head?.namespace!==value.namespace||head.counts_at!==value.countsAt||!identical(key,value.key))throw Error('statistics source changed');
  if(!await allowed({lookup}))throw Error('statistics paused or stopped');
  if(observedVersion!==version||engine!==release().engine)throw Error('statistics source changed');
  if(!currentStatistics(value.demand,now()))throw Error('statistics sample expired');return true;
 };
 return{
  async advance(){
   if(flight)return flight;
   const token={cancelled:false,version};active=token;
   flight=(async()=>{
    const began=clock();let steps=0,processed=0,sealed=false;
    try{
     if(!await ready())return{steps:0,sealed:false};
     const started=await projection.start();if(bundle?.namespace!==started.namespace)bundle=null;
     do{
      const progress=await projection.step();steps++;
      if(progress.worked||!progress.baseline)processed++;
      if(progress.baseline&&!progress.worked){
       const kept=await projection.read();
       if(!completeBundle(kept)||now()-Date.parse(kept.countsAt)>=STATISTICS_REFRESH_MS){bundle=(await projection.seal()).counts;sealed=true;}
       else bundle=kept;
       break;
      }
     }while(steps<STATISTICS_SLICE_STEPS&&clock()-began<STATISTICS_SLICE_MS&&await allowed());
     if(!await allowed())return{steps,sealed:false};
     if(processed||sealed)await env.LIBRARY.prepare(`INSERT INTO state_task_pipeline_progress(name,at,processed,failures,elapsed_ms,runtime,error)
      VALUES('statistics',?1::text,?2::bigint,0,?3::double precision,'provider',NULL) ON CONFLICT(name) DO UPDATE SET
      at=excluded.at,processed=state_task_pipeline_progress.processed+excluded.processed,elapsed_ms=excluded.elapsed_ms,runtime=excluded.runtime,error=NULL`)
      .bind(iso(now()),processed,clock()-began).run();
     await state({state:bundle?'sealed':'advancing',namespace:started.namespace,counts_at:bundle?.countsAt??null});
     return{steps,processed,sealed};
    }catch(error){
     if(!stopping())await state({state:'held',error:'statistics operation failed'});
     throw error;
    }
   })().finally(()=>{if(active===token)active=null;flight=null;});
   return flight;
  },
  async get(){
   if(!await ready())return null;
   if(!bundle){const [row]=await query(`SELECT n.published FROM state_statistics_head h JOIN state_statistics_namespaces n ON n.id=h.namespace WHERE h.id=1`,[]);bundle=row?.published??null;}
   if(!completeBundle(bundle)||!currentStatistics(bundle.demand,now()))return null;try{await assert(bundle);return bundle;}catch{bundle=null;return null;}
  },assert,
  changed(){version++;if(active)active.cancelled=true;bundle=null;},
  stop(){closed=true;version++;if(active)active.cancelled=true;},
  allowed,
  state:()=>lastState??{state:'unknown'},
 };
}
