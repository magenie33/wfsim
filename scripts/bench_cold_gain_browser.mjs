// SPDX-License-Identifier: AGPL-3.0-or-later
// node scripts/bench_cold_gain_browser.mjs OLD_ROOT OLD_PKG NEW_ROOT NEW_PKG results.json [lanes]
// Use no-modules wasm-bindgen packages with embedded data. Each menu gets fresh workers;
// engine loading is outside the timing. No DOM painting, board jobs or remote service.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { join } from 'node:path';
import { openApp } from './cdp.mjs';

const [oldRoot,oldPkg,newRoot,newPkg,output,laneCount='1'] = process.argv.slice(2);
const count = Number(laneCount);
assert(output && Number.isInteger(count) && count >= 1 && count <= 8, 'expected two worktrees, packages, output file and 1..8 lanes');
const fixture = new URL('../tests/fixtures/engine-compare/status-density-held-out-calls.json', import.meta.url);
const crowd = JSON.parse(readFileSync(fixture))[0].body.request.formation;
const cases = [
  ['torid',2,false,['hellfire',null,null,'infected_clip',null,'serration','vital_sense','split_chamber'],'cryo_rounds'],
  ['ocucor',4,true,['heated_charge',null,null,'convulsion',null,'hornet_strike','barrel_diffusion'],'pistol_gambit'],
  ['magistar',4,false,['shocking_touch',null,null,'fever_strike',null,'pressure_point',null,null,null,'crushing_ruin'],'organ_shatter'],
  ['phantasma_prime',6,true,['toxic_barrage',null,'incendiary_coat','charged_shell','hells_chamber','point_blank'],'ravage'],
].map(([weapon,duration,multi,slots,rank]) => ({weapon,slots,rank, fight:{enemy:'corrupted_heavy_gunner',level:100,
  duration,metric:multi?'kpm':'dps',...(multi && {formation:crowd}),headshot_pct:50,infinite_ammo:true,steel_path:true}}));

const assets = new Map([['/', ['text/html','<!doctype html><title>Cold gain scan</title>']]]);
const sources = {}, hashes = {};
for (const [tag,[root,pkg]] of Object.entries({A:[oldRoot,oldPkg],B:[newRoot,newPkg]})) {
  const source = file => readFileSync(join(root,'web/src/static',file),'utf8');
  const transport = source('app/06-transport.js');
  const start = transport.indexOf('async function laneSimulate('), end = transport.indexOf('/// STOP EVERY WASM CALL',start);
  assert(start >= 0 && end > start, 'missing simulation transport');
  sources[tag] = [transport.slice(start,end),source('app/54-gains.js'),source('app/56-quick-calc.js')].join('\n');
  const binary = readFileSync(join(pkg,'wfsim_wasm_bg.wasm'));
  hashes[tag] = createHash('sha256').update(binary).digest('hex');
  assets.set(`/${tag}/worker.js`,['text/javascript',source('worker.js')]);
  assets.set(`/${tag}/pkg/wfsim_wasm.js`,['text/javascript',readFileSync(join(pkg,'wfsim_wasm.js'))]);
  assets.set(`/${tag}/engine.wasm`,['application/wasm',binary]);
}

