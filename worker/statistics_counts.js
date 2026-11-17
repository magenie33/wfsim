// SPDX-License-Identifier: AGPL-3.0-or-later
// Raw cursor pages bound input before eligibility, joins and aggregation.
import { STATISTICS_WORK, DEMAND_SOURCE, DEMAND_COST_EVERY_MS, COMPUTING_MS } from './statistics_work.js';
import { askingIds } from './task_contract.js';
import { BOARD_KINDS } from './board_kinds.js';
import { iso } from './instant.js';
import { placeholders } from './placeholders.js';

export const STATISTICS_TOTAL_MS=120000,STATISTICS_STEP_MS=3000,STATISTICS_PAGE_ROWS=4096;
export const STATISTICS_FINDING='statistics_counts';
const kinds="("+BOARD_KINDS.map(kind=>"'"+kind+"'").join(',')+")";
const input=`SELECT id FROM jsonb_array_elements_text(($1::text)::jsonb) AS x(id)`;
const pageWork=`page_work AS MATERIALIZED(SELECT w.* FROM (${input}) i CROSS JOIN LATERAL(SELECT * FROM stats_work w WHERE w.id=i.id OFFSET 0)w)`;
const livePage=STATISTICS_WORK.replace('FROM stats_work q CROSS JOIN LATERAL(',`FROM (${input}) i CROSS JOIN LATERAL(SELECT * FROM stats_work w WHERE w.id=i.id OFFSET 0) q CROSS JOIN LATERAL(`);
const livePending=livePage.slice(0,livePage.lastIndexOf('  UNION ALL\n  SELECT q.* FROM stats_cut c'))+' )';
const source=alias=>DEMAND_SOURCE.replaceAll('q.',alias+'.');
const pendingBuckets=relation=>`SELECT count(*)::text n,
 (SELECT coalesce(jsonb_agg(x),'[]'::jsonb) FROM(SELECT
 CASE WHEN EXISTS(SELECT 1 FROM relation_task_consumers d WHERE d.state='live' AND d.producer='new_build' AND d.ref=q.build_id||'|'||q.ruler||'|'||q.mode OFFSET 0) THEN 0
 WHEN q.batch LIKE 'rescore%' THEN 1 WHEN q.batch LIKE 'refresh-%' THEN 3 ELSE 2 END k,
 ${source('q')} source,count(*)::text n FROM ${relation} q JOIN entity_compute_batches b ON b.id=q.batch GROUP BY k,source)x) owed FROM ${relation}`;
const pendingInput=`SELECT * FROM jsonb_to_recordset(($1::text)::jsonb) AS p(batch text,build_id text,ruler text,mode text,initiator text)`;
export const STATISTICS_RAW_PENDING_PAGE=`WITH pending_page AS MATERIALIZED(${pendingInput}) SELECT ${pendingBuckets('pending_page').slice(7)}`;
export const STATISTICS_POSTCUT_PAGE=`WITH pending_page AS MATERIALIZED(
 SELECT p.* FROM (${pendingInput})p WHERE NOT EXISTS(SELECT 1 FROM relation_task_consumers d CROSS JOIN LATERAL(
 SELECT 1 FROM state_compute_work w WHERE w.id=d.question OFFSET 0)live WHERE d.state='live' AND d.producer IN${kinds}
 AND d.ref=p.build_id||'|'||p.ruler||'|'||p.mode OFFSET 0)) SELECT ${pendingBuckets('pending_page').slice(7)}`;
const batchWork=livePending.replace(' stats_live_refs AS NOT MATERIALIZED(',pageWork+', stats_live_refs AS NOT MATERIALIZED(')
 .replace(`FROM (${input}) i CROSS JOIN LATERAL(SELECT * FROM stats_work w WHERE w.id=i.id OFFSET 0) q CROSS JOIN LATERAL(`,'FROM page_work q CROSS JOIN LATERAL(')
 .replace('SELECT q.* FROM state_board_pending_work q WHERE NOT EXISTS(SELECT 1 FROM stats_cut)','SELECT q.* FROM state_board_pending_work q WHERE false');
