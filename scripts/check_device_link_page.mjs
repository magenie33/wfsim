// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { openApp } from './cdp.mjs';
import { testDb } from './pg/test_db.mjs';
import { deviceLinkRoute } from '../worker/device_link.js';
import { sha256 } from '../worker/accounts.js';
import { clientRelease } from '../worker/client_release.js';

const env={ACCOUNTS:await testDb('accounts'),AUTH_SECRET:'browser-device-link'};
await env.ACCOUNTS.run("INSERT INTO entity_accounts(id,created_at,username) VALUES('alice','2026-01-01','alice')");
await env.ACCOUNTS.run("INSERT INTO state_login_sessions(token_hash,account,expires_at) VALUES(?,'alice','2999-01-01')",await sha256('alice'));
const verifier='a'.repeat(24);
const computeSource=readFileSync('web/src/static/app/73-compute.js','utf8');
const widget=computeSource.slice(computeSource.indexOf('let deviceLink ='));
const action=readFileSync('web/src/static/app/73-compute.js','utf8').match(/async function computeAct[\s\S]*?\n\}/)[0];
const manifest={v:1,version:'0.1.0-test',npm:false,files:Object.fromEntries(['x86_64-linux','x86_64-windows','aarch64-macos','x86_64-macos']
  .map(target=>[target,{sha256:'a'.repeat(64),urls:[`https://wfsim-1388973035.cos.ap-shanghai.myqcloud.com/cli/releases/0.1.0-test/wfsim-${target}`]}]))};
let origin;
const server=createServer(async(req,res)=>{
  const url=new URL(req.url,origin||'http://localhost');
  if(url.pathname==='/api/cli/latest'){
    const response=await clientRelease(new Request(origin+req.url,{method:req.method}),async()=>new Response(JSON.stringify(manifest)));
    res.writeHead(response.status,Object.fromEntries(response.headers));res.end(await response.text());return;
  }
  if(url.pathname.startsWith('/api/')){
    let body='';for await(const c of req)body+=c;
    const response=await deviceLinkRoute(new Request(origin+req.url,{method:req.method,headers:req.headers,body}),env,url.pathname);
    res.writeHead(response.status,Object.fromEntries(response.headers));res.end(await response.text());return;
  }
  if(url.pathname==='/style.css'){res.writeHead(200,{'content-type':'text/css'});res.end(readFileSync('web/src/static/style.css'));return;}
  if(url.pathname==='/widget.js'){
    res.writeHead(200,{'content-type':'text/javascript'});
    res.end(`let accountState={account:null}; const $=id=>document.getElementById(id);
      const escHtml=s=>String(s).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;');
      const aT=escHtml;
      const fetchJson=async url=>(await fetch(url)).json();
      const accountCall=async(method,path,body)=>(await fetch(path,{method,headers:{'content-type':'application/json'},body:JSON.stringify(body)})).json();
      const loadDevices=async()=>{window.deviceLoads=(window.deviceLoads||0)+1;};
      ${widget}\n${action}
      function computeDraw(main){main.innerHTML=computeCliHtml();deviceLinkDraw(main);loadComputeCli(main);}
      document.addEventListener('click',async e=>{const b=e.target.closest('[data-auth]');if(b){e.preventDefault();await computeAct(b,b.dataset.auth);}});
      computeDraw($('auth-page'));window.__wfsimReady=true;`);return;
  }
  res.writeHead(200,{'content-type':'text/html','set-cookie':'wfsim_session=alice; Path=/; SameSite=Lax'});
  res.end('<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><link rel="stylesheet" href="/style.css"><main style="max-width:1000px;margin:auto;padding:24px"><div id="auth-page"></div></main><script src="/widget.js"></script>');
});
let app;
const ready=async expression=>{const end=Date.now()+5000;while(Date.now()<end){if(await app.evaluate(expression))return;await app.sleep(50);}throw Error('device component did not settle');};
try{
  await new Promise(r=>server.listen(0,'127.0.0.1',r));origin=`http://127.0.0.1:${server.address().port}`;
  const start=await deviceLinkRoute(new Request(origin+'/api/account/devices/link/start',{method:'POST',headers:{'content-type':'application/json'},
    body:JSON.stringify({verifier,label:'My server'})}),env,'/api/account/devices/link/start');
  const code=(await start.json()).user_code;
  app=await openApp({base:origin,boot:200});
  await app.load(`/compute?device_code=${code}`,200);
  await ready(`!!document.querySelector('#compute-device-link a')`);
  await ready(`document.querySelectorAll('#compute-cli a.ghost-btn').length===4`);
  app.check('four native downloads load through the same-origin manifest gateway',await app.evaluate(`document.querySelectorAll('#compute-cli a.ghost-btn').length===4`));
  app.check('signed-out approval preserves the short code through sign-in',await app.evaluate(`document.querySelector('#compute-device-link a').getAttribute('href').includes(encodeURIComponent('/compute?device_code=${code}'))`));
  await app.evaluate(`accountState.account={id:'alice'};computeDraw($('auth-page'))`);await ready(`!!document.querySelector('[data-auth="compute-device-link-approve"]')`);
  app.check('the actual approval component reviews the device before claiming',await app.evaluate(`document.querySelector('#compute-device-link').textContent.includes('My server') && !!document.querySelector('[data-auth="compute-device-link-approve"]')`));
  app.check('review never claims automatically',!(await env.ACCOUNTS.get('SELECT * FROM relation_account_compute_devices WHERE verifier=?',verifier)));
  await app.evaluate(`document.querySelector('[data-auth="compute-device-link-approve"]').click()`);await ready(`document.querySelector('#compute-device-link').textContent.includes('Device claimed.')`);
  app.check('the compute page action claims through the shared browser endpoint',await app.evaluate(`document.querySelector('#compute-device-link').textContent.includes('Device claimed.') && window.deviceLoads===1`));
  assert.equal((await env.ACCOUNTS.get('SELECT account FROM relation_account_compute_devices WHERE verifier=?',verifier)).account,'alice');
  app.check('unpublished npm commands are hidden',await app.evaluate(`!document.querySelector('#compute-cli').textContent.includes('npx')`));
  await app.evaluate(`computeCliRelease.npm=true;computeDraw($('auth-page'))`);
  app.check('published npm entry is shown beside the native download',await app.evaluate(`document.querySelector('#compute-cli').textContent.includes('npx wfsim@latest join')`));
  await app.send('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:1,mobile:true});
  app.check('the native download and approval fit a phone',await app.evaluate(`document.documentElement.scrollWidth<=innerWidth`));
  mkdirSync('target/official-cli',{recursive:true});
  const shot=await app.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true});
  writeFileSync('target/official-cli/device-claim-390.png',Buffer.from(shot.result.data,'base64'));
  await app.evaluate(`history.replaceState(null,'','/compute?device_code=ZZZZ-ZZZZ');computeDraw($('auth-page'))`);await ready(`document.querySelector('#compute-device-link').textContent.includes('expired')`);
  app.check('expired codes offer a new login instead of approval',await app.evaluate(`document.querySelector('#compute-device-link').textContent.includes('expired') && !document.querySelector('[data-auth="compute-device-link-approve"]')`));
  await app.evaluate(`history.replaceState(null,'','/compute');computeDraw($('auth-page'))`);
  app.check('leaving the code removes the old approval surface',await app.evaluate(`!document.querySelector('#compute-device-link')`));
  assert.equal(app.failures,0);
  console.log('Device claim page: 10 checks passed with the actual component, manifest gateway, compute action and account route.');
}finally{
  if(app)await app.close();await new Promise(r=>server.close(r));await env.ACCOUNTS.close();
}
