// SPDX-License-Identifier: AGPL-3.0-or-later
// Drive the shipping loader through a large immutable partition and actual cancellation deadlines.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
const source=readFileSync(process.env.WFSIM_BOARD_READER_SOURCE||new URL('../web/src/static/app/40-official-builds.js',import.meta.url),'utf8');
const block=(from,until)=>{const start=source.indexOf(from),end=source.indexOf(until,start+from.length);assert(start>=0&&end>start);return source.slice(start,end);};
const loader=[block('async function readBoardPartitionCatalog(','/// `res.ok`'),
 block('const BOARD_HAVE','/// EVERY WEAPON\'S GROUP LEADERS'),
 block('async function loadWeaponBoard','function ensureBoardCell')].join('\n');
const weapon='furis',ruler='standard_single_target',revision='a'.repeat(64),step=7,total=step*64-3;
const stale={benchmark:ruler,score:-1},untouched={benchmark:'retained_other_ruler',score:123};

async function run(options={}) {
 const previous=[stale,untouched],rows=Array.from({length:options.empty?0:total},(_,score)=>({benchmark:ruler,score}));
 const state={active:0,maximum:0,pages:0,aborted:0};let timer;
 const wait=(duration,signal)=>new Promise((resolve,reject)=>{
  const cancel=()=>{clearTimeout(timeout);state.aborted++;reject(signal.reason||Error('aborted'));};
  const timeout=setTimeout(()=>{signal?.removeEventListener('abort',cancel);resolve();},duration);
  if(signal?.aborted)cancel();else signal?.addEventListener('abort',cancel,{once:true});
 });
 const context=vm.createContext({URLSearchParams,AbortController,AbortSignal,
  window:{},LIVE_ORIGIN:'https://wfsim.invalid',BOARD_ORIGIN:'https://board.invalid',boardOffOrigin:false,
  BOARD:{},BOARD_META:null,location:{pathname:'/weapons/Furis/build-library',search:'?bench='+ruler},
  finder:{weapon,b:ruler},raRuler:null,benchList:()=>[{id:ruler,cell_dir:''}],benchSceneOf:()=>'',
  libraryDefaultBenchmark:()=>ruler,modSuffix:()=> 'build-library',localStorage:{getItem:()=>null},
  boardActiveKey:()=> 'active',openedBoardKey:()=> 'opened',
  timeoutSignal:within=>AbortSignal.timeout(within===60000?(options.overall||3000):200),
  async fetchJsonPatient(path,tries,signal){
   const url=new URL(path,'https://wfsim.invalid');
   if(url.pathname==='/board/'+weapon+'.json')return previous;
   if(url.pathname==='/api/board/pages/catalog')return {ok:true,partitions:options.absent?[]:(options.extra?[ruler,ruler+'_extra']:[ruler]).map(ruler=>({weapon,ruler,revision,rows:rows.length}))};
   assert.equal(url.pathname,'/api/board/pages');assert.equal(url.searchParams.get('revision'),revision);
   const after=Number(url.searchParams.get('after'));assert(Number.isSafeInteger(after)&&after>=0&&after%step===0);
   state.pages++;state.active++;state.maximum=Math.max(state.maximum,state.active);
   try{
    await wait(options.hang&&after===step?500:options.delay||20+(after/step%3)*2,signal);
    const values=rows.slice(after,after+step).map(row=>({...row,benchmark:url.searchParams.get('ruler')})),end=after+values.length;
    const page={ok:true,revision,after,count:rows.length,rows:values,next:end<rows.length?end:null};
    if(after===step*3){
     if(options.wrongRevision)page.revision='b'.repeat(64);
     if(options.wrongCount)page.count++;
     if(options.gap)page.next++;
     if(options.short)page.rows=page.rows.slice(1);
    }
    return page;
   }finally{state.active--;}
  },
 });
 vm.runInContext(loader,context);
 if(options.cancel)timer=setTimeout(()=>vm.runInContext("boardFileState('furis').controller.abort()",context),35);
 const started=performance.now();
 try{
  await vm.runInContext("loadWeaponBoard('furis')",context);
  const result=JSON.parse(vm.runInContext('JSON.stringify(BOARD.furis)',context));
  return {result,state,elapsed:performance.now()-started,previous};
 }finally{clearTimeout(timer);}
}

const full=await run();
assert.equal(full.result.length,total+1,'all 64 pages must replace stale rows despite exceeding one page deadline: '+JSON.stringify({state:full.state,elapsed:full.elapsed}));
assert.equal(full.result[0].benchmark,untouched.benchmark);
assert.deepEqual(full.result.slice(1).map(row=>row.score),Array.from({length:total},(_,i)=>i),'out-of-order pages retain their immutable order');
assert(full.elapsed>200,'the complete partition must outlast one page deadline');
assert(full.state.maximum>1&&full.state.maximum<=4,'the page reader must use bounded parallel requests');
assert.equal(full.state.pages,64);assert.equal(full.state.active,0);
console.log('  ok  64 pages beyond one deadline, server-derived stride and complete ordered replacement');
const shared=await run({extra:true});assert.equal(shared.result.length,total*2+1);assert.equal(shared.state.pages,128);
assert(shared.state.maximum<=4,'all partitions share the same request bound');
console.log('  ok  multiple partitions share the request concurrency bound');

for(const [name,options] of [['revision changed',{wrongRevision:true}],['count changed',{wrongCount:true}],
 ['cursor gap',{gap:true}],['short page',{short:true}],['page timeout',{hang:true}],
 ['overall budget',{overall:110,delay:20}],['weapon cancellation',{cancel:true}]]){
 const value=await run(options);assert.deepEqual(value.result,value.previous,name+' retains the complete previous view');
 assert(value.state.maximum<=4,name+' retains the concurrency bound');
 await new Promise(resolve=>setTimeout(resolve,1));assert.equal(value.state.active,0,name+' cancels outstanding pages');
 console.log('  ok  '+name+' retains the previous complete view');
}
const absent=await run({absent:true});assert.deepEqual(absent.result,absent.previous);assert.equal(absent.state.pages,0);
const empty=await run({empty:true});assert.deepEqual(empty.result,[untouched]);assert.equal(empty.state.pages,1);
console.log('  ok  absent and complete-empty partitions remain distinct');