export const STATISTICS_QUESTION_PAGE=`WITH ${batchWork} SELECT
 (SELECT coalesce(jsonb_agg(x),'[]'::jsonb) FROM(SELECT verb,producer,${source('q')} source,count(*)::text n,
 count(*) FILTER(WHERE leased_at IS NOT NULL OR lease IS NOT NULL)::text dispatched FROM page_work q WHERE state='open' GROUP BY verb,producer,source)x) open_of,
 (SELECT coalesce(jsonb_agg(x),'[]'::jsonb) FROM(SELECT k,count(*)::text n FROM(SELECT x.id,min(CASE WHEN d.producer='new_build' THEN 0
 WHEN d.payload::jsonb->>'batch' LIKE 'rescore%' THEN 1 WHEN d.producer='refresh' THEN 3 ELSE 2 END)k
 FROM page_work x JOIN relation_task_consumers d ON d.question=x.id AND d.state='live' AND d.producer IN${kinds}
 WHERE x.state='open' AND x.verb='simulate' AND(x.leased_at IS NOT NULL OR x.lease IS NOT NULL) GROUP BY x.id)y GROUP BY k)x) taken,
 (SELECT count(*) FILTER(WHERE waits=0 AND(lease_until IS NULL OR lease_until<$2::text))::text FROM page_work WHERE state='open') open,
 (SELECT count(*) FILTER(WHERE waits>0)::text FROM page_work WHERE state='open') urgent,
 (SELECT row_to_json(p) FROM(${pendingBuckets('stats_pending')})p) pending`;
export const STATISTICS_KEY_SQL=`SELECT compute_order_cut_epoch() epoch,
 (SELECT body::jsonb->>'source' FROM state_factory_findings WHERE kind='task_inventory') source,
 (SELECT body::jsonb->>'snapshot' FROM state_factory_findings WHERE kind='task_inventory') snapshot,
 (SELECT body::jsonb->>'engine' FROM state_factory_findings WHERE kind='task_inventory') inventory_engine`;
const utf8=new TextEncoder();
const cCompare=(a,b)=>{const x=utf8.encode(a),y=utf8.encode(b);for(let i=0;i<Math.min(x.length,y.length);i++)if(x[i]!==y[i])return x[i]-y[i];return x.length-y.length;};
export function statisticsCandidate(row,cut,protectedIds){
 if(!cut||row.state!=='open'||row.task_id===null||row.zone===null||row.position===null)return true;
 if((typeof row.task_id!=='string'||typeof row.zone!=='string'||typeof row.position!=='string'||typeof cut.task_cutoff!=='string'||typeof cut.keep_zone!=='string'||typeof cut.keep_position!=='string'))throw Error('raw statistics integer must be text');
 if(BigInt(row.task_id)>BigInt(cut.task_cutoff)||protectedIds.has(row.id))return true;
 const zone=BigInt(row.zone),position=BigInt(row.position),keepZone=BigInt(cut.keep_zone),keepPosition=BigInt(cut.keep_position);
 return zone<keepZone||(zone===keepZone&&(position<keepPosition||(position===keepPosition&&cCompare(row.id,cut.keep_id)<=0)));
}
const identity=key=>JSON.stringify([key.engine,key.epoch,key.source,key.snapshot,key.inventory_engine]);

