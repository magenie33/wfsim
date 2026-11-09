// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';

const {clientRelease, CLI_MANIFEST_PATH} = await import(process.env.WFSIM_CLIENT_MANIFEST_MODULE || '../worker/client_release.js');
const canonical = 'https://wfsim-1388973035.cos.ap-shanghai.myqcloud.com/cli/latest.json';
const bytes = '{ "v":1, "version":"0.1.0", "files":{}, "npm":false, "future":{"keep":true} }\n';
let checks = 0;
const check = (label, condition) => {assert(condition, label); console.log('  ok '+label); checks++;};
let asked;
const incoming = new Request('https://wfsim.app'+CLI_MANIFEST_PATH+'?url=https://attacker.example/', {
  headers: {cookie:'private-session', authorization:'Bearer private-token', 'x-wfsim-account':'private-account'},
});
const result = await clientRelease(incoming, async request => {asked=request; return new Response(bytes, {
  headers:{'set-cookie':'never-forward=1','access-control-allow-origin':'https://elsewhere.example'},
});});
check('the same-origin gateway returns the canonical manifest bytes, including unknown fields', result.status===200 && await result.text()===bytes);
check('only the fixed public COS pointer is requested, regardless of caller query', asked.url===canonical && asked.method==='GET');
check('reader cookies, authorization and account identity never travel upstream', !asked.headers.has('cookie') && !asked.headers.has('authorization') && !asked.headers.has('x-wfsim-account'));
check('redirects are refused and the upstream request has a bounded signal', asked.redirect==='error' && !!asked.signal);
check('the reply is JSON without upstream cookies or CORS dependence', result.headers.get('content-type').includes('application/json') && !result.headers.has('set-cookie') && !result.headers.has('access-control-allow-origin') && result.headers.get('cache-control')==='no-store');
for(const method of ['POST','PUT','DELETE','HEAD']) {
  let fetched=false;
  const r=await clientRelease(new Request('https://wfsim.app'+CLI_MANIFEST_PATH,{method}),async()=>{fetched=true;});
  check(method+' cannot use the read-only gateway',r.status===405&&!fetched);
}
for(const response of [new Response('denied',{status:403}),new Response('<html>bad</html>'),new Response('{}'),new Response('x'.repeat(65537))]) {
  const r=await clientRelease(incoming,async()=>response);
  check('an unavailable, invalid or oversized upstream fails explicitly',r.status===502&&(await r.json()).error==='cli_manifest_unavailable');
}
const failed=await clientRelease(incoming,async()=>{throw Error('private upstream details');});
check('network failures produce a sanitized gateway error',failed.status===502&&!(await failed.text()).includes('private'));
const {default: deployed} = await import('../worker/index.js');
const originalFetch = globalThis.fetch;
try {
  globalThis.fetch = async request => {asked=request; return new Response(bytes);};
  const env = {};
  Object.defineProperty(env,'HYPERDRIVE',{get(){throw Error('the public manifest cannot open a database');}});
  const routed = await deployed.fetch(incoming,env);
  check('the actual Worker dispatch serves this GET without databases or account access',routed.status===200&&await routed.text()===bytes&&asked.url===canonical);
} finally {globalThis.fetch=originalFetch;}
const ui=readFileSync('web/src/static/app/73-compute.js','utf8');
check('compute reads the manifest through the same-origin door',ui.includes('fetchJson("'+CLI_MANIFEST_PATH+'")'));
const worker=readFileSync('worker/index.js','utf8'),native=readFileSync('web/src/main.rs','utf8');
check('the deployed door is dispatched before database setup',worker.indexOf('pathname === CLI_MANIFEST_PATH) return clientRelease(request)')<worker.indexOf('const f = withDatabases(env)',worker.indexOf('async fetch(request, env, ctx)')) && worker.includes('pathname === CLI_MANIFEST_PATH) return clientRelease(request)'));
check('native dev explicitly registers the same GET door',native.includes('("GET", "'+CLI_MANIFEST_PATH+'") => cli_release_response(&mut stream)')&&native.includes(canonical));
check('native curl ignores user configuration before applying its fixed arguments',/Command::new\("curl"\)\.args\(\[\s*"-q"/.test(native));
const workflow=readFileSync('.github/workflows/client-release.yml','utf8'),publish=workflow.slice(workflow.indexOf('\n  publish:'));
check('normal publication needs no bucket CORS management',!publish.includes('scripts/client_cors.py')&&publish.includes('python3 scripts/release_client.py target/client-release'));
check('normal publication retains main, release environment, qualified CI and OIDC',publish.includes("!inputs.cors_only && github.ref == 'refs/heads/main'")&&publish.includes('environment: release')&&publish.includes('id-token: write')&&publish.includes('node scripts/ci_runs.mjs ship "$EXPECTED_SHA"'));
check('npm availability is conditional on the release manifest flag',readFileSync('client/README.md','utf8').includes('`npm: true`'));
console.log(`CLI manifest gateway: ${checks} checks passed.`);
