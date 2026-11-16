-- SPDX-License-Identifier: AGPL-3.0-or-later
-- Frozen read views from 501b4458354d9030c3018879378d21b4eeeb4314.
CREATE OR REPLACE FUNCTION compute_order_cut() RETURNS SETOF entity_compute_order_cuts
LANGUAGE sql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
  SELECT c.* FROM entity_compute_order_cuts c JOIN record_compute_order_cut_activations a ON a.cut_id=c.id
$$;

CREATE OR REPLACE VIEW state_compute_effective_open AS
  SELECT s.* FROM state_compute_scheduling s WHERE s.state='open' AND NOT EXISTS(SELECT 1 FROM compute_order_cut())
  UNION ALL
  SELECT s.* FROM state_compute_scheduling s WHERE s.state='open' AND EXISTS(SELECT 1 FROM compute_order_cut())
    AND (coalesce(s.zone,factory_default_zone(s.producer)),coalesce(s.zone_position,s.task_id),s.id)
      <=(SELECT c.keep_zone,c.keep_position,c.keep_id FROM compute_order_cut() c)
  UNION ALL
  SELECT s.* FROM entity_compute_tasks t JOIN state_compute_scheduling s ON s.id=t.computation AND s.task_id=t.id AND s.state='open'
    WHERE t.id>(SELECT task_cutoff FROM compute_order_cut())
      AND (coalesce(s.zone,factory_default_zone(s.producer)),coalesce(s.zone_position,s.task_id),s.id)
        >(SELECT c.keep_zone,c.keep_position,c.keep_id FROM compute_order_cut() c)
  UNION ALL
  SELECT s.* FROM relation_compute_order_cut_protection p JOIN state_compute_scheduling s ON s.id=p.question AND s.state='open'
    WHERE p.cut_id=(SELECT id FROM compute_order_cut()) AND s.task_id<=(SELECT task_cutoff FROM compute_order_cut())
      AND (coalesce(s.zone,factory_default_zone(s.producer)),coalesce(s.zone_position,s.task_id),s.id)
        >(SELECT c.keep_zone,c.keep_position,c.keep_id FROM compute_order_cut() c);

CREATE OR REPLACE VIEW state_compute_effective_work AS
  SELECT w.* FROM state_compute_effective_open live JOIN state_compute_work w ON w.id=live.id
  UNION ALL SELECT w.* FROM state_compute_work w WHERE w.state<'open'
  UNION ALL SELECT w.* FROM state_compute_work w WHERE w.state>'open';

CREATE OR REPLACE VIEW state_board_effective_pending_work AS
  WITH cut AS MATERIALIZED(SELECT * FROM compute_order_cut())
  SELECT q.* FROM state_board_pending_work q WHERE NOT EXISTS(SELECT 1 FROM cut)
  UNION ALL
  SELECT DISTINCT b.* FROM cut CROSS JOIN state_compute_effective_work q
    JOIN relation_task_consumers d ON d.question=q.id AND d.state='live'
      AND d.producer IN('new_build','survey_build','refresh','sweep')
    JOIN state_board_pending_work b ON (b.build_id,b.ruler,b.mode)=
      (split_part(d.ref,'|',1),split_part(d.ref,'|',2),split_part(d.ref,'|',3))
  UNION ALL
  SELECT q.* FROM cut c JOIN entity_compute_batches batch ON batch.at>c.cut_at
    JOIN state_board_pending_work q ON q.batch=batch.id
    WHERE NOT EXISTS(SELECT 1 FROM relation_task_consumers d JOIN state_compute_work w ON w.id=d.question
      WHERE d.state='live' AND d.producer IN('new_build','survey_build','refresh','sweep')
        AND d.ref=q.build_id||'|'||q.ruler||'|'||q.mode);
