// SPDX-License-Identifier: AGPL-3.0-or-later
// The actual production command must start isolated consumers and release their locks on termination.
import assert from 'node:assert/strict';
import {spawn,execFileSync} from 'node:child_process';
import {DatabaseSync} from 'node:sqlite';
import {mkdtempSync,mkdirSync,readFileSync,rmSync,realpathSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve,dirname} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';

const source=resolve(process.env.WFSIM_BOARD_ENTRY_SOURCE||fileURLToPath(new URL('..',import.meta.url)));
const {BoardPipeline}=await import(pathToFileURL(join(source,'scripts/board_pipeline.mjs')));
const {InventoryControl,inspectInventoryTask}=await import(pathToFileURL(join(source,'scripts/inventory_control.mjs')));
const shell=readFileSync(join(source,'scripts/live_board.sh'),'utf8');
const command=shell.match(/^  node --max-old-space-size=192 .*$/m)?.[0].trim();
assert(command,'the production incremental board command must exist');
const root=realpathSync(mkdtempSync(join(tmpdir(),'board-cli-entry-'))),bin=join(root,'bin');mkdirSync(bin);
const pause=['normalization','inventory','projection','publication'];
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const childrenOf=pid=>execFileSync('ps',['-axo','pid=,ppid='],{encoding:'utf8'}).trim().split('\n')
 .map(line=>line.trim().split(/\s+/).map(Number)).filter(([,parent])=>parent===pid).map(([child])=>child);
const alive=pid=>{try{process.kill(pid,0);return true;}catch(error){if(error.code==='ESRCH')return false;throw error;}};
let child,pids=[];
async function start(name,once) {
 const work=join(root,name);mkdirSync(work);
 const pipeline=new BoardPipeline(work);for(const stage of pause)pipeline.control(stage,true);pipeline.close();
 const control=new InventoryControl(join(work,'inventory'));
 const task=control.request({mode:'board',engine:'a'.repeat(12),contract:{v:1,covers:['board']},
  reason:'preserve an explicit startup request',maxNewBytes:1024**3});control.close();
 const path=join(work,'inventory/controller.sqlite'),bytes=readFileSync(path);
 const env={PATH:process.env.PATH,HERE:join(source,'scripts'),WORK:work,BIN:bin,
  PG_URL:'postgres://unused:unused@127.0.0.1:1/unused',...(once?{ONCE:'1'}:{})};
 const executable=command.replace(/^node /,'"$REAL_NODE" ');env.REAL_NODE=process.execPath;
 child=spawn(process.env.WFSIM_BASH||'bash',['-c','exec '+executable],{env,detached:true,stdio:['ignore','pipe','pipe']});
 let stdout='',stderr='',exit;
 child.stdout.on('data',body=>{stdout+=body;});child.stderr.on('data',body=>{stderr+=body;});
 const ended=new Promise((resolve,reject)=>{child.once('error',reject);child.once('exit',(code,signal)=>{exit={code,signal};resolve(exit);});});
 const wait=async(predicate,message)=>{
  const until=Date.now()+12000;
  while(Date.now()<until){if(predicate())return;if(exit)throw Error(message+': '+JSON.stringify(exit)+'\n'+stderr);await delay(50);}
  throw Error(message+'\n'+stderr);
 };
 const progress=()=>{
  const p=new BoardPipeline(work);
  try{return p.status().stages.filter(x=>x.progress).map(x=>({name:x.name,body:JSON.parse(x.progress.body)}));}
  finally{p.close();}
 };
 if(once) {
  await wait(()=>exit,'ONCE CLI did not finish');assert.equal(exit.code,0,stderr);child=null;
 }else{
  await wait(()=>{pids=childrenOf(child.pid);return pids.length===4&&progress().length===3;},'supervisor did not start four isolated children');
 }
 const rows=progress();assert.deepEqual(rows.map(x=>x.name).sort(),['inventory','normalization','projection']);
 assert(rows.every(x=>x.body.paused===true),'startup bypassed a retained stage pause');
 assert.deepEqual(readFileSync(path),bytes,'startup changed the immutable controller request');
 assert.deepEqual(inspectInventoryTask(dirname(path)),task);
 for(const stage of ['normalization','inventory','projection']) {
  const db=new DatabaseSync(join(work,stage+'-consumer-lock.sqlite'));db.exec('PRAGMA busy_timeout=0');
  try {
   if(once){db.exec('BEGIN IMMEDIATE;ROLLBACK');}
   else assert.throws(()=>db.exec('BEGIN IMMEDIATE'),/locked/,'an isolated consumer did not own its writer lock');
  }finally{db.close();}
 }
 if(!once) {
  child.kill('SIGTERM');await wait(()=>exit,'supervisor did not drain after SIGTERM');
  assert.equal((await ended).code,0,stderr);child=null;
  await wait(()=>pids.every(pid=>!alive(pid)),'a consumer survived supervisor termination');
  for(const stage of ['normalization','inventory','projection']){
   const db=new DatabaseSync(join(work,stage+'-consumer-lock.sqlite'));
   try{db.exec('PRAGMA busy_timeout=0;BEGIN IMMEDIATE;ROLLBACK');}finally{db.close();}
  }
 }
 assert(!stderr.includes('unsettled top-level await'),stderr);
 assert.equal(stdout.includes('published'),false,'paused startup must not publish');
 console.log('ok: actual production '+(once?'ONCE command completes':'supervisor starts four children and drains their locks')+' with immutable request and pauses retained');
}
try{await start('once',true);await start('continuous',false);}
finally{
 if(child){try{process.kill(-child.pid,'SIGKILL');}catch(error){if(error.code!=='ESRCH')throw error;}await delay(100);}
 for(const pid of pids)if(alive(pid))try{process.kill(pid,'SIGKILL');}catch(error){if(error.code!=='ESRCH')throw error;}
 rmSync(root,{recursive:true,force:true});
}
