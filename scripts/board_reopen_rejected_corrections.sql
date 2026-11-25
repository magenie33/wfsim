-- SPDX-License-Identifier: AGPL-3.0-or-later
-- Operator-approved Native-validated source identities only; one bounded batch.
-- Run in a transaction with a short lock and statement timeout.
-- The caller supplies a temporary table correction_recovery_input(id text, input_sha256 text).
LOCK TABLE entity_measurement_reviews IN SHARE ROW EXCLUSIVE MODE;
WITH eligible AS MATERIALIZED (
  SELECT r.id AS review, q.id AS request
  FROM pg_temp.correction_recovery_input a
  JOIN entity_board_requests e ON e.id=a.id
  JOIN state_board_requests q ON q.id=e.id
  JOIN entity_measurement_reviews r ON r.id=(e.input->>'observation')::jsonb->>'review'
  JOIN state_board_measurement_selection s ON (s.identity,s.ruler,s.mode)=(r.identity,r.ruler,r.mode)
  WHERE e.kind='correction' AND e.id='review:'||r.id
    AND a.input_sha256=encode(sha256(convert_to(e.input::text,'UTF8')),'hex')
    AND q.phase='rejected' AND q.error='observation does not match the official calculation'
    AND q.intent IS NULL AND q.workflow IS NULL AND q.lease IS NULL AND q.lease_until IS NULL
    AND r.state='failed' AND r.task_id IS NULL AND r.workflow IS NULL
    AND r.invalidated_at IS NULL AND r.result_engine IS NULL AND r.completed_at IS NOT NULL
    AND e.ruler=r.ruler AND e.mode=r.mode
    AND s.score=r.baseline_score AND s.metric=r.baseline_metric
    AND s.measured_by=r.baseline_by AND s.finished_at=r.baseline_finished_at
    AND s.review_id IS NOT DISTINCT FROM r.baseline_review_id
    AND NOT EXISTS(SELECT 1 FROM entity_measurement_reviews newer
      WHERE newer.ref=r.ref AND newer.id<>r.id
      AND (newer.state='waiting' OR (newer.created_at,newer.id)>(r.created_at,r.id)))
  ORDER BY q.position
  LIMIT 32
  FOR UPDATE OF q,r,s
), reviews AS (
  UPDATE entity_measurement_reviews r SET state='waiting',completed_at=NULL
  FROM eligible e WHERE r.id=e.review RETURNING r.id
)
UPDATE state_board_requests q SET phase='pending',
  retry_at=board_input_timestamp(clock_timestamp()),error=NULL
FROM eligible e JOIN reviews r ON r.id=e.review WHERE q.id=e.request
RETURNING q.id;
