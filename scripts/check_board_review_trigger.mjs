// SPDX-License-Identifier: AGPL-3.0-or-later
// Equal local measurements do not create reviews merely because provenance differs.
import assert from 'node:assert/strict';
import { runInNewContext } from 'node:vm';
import { clientSource } from './ci_client_inputs.mjs';

const text=clientSource('scripts/check_board_review_trigger.mjs');
let fn=text.slice(text.indexOf('async function requestBoardReview('),text.indexOf('async function acceptBoardReview('));
if(process.argv.includes('--break-version-guard')) fn=fn.replace('if (Math.abs(measured - old)', 'if (task.engine === matched.row.measured_by && Math.abs(measured - old)');
for(const delta of [0,1e-13,1]) {
  const trackers=new Map(),ruler='standard_single_target';
  const matched={identity:'a'.repeat(32),weapon:'laetum',ruler,mode:'cycle',row:{score:10,measured_by:'old-engine'}};
  const context={structuredClone,matchingBoardReview:async()=>matched,metricValue:(_,result)=>result.score,metricOf:x=>({id:x}),
    benchList:()=>[{id:ruler,scenario:{metric:'kpm'}}],boardReviewTrackers:trackers,
    saveBoardReviewTrackers:async()=>{},renderBoardReviewNote(){},scheduleUserTasks(){},scheduleBoardReviews(){}};
  const run=runInNewContext(fn+'requestBoardReview',context);
  await run({engine:'e2',request:{weapon:'laetum'}},{score:10+delta},{id:'local'});
  assert.equal(trackers.size,delta===1?1:0);
}
