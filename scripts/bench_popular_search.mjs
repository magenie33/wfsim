// SPDX-License-Identifier: AGPL-3.0-or-later
// Run after popular_search_cases.mjs; both wasm directories contain nodejs bindgen glue.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { loadEngine } from './wasm_engine.mjs';

const [dir, before, after, mode = 'fight', input, output] = process.argv.slice(2);
if (!after || !['fight','fleet','opening','whole','whole-child'].includes(mode)) throw Error('dir before-wasm after-wasm fight|fleet|opening|whole [cases.json] [output.json]');
const engine = p => createRequire(import.meta.url)(path.resolve(p,'wfsim_wasm.js'));
const hash = v=>createHash('sha256').update(v).digest('hex');
const data = input || path.join(dir,({fight:'fights',fleet:'searches',opening:'starts',whole:'unrestricted'})[mode]+'.json');
const cases = JSON.parse(fs.readFileSync(data,'utf8')), out = [];
const cost = (e,p,s) => {const t=performance.now(); const res=e.api(p,s); return [res,performance.now()-t];};
const median = xs => {const s=xs.toSorted((a,b)=>a-b);return (s[(s.length-1)>>1]+s[s.length>>1])/2;};
// Isolate each whole-scope descent: one high-water WASM heap should not pressure the next trial.
if(mode==='whole-child'){
  const c=cases[Number(output)], text=JSON.stringify(c.body);
  // The browser package also exposes the WASM high-water mark; the node package is sufficient for timing.
  const browser=process.env.BENCH_BROWSER_SUFFIX && loadEngine({pkg:before.replace(/-wasm$/,process.env.BENCH_BROWSER_SUFFIX)});
  const e=browser?{api:browser.apiText}:engine(before);
  const [raw,ms]=cost(e,c.path,text), r=JSON.parse(raw);
  assert(r.ok && !r.cut && r.results.length,c.id);
  console.log(JSON.stringify({ms,heapBytes:browser?.memoryBytes(),hash:hash(raw),searched:r.searched,branches:r.branches,jobs:r.jobs,work:r.final_work}));
  process.exit(0);
}
if(mode==='whole'){
  for(const [i,c] of cases.entries()){
    const times={A:[],B:[]}, heaps={A:[],B:[]};let expected;
    for(const tag of i%2?'BAAB':'ABBA'){
      const r=spawnSync(process.execPath,[process.argv[1],dir,tag==='A'?before:after,after,'whole-child',data,String(i)],{encoding:'utf8',timeout:360000});
      if(r.status!==0)throw Error(String(r.error||r.stderr));
      const {ms,heapBytes,...result}=JSON.parse(r.stdout);expected??=result;assert.deepEqual(result,expected,c.id);times[tag].push(ms);
      if(heapBytes!==undefined)heaps[tag].push(heapBytes);
      console.log(c.id,tag,ms.toFixed(0),'ms');
    }
    out.push({id:c.id,msA:median(times.A),msB:median(times.B),times,...(heaps.A.length?{heaps}:{}),...expected});
    fs.writeFileSync(output||path.join(dir,'whole-comparison.json'),JSON.stringify(out));
  }
  process.exit(0);
}
const A = engine(before), B = engine(after);
const receiptSize = r => (r.shards||[r.shard]).reduce((n,x)=>n+(x==null?0:Buffer.byteLength(typeof x==='string'?x:JSON.stringify(x))),0);

