// SPDX-License-Identifier: AGPL-3.0-or-later
// A bounded complete partition joins accepted facts to the existing native publisher.
import {createHash} from 'node:crypto';
import {mkdtemp,readFile,writeFile,rm,mkdir,stat} from 'node:fs/promises';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {spawn} from 'node:child_process';
import {pathToFileURL} from 'node:url';
import {BoardInputMailbox} from './board_input_mailbox.mjs';
import {openInventoryOperator} from './inventory_operator.mjs';
import {boardStorage} from './board_storage.mjs';
import {enqueueRequestedPartition} from './board_requested_partitions.mjs';

const hash=value=>createHash('sha256').update(value).digest('hex');
const name=value=>typeof value==='string'&&/^[a-z0-9_]{1,160}$/.test(value);
const MAX_BYTES=8*1024*1024;
const MAX_INPUT_BYTES=8*1024*1024;
class BoardPartitionQualificationError extends Error {}

export async function captureBoardPartition(sql,{weapon,ruler,qualify_requested=false}) {
 if(!name(weapon)||!name(ruler))throw Error('invalid board partition');
 return sql.begin('isolation level repeatable read read only',async tx=>{
  const [{captured_at,snapshot}]=await tx.unsafe(`SELECT to_char(clock_timestamp() AT TIME ZONE 'UTC',
    'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') captured_at,pg_current_snapshot()::text snapshot`);
  const selected=qualify_requested
   ?(await tx.unsafe('SELECT board_requested_partition_selected($1,$2,false) part',[weapon,ruler]))[0].part
   :(await tx.unsafe('SELECT revision,canonical_sources FROM state_board_read_partitions WHERE weapon=$1 AND ruler=$2 AND enabled',[weapon,ruler]))[0];
  if(!selected||(!selected.canonical_sources&&!qualify_requested))throw Error('board source identities require qualification');
  const builds=[],facts=[],started=Date.now();let bytes=0;
  const query=selected.revision?`WITH identities AS MATERIALIZED (
     SELECT DISTINCT key::jsonb->>0 id FROM state_board_read_routes WHERE scope='measurement' AND weapon=$1 AND ruler=$2)
    SELECT b.id,b.at,b.record,f.rows facts FROM identities i JOIN entity_weapon_builds b ON b.id=i.id
    CROSS JOIN LATERAL (
     SELECT jsonb_agg(to_jsonb(s) ORDER BY s.mode) rows FROM board_facts s WHERE s.identity=b.id AND s.ruler=$2
    )f WHERE b.weapon=$1 AND f.rows IS NOT NULL`:`SELECT b.id,b.at,
     CASE WHEN f.rows IS NOT NULL THEN b.record END record,f.rows facts
    FROM entity_weapon_builds b LEFT JOIN LATERAL (
     SELECT jsonb_agg(to_jsonb(s) ORDER BY s.mode) rows FROM board_facts s WHERE s.identity=b.id AND s.ruler=$2
    )f ON true WHERE b.weapon=$1`;
  await tx.unsafe('DECLARE board_source_capture NO SCROLL CURSOR FOR '+query,[weapon,ruler]);
  for(;;){
   if(Date.now()-started>120000)throw new BoardPartitionQualificationError('partition capture exceeded its time budget; current published view retained');
   const page=await tx.unsafe('FETCH FORWARD 256 FROM board_source_capture');
   for(const row of page){
    if(!row.facts)continue;
    bytes+=Buffer.byteLength(row.record)+Buffer.byteLength(JSON.stringify(row.facts));
    if(bytes>MAX_INPUT_BYTES)throw new BoardPartitionQualificationError(`computed partition requires incremental preparation (${builds.length} builds, ${facts.length} facts, ${bytes} bytes); current published view retained`);
    builds.push({id:row.id,at:row.at,record:JSON.parse(row.record)});facts.push(...row.facts);
   }
   if(page.length<256)break;
  }
  await tx.unsafe('CLOSE board_source_capture');
  builds.sort((a,b)=>a.id.localeCompare(b.id));
  facts.sort((a,b)=>a.identity.localeCompare(b.identity)||a.mode.localeCompare(b.mode));
  const reviews=[],ids=[...new Set(facts.map(row=>row.review_id).filter(Boolean))];
  for(let i=0;i<ids.length;i+=256){
   const rows=await tx.unsafe("SELECT to_jsonb(r)-'task_id' body FROM entity_measurement_reviews r WHERE id IN(SELECT jsonb_array_elements_text(($1::text)::jsonb)) ORDER BY id",[JSON.stringify(ids.slice(i,i+256))]);
   for(const row of rows){bytes+=Buffer.byteLength(JSON.stringify(row.body));if(bytes>MAX_INPUT_BYTES)throw new BoardPartitionQualificationError('partition review inputs exceed byte budget');reviews.push(row.body);}
  }
  return {weapon,ruler,captured_at,snapshot,bootstrap:!selected.revision,builds,facts,reviews,inputs_sha256:hash(JSON.stringify({builds,facts,reviews}))};
 });
}

