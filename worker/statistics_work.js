// SPDX-License-Identifier: AGPL-3.0-or-later
// Statistical reads project membership without loading requests, results or proofs.
const columns='id,verb,task_id,state,waits,lease,lease_until,leased_to,leased_at,producer,initiator';
const projection=alias=>columns.split(',').map(name=>alias+'.'+name).join(',');
export const statisticsNotCancelled=alias=>`NOT EXISTS(SELECT 1 FROM stats_cancelled r WHERE r.question=${alias}.id AND r.task_id=${alias}.task_id)`;
const nonopen=`q.state<>'open' AND ${statisticsNotCancelled('q')}
 AND(q.state<>'withdrawn' OR NOT EXISTS(SELECT 1 FROM stats_cut)
  OR compute_order_cancelled(q.id,q.task_id,coalesce(q.zone,factory_default_zone(q.producer)),coalesce(q.zone_position,q.task_id),q.state) IS NULL)`;

export const STATISTICS_OPEN = `stats_cut AS MATERIALIZED(SELECT * FROM compute_order_cut()),
 stats_cancelled AS MATERIALIZED(
  SELECT r.question,r.task_id FROM record_compute_order_cancellations r JOIN stats_cut c ON c.id=r.cut_id
 ),
 stats_open AS NOT MATERIALIZED(
  SELECT ${projection('q')} FROM state_compute_effective_open q
  WHERE ${statisticsNotCancelled('q')}
 )`;

export const STATISTICS_WORK = `${STATISTICS_OPEN},
 stats_work AS NOT MATERIALIZED(
  SELECT * FROM stats_open
  UNION ALL
  SELECT ${projection('q')} FROM state_compute_scheduling q WHERE ${nonopen}
 ),
 stats_live_refs AS NOT MATERIALIZED(
  SELECT q.id,d.ref,d.producer FROM stats_work q CROSS JOIN LATERAL(
   SELECT d.ref,d.producer FROM relation_task_consumers d WHERE d.question=q.id
    AND d.state='live' AND d.producer IN('new_build','survey_build','refresh','sweep') OFFSET 0) d
   WHERE EXISTS(SELECT 1 FROM stats_cut)
 ),
 stats_pending AS NOT MATERIALIZED(
  SELECT q.* FROM state_board_pending_work q WHERE NOT EXISTS(SELECT 1 FROM stats_cut)
  UNION ALL
  SELECT b.* FROM stats_live_refs d CROSS JOIN LATERAL(
   SELECT b.* FROM state_board_pending_work b
   WHERE (b.build_id,b.ruler,b.mode)=(split_part(d.ref,'|',1),split_part(d.ref,'|',2),split_part(d.ref,'|',3)) OFFSET 0) b
  -- The first effective (ref,producer) emits a key once; C-collated suffixes use its indexed range.
  WHERE NOT EXISTS(SELECT 1 FROM(
   SELECT e.question,e.ref,e.producer,e.state FROM relation_task_consumers e WHERE e.ref=b.build_id||'|'||b.ruler||'|'||b.mode
   UNION ALL
   SELECT e.question,e.ref,e.producer,e.state FROM relation_task_consumers e WHERE b.mode='' AND e.ref=b.build_id||'|'||b.ruler
   UNION ALL
   SELECT e.question,e.ref,e.producer,e.state FROM relation_task_consumers e WHERE b.mode='' AND b.ruler='' AND e.ref=b.build_id
   UNION ALL
   SELECT e.question,e.ref,e.producer,e.state FROM relation_task_consumers e
    WHERE d.ref>=b.build_id||'|'||b.ruler||'|'||b.mode||'|'
     AND e.ref>=b.build_id||'|'||b.ruler||'|'||b.mode||'|'
     AND e.ref<b.build_id||'|'||b.ruler||'|'||b.mode||'}'
   ) e WHERE (e.ref,e.producer)<(d.ref,d.producer)
    AND e.state='live' AND e.producer IN('new_build','survey_build','refresh','sweep')
    AND (e.question=d.id OR EXISTS(SELECT 1 FROM stats_work w WHERE w.id=e.question OFFSET 0)) OFFSET 0)
  UNION ALL
  SELECT q.* FROM stats_cut c JOIN entity_compute_batches batch ON batch.at>c.cut_at
  JOIN state_board_pending_work q ON q.batch=batch.id
  WHERE NOT EXISTS(SELECT 1 FROM relation_task_consumers d CROSS JOIN LATERAL(
   SELECT 1 FROM state_compute_work w WHERE w.id=d.question OFFSET 0) live
   WHERE d.state='live' AND d.producer IN('new_build','survey_build','refresh','sweep')
    AND d.ref=q.build_id||'|'||q.ruler||'|'||q.mode OFFSET 0)
 )`;
