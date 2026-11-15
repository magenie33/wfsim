// SPDX-License-Identifier: AGPL-3.0-or-later
// The real intake loop drains only fully acknowledged pages and backs off on idle or refusal.
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,readFileSync,writeFileSync,existsSync,rmSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';

const here=fileURLToPath(new URL('.',import.meta.url));
const source=readFileSync(process.env.WFSIM_LIVE_LOOP_SOURCE||join(here,'live_board.sh'),'utf8');
const functions=['intake_arrivals','intake_loop'].map(name=>{
 const body=source.match(new RegExp('^'+name+'\\(\\) \\{[\\s\\S]*?^\\}','m'));
 assert(body,'missing production '+name);return body[0];
}).join('\n');
const root=mkdtempSync(join(tmpdir(),'live-intake-loop-')),shim=join(root,'node.mjs');
writeFileSync(shim,String.raw`
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {join,basename} from 'node:path';
import {pathToFileURL} from 'node:url';
import {spawnSync} from 'node:child_process';
const args=process.argv.slice(2);while(args[0]?.startsWith('--'))args.shift();
const script=args.shift(),name=basename(script),path=join(process.env.WORK,'fixture-state.json');
const state=JSON.parse(readFileSync(path));
const save=()=>writeFileSync(path,JSON.stringify(state));
const value=flag=>args[args.indexOf(flag)+1];
if(name==='inbox_reader.mjs'&&args[0]==='--out') {
 state.captures++;save();assert.equal(value('--max-rows'),'100');assert.equal(value('--max-seconds'),'15');
 const {captureInbox,readInboxCursor}=await import(pathToFileURL(join(process.env.HERE,'inbox_reader.mjs')));
 let pages=0;const output=value('--out');writeFileSync(output,'');
 const receipt=await captureInbox({after:readInboxCursor(value('--cursor')),maxRows:100,maxSeconds:15,
  source:{priority:async()=>[],byIds:async()=>[],after:async(after,max)=>{
   if(state.mode==='read-fail'||state.mode==='partial-read'&&pages++)throw Error('fixture database backpressure');
   return state.rows.filter(row=>!state.done.includes(row.id)&&row.id>after).slice(0,max);
  }},emit:line=>writeFileSync(output,line,{flag:'a'})});
 writeFileSync(value('--receipt'),JSON.stringify(receipt));
}else if(name==='live_intake.mjs') {
 state.intakes++;save();
 if(state.mode==='intake-fail')process.exit(23);
 if(state.mode==='paused')process.exit(0);
 const file=args[3],rows=readFileSync(file,'utf8').trim().split('\n').filter(Boolean).map(JSON.parse);
 const done=state.mode==='partial'?rows.slice(0,50):rows;
 state.done.push(...done.map(row=>row.id));save();
 writeFileSync(file+'.done',done.map(row=>row.id).join('\n')+'\n');
}else {
 const result=spawnSync(process.execPath,[script,...args],{stdio:'inherit'});
 process.exit(result.status??1);
}
`);
const script=`set -euo pipefail
${functions}
NODE_CALLS=0
node() {
 NODE_CALLS=$((NODE_CALLS + 1))
 if [ "$NODE_CALLS" -gt 60 ]; then exit 77; fi
 "$REAL_NODE" "$FIXTURE_SHIM" "$@"
}
sleep() {
 echo "$*" >> "$WORK/fixture-sleeps"
 exit 0
}
cd "$WORK"
intake_loop
`;
try {
 for(const mode of ['drain','idle','read-fail','intake-fail','partial','paused','partial-read','storage-fail']) {
  const work=join(root,mode);mkdirSync(work);
  const count=mode==='idle'?0:mode==='partial-read'?50:200;
  const rows=Array.from({length:count},(_,index)=>({id:'s'+String(index).padStart(4,'0'),at:'2026-01-01',record:'{}'}));
  writeFileSync(join(work,'fixture-state.json'),JSON.stringify({mode,rows,done:[],captures:0,intakes:0}));
  if(mode==='paused')writeFileSync(join(work,'inbox-new.ndjson.done'),rows.slice(0,100).map(row=>row.id).join('\n')+'\n');
  const env={...process.env,WORK:work,HERE:here,BIN:root,POLL_SECONDS:'20',INCREMENTAL_INVENTORY:'1',
   REAL_NODE:process.execPath,FIXTURE_SHIM:shim};
  for(const key of ['PG_URL','WFSIM_BOARD_WORK_ROOT','WFSIM_BOARD_DATA_MOUNT','WFSIM_BOARD_DATA_UUID','WFSIM_BOARD_STORAGE_REQUIRED'])delete env[key];
  if(mode==='storage-fail')Object.assign(env,{WFSIM_BOARD_STORAGE_REQUIRED:'1',WFSIM_BOARD_WORK_ROOT:work,
   WFSIM_BOARD_DATA_MOUNT:join(root,'missing'),WFSIM_BOARD_DATA_UUID:'12345678-1234-1234-1234-123456789abc'});
  const result=spawnSync(process.env.WFSIM_BASH||'bash',['-c',script],{env,encoding:'utf8',timeout:20000});
  assert.equal(result.status,0,mode+': '+result.stderr);
  const state=JSON.parse(readFileSync(join(work,'fixture-state.json')));
  assert.equal(readFileSync(join(work,'fixture-sleeps'),'utf8'),'20\n',mode+' did not back off exactly once');
  assert.equal(state.captures,mode==='drain'?3:mode==='storage-fail'?0:1,mode+' capture count');
  assert.equal(state.intakes,mode==='drain'?2:['idle','read-fail','storage-fail'].includes(mode)?0:1,mode+' intake count');
  assert.equal(state.done.length,mode==='drain'?200:['partial','partial-read'].includes(mode)?50:0,mode+' durable acknowledgements');
  const cursor=join(work,'inbox-cursor.json');
  if(['drain','idle'].includes(mode))assert.equal(JSON.parse(readFileSync(cursor)).after,'');
  else if(mode==='partial-read')assert.equal(JSON.parse(readFileSync(cursor)).after,'s0049');
  else assert.equal(existsSync(cursor),false,mode+' advanced an unacknowledged scan');
  console.log('ok: actual intake loop '+mode);
 }
 console.log('ok: confirmed bounded pages drain without polling delay; idle, errors, pause, partial ACK and storage refusal back off');
}finally{rmSync(root,{recursive:true,force:true});}
