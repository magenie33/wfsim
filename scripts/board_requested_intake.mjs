// SPDX-License-Identifier: AGPL-3.0-or-later
// Native normalization supplies the sole requested calculation; admission keeps its durable handoff.
import {execFile} from 'node:child_process';
import {readFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {pathToFileURL} from 'node:url';
import {prepareIntakeBuild} from './build_batches.mjs';
import {boardStorage} from './board_storage.mjs';

class RefusedBoardRequest extends Error {}

export function requestedPlan(build,request) {
 const plan=build.flow,weapon=build.record.weapon;
 if(request.kind==='correction'&&build.observation?.valid!==true)throw new RefusedBoardRequest('observation does not match the official calculation');
 if(request.kind==='missing'&&plan&&typeof plan.public_identity!=='string')throw Error('native intake must supply a public measurement identity');
 const identity=request.kind==='missing'?plan?.public_identity:build.id;
 const nodes=plan?.nodes?.filter(node=>node.kind==='measure'&&node.identity===identity&&node.ruler===request.ruler&&node.mode===request.mode);
 const boards=plan?.scope?.boards?.filter(board=>board.id===request.ruler);
 const builds=plan?.builds?.filter(row=>row.id===identity||row.id===build.id);
 const record=builds?.find(row=>row.id===identity)?.record;
 if(!record||nodes?.length!==1||boards?.length!==1)throw new RefusedBoardRequest('requested calculation is not admitted by the native board contract');
 const node=nodes[0];
 const {family_flow,...root}=build;
 const family=request.kind==='missing'&&family_flow?{family_flow}:{};
 if(family.family_flow&&build.id!==identity)family.family_root={id:build.id,record:build.record};
 return {...root,...family,id:identity,record,derived:false,flow:{schema:plan.schema,nodes:[{...node,template:0,patch:0}],
   templates:[plan.templates[node.template]],patches:[plan.patches[node.patch]],
   builds,scope:{boards,modes:{[weapon]:[request.mode]}}}};
}

async function normalize(binary,input) {
 const output=await new Promise((resolve,reject)=>{
  const child=execFile(binary,['--deadline','30'],{timeout:35000,maxBuffer:8*1024*1024,encoding:'utf8'},(error,stdout,stderr)=>
   error?reject(Error('native intake failed: '+stderr.slice(-2000))):resolve(stdout));
  child.stdin.on('error',reject);child.stdin.end(JSON.stringify(input)+'\n');
 });
 const rows=output.trim().split('\n').filter(Boolean).map(JSON.parse);
 return rows;
}

export async function normalizeBoardRequest(binary,request) {
 const input={...request.input,record:typeof request.input.record==='string'?JSON.parse(request.input.record):request.input.record};
 input.record={...input.record,benchmark:request.ruler,mode:request.mode};
 const rows=await normalize(binary,input);
 if(rows.length!==1)throw new RefusedBoardRequest('native intake refused the requested build');
 return prepareIntakeBuild(requestedPlan(rows[0],request));
}

export async function familyBoardPass(sql,{binary,max=16,clock=()=>new Date().toISOString()}){
 const [{input}]=await sql.unsafe('SELECT build_family_claim($1::text) input',[clock()]);
 if(input){
  const {lease,...raw}=input;
  try{
   const rows=await normalize(binary,raw);
   if(rows.length>1)throw Error('family intake returned multiple builds for one result');
   const build=rows.length?JSON.stringify(await prepareIntakeBuild(rows[0])):null;
   await sql.unsafe('SELECT build_family_stage($1,$2::uuid,$3::text,$4::text)',[raw.id,lease,build,clock()]);
  }catch(error){
   await sql.unsafe('SELECT build_family_retry($1,$2::uuid,$3,$4)',[raw.id,lease,error.message,clock()]);
   throw error;
  }
 }
 const [{result}]=await sql.unsafe('SELECT build_family_admit($1::text,$2::bigint) result',[clock(),max]);
 if(result?.error)throw Error('family admission: '+result.error);
 return input||result?{input:input?.id,admission:result}:{idle:true};
}

export async function requestedBoardPass(sql,{source,binary}) {
 const [{request}]=await sql.unsafe('SELECT board_request_claim($1::uuid) request',[source]);
 if(!request)return {idle:true};
 try{
  if(request.phase==='pending'){
   let input;
   try{input=await normalizeBoardRequest(binary,request);}
   catch(error){
    if(!(error instanceof RefusedBoardRequest))throw error;
    await sql.unsafe('SELECT board_request_retry($1,$2::uuid,$3,true)',[request.id,request.lease,error.message]);
    return {id:request.id,rejected:true,error:error.message};
   }
   await sql.unsafe('SELECT board_request_stage($1,$2::uuid,$3::text)',[request.id,request.lease,JSON.stringify(input)]);
   return {id:request.id,staged:true};
  }
  const [{result}]=await sql.unsafe('SELECT board_request_admit($1,$2::uuid) result',[request.id,request.lease]);
  return {id:request.id,...result};
 }catch(error){
  await sql.unsafe('SELECT board_request_retry($1,$2::uuid,$3,false)',[request.id,request.lease,error.message]);
  throw error;
 }
}

if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url){
 const {default:postgres}=await import('./pg/node_modules/postgres/src/index.js');
 const binary=process.env.WFSIM_REQUESTED_INTAKE_BIN,source=process.env.WFSIM_REQUESTED_SOURCE;
 boardStorage(process.env.WFSIM_BOARD_WORK_ROOT||'.');
 if(!binary||!source||!/^[0-9a-f]{64}$/.test(process.env.WFSIM_REQUESTED_INTAKE_SHA256||''))throw Error('pin requested source and native intake binary');
 const bytes=await readFile(binary),sha=createHash('sha256').update(bytes).digest('hex');
 if(sha!==process.env.WFSIM_REQUESTED_INTAKE_SHA256)throw Error('requested native intake changed');
 const sql=postgres(process.env.WFSIM_REQUESTED_DATABASE_URL,{max:2,fetch_types:false,connection:{application_name:'wfsim-requested-board',statement_timeout:10000,lock_timeout:1000}});
 let stopping=false;process.once('SIGTERM',()=>{stopping=true;});process.once('SIGINT',()=>{stopping=true;});
 try{
  const [{family}]=await sql.unsafe("SELECT to_regprocedure('build_family_admit(text,bigint)') IS NOT NULL family");
  let familyAt=0;
  while(!stopping){
   let delay=0;
   try{
    const result=await requestedBoardPass(sql,{source,binary});
    console.log(JSON.stringify(result));
    if(result.idle)delay=1000;
   }catch(error){console.error(error.message);delay=5000;}
   if(family&&Date.now()>=familyAt){
    try{
     const work=await familyBoardPass(sql,{binary});
     if(!work.idle)console.log(JSON.stringify({family:work}));
    }catch(error){console.error(error.message);}
    familyAt=Date.now()+5000;
   }
   if(delay)await new Promise(resolve=>setTimeout(resolve,delay));
  }
 }finally{await sql.end();}
}