// Expand only for the comparison, outside the timed calls. The dense reference supplies the seat schema.
function compareReceipt(actual,expected) {
  if (actual==null) return assert.equal(expected,null);
  assert.equal(typeof actual,'string');
  const [version,widths,v]=JSON.parse(actual); assert.equal(version,1);
  for(const [i,k] of ['by_body','by_combatant'].entries()){
    assert.equal(widths[i],expected[k].length);while(v[k].length<widths[i])v[k].push(0);
  }
  const blank=Object.fromEntries(Object.keys(expected.seat_counters[0]).map(k=>[k,0]));
  while(v.seat_counters.length<expected.seat_counters.length)v.seat_counters.push(blank);
  assert.deepEqual(v,expected);
}
function compareBatch(a,b){
  for(const k of ['pending','scores']) if(a[k]) {
    assert.equal(b[k].length,a[k].length);
    for(let i=0;i<a[k].length;i++){
      const x=a[k][i],y=b[k][i];
      if(x.shards){assert.equal(y.shards.length,x.shards.length);x.shards.forEach((s,j)=>compareReceipt(y.shards[j],s));}
      if(x.shard)compareReceipt(y.shard,x.shard);
    }
  }
  const clean = o => {const v=structuredClone(o);for(const k of ['pending','scores'])for(const r of v[k]||[]){delete r.shards;delete r.shard;}return v;};
  assert.deepEqual(clean(a),clean(b));
}

for(const [ci,c] of cases.entries()){
  if(mode==='fight'){
    const s=JSON.stringify(c.body), ref=A.api(c.path,s), baseline=JSON.parse(ref);
    assert.strictEqual(baseline.ok,true,c.id);assert.equal(B.api(c.path,s),ref,c.id);
    const times={A:[],B:[]};
    for(const tag of ci%2?'BAABBAAB':'ABBAABBA'){
      const [v,t]=cost(tag==='A'?A:B,c.path,s);times[tag].push(t);assert.equal(v,ref,c.id);
    }
    const sh=baseline.shards, mean=k=>sh.reduce((n,x)=>n+x[k],0)/sh.length;
    out.push({id:c.id,msA:median(times.A)/sh.length,msB:median(times.B)/sh.length,times,sha256:hash(ref),
      shots:mean('shots'),ticks:mean('dot_ticks'),procs:mean('procs'),gas:mean('gas')});
  }else{
    const metrics={msA:0,msB:0,leadA:0,leadB:0,scoreA:0,scoreB:0,bytesA:0,bytesB:0,carryA:0,carryB:0,builds:0,fights:0,work:0,steps:0};
    let sa=[],sb=[];
    const req=(q,phase,side)=>{
      const body=JSON.stringify({...c.body,quick_fleet:q});
      const [raw,ms]=cost(side==='A'?A:B,c.path,body);
      metrics['ms'+side]+=ms;metrics[phase+side]+=ms;
      metrics['bytes'+side]+=Buffer.byteLength(body)+Buffer.byteLength(raw);
      const v=JSON.parse(raw);assert.equal(v.ok,true,c.id+': '+v.error);return v;
    };
    for(let step=0;step<1000;step++){
      const a=req({lead:true,fresh:step===0,scores:sa},'lead','A');
      const b=req({lead:true,fresh:step===0,scores:sb},'lead','B');
      compareBatch(a,b);
      if(!a.pending){assert(!a.cut && a.results.length,c.id);metrics.result=hash(JSON.stringify(a));metrics.jobs=a.jobs;break;}
      assert(a.pending.length,c.id);metrics.steps++;sa=[];sb=[];
      // In opening mode measure the same four real candidates, not the cost of a fabricated pool.
      const count=mode==='opening'?Math.min(4,a.pending.length):a.pending.length;
      metrics.batch0??=a.pending.length;
      for(let i=0;i<count;i+=32){const stop=Math.min(count,i+32);
        const aa=req({score:a.pending.slice(i,stop)},'score','A');
        const bb=req({score:b.pending.slice(i,stop),compact_shards:1},'score','B');
        compareBatch(aa,bb);sa.push(...aa.scores);sb.push(...bb.scores);
        metrics.fights+=aa.fights;metrics.work+=aa.work;metrics.builds+=aa.scores.length;
        metrics.carryA+=aa.scores.reduce((n,v)=>n+receiptSize(v),0);metrics.carryB+=bb.scores.reduce((n,v)=>n+receiptSize(v),0);
      }
      if(mode==='opening')break;
      assert(step<999,'nonconvergence: '+c.id);
    }
    out.push({id:c.id,...metrics});
  }
  if((ci+1)%20===0)console.log(mode,ci+1,'/',cases.length);
}
fs.writeFileSync(output||path.join(dir,mode+'-comparison.json'),JSON.stringify(out));