async function nativeProject(binary,input,directory) {
 const facts=join(directory,'facts.ndjson'),board=join(directory,'board');
 await writeFile(facts,input.facts.map(row=>JSON.stringify(row)).join('\n')+'\n');
 const body=JSON.stringify(input.builds.map(row=>row.record));
 if(Buffer.byteLength(body)>MAX_INPUT_BYTES)throw new BoardPartitionQualificationError('native board input exceeds byte budget');
 await new Promise((resolve,reject)=>{
  const child=spawn(binary,['--project',board,'--facts-in',facts,'--state',join(directory,'state.yaml')],{stdio:['pipe','ignore','pipe']});
  let tail='',done=false;const timer=setTimeout(()=>{child.kill('SIGKILL');},60000);
  const finish=error=>{if(done)return;done=true;clearTimeout(timer);error?reject(error):resolve();};
  child.on('error',finish);child.stdin.on('error',finish);
  child.stderr.on('data',chunk=>{tail=(tail+chunk).slice(-4096);});
  child.once('close',(code,signal)=>finish(code===0?null:Error(`native partition failed ${code??signal}: ${tail}`)));
  child.stdin.end(body);
 });
 return board;
}

export async function openBoardResultFlow({sql,source,engine,binary,work=tmpdir(),nativeSha,requestedPartitions=false}) {
 if(typeof engine!=='string'||!engine)throw Error('board engine required');
 work=boardStorage(work);
 const mailbox=new BoardInputMailbox(sql,{source}),operator=await openInventoryOperator(binary);
 if(nativeSha&&operator.adapter!==nativeSha){await operator.close();throw Error('board native contract changed');}
 const nativeIdentity=await stat(binary),sameNative=async()=>{
  const current=await stat(binary);
  if(current.ino!==nativeIdentity.ino||current.size!==nativeIdentity.size||current.mtimeMs!==nativeIdentity.mtimeMs)
   throw Error('board native contract changed');
 };
 const rulers=new Map(operator.contract.rulers.map(row=>[row.id,row.scene]));
 await mkdir(work,{recursive:true});
 const route=async(tx,input)=>{
  const [old]=await tx.unsafe('SELECT weapon,ruler FROM state_board_read_routes WHERE scope=$1 AND key=$2',[input.scope,input.key]);
  let weapon,ruler='';
  if(input.body){
   ruler=input.scope==='build'?'':input.body.ruler;
   if(input.scope==='build')weapon=input.body.weapon??JSON.parse(input.body.record).weapon;
   else {
    const [build]=await tx.unsafe('SELECT weapon FROM entity_weapon_builds WHERE id=$1',[input.body.identity]);
    weapon=build?.weapon;
   }
  }else if(input.scope==='measurement'){
   const [identity,target]=JSON.parse(input.key);ruler=target;
   const [build]=await tx.unsafe('SELECT weapon FROM entity_weapon_builds WHERE id=$1',[identity]);weapon=build?.weapon;
  }
  const targets=new Map();
  for(const target of [old,weapon?{weapon,ruler}:null].filter(Boolean)){
   const parts=await tx.unsafe(`SELECT weapon,ruler FROM state_board_read_partitions WHERE enabled AND weapon=$1
     AND ($2::text='' OR ruler=$2::text)`,[target.weapon,target.ruler]);
   for(const part of parts)targets.set(JSON.stringify([part.weapon,part.ruler]),part);
  }
  if(input.deleted)await tx.unsafe('DELETE FROM state_board_read_routes WHERE scope=$1 AND key=$2',[input.scope,input.key]);
  else if(weapon)await tx.unsafe(`INSERT INTO state_board_read_routes VALUES($1,$2,$3,$4) ON CONFLICT(scope,key)
    DO UPDATE SET weapon=excluded.weapon,ruler=excluded.ruler
    WHERE (state_board_read_routes.weapon,state_board_read_routes.ruler) IS DISTINCT FROM (excluded.weapon,excluded.ruler)`,[input.scope,input.key,weapon,ruler]);
  if(input.scope==='build'&&weapon)await tx.unsafe(`INSERT INTO state_board_read_routes(scope,key,weapon,ruler)
    SELECT 'measurement',jsonb_build_array(identity,ruler,mode)::text,$2,ruler FROM board_facts WHERE identity=$1
    ON CONFLICT(scope,key) DO UPDATE SET weapon=excluded.weapon,ruler=excluded.ruler
    WHERE (state_board_read_routes.weapon,state_board_read_routes.ruler) IS DISTINCT FROM (excluded.weapon,excluded.ruler)`,[input.key,weapon]);
  for(const [key,part] of targets)await tx.unsafe('SELECT board_input_put($1::text,$2::text,$3::text)',
   ['partition',key,JSON.stringify({...part,input:{scope:input.scope,key:input.key,version:input.version}})]);
  return {partitions:targets.size};
 };
 const prepare=async input=>{
  const {weapon,ruler}=input.body,scene=rulers.get(ruler);
  if(scene===undefined)throw Error('unknown board partition ruler');
  await sameNative();
  const qualifying=requestedPartitions&&input.body.qualify_requested===true;
  const snapshot=await captureBoardPartition(sql,{weapon,ruler,qualify_requested:qualifying});
  for(let i=0;i<snapshot.builds.length;i+=256){
   const sources=snapshot.builds.slice(i,i+256);
   const described=await operator.call('describe',sources.map(row=>({id:row.id,record:row.record})));
   if(described.some(row=>row.candidate!==null&&row.candidate!==row.id))throw new BoardPartitionQualificationError('board source identities require qualification');
  }
  boardStorage(work);
  const directory=await mkdtemp(join(work,'board-result-'));
  try {
   const board=await nativeProject(binary,snapshot,directory);
   await sameNative();
   let rows=[],index={};
   try{rows=JSON.parse(await readFile(join(board,scene,weapon+'.json'),'utf8'));}
   catch(error){if(error.code!=='ENOENT')throw error;}
   try{index=JSON.parse(await readFile(join(board,scene,'index.json'),'utf8'));}
   catch(error){if(error.code!=='ENOENT')throw error;}
   rows=rows.filter(row=>row.benchmark===ruler);
   if(rows.some(row=>Buffer.byteLength(JSON.stringify(row))>65536)||Buffer.byteLength(JSON.stringify(rows))>MAX_BYTES)
    throw new BoardPartitionQualificationError('native partition output exceeds byte budget');
   const leaders=(index[weapon]??[]).filter(row=>row.benchmark===ruler);
   const revision=hash(JSON.stringify({source,engine,adapter:operator.adapter,inputs:snapshot.inputs_sha256,rows,leaders}));
   return {...snapshot,scene,rows,leaders,revision,qualifying};
  }finally{await rm(directory,{recursive:true,force:true});}
 };
 const publish=async(tx,input,prepared)=>{
  const {weapon,ruler,scene,revision,rows,leaders}=prepared;
  const part=prepared.qualifying
   ?(await tx.unsafe('SELECT board_requested_partition_selected($1,$2,true) part',[weapon,ruler]))[0].part
   :(await tx.unsafe('SELECT * FROM state_board_read_partitions WHERE weapon=$1 AND ruler=$2 AND enabled FOR UPDATE',[weapon,ruler]))[0];
  if(!part||part.scene!==scene)throw Error('board partition selection changed');
  await tx.unsafe(`INSERT INTO record_board_read_snapshots VALUES($1,$2,$3,$4,$5,$6,$7,($8::text)::jsonb,$9,$10)
    ON CONFLICT DO NOTHING`,[weapon,ruler,revision,source,engine,prepared.inputs_sha256,rows.length,JSON.stringify(leaders),prepared.captured_at,new Date().toISOString()]);
  const body=rows.map((row,i)=>({ordinal:i+1,body:JSON.stringify(row)}));
  await tx.unsafe(`INSERT INTO state_board_read_rows SELECT $1,$2,$3,x.ordinal,x.body
    FROM jsonb_to_recordset(($4::text)::jsonb)x(ordinal bigint,body text) ON CONFLICT DO NOTHING`,[weapon,ruler,revision,JSON.stringify(body)]);
  if(prepared.qualifying)await tx.unsafe('SELECT board_requested_partition_activate($1,$2,$3,$4::uuid)',[weapon,ruler,revision,source]);
  const routes=prepared.builds.map(row=>({scope:'build',key:row.id,weapon,ruler:''}));
  for(const fact of prepared.facts){
   routes.push({scope:'measurement',key:'['+[fact.identity,fact.ruler,fact.mode].map(value=>JSON.stringify(value)).join(', ')+']',weapon,ruler});
   if(fact.review_id)routes.push({scope:'review',key:fact.review_id,weapon,ruler});
  }
  await tx.unsafe(`INSERT INTO state_board_read_routes SELECT DISTINCT ON(scope,key) scope,key,weapon,ruler
    FROM jsonb_to_recordset(($1::text)::jsonb)x(scope text,key text,weapon text,ruler text)
    ON CONFLICT(scope,key) DO UPDATE SET weapon=excluded.weapon,ruler=excluded.ruler
    WHERE (state_board_read_routes.weapon,state_board_read_routes.ruler) IS DISTINCT FROM (excluded.weapon,excluded.ruler)`,[JSON.stringify(routes)]);
  await tx.unsafe('UPDATE state_board_read_partitions SET revision=$3 WHERE weapon=$1 AND ruler=$2',[weapon,ruler,revision]);
  await tx.unsafe(`DELETE FROM record_board_read_snapshots s WHERE weapon=$1 AND ruler=$2 AND revision<>$3
    AND revision NOT IN (SELECT revision FROM record_board_read_snapshots WHERE weapon=$1 AND ruler=$2
      ORDER BY published_at DESC,revision LIMIT 2)`,[weapon,ruler,revision]);
  return {weapon,ruler,revision,rows:rows.length,inputs_sha256:prepared.inputs_sha256};
 };
 return {close:()=>operator.close(),contract:operator.contract,async step(){
  if(requestedPartitions)await enqueueRequestedPartition(sql,source,operator.contract.rulers);
  const claim=await mailbox.claim({items:1,leaseSeconds:300});
  const item=claim.inputs[0];if(!item)return {status:'idle'};
  let qualifying=false;
  try{
   const input=await mailbox.read(item,claim.token);
   if(input.status==='superseded'){
    await sql.unsafe('SELECT board_input_ack($1::text,$2::text,$3::text,$4::bigint)',[item.scope,item.key,claim.token,item.version]);
    return {status:'superseded'};
   }
   if(input.status!=='input')throw Error('board input '+input.status);
   qualifying=requestedPartitions&&input.body?.qualify_requested===true;
   if(input.scope==='partition'){
    const prepared=await prepare(input);
    return {...await mailbox.apply(input,claim.token,(tx,packet)=>publish(tx,packet,prepared)),stage:'publication'};
   }
   return {...await mailbox.apply(input,claim.token,route),stage:'routing'};
  }catch(error){
   const delaySeconds=qualifying&&error instanceof BoardPartitionQualificationError?null:5;
   await mailbox.retry(item,claim.token,{error:error.message,delaySeconds});throw error;
  }
 }};
}

