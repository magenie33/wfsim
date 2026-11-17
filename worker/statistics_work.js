// SPDX-License-Identifier: AGPL-3.0-or-later
// Statistical reads project membership without loading requests, results or proofs.
const columns='id,verb,task_id,state,waits,lease,lease_until,leased_to,leased_at,producer,initiator';
const projection=alias=>columns.split(',').map(name=>alias+'.'+name).join(',');
export const statisticsNotCancelled=alias=>`NOT EXISTS(SELECT 1 FROM stats_cancelled r WHERE r.question=${alias}.id AND r.task_id=${alias}.task_id)`;
const nonopen=`q.state<>'open' AND ${statisticsNotCancelled('q')}
 AND(q.state<>'withdrawn' OR NOT EXISTS(SELECT 1 FROM stats_cut)
  OR compute_order_cancelled(q.id,q.task_id,coalesce(q.zone,factory_default_zone(q.producer)),coalesce(q.zone_position,q.task_id),q.state) IS NULL)`;
const consumers=`JOIN relation_task_consumers d ON d.question=q.id AND d.state='live' AND d.producer IN('new_build','survey_build','refresh','sweep')`;

export const STATISTICS_WORK = `stats_cut AS MATERIALIZED(SELECT * FROM compute_order_cut()),
 stats_cancelled AS MATERIALIZED(
  SELECT r.question,r.task_id FROM record_compute_order_cancellations r JOIN stats_cut c ON c.id=r.cut_id
 ),
 stats_open AS NOT MATERIALIZED(
  SELECT ${projection('q')} FROM state_compute_effective_open q
  WHERE ${statisticsNotCancelled('q')}
 ),
 stats_work AS NOT MATERIALIZED(
  SELECT * FROM stats_open
  UNION ALL
  SELECT ${projection('q')} FROM state_compute_scheduling q WHERE ${nonopen}
 ),
 stats_open_refs AS NOT MATERIALIZED(
  SELECT d.ref FROM state_compute_effective_open q ${consumers}
   WHERE EXISTS(SELECT 1 FROM stats_cut) AND ${statisticsNotCancelled('q')} OFFSET 0
 ),
 stats_other_refs AS NOT MATERIALIZED(
  SELECT d.ref FROM state_compute_scheduling q ${consumers}
   WHERE EXISTS(SELECT 1 FROM stats_cut) AND ${nonopen} OFFSET 0
 ),
 stats_pending AS NOT MATERIALIZED(
  SELECT q.* FROM state_board_pending_work q WHERE NOT EXISTS(SELECT 1 FROM stats_cut)
  UNION ALL
  SELECT b.* FROM state_board_pending_work b WHERE EXISTS(SELECT 1 FROM stats_cut)
   AND EXISTS(SELECT 1 FROM(SELECT ref FROM stats_open_refs UNION ALL SELECT ref FROM stats_other_refs) d
    WHERE (b.build_id,b.ruler,b.mode)=(split_part(d.ref,'|',1),split_part(d.ref,'|',2),split_part(d.ref,'|',3)))
  UNION ALL
  SELECT q.* FROM stats_cut c JOIN entity_compute_batches batch ON batch.at>c.cut_at
  JOIN state_board_pending_work q ON q.batch=batch.id
  WHERE NOT EXISTS(SELECT 1 FROM relation_task_consumers d CROSS JOIN LATERAL(
   SELECT 1 FROM state_compute_work w WHERE w.id=d.question OFFSET 0) live
   WHERE d.state='live' AND d.producer IN('new_build','survey_build','refresh','sweep')
    AND d.ref=q.build_id||'|'||q.ruler||'|'||q.mode OFFSET 0)
 )`;