export function createStatisticsCounts({transaction,writeTransaction=transaction,keyOf,stopped=()=>false,clock=()=>performance.now(),now=()=>Date.now(),pageRows=STATISTICS_PAGE_ROWS,onPage=()=>{},afterPage=()=>{}}){
 if(!Number.isSafeInteger(pageRows)||pageRows<1||pageRows>STATISTICS_PAGE_ROWS)throw Error('invalid statistics input page');
 let held=null,flight=null,version=0,failure=null,lastFailure=null,resolvedAt=null;
 const assert=async bundle=>{const key=await keyOf();if(stopped()||bundle.version!==version||identity(key)!==identity(bundle.key))throw Error('statistics source changed or stopped');};
 const bounded=async(promise,check,deadline,onAbort=()=>{})=>{
  let timer;
  const watch=new Promise((_,reject)=>{const poll=()=>{try{check();if(clock()>=deadline)throw Error('statistics total deadline');}
   catch(error){onAbort();reject(error);return;}timer=setTimeout(poll,Math.max(1,Math.min(STATISTICS_STEP_MS,deadline-clock())));};poll();});
  try{return await Promise.race([promise,watch]);}finally{clearTimeout(timer);}
 };
 const scan=async(key,mine,began)=>{
  const countsAt=iso(now());
  const check=()=>{if(stopped()||mine!==version)throw Error('statistics source changed or stopped');if(clock()-began>=STATISTICS_TOTAL_MS)throw Error('statistics total deadline');};
  const result=await transaction(async tx=>{
   const query=async(sql,args=[])=>{check();const remaining=Math.floor(STATISTICS_TOTAL_MS-(clock()-began));
    await tx.query('SET LOCAL statement_timeout='+Math.max(1,Math.min(STATISTICS_STEP_MS,remaining)),[]);
    const out=await tx.query(placeholders(sql),args);check();return out.rows;};
   await query('SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY');
   await query("SET LOCAL lock_timeout='250ms'");
   const actual=(await query(STATISTICS_KEY_SQL))[0];
   if(identity({...actual,engine:key.engine})!==identity(key))throw Error('statistics source changed');
   const cut=(await query('SELECT id,task_cutoff::text,keep_zone::text,keep_position::text,keep_id,cut_at FROM compute_order_cut()'))[0];
   const protection=new Set(cut?(await query('SELECT question FROM compute_order_cut_protections()')).map(x=>x.question):[]);
   const out={countsAt,key,version:mine,deadline:began+STATISTICS_TOTAL_MS,by:{},sources:{},takenBy:{},openOf:[],snapshot:{at:countsAt,engine:key.engine,release:key.release??null,queue_rows:0}};
   const open=new Map();
   const add=rows=>{for(const row of rows){out.snapshot.queue_rows+=Number(row.n);for(const x of row.owed){out.by[x.k]=(out.by[x.k]||0)+Number(x.n);const s=out.sources[x.k]??={requested:0,system:0,unattributed:0};s[x.source]+=Number(x.n);}}};
   const pages=async(name,sql,args,work)=>{
    await query('DECLARE '+name+' NO SCROLL CURSOR FOR '+sql,args);
    try{for(;;){const rows=await query('FETCH FORWARD '+pageRows+' FROM '+name);if(!rows.length)break;
     await onPage(name,rows);check();await work(rows);check();await afterPage(name,rows.length);check();}}
    finally{await query('CLOSE '+name).catch(()=>{});}
   };
   const small=(await query(`SELECT
    (SELECT count(*) FROM ${askingIds(1,2)} x)::text asking,
    (SELECT count(*) FROM entity_compute_devices WHERE banned=0 AND last_at>=$3::text)::text computing,
    (SELECT count(*) FROM state_pending_verifications)::text settle_queue_rows`,[iso(Date.parse(countsAt)-2*key.dials.asked_every_ms),countsAt,iso(Date.parse(countsAt)-COMPUTING_MS)]))[0];
   Object.assign(out.snapshot,{asking:Number(small.asking),computing:Number(small.computing),settle_queue_rows:Number(small.settle_queue_rows),open:0,urgent:0});
   // Every raw question is counted once, independently of its consumer fanout.
   await pages('statistics_questions',`SELECT id,task_id::text,state,coalesce(zone,factory_default_zone(producer))::text zone,
    coalesce(zone_position,task_id)::text position FROM state_compute_scheduling`,[],async rows=>{
    const ids=rows.filter(row=>statisticsCandidate(row,cut,protection)).map(row=>row.id);if(!ids.length)return;
    const args=[JSON.stringify(ids)];
    const page=(await query(STATISTICS_QUESTION_PAGE,[args[0],countsAt]))[0];
    for(const row of page.open_of){const id=JSON.stringify([row.verb,row.producer,row.source]);
     const old=open.get(id)??{...row,n:0,dispatched:0};old.n+=Number(row.n);old.dispatched+=Number(row.dispatched);open.set(id,old);}
    for(const row of page.taken)out.takenBy[row.k]=(out.takenBy[row.k]||0)+Number(row.n);
    out.snapshot.open+=Number(page.open);out.snapshot.urgent+=Number(page.urgent);
    if(cut)add([page.pending]);
   });
   out.openOf=[...open.values()];out.snapshot.open=Math.min(out.snapshot.open,Math.max(1,out.snapshot.asking)*Math.max(key.dials.pack8_max,key.dials.pack_max));
   if(!cut)await pages('statistics_pending','SELECT batch,build_id,ruler,mode,initiator FROM state_board_pending_work',[],async rows=>add(await query(STATISTICS_RAW_PENDING_PAGE,[JSON.stringify(rows)])));
   else await pages('statistics_batches','SELECT id,at FROM entity_compute_batches',[],async rows=>{
    for(const batch of rows){if(typeof batch.at!=='string'||batch.at<=cut.cut_at)continue;
     await pages('statistics_batch_pending','SELECT batch,build_id,ruler,mode,initiator FROM state_board_pending_work WHERE batch=$1::text',[batch.id],async pending=>add(await query(STATISTICS_POSTCUT_PAGE,[JSON.stringify(pending)])));}
   });
   check();await assert(out);check();return out;
  });
  check();await assert(result);check();return result;
 };
 return {assert,status:()=>lastFailure?{failure:lastFailure,resolved_at:resolvedAt}:null,commit:async(bundle,statements)=>writeTransaction(async tx=>{
  await assert(bundle);await tx.query('SET LOCAL statement_timeout=3000',[]);const results=[];
  for(const statement of statements){if(stopped())throw Error('statistics stopped');results.push(await tx.query(placeholders(statement.sql),statement.args));}
  await assert(bundle);return results;
 }),async get(request={}){
  const began=clock(),deadline=began+STATISTICS_TOTAL_MS;
  const alive=()=>{if(stopped())throw Error('statistics stopped');};
  const current=await bounded(keyOf(),alive,deadline);
  if(('engine' in request&&request.engine!==current.engine)||('epoch' in request&&request.epoch!==current.epoch)){version++;held=null;throw Error('statistics source request changed');}
  const key={...current,...request};if(key.inventory_engine!==null&&key.inventory_engine!==undefined&&key.inventory_engine!==key.engine)throw Error('statistics engine contract changed');if(stopped())throw Error('statistics stopped');
  if(failure&&failure.id===identity(key)&&now()<Date.parse(failure.retry_at))throw Error('statistics retry due '+failure.retry_at);
  if(held&&held.version===version&&identity(held.key)===identity(key)&&now()>=Date.parse(held.countsAt)&&now()-Date.parse(held.countsAt)<DEMAND_COST_EVERY_MS){await bounded(assert(held),alive,deadline);return held;}
  if(flight&&flight.version===version&&flight.id===identity(key))return flight.promise;
  const mine=++version,entry={id:identity(key),version:mine};
  const work=scan(key,mine,began).then(async value=>{await assert(value);if(clock()>=value.deadline)throw Error('statistics total deadline');if(mine!==version)throw Error('statistics superseded');held=value;failure=null;if(lastFailure)resolvedAt=value.countsAt;return value;});
  entry.promise=bounded(work,()=>{alive();if(version!==mine)throw Error('statistics superseded');},deadline,()=>{if(version===mine)version++;})
   .catch(error=>{
    if(flight===entry&&!stopped()&&!/source changed|request changed|superseded|stopped|engine contract/.test(error.message)){
     failure={id:entry.id,key,at:iso(now()),retry_at:iso(now()+DEMAND_COST_EVERY_MS),code:error.code||'error'};lastFailure=failure;resolvedAt=null;
    }
    throw error;
   }).finally(()=>{if(flight===entry)flight=null;});
  flight=entry;return entry.promise;
 }};
}