if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href){
 const value=name=>{const index=process.argv.indexOf(name);return index<0?null:process.argv[index+1];};
 const source=value('--source'),bin=value('--bin'),work=value('--work'),nativeSha=value('--native-sha');
 if(!process.env.PG_URL||!source||!bin||!work||!nativeSha)throw Error('PG_URL and --source --bin --work --native-sha are required');
 const engine=(await readFile(join(bin,'ENGINE'),'utf8')).trim();
 if(!/^[a-f0-9]{12}$/.test(engine)||!/^[a-f0-9]{64}$/.test(nativeSha))throw Error('invalid board execution contract');
 const {default:postgres}=await import('./pg/node_modules/postgres/src/index.js');
 const sql=postgres(process.env.PG_URL,{max:3,fetch_types:false,connection:{application_name:'wfsim-board-read',statement_timeout:10000,lock_timeout:1000,temp_file_limit:'16MB'}});
 let running=true,flow;
 process.once('SIGTERM',()=>{running=false;});process.once('SIGINT',()=>{running=false;});
 try{
  flow=await openBoardResultFlow({sql,source,engine,binary:join(bin,'wfsim-board'),work,nativeSha,
   requestedPartitions:process.env.WFSIM_BOARD_READ_REQUESTED_PARTITIONS==='1'});
  while(running){
   try{const step=await flow.step();if(step.status==='idle')await new Promise(resolve=>setTimeout(resolve,1000));
    else console.log(JSON.stringify({event:'board_read',at:new Date().toISOString(),...step}));
   }catch(error){console.error(JSON.stringify({event:'board_read',at:new Date().toISOString(),error:error.message}));
    if(/source changed|native contract changed/.test(error.message))throw error;
    await new Promise(resolve=>setTimeout(resolve,1000));
   }
  }
 }finally{await flow?.close();await sql.end();}
}
