// SPDX-License-Identifier: AGPL-3.0-or-later
// Exercise a real Chromium worker fleet; no site build, uploads, or shared user profile.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { join } from 'node:path';
import { openApp } from './cdp.mjs';

const [pkg, calls, output, ref = 'working', lanes = '4', compare] = process.argv.slice(2);
if (!output) throw Error('pkg-dir calls.json output.json [git-ref|working] [lanes] [baseline-result.json]');
assert(Number.isInteger(Number(lanes)) && Number(lanes) > 0);
const baseline = compare && JSON.parse(readFileSync(compare, 'utf8'));
const withoutClock = ({cpu_ms, ...result}) => result;
const source = p => ref === 'working' ? readFileSync(p,'utf8') : execFileSync('git',['show',`${ref}:${p}`],{encoding:'utf8'});
const transport = source('web/src/static/app/06-transport.js');
const watch = transport.slice(transport.indexOf('function watchSilence('), transport.indexOf('/// WHERE A WORKER'));
const fleet = source('web/src/static/app/08-checkpoint-api.js');
const assets = new Map([
  ['/', ['text/html','<script src="/harness.js"></script>']],
  ['/worker.js',['text/javascript',source('web/src/static/worker.js')]],
  ['/pkg/wfsim_wasm.js',['text/javascript',readFileSync(join(pkg,'wfsim_wasm.js'))]],
  ['/engine.wasm',['application/wasm',readFileSync(join(pkg,'wfsim_wasm_bg.wasm'))]],
  ['/harness.js',['text/javascript', `let wopt=null,woptNextId=1; const LANE_WATCHDOG={loading:90000,stall:30000,tick:5000};
    const computeRun=async(_,run,live)=>live()?run():{cancelled:true};
    const engine=WebAssembly.compileStreaming(fetch('/engine.wasm'));
    function spawnEngine(){const w=new Worker('/worker.js');engine.then(module=>w.postMessage({kind:'engine',module,packs:[]}));return w;}
    ${watch}\n${fleet}`]],
]);
const server = createServer((req,res) => {
  const asset = assets.get(req.url);
  if (!asset) { res.writeHead(404); return res.end(); }
  res.writeHead(200,{'Content-Type':asset[0]}); res.end(asset[1]);
});
await new Promise(resolve => server.listen(0,'127.0.0.1',resolve));
let app;
try {
  app = await openApp({base:`http://127.0.0.1:${server.address().port}`,boot:100});
  const rows = [];
  for (const c of JSON.parse(readFileSync(calls,'utf8'))) {
    const row = await app.evaluate(`(async()=>{
      await engine; const t=performance.now(); const job=quickFleet(${JSON.stringify(c.body)},${Number(lanes)});
      let finalAt=null,active=0, jobs=0, last=0;
      while(!job.result){
        if(performance.now()-t>1800000) throw Error('benchmark timed out');
        const s=job.status;
        if(s?.activity==='final_simulation') {finalAt??=performance.now();active=Math.max(active,s.workers_active||0);jobs=Math.max(jobs,s.jobs||0); last=Math.max(last,s.sims_done||0);}
        await new Promise(r=>setTimeout(r,20));
      }
      const end=performance.now();return {ms:end-t,final_ms:finalAt==null?null:end-finalAt,active,jobs,last,result:job.result};
    })()`);
    assert.equal(row.result.ok,true,JSON.stringify(row.result));
    if (baseline) {
      assert.equal(c.id, baseline[rows.length]?.id);
      assert.deepEqual(withoutClock(row.result), withoutClock(baseline[rows.length].result));
    }
    rows.push({id:c.id,...row});
    writeFileSync(output,JSON.stringify(rows));
    console.log(c.id, 'ms',row.ms,'final_ms',row.final_ms,'final workers',row.active,'jobs',row.result.jobs,'fights',row.result.fights);
  }
  if (baseline) assert.equal(rows.length, baseline.length);
} finally { if (app) await app.close(); server.close(); }
