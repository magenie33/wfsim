// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { runInNewContext } from 'node:vm';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const binary = path.join(root, 'client/target/debug', process.platform === 'win32' ? 'wfsim.exe' : 'wfsim');
assert(existsSync(binary), 'build the client first: cargo build --manifest-path client/Cargo.toml');
const temp = mkdtempSync(path.join(os.tmpdir(), 'wfsim-cli-check-'));
const device = path.join(temp, 'device');
let child, server, checks = 0;
const check = (name, ok) => { assert(ok, name); console.log(`  ok ${name}`); checks++; };
const run = (...args) => spawnSync(binary, ['--data-dir', device, ...args], { encoding: 'utf8', timeout: 15_000 });
const until = async (fn, ms = 20_000) => { const end = Date.now()+ms; while(Date.now()<end) { if(fn()) return; await new Promise(r=>setTimeout(r,100)); } throw new Error('condition timed out'); };
const json = file => JSON.parse(readFileSync(path.join(device, file), 'utf8'));
try {
  const help = run('--help'); check('native program runs without game-engine files or Node packages', help.status === 0 && help.stdout.includes('Join WFSim'));
  check('invalid ranges are rejected before participation', run('config', '--range', '50-0').status !== 0);
  const no = run('join'); check('noninteractive computing requires explicit consent', no.status !== 0 && no.stdout.includes('voluntary') && !existsSync(path.join(device,'identity')));
  check('whole percentages including zero are accepted', run('config', '--range', '0-0', '--name', 'Protocol fixture').status === 0);
  const identity = readFileSync(path.join(device,'identity'),'utf8');
  check('identity is persistent and recognized', /^[0-9a-f]{24}$/.test(identity) && run('config','--range','1-99').status === 0 && readFileSync(path.join(device,'identity'),'utf8') === identity);
  if(process.platform!=='win32') {
    const { statSync } = await import('node:fs');
    check('device credentials are private to the user', (statSync(path.join(device,'identity')).mode & 0o077) === 0);
  }
  const config = json('config.json'); config.range = { min:0,max:0 }; config.consent = { v:1,at:new Date().toISOString() };
  let allowAck=false, requests=0, controls=0;
  server=createServer(async(req,res)=>{
    if(req.url==='/cli/latest.json'){res.writeHead(404).end();return;}
    if(req.url!=='/api/board/sync'){controls++;res.writeHead(500).end();return;}
    let raw='';for await(const c of req)raw+=c;
    const b=JSON.parse(raw);requests++;
    assert.equal(b.want_ms,0);assert.deepEqual(b.held,[]);assert.equal(b.verifier,identity);
    const reply={ok:true,kept:true,taken:allowAck?b.results.map(x=>x.question):[],dropped:[],pack:null,work:null,next_ms:1000};
    if(requests===1){req.socket.destroy();return;}
    res.writeHead(200,{'content-type':'application/json'}).end(JSON.stringify(reply));
  });
  await new Promise(r=>server.listen(0,'127.0.0.1',r));
  config.server=`http://127.0.0.1:${server.address().port}`;
  writeFileSync(path.join(device,'config.json'),JSON.stringify(config));
  writeFileSync(path.join(device,'pending.json'),JSON.stringify([{engine:'fixture',path:'/api/board/sync',body:{lease:'a'.repeat(32),question:'synthetic',score:1.2345678901234567,metric:'kpm',work:100}}]));
  child=spawn(binary,['--data-dir',device,'join'],{stdio:['ignore','pipe','pipe']});
  let log='';child.stdout.on('data',b=>{log+=b;});child.stderr.on('data',b=>{log+=b;});
  await until(()=>requests>=2);
  check('lost replies and missing acknowledgments preserve the durable outbox',json('pending.json').length===1);
  check('one device cannot compute in two instances',run('join').status!==0);
  allowAck=true;await until(()=>json('pending.json').length===0);
  check('only explicit acknowledgment removes a completed answer',requests>=3);
  check('zero budget never loads an engine, polls for work or requests a lease',controls===0);
  check('stop targets its own run without killing other clients',run('stop').status===0);
  await until(()=>child.exitCode!==null);check('shutdown exits cleanly',child.exitCode===0);
  child=null;
  check('background start is available at zero without renewed consent',run('start').status===0);
  await until(()=>existsSync(path.join(device,'status.json'))&&json('status.json').state==='CPU budget is zero');
  check('pause and resume preserve the chosen range',run('pause').status===0 && run('resume').status===0 && json('config.json').range.max===0);
  run('stop');await until(()=>json('status.json').state==='stopped');
  check('background operation preserves the same device identity',readFileSync(path.join(device,'identity'),'utf8')===identity);
  const bound=json('config.json');writeFileSync(path.join(device,'config.json'),JSON.stringify({...bound,server:'https://another.invalid'}));
  check('a saved device cannot send its credential or pending results to another server',run('join').status!==0);
  writeFileSync(path.join(device,'config.json'),JSON.stringify(bound));

  const packageDir=path.join(temp,'npm');cpSync(path.join(root,'client/npm'),packageDir,{recursive:true});
  const bytes=readFileSync(binary), sha256=createHash('sha256').update(bytes).digest('hex');
  const target=`${process.arch==='arm64'?'aarch64':'x86_64'}-${{darwin:'macos',win32:'windows',linux:'linux'}[process.platform]}`;
  const manifest={v:1,version:'0.1.0-test',files:{[target]:{size:bytes.length,sha256,urls:[]}}};
  writeFileSync(path.join(packageDir,'release.json'),JSON.stringify(manifest));
  const cache=path.join(temp,'cache'), binDir=path.join(cache,manifest.version,target);mkdirSync(binDir,{recursive:true});
  const cached=path.join(binDir,process.platform==='win32'?'wfsim.exe':'wfsim');cpSync(binary,cached);
  const launch=(...args)=>spawnSync(process.execPath,[path.join(packageDir,'bin/wfsim.mjs'),...args],{encoding:'utf8',env:{...process.env,WFSIM_CLI_CACHE:cache},timeout:15_000});
  check('npm entry launches the same verified native binary',launch('--version').status===0);
  check('npm entry preserves the native exit code',launch('no-such-command').status===2);
  bytes[0]^=1;writeFileSync(cached,bytes);
  const corrupt=launch('--version');check('corrupt cached executable is refused before execution',corrupt.status===1&&corrupt.stderr.includes('No CLI download source'));
  const fixtures=JSON.parse(readFileSync(path.join(root,'tests/client_payloads.json'),'utf8'));
  const src=readFileSync(path.join(root,'mcp/headless.js'),'utf8');
  const adapter=runInNewContext(src.match(/function headlessBoardPayloadFromResult[\s\S]*?\n\}/)[0]+'\nheadlessBoardPayloadFromResult');
  for(const f of fixtures)assert.deepEqual(JSON.parse(JSON.stringify(adapter(f.result,f.context))),f.expected);
  check('CLI payload fixtures match the authoritative browser adapter',fixtures.length>=3);
  console.log(`CLI launcher and lifecycle: ${checks} checks passed.`);
} finally {
  if(child){run('stop');await until(()=>child.exitCode!==null,5000).catch(()=>child.kill('SIGTERM'));}
  if(existsSync(path.join(device,'status.json'))){run('stop');await new Promise(r=>setTimeout(r,1100));}
  if(server)await new Promise(r=>server.close(r));
  rmSync(temp,{recursive:true,force:true});
}
