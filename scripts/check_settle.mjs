// SPDX-License-Identifier: AGPL-3.0-or-later
// Run the official cycle and CLI with local transports; no production work is claimed.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, existsSync, rmSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const dir = realpathSync(mkdtempSync(join(tmpdir(), 'wfsim-settle-')));
let failures = 0, passed = 0;
const check = (name, fn) => {
  try { fn(); passed++; console.log('  ok  ' + name); }
  catch (e) { failures++; console.log('  FAIL ' + name + ': ' + e.message); }
};
const put = (name, body, mode = 0o644) => writeFileSync(join(dir, name), body, { mode });
try {
  mkdirSync(join(dir, 'bin'));
  put('bin/date', '#!/usr/bin/env bash\nf="$FIXTURE_CLOCK"; n=1000; [ ! -f "$f" ] || n=$(cat "$f"); n=$((n+1)); echo "$n" > "$f"; echo "$n"\n', 0o755);
  put('bin/sleep', '#!/usr/bin/env bash\nexit 0\n', 0o755);
  put('bin/nproc', '#!/usr/bin/env bash\necho 2\n', 0o755);
  put('bin/node', ['#!/usr/bin/env bash',
    'n=0; [ ! -f "$FIXTURE_COUNT" ] || n=$(cat "$FIXTURE_COUNT"); n=$((n+1)); echo "$n" > "$FIXTURE_COUNT"',
    'read -ra steps <<< "$FIXTURE_CODES"; i=$((n-1)); [ "$i" -lt "${#steps[@]}" ] || i=$((${#steps[@]}-1))',
    'exit "${steps[$i]}"', ''].join('\n'), 0o755);
  const cycle = codes => {
    for (const n of ['clock', 'count']) rmSync(join(dir, n), { force: true });
    return spawnSync('bash', [new URL('./settle_cycles.sh', import.meta.url).pathname, 'work', '.'], {
      cwd: dir, encoding: 'utf8', timeout: 10000,
      env: { ...process.env, PATH: join(dir, 'bin') + ':' + process.env.PATH, SETTLE_MINUTES: '1',
        FIXTURE_CLOCK: join(dir, 'clock'), FIXTURE_COUNT: join(dir, 'count'), FIXTURE_CODES: codes },
    });
  };
  check('an empty official queue finishes successfully', () => assert.equal(cycle('3').status, 0));
  check('a recovered transient cycle does not fail the run', () => assert.equal(cycle('1 1 0 3').status, 0));
  check('persistent failed cycles fail the run and stop retrying', () => {
    const r = cycle('1');
    assert.notEqual(r.status, 0);
    assert.ok(Number(readFileSync(join(dir, 'count'), 'utf8')) <= 3);
  });
  mkdirSync(join(dir, 'scripts')); mkdirSync(join(dir, 'worker'));
  put('package.json', '{"type":"module"}');
  put('scripts/live_orders.mjs', readFileSync(process.env.WFSIM_LIVE_ORDERS_SOURCE || new URL('./live_orders.mjs', import.meta.url), 'utf8'));
  put('scripts/order_credit.mjs', readFileSync(process.env.WFSIM_ORDER_CREDIT_SOURCE || new URL('./order_credit.mjs', import.meta.url), 'utf8'));
  put('worker/board_kinds.js', readFileSync(new URL('../worker/board_kinds.js', import.meta.url), 'utf8'));
  put('scripts/wasm_witness.mjs', `import {appendFileSync,writeFileSync} from 'node:fs';
export async function witness() { return { engine: 'fixture-engine', row: async () => {
  appendFileSync('fought','row\\n');return {score:1,metric:'kpm',work:1};
}, close: async () => {writeFileSync('closed','yes');} }; }`);
  put('scripts/ship_facts.sh', '#!/usr/bin/env bash\necho bypass > direct-shipment\n');
  put('transport.mjs', `import {appendFileSync,writeFileSync} from 'node:fs';
const mode=process.env.FIXTURE_MODE;
const tasks=['sweep','board_review','build_workflow'].map((producer,i)=>({
  question:'question-'+i,task_id:101+i,lease:'captured-'+i,state:'spot',
  request:{record:{},ruler:'fixture-ruler',mode:'base'},
  demands:[{producer,ref:producer==='sweep'?'fixture-build|fixture-ruler|base':producer,
    payload:{identity:'fixture-build',ruler:'fixture-ruler',mode:'base'}}]
}));
globalThis.fetch=async(url,options)=>{
  const what=url.split('/').at(-1),body=JSON.parse(options.body);
  appendFileSync('calls',JSON.stringify({what,body})+'\\n');
  const reply=value=>new Response(JSON.stringify({ok:true,...value}));
  if(what==='release'){
    if(!Array.isArray(body.claims)||body.claims.length>200||body.claims.some(c=>typeof c.question!=='string'||!Number.isSafeInteger(c.task_id)||!c.lease))
      return new Response(JSON.stringify({ok:false,error:'captured release required'}),{status:400});
    if(mode==='release-failure')return new Response(JSON.stringify({ok:false,error:'fixture release unavailable'}),{status:503});
    writeFileSync('released','yes');return reply({});
  }
  if(what==='claim'){
    if(mode==='fallback-failure'&&!body.states.includes('spot'))return new Response(JSON.stringify({ok:false,error:'fixture fallback unavailable'}),{status:503});
    const selected=mode==='empty'||!body.states.includes('spot')?[]:structuredClone(tasks);
    if(mode==='fallback-failure')selected.splice(1);
    if(mode==='invalid-lease'&&selected.length)selected[0].lease='';
    if(mode==='missing-round'&&selected.length)delete selected[0].task_id;
    return reply({tasks:selected});
  }
  if(what!=='answer')throw Error('unexpected official action '+what);
  const kind=body.answers[0].ref==='build_workflow'?'workflow':body.answers[0].ref==='board_review'?'review':'board';
  if(mode==='transport-failure')return new Response(JSON.stringify({ok:false,error:'fixture: database refused delivery'}),{status:503});
  if(mode==='missing-'+kind)return reply({});
  if(mode==='short-'+kind)return reply({states:[]});
  const states=body.answers.map(answer=>{
    const task=tasks.find(t=>t.demands[0].ref===answer.ref);
    const current=task.task_id+(mode==='late-round'?1:0);
    const valid=answer.question===task.question&&answer.task_id===current&&answer.lease===task.lease;
    if(!valid||mode==='refuse-'+kind)return null;
    appendFileSync('acknowledged',JSON.stringify({kind,answer})+'\\n');return 'fact';
  });
  return reply({states});
};`);
  const clean = () => {
    for (const file of ['calls','fought','closed','released','acknowledged','direct-shipment','facts.ndjson.credited']) rmSync(join(dir,file),{force:true});
    rmSync(join(dir,'work'),{recursive:true,force:true});
  };
  const rows = file => existsSync(join(dir,file)) ? readFileSync(join(dir,file),'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
  const run = (mode, script='live_orders.mjs', args=['settle','work','.']) => {
    clean();
    return spawnSync(process.execPath,['--import',join(dir,'transport.mjs'),join(dir,'scripts',script),...args],{
      cwd:dir,encoding:'utf8',timeout:10000,env:{...process.env,WITNESS:'official:fixture',FACTORY_TOKEN:'fixture',FIXTURE_MODE:mode},
    });
  };
  check('all official producers submit the captured round, work and lease before delivery', () => {
    const r=run('success');assert.equal(r.status,0,r.stderr);
    const accepted=rows('acknowledged');assert.equal(accepted.length,3);
    for(const {answer} of accepted){assert.match(answer.question,/^question-[012]$/);assert.match(answer.lease,/^captured-[012]$/);assert.equal(answer.work,1);}
    const board=accepted.find(x=>x.kind==='board').answer;
    assert.equal(board.task_id,101);assert.equal(board.result.measurement.measured_by,'fixture-engine');
    assert.ok(Date.parse(board.result.measurement.started_at)<=Date.parse(board.result.measurement.finished_at));
    assert.equal(existsSync(join(dir,'direct-shipment')),false);
    assert.equal(existsSync(join(dir,'released')),true);assert.equal(existsSync(join(dir,'closed')),true);
    const released=rows('calls').filter(c=>c.what==='release').flatMap(c=>c.body.claims);
    assert.deepEqual(released,[0,1,2].map(i=>({question:'question-'+i,task_id:101+i,lease:'captured-'+i})));
    assert.deepEqual(rows('work/claimed-leases.ndjson'),released);
  });
  for(const mode of ['refuse-workflow','refuse-review','refuse-board','missing-workflow','short-review','missing-board','late-round'])check(mode+' fails without direct shipment and releases its claims',()=>{
    const r=run(mode);assert.notEqual(r.status,0,r.stderr);
    assert.match(r.stderr,/not fully acknowledged/);
    assert.equal(existsSync(join(dir,'direct-shipment')),false);
    assert.equal(existsSync(join(dir,'released')),true);assert.equal(existsSync(join(dir,'closed')),true);
    if(mode==='late-round')assert.equal(rows('acknowledged').length,0);
  });
  for(const mode of ['invalid-lease','missing-round'])check(mode+' refuses computation before it starts',()=>{
    const r=run(mode);assert.notEqual(r.status,0);assert.match(r.stderr,/invalid captured official claim/);
    assert.equal(existsSync(join(dir,'fought')),false);assert.equal(existsSync(join(dir,'released')),true);
  });
  check('delivery failure is nonzero, preserves its error and releases official claims',()=>{
    const r=run('transport-failure');assert.notEqual(r.status,0);assert.match(r.stderr,/fixture: database refused delivery/);
    assert.equal(rows('acknowledged').length,0);assert.equal(existsSync(join(dir,'direct-shipment')),false);
    assert.equal(existsSync(join(dir,'released')),true);
  });
  check('an empty official claim computes nothing',()=>{
    assert.equal(run('empty').status,3);assert.equal(existsSync(join(dir,'fought')),false);assert.equal(existsSync(join(dir,'released')),false);
  });
  check('a failed fallback releases the first captured page before any computation',()=>{
    const r=run('fallback-failure');assert.notEqual(r.status,0);assert.match(r.stderr,/fixture fallback unavailable/);
    assert.equal(existsSync(join(dir,'fought')),false);
    const released=rows('calls').filter(c=>c.what==='release').flatMap(c=>c.body.claims);
    assert.deepEqual(released,[{question:'question-0',task_id:101,lease:'captured-0'}]);
  });
  check('failed cleanup retains every captured lease for the outer retry',()=>{
    const r=run('release-failure');assert.notEqual(r.status,0);assert.match(r.stderr,/fixture release unavailable/);
    const receipts=rows('work/claimed-leases.ndjson');assert.equal(receipts.length,3);assert.equal(rows('acknowledged').length,3);
    put('retry-leases.ndjson',receipts.map(c=>JSON.stringify(c)+'\n').join(''));
    const retry=run('success','order_credit.mjs',['--release','retry-leases.ndjson']);assert.equal(retry.status,0,retry.stderr);
    assert.deepEqual(rows('calls').flatMap(c=>c.body.claims),receipts);
  });
  const fact={identity:'fixture-build',ruler:'fixture-ruler',mode:'base',score:1,metric:'kpm',work:1,
    measured_by:'fixture-engine',cost_seconds:1,started_at:'2026-11-15T00:00:00.000Z',finished_at:'2026-11-15T00:00:01.000Z'};
  put('facts.ndjson',JSON.stringify(fact)+'\n');
  put('claims.ndjson',JSON.stringify({build_id:fact.identity,ruler:fact.ruler,mode:fact.mode,question:'question-0',task_id:101,lease:'captured-0'})+'\n');
  check('the scorer CLI requires its captured claims',()=>{
    const r=run('success','order_credit.mjs',['facts.ndjson','fixture-engine']);assert.equal(r.status,2);
    assert.equal(rows('calls').length,0);assert.equal(existsSync(join(dir,'facts.ndjson.credited')),false);
  });
  check('a refused CLI answer retains its cursor for retry',()=>{
    const r=run('refuse-board','order_credit.mjs',['facts.ndjson','fixture-engine','claims.ndjson']);assert.notEqual(r.status,0);
    assert.equal(existsSync(join(dir,'facts.ndjson.credited')),false);assert.equal(rows('acknowledged').length,0);
  });
  check('an accepted CLI answer advances only after official acknowledgement',()=>{
    const r=run('success','order_credit.mjs',['facts.ndjson','fixture-engine','claims.ndjson']);assert.equal(r.status,0,r.stderr);
    assert.equal(readFileSync(join(dir,'facts.ndjson.credited'),'utf8'),'1');assert.equal(rows('acknowledged').length,1);
  });
  check('the release CLI sends bounded captured rounds and leases',()=>{
    const claims=Array.from({length:401},(_,i)=>({question:'release-'+i,task_id:i+1,lease:'lease-'+i}));
    put('release.ndjson',claims.map(c=>JSON.stringify(c)+'\n').join(''));
    const r=run('success','order_credit.mjs',['--release','release.ndjson']);assert.equal(r.status,0,r.stderr);
    const calls=rows('calls');assert.deepEqual(calls.map(c=>c.body.claims.length),[200,200,1]);
    assert(calls.every(c=>c.what==='release'));assert.deepEqual(calls.flatMap(c=>c.body.claims),claims);
  });
  check('invalid release receipts fail before any lease is touched',()=>{
    put('release.ndjson',JSON.stringify({question:'a',task_id:1,lease:'a'})+'\n'+JSON.stringify({question:'b',task_id:2})+'\n');
    const r=run('success','order_credit.mjs',['--release','release.ndjson']);assert.notEqual(r.status,0);
    assert.equal(rows('calls').length,0);
  });
} finally { rmSync(dir, { recursive: true, force: true }); }
console.log(`\n${passed} passed, ${failures} failed`);
if (failures) process.exitCode = 1;