// The calculator uses its production candidate listing, fleet baseline, slot scheduling
// and paired comparisons. The host supplies the selected build and unrestricted lanes.
async function scan(tag,source,state,count) {
  const WASM = true, mode = 'base', slots = state.slots, evoSel = {}, valence = {}, assembly = null;
  const $ = () => ({value:state.weapon}), tr = String, arcanes = [], arcaneRanks = [], slotModId = id => id;
  const scenarioList = () => [], activeScenario = 'test', theFight = o => ({...state.fight,...o});
  const everyRank = () => ({mods:[state.rank],arcanes:[]}), quickTask = {replace(){}}, pickerPrefs = {dir:'desc'};
  const rivenPayload = () => [.95,1.05].map((roll,i)=>({id:`generated:${i}`,spec:{rank:4,bonuses:[{id:'critical_chance',roll}]}}));
  const buildPayload = () => ({weapon:state.weapon,mods:slots.filter(Boolean),rivens:rivenPayload(),mode,evolutions:[]});
  const metricOf = m => ({per_minute:m==='kpm'}), yieldToForeground = async () => {};
  const workers = [], poolSize = () => count, lanes = async n => workers.slice(0,n), freeLane = () => workers[0];
  const measurements = [], calls = {shard:0,merge:0,api:0};
  let indexedRuns = 0, nextId = 0, measuringBase = false, nextBase = 0;
  const waiting = new Map();
  const compiled = await WebAssembly.compileStreaming(fetch(`/${tag}/engine.wasm`));
  try {
    for(let i=0;i<count;i++) {
      const worker = new Worker(`/${tag}/worker.js`), pending = new Map();
      const send = (msg,tick) => new Promise((resolve,reject) => {
        calls[msg.kind]++; if(msg.kind==='shard') indexedRuns+=msg.count;
        const id=++nextId; pending.set(id,{resolve,reject,tick,msg,baseline:measuringBase && msg.kind==='shard'}); worker.postMessage({...msg,id});
      });
      worker.onmessage = ({data:m}) => {
        const job = pending.get(m.id); if(!job) return;
        if(m.kind==='progress') job.tick?.(m.done,m.total);
        if(!m.kind && 'payload' in m) {
          pending.delete(m.id);
          if(!job.baseline) { job.resolve(m.payload); return; }
          waiting.set(job.msg.from, {job,payload:m.payload});
          while(waiting.has(nextBase)) {
            const part=waiting.get(nextBase);waiting.delete(nextBase);
            nextBase+=part.job.msg.count;part.job.resolve(part.payload);
          }
        }
      };
      worker.onerror = e => { for(const job of pending.values()) job.reject(Error(e.message)); pending.clear(); };
      worker.postMessage({kind:'engine',module:compiled});
      workers.push({worker,send,call:(path,body)=>send({kind:'api',path,body})});
    }
    const api = (path,body) => freeLane().call(path,body);
    await Promise.all(workers.map(w=>w.call('/api/candidates',{weapon:state.weapon,axis:{kind:'mode',idx:0}})));
    calls.api=0;
    const digest = async v => Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(JSON.stringify(v)))),b=>b.toString(16).padStart(2,'0')).join('');
    // Hold the transport clock and baseline delivery order fixed only in this check.
    // Otherwise even upstream vs itself changes paired series order and sum grouping.
    return await eval(`(async()=>{const performance={now:()=>0};${source}\ncalcDirty=()=>{};
      const fleet=simulateFleet;simulateFleet=async(...args)=>{measuringBase=true;try{return await fleet(...args);}finally{measuringBase=false;}};
      const original=laneSimulate;laneSimulate=async(...args)=>{const r=await original(...args);measurements.push([JSON.stringify(args[1]),r]);return r;};
      const t=globalThis.performance.now();await scanGains({kind:'mods',idx:1});const ms=globalThis.performance.now()-t;
      const rows=[...gainScan.ids].map(id=>[id,gainScan.by[id]??gainScan.refused?.[id]??null]);
      if(!gainScan.key||!gainScan.base||!rows.length||gainScan.done!==rows.length||rows.some(r=>r[1]===null)) throw Error(gainScan.note||'incomplete scan');
      measurements.sort((a,b)=>a[0]<b[0]?-1:a[0]>b[0]?1:0);
      return {ms,rows:rows.length,calls,indexedRuns,gains:await digest([rows,gainScan.base,gainScan.floor]),reports:await digest(measurements)};
    })()`);
  } finally { for(const w of workers) w.worker.terminate(); }
}

const server = createServer((req,res) => {
  const item=assets.get(req.url); if(!item){res.writeHead(404);return res.end();}
  res.writeHead(200,{'Content-Type':item[0]}); res.end(item[1]);
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
let app;
try {
  app=await openApp({base:`http://127.0.0.1:${server.address().port}`,boot:100});
  const result={hashes,count,browser:(await app.send('Browser.getVersion')).result.product,rows:[]}, expected=new Map();
  for(const tag of 'ABBAABBA') for(const c of cases) {
    const args=[tag,sources[tag],c,count].map(v=>JSON.stringify(v)).join(',');
    const r=await app.evaluate(`(${scan})(${args})`);
    const identity=[r.rows,r.indexedRuns,r.gains,r.reports];
    if(expected.has(c.weapon)) assert.deepEqual(identity,expected.get(c.weapon),c.weapon);
    else expected.set(c.weapon,identity);
    result.rows.push({tag,weapon:c.weapon,...r});writeFileSync(output,JSON.stringify(result,null,2));
    console.log(tag,c.weapon,r.rows,Math.round(r.ms),'ms');
  }
} finally { if(app) await app.close(); server.closeAllConnections(); await new Promise(r=>server.close(r)); }
