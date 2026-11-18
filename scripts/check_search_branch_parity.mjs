// SPDX-License-Identifier: AGPL-3.0-or-later
// Compare certified nodejs bindgen packages; never enqueue or publish an appraisal.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {createHash} from 'node:crypto';
import {headlessRivenGainRequest} from '../mcp/headless.js';

const [oldDir,newDir,out] = process.argv.slice(2);
assert(oldDir && newDir,'give old and new nodejs WASM package directories');
const load = dir => createRequire(import.meta.url)(path.resolve(dir,'wfsim_wasm.js'));
const [old, next] = [oldDir,newDir].map(load);
const meta = JSON.parse(old.api('/api/meta','{}'));
const samples = [];
for (const [weapon,suffix,cards] of [
  ['braton_prime','single',['rime_rounds','thermite_rounds','high_voltage','malignant_force','serration','point_strike']],
  ['synoid_gammacor','multi',['frostbite','scorch','jolt','pistol_pestilence','hornet_strike','lethal_torrent']],
  ['torid','multi',['rime_rounds','thermite_rounds','high_voltage','malignant_force','split_chamber','vile_acceleration']],
]) for (const [label,stats,pin] of [
  ['two-elements',['heat','cold'],false], ['reversed-and-pinned',['cold','heat'],true],
  ['three-positive',['multishot','heat','critical_damage'],false],
]) {
  const id='riven:appraisal', card={bonuses:stats.map((id,i)=>({id,roll:[0.93,1.07,1.03][i]})),malus:{id:'zoom',roll:.97}};
  const req=headlessRivenGainRequest(meta,{weapon,ruler:`standard_${suffix}_target__aimed_half`,card}).request;
  const start=req.starts[0];
  if (pin) start.fixed[0].placement='slot';
  const mods=Object.fromEntries([...cards,id].map(m=>[m,m===id?'fixed':'search']));
  const body={...req,mods,arcanes:{none:'fixed'},build_size:4,build_min:4,threads:1,duration:6,candidate_runs:10,runs:12,finalists:4};
  const payload=JSON.stringify(body), route='/api/compute/optimize';
  const ref=old.api(route,payload); assert.equal(next.api(route,payload),ref,`${weapon}:${label}`);
  const r=JSON.parse(ref); assert(r.ok && !r.cut && r.results?.length && r.results.every(row=>row.mods.includes(id)),JSON.stringify(r).slice(0,400));
  if(pin) assert(r.results.every(row=>row.replay.mod_slots[0]===id));
  samples.push({weapon,label,branches:r.branches,searched:r.searched,jobs:r.jobs,final_work:r.final_work,sha256:createHash('sha256').update(ref).digest('hex')});
}
if(out) fs.writeFileSync(out,JSON.stringify(samples,null,2)+'\n');
console.log(`${samples.length} complete riven appraisals identical, including slot layouts, encounter totals, leaders, work and all replays`);
