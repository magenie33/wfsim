// SPDX-License-Identifier: AGPL-3.0-or-later
// Integration fixtures explicitly run the independent stage before inspecting its outputs.
import assert from 'node:assert/strict';
import {admitBuildWork} from '../../worker/build_admission.js';

export function admissionFixtureDb(query) {
  return {prepare:text=>{
    const statement=args=>({bind:(...values)=>statement(values),first:async()=>(await query(text,args))[0]||null,
      run:async()=>{await query(text,args);}});
    return statement([]);
  }};
}
export async function drainBuildAdmission(db,now=Date.now()) {
  for(let turn=0;turn<500;turn++) {
    const row=await db.prepare('SELECT count(*) n FROM state_build_admission_work').first();
    if(Number(row.n)===0)return;
    await admitBuildWork({LIBRARY:db},now,{max:128,budget_ms:5000});
  }
  assert.fail('fixture admission did not reach its durable handoff');
}
