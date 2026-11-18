// SPDX-License-Identifier: AGPL-3.0-or-later
// Baselines never publish. Only a bounded, complete RR tail seals a sample.
import { iso } from './instant.js';
import {statisticsSourceSql} from './statistics_source.js';
export {statisticsSourceSql} from './statistics_source.js';

export const STATISTICS_PROJECTION_CONTRACT='statistics-projection-1';
export const STATISTICS_RETAINED_GENERATIONS=2;
export const STATISTICS_PROJECTION_LIMITS={pageRows:128,maxRows:8192,maxBytes:2097152,maxQueries:1024,maxMs:3000,maxWork:4096,maxStorageBytes:1073741824,storageReserveBytes:0,maxNamespaces:STATISTICS_RETAINED_GENERATIONS};
const kinds=['new_build','survey_build','refresh','sweep'],kindSql="('new_build','survey_build','refresh','sweep')";
const same=(a,b)=>JSON.stringify(a)===JSON.stringify(b),encoded=value=>JSON.stringify(value),decoded=key=>JSON.parse(key);
const keyText=key=>'['+key.map(encoded).join(', ')+']';
const namespaceOf=async key=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(encoded(key))))).map(x=>x.toString(16).padStart(2,'0')).join('');
const dimension=(key,n=1)=>[key,n];
const ordinary=producer=>kinds.includes(producer);
export const statisticsConsumerPageSql=after=>`WITH input AS MATERIALIZED(SELECT $1::text question,${after?'$2::text producer,$3::text ref,':''}$${after?4:2}::bigint cap)
 SELECT d.* FROM input i CROSS JOIN LATERAL(SELECT producer,ref,state,
  CASE WHEN producer='new_build' THEN 0 WHEN payload::jsonb->>'batch' LIKE 'rescore%' THEN 1 WHEN producer='refresh' THEN 3 ELSE 2 END k
  FROM relation_task_consumers WHERE question=i.question AND state='live' AND producer IN${kindSql} ${after?'AND(producer,ref)>(i.producer,i.ref)':''}
  ORDER BY producer,ref LIMIT i.cap OFFSET 0)d`;
const point=(id='$1::text')=>`SELECT s.id,s.verb,s.state,s.producer,${statisticsSourceSql('s')} source,s.waits::text,s.lease_until,s.lease,s.leased_at,
 EXISTS(SELECT 1 FROM state_compute_effective_work w WHERE w.id=s.id OFFSET 0) effective
 FROM state_compute_scheduling s WHERE s.id=${id}`;
const definitions=`SELECT string_agg(def,E'\n' ORDER BY name) FROM(
 SELECT c.relname name,pg_get_viewdef(c.oid,true) def FROM pg_class c WHERE c.oid IN('state_compute_effective_open'::regclass,'state_compute_work'::regclass,'state_compute_effective_work'::regclass,'state_board_effective_pending_work'::regclass)
 UNION ALL SELECT p.proname,pg_get_functiondef(p.oid) FROM pg_proc p WHERE p.oid IN('compute_order_cut()'::regprocedure,'compute_order_cut_epoch()'::regprocedure,'compute_order_cancelled(text,bigint,bigint,bigint,text)'::regprocedure,'compute_order_existing_evidence(text,bigint,text)'::regprocedure,'factory_default_zone(text)'::regprocedure)) d`;
export async function statisticsProjectionIdentity(query){
 const rows=await query(`SELECT compute_order_cut_epoch() epoch,g.revision::text generation,coalesce(g.blocked,(SELECT reason FROM state_statistics_invalidations LIMIT 1)) blocked,
  (SELECT jsonb_build_array(body::jsonb->>'engine',body::jsonb->>'source',body::jsonb->>'snapshot') FROM state_factory_findings WHERE kind='task_inventory') inventory,
  md5((${definitions})) fingerprint FROM state_statistics_generation g WHERE id=1`,[]);
 return {contract:STATISTICS_PROJECTION_CONTRACT,...rows[0]};
}
export class StatisticsProjectionBudget extends Error {}
export class StatisticsStorageCapacityHold extends StatisticsProjectionBudget {
 constructor(reason,capacity){super('statistics '+reason);this.capacity={kind:'statistics_storage',reason,...capacity};}
}

