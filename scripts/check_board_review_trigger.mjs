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

// Completion delivers submit and review from the frozen final replay, off-page.
const reviewDefinition=text.match(/async function requestOptimizationReviews\([^]*?\n\}/)?.[0] || '';
const completion=text.match(/  async completed\(task\) \{[^]*?\n  \},/)[0];
const request={weapon:'laetum',mods:['serration'],mode:'cycle',runs:1000,seed:123,duration:180};
for(const cancelled of [false,true]) {
  const calls=[],types=new Map(),record={id:'artifact',result:{ok:true,cancelled,final_runs:1000,
    results:[{rank:1,replay:request,kill_progress:12,dps:34},{rank:2,simulator_error:'refused',replay:request,kill_progress:99},
      {rank:3,replay:{...request,runs:10},kill_progress:99},{rank:4,replay:request,rough:true,kill_progress:99}]}};
  const complete=runInNewContext(reviewDefinition+"\n({"+completion+"}).completed",{
    USER_TASK_TYPES:types,window:{addEventListener(){}},structuredClone,taskClone:structuredClone,
    userTasks:{store:{value:async()=>record}},offerOptBoardSubmit:async()=>calls.push({kind:'submit'}),
    optimizationTaskBuildSource:async()=>({kind:'public'}),boardReviewEnabled:()=>true,
    requestBoardReview:async(task,result,source)=>calls.push({kind:'review',task,result,source})
  });
  await complete({id:'search',kind:'optimization',engine:'e',result_ref:'artifact',origin:{board:{},fight:{},scenario:'standard_single_target'}});
  assert.equal(calls.filter(c=>c.kind==='submit').length,1);
  const reviews=calls.filter(c=>c.kind==='review');
  assert.equal(reviews.length,cancelled?0:1,'only completed valid finalists reuse review');
  if(!cancelled) {
    assert.deepEqual(reviews[0].task.request,request);
    assert.equal(reviews[0].result.score,12);
    assert.equal(reviews[0].result.duration,180);
    assert.ok(reviews[0].source.id.includes('artifact'),'review delivery retains a stable completion source');
  }
}
const precision=text.match(/function optimizationFinalRuns\(\) \{[^]*?\n\}/)?.[0];
assert.ok(precision,'official optimizer final precision has one resolver');
for(const builtin of ['standard_single_target',null]) {
  const runs=runInNewContext(precision+'\noptimizationFinalRuns()',{
    activeScenario:'selected',scenarioNamed:()=>({builtin}),benchList:()=>[{id:'standard_single_target',scenario:{runs:1000}}],simRuns:()=>7
  });
  assert.equal(runs,builtin?1000:7);
}
console.log('review triggers: intermediate submit, final completion review, cancellation and official final precision pass');
