// SPDX-License-Identifier: AGPL-3.0-or-later
// The work signal reads effective open membership without requests, proofs or full queue counts.
const columns='id,verb,task_id,state,waits,lease,lease_until,leased_to,leased_at,producer,initiator';
const projection=alias=>columns.split(',').map(name=>alias+'.'+name).join(',');
export const statisticsNotCancelled=alias=>`NOT EXISTS(SELECT 1 FROM stats_cancelled r WHERE r.question=${alias}.id AND r.task_id=${alias}.task_id)`;
export const STATISTICS_OPEN = `stats_cut AS MATERIALIZED(SELECT * FROM compute_order_cut()),
 stats_cancelled AS MATERIALIZED(
  SELECT r.question,r.task_id FROM record_compute_order_cancellations r JOIN stats_cut c ON c.id=r.cut_id
 ),
 stats_open AS NOT MATERIALIZED(
  SELECT ${projection('q')} FROM state_compute_effective_open q
  WHERE ${statisticsNotCancelled('q')}
 )`;