export function createStatisticsProjection({transaction,currentIdentity,stopped=()=>false,allowed=async()=>true,complete=async(_q,counts)=>counts,clock=()=>performance.now(),now=()=>Date.now(),limits={},onQuery=()=>{}}){
 const bound={...STATISTICS_PROJECTION_LIMITS,...limits};
 for(const [name,value] of Object.entries(bound))if(!Number.isSafeInteger(value)||value<(name==='storageReserveBytes'?0:1))throw Error('invalid statistics limit '+name);
 if(bound.maxNamespaces>STATISTICS_RETAINED_GENERATIONS)throw Error('invalid statistics generation budget');
 if(bound.pageRows>4096)throw Error('statistics page exceeds input bound');
 if(typeof currentIdentity!=='function')throw Error('statistics current identity is required');
 let scanRows=bound.pageRows;
 const check=()=>{if(stopped())throw Error('statistics stopped');};
 const permit=async context=>{context.guard();if(!await allowed({signal:context.signal,remainingMs:Math.max(1,bound.maxMs-(clock()-context.began))}))throw Error('statistics paused or stopped');context.guard();};
 const operation=async work=>{
  const began=clock(),controller=new AbortController();let expired=false,timer;
  const guard=()=>{check();if(expired||clock()-began>=bound.maxMs)throw new StatisticsProjectionBudget('statistics time budget');};
  const timeout=new Promise((_,reject)=>{timer=setTimeout(()=>{expired=true;const error=new StatisticsProjectionBudget('statistics time budget');controller.abort(error);reject(error);},bound.maxMs);});
  try{return await Promise.race([Promise.resolve().then(()=>work({began,guard,signal:controller.signal,identityBytes:0})),timeout]);}finally{clearTimeout(timer);}
 };
 const identity=async context=>{await permit(context);const key=await currentIdentity({signal:context.signal,remainingMs:Math.max(1,bound.maxMs-(clock()-context.began))});await permit(context);
  context.identityBytes+=new TextEncoder().encode(encoded(key)).length;if(context.identityBytes>bound.maxBytes)throw new StatisticsProjectionBudget('statistics identity byte budget');
  if(key.blocked)throw Error(key.blocked);return key;};
 const assert=async(key,context)=>{if(!same(await identity(context),key))throw Error('statistics namespace changed');};
 const run=async(fn,{rr=false,key=null,context,validateAfter=true}={})=>{
  const {began,guard}=context,usage={rows:0,bytes:0,queries:0};
  await permit(context);
  return transaction(async tx=>{
   const query=async(sql,args=[])=>{
    guard();if(++usage.queries>bound.maxQueries)throw new StatisticsProjectionBudget('statistics query budget');
    const remaining=Math.max(1,Math.min(1000,Math.floor(bound.maxMs-(clock()-began))));
    await tx.query('SET LOCAL statement_timeout='+remaining,[]);guard();
    const {rows}=await tx.query(sql,args);guard();usage.rows+=rows.length;usage.bytes+=new TextEncoder().encode(encoded(rows)+encoded(args)).length;
    if(usage.rows>bound.maxRows||usage.bytes+context.identityBytes>bound.maxBytes)throw new StatisticsProjectionBudget('statistics input budget');
    await onQuery({sql,args,rows,rr,usage:{...usage}});guard();return rows;
   };
   if(rr)await query('SET TRANSACTION ISOLATION LEVEL REPEATABLE READ');
   await query("SET LOCAL lock_timeout='250ms'");
   const [lock]=await query("SELECT pg_try_advisory_xact_lock(hashtextextended('statistics-projector:'||current_schema(),0)) held");
   if(!lock.held)throw Error('another statistics projector owns this transaction');
   let namespace=null;
   if(key){
    const actual=await statisticsProjectionIdentity(query);if(!same(actual,key))throw Error('statistics namespace changed');
    const [head]=await query(`SELECT h.namespace FROM state_statistics_head h JOIN state_statistics_namespaces n ON n.id=h.namespace
     WHERE h.id=1 AND n.identity-'engine'=($1::text)::jsonb`,[encoded(key)]);
    if(!head)throw Error('statistics target changed');namespace=head.namespace;
   }
   const value=await fn(query,usage,namespace);
   if(key&&validateAfter)await assert(key,context);else await permit(context);
   return value;
  });
 };
 const enqueuePage=async(q,ns,kind,keys)=>{
  if(!keys.length)return;
  await q(`INSERT INTO state_statistics_obligations(namespace,kind,key)
    SELECT DISTINCT $1,$2,value::text FROM jsonb_array_elements(($3::text)::jsonb)
    ON CONFLICT(namespace,kind,key) DO UPDATE SET cursor='{}'::jsonb`,[ns,kind,encoded(keys)]);
 };
 const replace=async(q,ns,kind,key,body)=>q('SELECT statistics_replace($1,$2,$3,($4::text)::jsonb)',[ns,kind,key,encoded(body)]);
 const replacePage=(q,ns,changes)=>q(`SELECT statistics_replace($1,c.kind,c.key,c.body)
   FROM jsonb_to_recordset(($2::text)::jsonb) AS c(kind text,key text,body jsonb)`,[ns,encoded(changes)]);
 const finish=async(q,ns,work)=>q('DELETE FROM state_statistics_obligations WHERE namespace=$1 AND kind=$2 AND key=$3',[ns,work.kind,work.key]);
 const finishPage=(q,ns,kind,keys)=>q(`DELETE FROM state_statistics_obligations WHERE namespace=$1 AND kind=$2
   AND key IN(SELECT jsonb_array_elements_text(($3::text)::jsonb))`,[ns,kind,encoded(keys)]);
 const save=async(q,ns,work,cursor)=>q('UPDATE state_statistics_obligations SET cursor=($4::text)::jsonb WHERE namespace=$1 AND kind=$2 AND key=$3',[ns,work.kind,work.key,encoded(cursor)]);
 const importJournal=async(q,ns)=>{
  const [size]=await q('SELECT count(*)::text n FROM state_statistics_obligations WHERE namespace=$1',[ns]);
  if(BigInt(size.n)>=BigInt(2*bound.pageRows))return;
  return q(`WITH page AS MATERIALIZED(SELECT j.* FROM state_statistics_journal j WHERE NOT EXISTS(
   SELECT 1 FROM state_statistics_checkpoints c WHERE c.namespace=$1 AND(c.owner,c.kind,c.key,c.revision)=(j.owner,j.kind,j.key,j.revision))
   ORDER BY j.owner,j.kind,j.key LIMIT $2::bigint),
  enqueued AS(INSERT INTO state_statistics_obligations(namespace,kind,key)
   SELECT DISTINCT $1,kind,key FROM page ON CONFLICT(namespace,kind,key) DO UPDATE SET cursor='{}'::jsonb RETURNING 1)
  INSERT INTO state_statistics_checkpoints(namespace,owner,kind,key,revision)
   SELECT $1,p.owner,p.kind,p.key,p.revision FROM page p CROSS JOIN(SELECT count(*) FROM enqueued)gate
   ON CONFLICT(namespace,owner,kind,key) DO UPDATE SET revision=excluded.revision`,[ns,bound.pageRows]);
 };
 const pendingImports=async(q,ns)=>!!(await q(`SELECT 1 FROM state_statistics_journal j WHERE NOT EXISTS(
  SELECT 1 FROM state_statistics_checkpoints c WHERE c.namespace=$1 AND(c.owner,c.kind,c.key,c.revision)=(j.owner,j.kind,j.key,j.revision)) LIMIT 1`,[ns])).length;
 const nextWork=async(q,ns)=>(await q(`SELECT kind,key,cursor FROM state_statistics_obligations WHERE namespace=$1
  ORDER BY CASE kind WHEN 'p' THEN 0 WHEN 'k' THEN 1 WHEN 'r' THEN 1 ELSE 2 END,kind,key LIMIT 1`,[ns]))[0];
 const questionState=(row,at)=>{
  const body=[];
  if(row?.state==='open'&&row.effective){
   body.push(dimension(['open_of',row.verb,row.producer,row.source]));
   if(row.leased_at!==null||row.lease!==null)body.push(dimension(['dispatched',row.verb,row.producer,row.source]));
   if(BigInt(row.waits)>0n)body.push(dimension(['urgent']));
   if(BigInt(row.waits)===0n&&(row.lease_until===null||row.lease_until<at))body.push(dimension(['open']));
  }
  const expires=row?.state==='open'&&row.effective&&row.lease_until!==null&&row.lease_until>=at&&BigInt(row.waits)===0n?row.lease_until:null;
  const taken=row?.state==='open'&&row.effective&&row.verb==='simulate'&&(row.leased_at!==null||row.lease!==null);
  return {body,expires,taken};
 };
 const readyQuestion=async(q,ns,id,at)=>{
  const [row]=await q(point(),[id]),{body,expires,taken}=questionState(row,at);
  await replace(q,ns,'q',keyText([id]),body);
  await q('DELETE FROM state_statistics_expiry WHERE namespace=$1 AND question=$2',[ns,id]);
  if(expires)await q('INSERT INTO state_statistics_expiry(namespace,question,at) VALUES($1,$2,$3)',[ns,id,expires]);
  return taken;
 };
 // A question page commits contributions and its downstream keys together.
 // Deep consumer lists continue through the existing one-question cursor.
 const questionPage=async(q,ns,at)=>{
  const consumerLimit=Math.min(16,bound.pageRows);
  const questionLimit=Math.min(128,Math.floor(4096/(consumerLimit+1)));
  const [workCount]=await q('SELECT count(*) n FROM state_statistics_obligations WHERE namespace=$1',[ns]);
  const remaining=bound.maxWork-Number(workCount.n);
  const sources=await q(`SELECT work.key,work.key::jsonb->>0 question,to_jsonb(s) state,consumers.rows
   FROM (SELECT key FROM state_statistics_obligations WHERE namespace=$1 AND kind='q' AND cursor='{}'
    ORDER BY key LIMIT $2::bigint) work
   LEFT JOIN LATERAL (${point("(work.key::jsonb->>0)")} OFFSET 0) s ON true
   CROSS JOIN LATERAL (SELECT coalesce(jsonb_agg(to_jsonb(d)),'[]') rows FROM
     (SELECT ref,producer,CASE WHEN producer='new_build' THEN 0
       WHEN payload::jsonb->>'batch' LIKE 'rescore%' THEN 1 WHEN producer='refresh' THEN 3 ELSE 2 END k
      FROM relation_task_consumers WHERE question=work.key::jsonb->>0 AND state='live' AND producer IN${kindSql}
      ORDER BY producer,ref LIMIT $3::bigint OFFSET 0) d) consumers ORDER BY work.key`,[ns,questionLimit,consumerLimit+1]);
  if(sources[0]?.rows.length>consumerLimit)return 0;
  const completed=[],changes=[],expiries=[],keys=[],refs=[];
  for(const source of sources){
   if(source.rows.length>consumerLimit)continue;
   if(2*(refs.length+source.rows.length)>remaining)break;
   const {body,expires,taken}=questionState(source.state,at);
   let minimum=null;
   for(const row of source.rows){
    const [build='',ruler='',mode='']=row.ref.split('|');
    keys.push([build,ruler,mode]);refs.push([row.ref]);
    if(taken)minimum=minimum===null?row.k:Math.min(minimum,row.k);
   }
   completed.push(source.key);expiries.push({question:source.question,at:expires});
   changes.push({kind:'q',key:source.key,body},{kind:'t',key:source.key,body:minimum===null?[]:[dimension(['taken',minimum])]});
  }
  if(!completed.length)return 0;
  await replacePage(q,ns,changes);
  await q(`DELETE FROM state_statistics_expiry WHERE namespace=$1 AND question IN
   (SELECT question FROM jsonb_to_recordset(($2::text)::jsonb) AS e(question text,at text))`,[ns,encoded(expiries)]);
  await q(`INSERT INTO state_statistics_expiry(namespace,question,at)
   SELECT $1,question,at FROM jsonb_to_recordset(($2::text)::jsonb) AS e(question text,at text) WHERE at IS NOT NULL`,[ns,encoded(expiries)]);
  await enqueuePage(q,ns,'k',keys);
  await enqueuePage(q,ns,'r',refs);
  await finishPage(q,ns,'q',completed);
  return completed.length;
 };
 const pendingPoint=tuple=>`SELECT p.initiator,${statisticsSourceSql('p')} source,b.id IS NOT NULL has_batch,b.at,
  (SELECT cut_at FROM compute_order_cut()) cut_at,
  EXISTS(SELECT 1 FROM(SELECT question,state FROM relation_task_consumers WHERE producer IN${kindSql} AND ref=p.build_id||'|'||p.ruler||'|'||p.mode OFFSET 0)d
    CROSS JOIN LATERAL(SELECT 1 FROM state_compute_work w WHERE w.id=d.question OFFSET 0)live WHERE d.state='live') exact,
  EXISTS(SELECT 1 FROM relation_task_consumers WHERE producer='new_build' AND ref=p.build_id||'|'||p.ruler||'|'||p.mode AND state='live') new_build
  FROM state_board_pending_work p LEFT JOIN entity_compute_batches b ON b.id=p.batch WHERE(p.batch,p.build_id,p.ruler,p.mode)=${tuple}`;
 const pendingBody=(row,batch,live,epoch)=>{
  if(!row)return [];
  const n=epoch===null?1:Number(live)+Number(row.has_batch&&row.at>row.cut_at&&!row.exact),body=[];
  if(n)body.push(dimension(['queue_rows'],n));
  if(n&&row.has_batch){
   const k=row.new_build?0:batch.startsWith('rescore')?1:batch.startsWith('refresh-')?3:2;
   body.push(dimension(['owed',k,row.source],n));
  }
  return body;
 };
 // Exact references cover the common case. Alias candidates retain the
 // existing bounded reference cursor.
 const pendingPage=async(q,ns,epoch)=>{
  const rows=await q(`WITH keys AS MATERIALIZED (
    SELECT key,key::jsonb->>0 batch,key::jsonb->>1 build,key::jsonb->>2 ruler,key::jsonb->>3 mode
    FROM state_statistics_obligations WHERE namespace=$1 AND kind='p' AND cursor='{}' ORDER BY key LIMIT 32
   )
   SELECT keys.key,keys.batch,to_jsonb(p) row,
    CASE WHEN $2::boolean AND p.exact THEN
      strpos(keys.build||keys.ruler||keys.mode,'|')=0 AND EXISTS(
        SELECT 1 FROM relation_task_consumers d CROSS JOIN LATERAL
          (SELECT 1 FROM state_compute_effective_work w WHERE w.id=d.question OFFSET 0) live
        WHERE d.state='live' AND d.producer IN${kindSql} AND d.ref=ref.value LIMIT 1) END direct,
    CASE WHEN $2::boolean AND p.exact IS NOT NULL THEN
      EXISTS(SELECT 1 FROM relation_task_consumers d WHERE d.state='live' AND d.producer IN${kindSql}
        AND d.ref>=ref.value||'|' AND d.ref<ref.value||'}' LIMIT 1)
      OR(keys.mode='' AND EXISTS(SELECT 1 FROM relation_task_consumers d WHERE d.state='live' AND d.producer IN${kindSql}
        AND d.ref=keys.build||'|'||keys.ruler LIMIT 1))
      OR(keys.mode='' AND keys.ruler='' AND EXISTS(SELECT 1 FROM relation_task_consumers d
        WHERE d.state='live' AND d.producer IN${kindSql} AND d.ref=keys.build LIMIT 1))
      OR strpos(keys.build||keys.ruler||keys.mode,'|')>0
    END may_alias
   FROM keys
   CROSS JOIN LATERAL (SELECT keys.build||'|'||keys.ruler||'|'||keys.mode value) ref
   LEFT JOIN LATERAL (${pendingPoint('(keys.batch,keys.build,keys.ruler,keys.mode)')} OFFSET 0) p ON true
   ORDER BY keys.key`,[ns,epoch!==null]);
  const complete=rows.filter(value=>!value.row || epoch===null || value.direct || value.may_alias===false);
  if(complete[0]?.key!==rows[0]?.key || !complete.length)return 0;
  await replacePage(q,ns,complete.map(({key,batch,row,direct})=>({kind:'p',key,body:pendingBody(row,batch,!!direct,epoch)})));
  await finishPage(q,ns,'p',complete.map(value=>value.key));
  return complete.length;
 };
 const referencePage=async(q,ns,kind)=>{
  const pendingLimit=Math.min(32,bound.pageRows);
  const [workCount]=await q('SELECT count(*) n FROM state_statistics_obligations WHERE namespace=$1',[ns]);
  const remaining=bound.maxWork-Number(workCount.n);
  const rows=await q(`WITH keys AS MATERIALIZED (
    SELECT key,key::jsonb value FROM state_statistics_obligations
    WHERE namespace=$1 AND kind=$2 AND cursor='{}' ORDER BY key LIMIT 64
   ), resolved AS (
    SELECT key,CASE WHEN $2='k' THEN value ELSE to_jsonb(string_to_array(value->>0,'|')) END identity
    FROM keys
   )
   SELECT r.key,jsonb_array_length(r.identity) parts,coalesce(m.pending,'[]') pending FROM resolved r
   LEFT JOIN LATERAL (
    SELECT jsonb_agg(jsonb_build_array(p.batch,p.build_id,p.ruler,p.mode)) pending FROM (
      SELECT batch,build_id,ruler,mode FROM state_board_pending_work
      WHERE(build_id,ruler,mode)=(r.identity->>0,r.identity->>1,r.identity->>2)
      ORDER BY batch LIMIT $3::bigint OFFSET 0
    ) p
   ) m ON jsonb_array_length(r.identity)=3 ORDER BY r.key`,[ns,kind,pendingLimit+1]);
  const completed=[],keys=[];
  for(const row of rows){
   if(row.parts!==3||row.pending.length>pendingLimit||keys.length+row.pending.length>remaining)break;
   completed.push(row.key);keys.push(...row.pending);
  }
  if(!completed.length)return 0;
  await enqueuePage(q,ns,'p',keys);
  await finishPage(q,ns,kind,completed);
  return completed.length;
 };
 const fanout=async(q,ns,work,at)=>{
  const key=decoded(work.key),cursor=work.cursor;
  if(work.kind==='q'){
   if(!cursor.started){cursor.started=true;cursor.taken=await readyQuestion(q,ns,key[0],at);cursor.min=null;cursor.after=null;}
   const args=[key[0],...(cursor.after??[]),bound.pageRows];
   const rows=await q(statisticsConsumerPageSql(!!cursor.after),args);
   const keys=[],refs=[];
   for(const row of rows)if(ordinary(row.producer)){
    const [build='',ruler='',mode='']=row.ref.split('|');
    keys.push([build,ruler,mode]);refs.push([row.ref]);
    if(cursor.taken&&row.state==='live')cursor.min=cursor.min===null?Number(row.k):Math.min(cursor.min,Number(row.k));
   }
   await enqueuePage(q,ns,'k',keys);await enqueuePage(q,ns,'r',refs);
   if(rows.length){const last=rows.at(-1);cursor.after=[last.producer,last.ref];}
   if(rows.length===bound.pageRows)return save(q,ns,work,cursor);
   await replace(q,ns,'t',work.key,cursor.min===null?[]:[dimension(['taken',cursor.min])]);
  }else{
   let rows;
   if(work.kind==='b'){const args=[key[0],...(cursor.after?.slice(1)??[]),bound.pageRows];rows=await q(`SELECT batch,build_id,ruler,mode FROM state_board_pending_work WHERE batch=$1 ${cursor.after?'AND(build_id,ruler,mode)>($2,$3,$4)':''} ORDER BY build_id,ruler,mode LIMIT $${args.length}::bigint`,args);}
   if(work.kind==='k'){const args=[...key,...(cursor.after?[cursor.after[0]]:[]),bound.pageRows];rows=await q(`SELECT batch,build_id,ruler,mode FROM state_board_pending_work WHERE(build_id,ruler,mode)=($1,$2,$3) ${cursor.after?'AND batch>$4':''} ORDER BY batch LIMIT $${args.length}::bigint`,args);}
   if(work.kind==='r'){
    const ref=key[0],pipes=[];for(let i=0;i<ref.length;i++)if(ref[i]==='|')pipes.push(i);
    let i=cursor.i??0,j=cursor.j??1;const candidates=[];
    while(i<pipes.length-1&&candidates.length<bound.pageRows){
     candidates.push({build_id:ref.slice(0,pipes[i]),ruler:ref.slice(pipes[i]+1,pipes[j]),mode:ref.slice(pipes[j]+1)});
     if(++j>=pipes.length){i++;j=i+1;}
    }
    rows=candidates.length?await q(`SELECT b.batch,b.build_id,b.ruler,b.mode FROM jsonb_to_recordset(($1::text)::jsonb) AS k(build_id text,ruler text,mode text)
     CROSS JOIN LATERAL(SELECT batch,build_id,ruler,mode FROM state_board_pending_work b WHERE(b.build_id,b.ruler,b.mode)=(k.build_id,k.ruler,k.mode) OFFSET 0)b`,[encoded(candidates)]):[];
    await enqueuePage(q,ns,'p',rows.map(row=>[row.batch,row.build_id,row.ruler,row.mode]));
    if(i<pipes.length-1)return save(q,ns,work,{i,j});return finish(q,ns,work);
   }
   await enqueuePage(q,ns,'p',rows.map(row=>[row.batch,row.build_id,row.ruler,row.mode]));
   if(rows.length===bound.pageRows){const last=rows.at(-1);cursor.after=work.kind==='r'?encoded([last.batch,last.build_id,last.ruler,last.mode]):[last.batch,last.build_id,last.ruler,last.mode];return save(q,ns,work,cursor);}
  }
  return finish(q,ns,work);
 };
 const pending=async(q,ns,work,key)=>{
  const [batch,build,ruler,mode]=decoded(work.key),cursor=work.cursor;
  if(!cursor.started){
   const [row]=await q(pendingPoint('($1,$2,$3,$4)'),[batch,build,ruler,mode]);
   if(!row){await replace(q,ns,'p',work.key,[]);return finish(q,ns,work);}
   Object.assign(cursor,{started:true,row,phase:0,after:null,live:false});
  }
  const row=cursor.row,ref=build+'|'+ruler+'|'+mode;
  const phases=[['ref=$1',ref],...(mode===''?[['ref=$1',build+'|'+ruler]]:[]),...(mode===''&&ruler===''?[['ref=$1',build]]:[]),['ref>=$1 AND ref<$2',ref+'|',ref+'}']];
  if(key.epoch!==null&&!cursor.live&&cursor.phase<phases.length){
   const [where,...range]=phases[cursor.phase],offset=range.length,after=cursor.after,args=[...range,...(after??[]),bound.pageRows];
   const rows=await q(`SELECT d.ref,d.producer,d.state,split_part(d.ref,'|',1) build,split_part(d.ref,'|',2) ruler,split_part(d.ref,'|',3) mode,
    CASE WHEN d.state='live' THEN EXISTS(SELECT 1 FROM state_compute_effective_work w WHERE w.id=d.question OFFSET 0) ELSE false END effective
    FROM(SELECT ref,producer,question,state FROM relation_task_consumers WHERE producer IN${kindSql} AND ${where}
      ${after?`AND(ref,producer)>($${offset+1},$${offset+2})`:''} ORDER BY ref,producer LIMIT $${args.length}::bigint OFFSET 0)d`,args);
   cursor.live=rows.some(d=>d.effective&&d.build===build&&d.ruler===ruler&&d.mode===mode);
   if(rows.length===bound.pageRows){const last=rows.at(-1);cursor.after=[last.ref,last.producer];}else{cursor.phase++;cursor.after=null;}
   if(!cursor.live&&cursor.phase<phases.length)return save(q,ns,work,cursor);
  }
  await replace(q,ns,'p',work.key,pendingBody(row,batch,cursor.live,key.epoch));return finish(q,ns,work);
 };
 const consume=async(q,ns,work,key,at)=>{
  if(!work)return 0;
  let processed=0;
  if(work.kind==='q'&&!work.cursor.started)processed=await questionPage(q,ns,at);
  if(work.kind==='p'&&!work.cursor.started)processed=await pendingPage(q,ns,key.epoch);
  if((work.kind==='k'||work.kind==='r')&&Object.keys(work.cursor).length===0)processed=await referencePage(q,ns,work.kind);
  if(processed)return processed;
  if(work.kind==='p')await pending(q,ns,work,key);
  else await fanout(q,ns,work,at);
  return 1;
 };
 const baseline=async(q,ns,state,key)=>{
  const cursor=state.cursor;let rows;
  if(Number(state.phase)===0){
   const args=[...cursor,scanRows];rows=await q(`WITH page AS MATERIALIZED(SELECT id,state FROM state_compute_scheduling ${cursor.length?'WHERE id>$1':''} ORDER BY id LIMIT $${args.length}::bigint)
    SELECT p.id,CASE WHEN p.state='open' OR EXISTS(SELECT 1 FROM relation_task_consumers d
      WHERE d.question=p.id AND d.state='live' AND d.producer IN${kindSql} LIMIT 1 OFFSET 0)
    THEN EXISTS(SELECT 1 FROM state_compute_effective_work w WHERE w.id=p.id OFFSET 0) ELSE false END candidate
    FROM page p ORDER BY p.id`,args).catch(error=>{scanRows=bound.pageRows;throw error;});
   const candidates=rows.filter(row=>row.candidate),page=candidates.slice(0,bound.pageRows);
   const limited=candidates.length>page.length;
   const last=limited?page.at(-1):rows.at(-1),done=!limited&&rows.length<scanRows;
   await enqueuePage(q,ns,'q',page.map(row=>[row.id]));
   await q('UPDATE state_statistics_namespaces SET phase=$2::bigint,cursor=($3::text)::jsonb WHERE id=$1',
    [ns,done?1:0,encoded(done?[]:[last.id])]);
   scanRows=limited?bound.pageRows:Math.max(bound.pageRows,Math.min(scanRows*2,1024,Math.floor(bound.maxRows/2)));
  }else if(Number(state.phase)===1&&key.epoch!==null){
   const args=[...cursor,bound.pageRows];rows=await q(`SELECT id,at>(SELECT cut_at FROM compute_order_cut()) post FROM entity_compute_batches ${cursor.length?'WHERE id>$1':''} ORDER BY id LIMIT $${args.length}::bigint`,args);
   await enqueuePage(q,ns,'b',rows.filter(row=>row.post).map(row=>[row.id]));
   await q('UPDATE state_statistics_namespaces SET phase=$2::bigint,cursor=($3::text)::jsonb WHERE id=$1',[ns,rows.length===bound.pageRows?1:2,encoded(rows.length===bound.pageRows?[rows.at(-1).id]:[])]);
  }else if(Number(state.phase)===1){
   const args=[...cursor,bound.pageRows];rows=await q(`SELECT batch,build_id,ruler,mode FROM state_board_pending_work ${cursor.length?'WHERE(batch,build_id,ruler,mode)>($1,$2,$3,$4)':''} ORDER BY batch,build_id,ruler,mode LIMIT $${args.length}::bigint`,args);
   await enqueuePage(q,ns,'p',rows.map(row=>[row.batch,row.build_id,row.ruler,row.mode]));
   const last=rows.at(-1);await q('UPDATE state_statistics_namespaces SET phase=$2::bigint,cursor=($3::text)::jsonb WHERE id=$1',[ns,rows.length===bound.pageRows?1:2,encoded(rows.length===bound.pageRows?[last.batch,last.build_id,last.ruler,last.mode]:[])]);
  }
 };
 const cleanup=async(q,ns)=>q(`WITH page AS MATERIALIZED(SELECT owner,kind,key,revision FROM state_statistics_checkpoints WHERE namespace=$1 ORDER BY owner,kind,key LIMIT $2::bigint),
  removed AS(DELETE FROM state_statistics_journal j USING page p WHERE(j.owner,j.kind,j.key,j.revision)=(p.owner,p.kind,p.key,p.revision) RETURNING 1)
  DELETE FROM state_statistics_checkpoints c USING page p,(SELECT count(*) FROM removed)gate
   WHERE c.namespace=$1 AND(c.owner,c.kind,c.key,c.revision)=(p.owner,p.kind,p.key,p.revision) RETURNING c.revision::text`,[ns,bound.pageRows]);
 // This is qualified admission headroom, not a PostgreSQL allocation or WAL cap.
 // Zero refuses writes until the operator supplies a measured capacity reservation.
 const storage=async(q,ns,{admit=false}={})=>{
  const [row]=await q(`SELECT(SELECT count(*) FROM state_statistics_obligations WHERE namespace=$1)::text work,
   (SELECT sum(pg_total_relation_size(c.oid)) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
    WHERE n.nspname=current_schema() AND c.relkind='r' AND c.relname LIKE 'state_statistics_%')::text bytes`,[ns]);
  const capacity={bytes:row.bytes,work:row.work,limitBytes:bound.maxStorageBytes,reserveBytes:bound.storageReserveBytes};
  if(admit&&!bound.storageReserveBytes)throw new StatisticsStorageCapacityHold('storage reservation unqualified',capacity);
  if(BigInt(row.work)>BigInt(bound.maxWork)||BigInt(row.bytes)+BigInt(admit?bound.storageReserveBytes:0)>BigInt(bound.maxStorageBytes))
   throw new StatisticsStorageCapacityHold('retained storage budget',capacity);
  return capacity;
 };
 const reading=async(q,ns,at)=>{
  const rows=await q('SELECT key,n::text FROM state_statistics_rollup WHERE namespace=$1 ORDER BY key',[ns]);
  const out={countsAt:at,namespace:ns,by:{},sources:{},takenBy:{},openOf:[],snapshot:{at,queue_rows:0,open:0,urgent:0}},open=new Map();
  for(const row of rows){const [metric,...axes]=decoded(row.key),n=Number(row.n);if(n<0)throw Error('negative statistics rollup');
   if(['queue_rows','open','urgent'].includes(metric))out.snapshot[metric]=n;
   else if(metric==='taken')out.takenBy[axes[0]]=n;
   else if(metric==='owed'){const [k,source]=axes;out.by[k]=(out.by[k]??0)+n;(out.sources[k]??={requested:0,system:0,unattributed:0})[source]+=n;}
   else{const id=encoded(axes),entry=open.get(id)??{verb:axes[0],producer:axes[1],source:axes[2],n:0,dispatched:0};entry[metric==='open_of'?'n':'dispatched']=n;open.set(id,entry);}
  }
  out.openOf=[...open.values()];return out;
 };
 return {
  async start(){return operation(async context=>{const key=await identity(context);
   return run(async q=>{
    const [previous]=await q('SELECT n.identity FROM state_statistics_head h JOIN state_statistics_namespaces n ON n.id=h.namespace WHERE h.id=1');
    if(previous?.identity.contract===key.contract&&previous.identity.fingerprint!==key.fingerprint)throw Error('statistics membership contract requires upgrade');
    const [current]=await q(`SELECT h.namespace FROM state_statistics_head h JOIN state_statistics_namespaces n ON n.id=h.namespace
     WHERE h.id=1 AND n.identity-'engine'=($1::text)::jsonb`,[encoded(key)]);
    if(current){await storage(q,current.namespace,{admit:true});await assert(key,context);return{key,namespace:current.namespace};}
    const retained=await q('SELECT id FROM state_statistics_namespaces ORDER BY id LIMIT $1::bigint',[bound.maxNamespaces]);
    if(retained.length>=bound.maxNamespaces)throw new StatisticsProjectionBudget('statistics retained generation budget');
    await storage(q,null,{admit:true});
    const [version]=await q("SELECT nextval('statistics_revision')::text incarnation"),ns=await namespaceOf(key)+'-'+version.incarnation;
    await q('INSERT INTO state_statistics_namespaces(id,identity) VALUES($1,($2::text)::jsonb)',[ns,encoded(key)]);
    await q('INSERT INTO state_statistics_head(id,namespace) VALUES(1,$1) ON CONFLICT(id) DO UPDATE SET namespace=excluded.namespace',[ns]);await storage(q,ns);await assert(key,context);return{key,namespace:ns};},{context});});
  },
  async step(){return operation(async context=>{const key=await identity(context),at=iso(now());const progress=await run(async(q,usage,ns)=>{
   const [state]=await q('SELECT phase::text,cursor FROM state_statistics_namespaces WHERE id=$1',[ns]);
   if(!state)throw Error('statistics baseline not started');
   await storage(q,ns,{admit:true});
   if(Number(state.phase)<2 && Number((await q('SELECT count(*) n FROM state_statistics_obligations WHERE namespace=$1',[ns]))[0].n)<2*bound.pageRows)
    await baseline(q,ns,state,key);
   await importJournal(q,ns);const processed=await consume(q,ns,await nextWork(q,ns),key,at);
   await storage(q,ns);return{baseline:Number(state.phase)===2,worked:processed>0,processed,usage};
  },{key,context});
   await run(async(q,_usage,ns)=>{await storage(q,ns,{admit:true});const value=await cleanup(q,ns);await storage(q,ns);return value;},{key,context,validateAfter:false});return progress;});},
  async seal(){return operation(async context=>{const key=await identity(context),at=iso(now());return run(async(q,usage,ns)=>{
   await q('SELECT compute_order_cut_lock()');
   const [state]=await q('SELECT phase::text FROM state_statistics_namespaces WHERE id=$1',[ns]);
   if(Number(state?.phase)!==2)throw Error('statistics baseline incomplete');
   await storage(q,ns,{admit:true});
   for(;;){await importJournal(q,ns);
    if(await consume(q,ns,await nextWork(q,ns),key,at))continue;
    const due=await q('SELECT question FROM state_statistics_expiry WHERE namespace=$1 AND at<$2 ORDER BY at,question LIMIT $3::bigint',[ns,at,bound.pageRows]);
    if(due.length){for(const row of due)await readyQuestion(q,ns,row.question,at);continue;}
    if(await pendingImports(q,ns))continue;break;
   }
   await storage(q,ns);const counts=await complete(q,{...await reading(q,ns,at),key},{at,key});
   if(counts.countsAt!==at||counts.namespace!==ns||!same(counts.key,key)||counts.snapshot.at!==at)throw Error('statistics completion changed sample identity');
   await q('UPDATE state_statistics_namespaces SET counts_at=$2,published=($3::text)::jsonb WHERE id=$1',[ns,at,encoded(counts)]);
   await storage(q,ns);return{counts,usage};
  },{rr:true,key,context});});},
  async checkStorage(){return operation(async context=>{await identity(context);return run(q=>storage(q,null,{admit:true}),{context});});},
  async read(){return operation(async context=>{const key=await identity(context);return run(async(q,_usage,ns)=>(await q('SELECT published FROM state_statistics_namespaces WHERE id=$1',[ns]))[0]?.published??null,{key,context});});}
 };
}
