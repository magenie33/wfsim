// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { loadEngine } from './wasm_engine.mjs';
import { randomBytes } from 'node:crypto';

const response=await fetch('https://wfsim.app/release.json',{
  headers:{'user-agent':'WFSim-CLI-release-check'},signal:AbortSignal.timeout(30_000),
});
assert(response.ok,`public release returned HTTP ${response.status}`);
const current=await response.json(),engine=loadEngine();
assert.equal(engine.release.engine,current.engine,'publish the matching calculator/native engine before the CLI');
const marker='__wfsim_cli_capability_probe__';
const probe=engine.api('/api/compute/optimize',{weapon:marker});
assert(probe.ok===false && typeof probe.error==='string' && probe.error.includes(marker),
  'publish the calculator/native engine with /api/compute/optimize before the CLI');
const claim=await fetch('https://wfsim.app/api/account/devices/link/poll',{
  method:'POST',headers:{'content-type':'application/json','user-agent':'WFSim-CLI-release-check'},
  body:JSON.stringify({device_code:randomBytes(32).toString('base64url')}),signal:AbortSignal.timeout(30_000),
});
assert(claim.ok,'deploy the public device-link route and account schema before the CLI');
const readiness=await claim.json();
assert(readiness.ok===true && readiness.status==='expired','device-link polling must be ready without registering or claiming a device');
console.log(`CLI: published engine ${current.engine} matches the checked artifact and supports the optimizer door.`);
