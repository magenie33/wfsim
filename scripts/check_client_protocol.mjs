// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:http';

const args=Object.fromEntries(process.argv.slice(2).map(a=>{const at=a.indexOf('=');return[a.slice(0,at),a.slice(at+1)];}));
assert(args.cache,'give cache=<verified CLI engines directory>; all dispatch traffic remains on loopback');
const binary=path.resolve(args.binary||`client/target/debug/wfsim${process.platform==='win32'?'.exe':''}`);
const release=JSON.parse(readFileSync(args.release||'site/release.json','utf8'));
const row=JSON.parse(readFileSync('tests/gas/rows.json','utf8')).find(r=>r.record.weapon==='braton_prime');
const dir=mkdtempSync(path.join(os.tmpdir(),'wfsim-client-protocol-'));
cpSync(args.cache,path.join(dir,'engines'),{recursive:true});
assert(existsSync(path.join(dir,`engines/manifest.${release.engine}.json`)),'cache must contain the signed engine manifest');
let child, phase=0, handed=false, invalidate=false, renewals=0, releases=0, refusals=0, log='';
const verifiers=new Set();
const until=async(fn,ms=30_000)=>{const end=Date.now()+ms;while(Date.now()<end){if(fn())return;if(child?.exitCode!==null&&child?.exitCode!==undefined)throw Error(`client exited ${child.exitCode}: ${log.slice(-2000)}`);await new Promise(r=>setTimeout(r,100));}throw new Error(`condition timed out: ${log.slice(-2000)}`);};
const status=()=>{try{return JSON.parse(readFileSync(path.join(dir,'status.json'),'utf8'));}catch{return{};}};
const run=(...a)=>spawnSync(binary,['--data-dir',dir,...a],{encoding:'utf8',timeout:10_000});
const server=createServer(async(req,res)=>{
  let reply;
  if(req.url==='/release.json')reply=release;
  else if(req.url==='/work.json')reply={v:1,at:new Date().toISOString(),open:3,urgent:0,engine:release.engine,release:release.release,
    mode:'signal',poll_ms:1000,jitter_ms:0,urgent_jitter_ms:0};
  else if(req.url==='/api/board/sync'){
    let raw='';for await(const c of req)raw+=c;const b=JSON.parse(raw);
    assert.equal(b.engine,release.engine);assert.equal(b.protocol,8);
    verifiers.add(b.verifier);
    refusals+=(b.refused||[]).length;
    const held=(b.held||[]).flatMap(h=>h.rows);
    reply={ok:true,kept:true,taken:b.results.map(r=>r.question),dropped:[],held:held.map(question=>({question,held:true})),pack:null,work:null,
      next_ms:1000,dials:{sync_lease_ms:1_500_000,report_ms:0,pack8_ms:600_000,poll_ms:1000}};
    if(b.want_ms>0&&invalidate){
      invalidate=false;reply.stale=true;
    }else if(b.want_ms>0&&!handed){
      handed=true;reply.pack={lease:'a'.repeat(32),lease_ms:4000,tasks:[0,1,2].map(i=>({...row,question:`fixture-${phase}-${i}`}))};
    }else if(handed&&held.length===3){renewals++;}
    else if(handed&&held.length===0&&b.want_ms===0){releases++;}
  }else{res.writeHead(500).end('unexpected endpoint');throw Error(`unexpected ${req.url}`);}
  res.writeHead(200,{'content-type':'application/json'}).end(JSON.stringify(reply));
});
try{
  await new Promise(r=>server.listen(0,'127.0.0.1',r));
  mkdirSync(dir,{recursive:true});writeFileSync(path.join(dir,'config.json'),JSON.stringify({server:`http://127.0.0.1:${server.address().port}`,
    range:{min:1,max:1},name:'Protocol fixture',paused:false,consent:{v:1,at:new Date().toISOString()}}));
  child=spawn(binary,['--data-dir',dir,'join'],{stdio:['ignore','pipe','pipe']});
  child.stdout.on('data',b=>{log+=b;});child.stderr.on('data',b=>{log+=b;});
  await until(()=>renewals>0);
  const first=status(), identity=readFileSync(path.join(dir,'identity'),'utf8');
  console.log('  ok short initial leases renew with all queued and active rows named');
  assert.equal(run('config','--range','0-0').status,0);
  await until(()=>releases>0&&status().target_percent===0);
  console.log('  ok changing the upper bound to zero returns every unfinished row');
  assert.equal(refusals,0);console.log('  ok cancelling honest computation does not report an engine refusal');
  phase=1;handed=false;invalidate=true;const before=renewals;
  assert.equal(run('config','--range','1-1').status,0);
  await until(()=>renewals>before);
  console.log('  ok resuming acquires work again under the same device identity');
  assert.notEqual(status().pid,first.pid);assert.equal(status().run,first.run);
  assert.equal(readFileSync(path.join(dir,'identity'),'utf8'),identity);assert.deepEqual([...verifiers],[identity]);
  assert(log.includes('Engine update ready; restarting computation.'));
  console.log('  ok engine invalidation replaces the native process while preserving device identity and stop token');
  const given=releases;assert.equal(run('stop').status,0);
  await until(()=>child.exitCode!==null);
  assert.equal(child.exitCode,0);assert(releases>given);assert.equal(refusals,0);
  console.log('  ok shutdown returns the resumed lease without fabricating a result');
  child=null;console.log('CLI protocol: 6 checks passed using the verified native engine and synthetic local leases.');
}finally{
  if(child){run('stop');await until(()=>child.exitCode!==null,5000).catch(()=>child.kill('SIGTERM'));}
  await new Promise(r=>server.close(r));rmSync(dir,{recursive:true,force:true});
}
