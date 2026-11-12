// SPDX-License-Identifier: AGPL-3.0-or-later
// Delivery advances a captured workflow; derived output does not start another fanout.
import { iso } from './instant.js';
import { BOARD_REVIEW_PRODUCER } from './board_reviews.js';
import { scoreWitnessWrites } from './computation_records.js';
import { headlessBoardPayloadFromResult } from './board_payload.js';
import { userCalculationWork } from './user_tasks.js';

export function workflowWork(leased) {
  const payload=leased.demands[0].payload;
  if(payload.node_kind==='optimize') return userCalculationWork(leased);
  return {question:leased.question,lease:leased.lease,identity:payload.identity,record:payload.record,
    ruler:payload.ruler,mode:payload.mode,fight:leased.request};
}
const linkedReview=d=>({...d,ref:d.payload.review_ref});
export const BUILD_WORKFLOW_PRODUCER={
  wanted_in_sql:true,
  still_wanted:async(env,d)=>!!await env.LIBRARY.prepare(`SELECT 1 FROM state_build_workflow_nodes n JOIN state_build_workflows f ON f.id=n.workflow
    WHERE n.workflow=? AND n.id=? AND n.computation=? AND n.state='queued' AND f.state='active'`)
    .bind(d.payload.workflow,d.payload.node,d.question).first(),
  on_fact_writes(env,d,fact) {
    const p=d.payload,db=env.LIBRARY,at=iso(Date.now()),writes=[
      db.prepare('SELECT build_workflow_delivery_guard(?::text,?::text,?::text,?::bigint)').bind(p.workflow,p.node,d.question,fact.task_id)];
    if(p.node_kind==='optimize') {
      const best=fact.result?.report?.results?.[0];
      if(!best) writes.push(db.prepare("UPDATE state_build_workflow_nodes SET state='failed',error='optimization returned no build' WHERE workflow=? AND id=? AND state='queued'").bind(p.workflow,p.node));
      else {
        const record=headlessBoardPayloadFromResult(best,p.context);
        const id='workflow:'+p.workflow+':'+p.node;
        writes.push(db.prepare(`INSERT INTO record_build_submissions(id,at,record,initiator,family_source)
          SELECT ?1::text,?2::text,?3::text,'system',?4::text WHERE EXISTS(SELECT 1 FROM state_build_workflow_nodes n JOIN state_build_workflows f ON f.id=n.workflow
            WHERE n.workflow=?5::text AND n.id=?6::text AND n.state='queued' AND n.task_id=?7::bigint AND f.state='active') ON CONFLICT DO NOTHING`)
          .bind(id,at,JSON.stringify(record),'workflow|'+p.workflow+'|'+p.node,p.workflow,p.node,fact.task_id));
        writes.push(db.prepare("UPDATE state_build_workflow_nodes SET state='awaiting_intake',result_id=? WHERE workflow=? AND id=? AND state='queued' AND task_id=?")
          .bind(fact.result_id||null,p.workflow,p.node,fact.task_id));
      }
    } else {
      if(p.review) writes.push(...BOARD_REVIEW_PRODUCER.on_fact_writes(env,linkedReview(d),fact));
      else {
        writes.push(db.prepare(`INSERT INTO state_board_measurement_selection(identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at)
          VALUES(?1::text,?2::text,?3::text,?4::text,?5::double precision,?6::text,0,?7::text,?7::text)
          ON CONFLICT(identity,ruler,mode) DO UPDATE SET measured_by=excluded.measured_by,score=excluded.score,metric=excluded.metric,
            cost_seconds=0,started_at=excluded.started_at,finished_at=excluded.finished_at,review_id=NULL
          WHERE state_board_measurement_selection.finished_at<=excluded.finished_at AND NOT EXISTS(
            SELECT 1 FROM entity_measurement_reviews r WHERE r.id=state_board_measurement_selection.review_id
              AND r.invalidated_at IS NOT NULL AND r.state<>'succeeded')`)
          .bind(p.identity,p.ruler,p.mode,'verified:'+fact.engine,fact.result.score,fact.result.metric,at));
        writes.push(...scoreWitnessWrites(db,p.identity,p.ruler,p.mode,at,fact));
      }
      writes.push(db.prepare("UPDATE state_build_workflow_nodes SET state='succeeded',result_id=?,delivered_at=? WHERE workflow=? AND id=? AND state='queued' AND task_id=?")
        .bind(fact.result_id||null,at,p.workflow,p.node,fact.task_id));
    }
    writes.push(db.prepare('SELECT build_workflow_finish(?::text,?::text)').bind(p.workflow,at));
    return writes;
  },
  on_retract:async(env,d)=>{
    await env.LIBRARY.prepare('SELECT build_workflow_repair(?::text,?::text,?::text,?::text)')
      .bind(d.payload.workflow,d.payload.node,d.question,iso(Date.now())).run();
  },
};
