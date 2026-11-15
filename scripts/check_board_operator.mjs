// SPDX-License-Identifier: AGPL-3.0-or-later
// Real stopped and killed native processes cannot leave a board stage waiting on stdin forever.
import assert from 'node:assert/strict';
import childProcess from 'node:child_process';
import {syncBuiltinESMExports} from 'node:module';
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';

const pipeline=readFileSync(new URL('./split_board_pipeline.mjs',import.meta.url),'utf8');
assert.match(pipeline,/import \{openBoardOperator as openInventoryOperator\} from '\.\/board_operator\.mjs'/);
assert(!pipeline.includes("from './inventory_operator.mjs'"),'every production stage opens the bounded operator');
const original=childProcess.spawn,children=new Map(),writes=new Map();
childProcess.spawn=(...args)=>{
 const child=original(...args);
 if(args[1]?.includes('--inventory-operator')) {
  children.set(child.pid,child);const write=child.stdin.write.bind(child.stdin);
  child.stdin.write=(...values)=>{const result=write(...values);writes.set(child.pid,{bytes:Buffer.byteLength(values[0]),returned:result});return result;};
 }
 return child;
};
syncBuiltinESMExports();
const [{openInventoryOperator},{openBoardOperator}]=await Promise.all([import('./inventory_operator.mjs'),import('./board_operator.mjs')]);
const open=process.argv[2]==='unguarded'?openInventoryOperator:openBoardOperator;
const binary=resolve(process.env.WFSIM_INVENTORY_BOARD_BIN||'target/debug/wfsim-board');
const record=JSON.parse(readFileSync(new URL('../tests/gas/rows.json',import.meta.url)))[0].record;
const items=Array.from({length:256},(_,id)=>({id:String(id),record:{...record,qualification_padding:'x'.repeat(24000)}}));
let native;
try {
 const plain=await openInventoryOperator(binary);
 let expected;
 try{expected=await plain.call('describe',[{id:'baseline',record}]);}finally{await plain.close();}
 for(const killed of [false,true]) {
  native=await open(binary,{timeoutMs:5000});
  assert.deepEqual(await native.call('describe',[{id:'baseline',record}]),expected,'successful values changed');
  assert.equal((await native.call('describe',[items[0]]))[0].candidate,expected[0].candidate,'padding changes the candidate');
  process.kill(native.pid,'SIGSTOP');
  let timer,killTimer;
  const started=Date.now(),request=native.call('describe',items).then(value=>({value}),error=>({error}));
  assert.equal(writes.get(native.pid).returned,false,'the real pipe did not apply backpressure');
  if(killed)killTimer=setTimeout(()=>{try{process.kill(native.pid,'SIGKILL');}catch{}},50);
  let result;
  try{result=await Promise.race([request,new Promise(resolve=>{timer=setTimeout(()=>resolve({expired:true}),12000);})]);}
  finally{clearTimeout(timer);clearTimeout(killTimer);}
  assert(!result.expired,'the request exceeded its deadline while waiting on the real native pipe');
  assert(result.error,'a stopped or killed native operator answered successfully');
  if(!killed)assert.equal(result.error.code,'INVENTORY_OPERATOR_DEADLINE');
  assert.equal(native.healthy,false,'a failed stage retained a reusable operator');
  const child=children.get(native.pid);
  assert(child.exitCode!==null||child.signalCode!==null,'native process survived failed-call cleanup');
  console.log(JSON.stringify({killed,bytes:writes.get(native.pid).bytes,elapsed_ms:Date.now()-started,closed:true,code:result.error.code||''}));
  await native.close();native=null;
 }
 console.log('ok: production stages bound the complete native request; stopped pipe and SIGKILL settle, close and retain successful values');
}finally {
 if(native){try{process.kill(native.pid,'SIGCONT');}catch{}await native.close();}
 childProcess.spawn=original;syncBuiltinESMExports();
}
