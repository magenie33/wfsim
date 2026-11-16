CREATE OR REPLACE VIEW state_compute_effective_open AS
  SELECT s.* FROM state_compute_scheduling s WHERE s.state='open' AND NOT EXISTS(SELECT 1 FROM compute_order_cut())
  UNION ALL
  SELECT s.* FROM state_compute_scheduling s WHERE s.state='open' AND EXISTS(SELECT 1 FROM compute_order_cut())
    AND (coalesce(s.zone,factory_default_zone(s.producer)),coalesce(s.zone_position,s.task_id),s.id)
      <=(SELECT c.keep_zone,c.keep_position,c.keep_id FROM compute_order_cut() c)
  UNION ALL
  SELECT s.* FROM entity_compute_tasks t CROSS JOIN LATERAL (
    SELECT s.* FROM state_compute_scheduling s WHERE s.id=t.computation AND s.task_id=t.id AND s.state='open' OFFSET 0
  ) s
    WHERE t.id>(SELECT task_cutoff FROM compute_order_cut())
      AND (coalesce(s.zone,factory_default_zone(s.producer)),coalesce(s.zone_position,s.task_id),s.id)
        >(SELECT c.keep_zone,c.keep_position,c.keep_id FROM compute_order_cut() c)
  UNION ALL
  SELECT s.* FROM relation_compute_order_cut_protection p CROSS JOIN LATERAL (
    SELECT s.* FROM state_compute_scheduling s WHERE s.id=p.question AND s.state='open' OFFSET 0
  ) s
    WHERE p.cut_id=(SELECT id FROM compute_order_cut()) AND s.task_id<=(SELECT task_cutoff FROM compute_order_cut())
      AND (coalesce(s.zone,factory_default_zone(s.producer)),coalesce(s.zone_position,s.task_id),s.id)
        >(SELECT c.keep_zone,c.keep_position,c.keep_id FROM compute_order_cut() c);

CREATE OR REPLACE VIEW state_compute_effective_work AS
  SELECT live.id,w.verb,w.request,live.state,w.due_at,live.waits,w.opened_at,w.lanes,w.lease,live.lease_until,
    w.leased_to,w.leased_at,w.lease_prev,w.leased_to_prev,w.lease_prev_until,w.needs,w.canon,w.fact_engine,w.fact_at,
    w.odd,w.withdrawn_at,w.witnesses,w.producer,w.initiator,w.task_id,w.result_id,w.zone,w.zone_position,
    w.required_capability,w.required_engine
  FROM state_compute_effective_open live CROSS JOIN LATERAL (
    SELECT w.* FROM state_compute_work w WHERE w.id=live.id OFFSET 0
  ) w
  UNION ALL SELECT w.* FROM state_compute_work w WHERE w.state<'open'
  UNION ALL SELECT w.* FROM state_compute_work w WHERE w.state>'open';

CREATE OR REPLACE VIEW state_board_effective_pending_work AS
  WITH cut AS MATERIALIZED(SELECT * FROM compute_order_cut()),
  current_ids AS (
    SELECT id FROM state_compute_effective_open
    UNION ALL SELECT id FROM state_compute_scheduling WHERE state<'open'
    UNION ALL SELECT id FROM state_compute_scheduling WHERE state>'open'
  ),
  live_keys AS (
    SELECT DISTINCT k.build_id,k.ruler,k.mode FROM cut CROSS JOIN current_ids q
    CROSS JOIN LATERAL (
      SELECT d.question,b.build_id,b.ruler,b.mode FROM relation_task_consumers d
      JOIN state_board_pending_work b ON (b.build_id,b.ruler,b.mode)=
        (split_part(d.ref,'|',1),split_part(d.ref,'|',2),split_part(d.ref,'|',3))
      WHERE d.question=q.id AND d.state='live' AND d.producer IN('new_build','survey_build','refresh','sweep') OFFSET 0
    ) k
    CROSS JOIN LATERAL (
      SELECT 1 FROM state_compute_work w WHERE w.id=k.question OFFSET 0
    ) live
  )
  SELECT q.* FROM state_board_pending_work q WHERE NOT EXISTS(SELECT 1 FROM cut)
  UNION ALL
  SELECT b.* FROM live_keys k JOIN state_board_pending_work b USING(build_id,ruler,mode)
  UNION ALL
  SELECT q.* FROM cut c JOIN entity_compute_batches batch ON batch.at>c.cut_at
    CROSS JOIN LATERAL (SELECT q.* FROM state_board_pending_work q WHERE q.batch=batch.id OFFSET 0) q
    WHERE NOT EXISTS(SELECT 1 FROM relation_task_consumers d CROSS JOIN LATERAL (
        SELECT 1 FROM state_compute_work w WHERE w.id=d.question OFFSET 0
      ) live
      WHERE d.state='live' AND d.producer IN('new_build','survey_build','refresh','sweep')
        AND d.ref=q.build_id||'|'||q.ruler||'|'||q.mode OFFSET 0);

CREATE OR REPLACE FUNCTION board_owed(text, text, text) RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT EXISTS (SELECT 1 FROM state_board_effective_pending_work WHERE build_id = $1 AND ruler = $2 AND mode = $3)
$$;
