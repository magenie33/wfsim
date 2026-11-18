-- Each writer owns its zone counters. A queue change and its small signed
-- difference share a commit. Sampling and compaction never lock queue rows.
CREATE TABLE IF NOT EXISTS state_queue_zone_changes (
  epoch text COLLATE "C" NOT NULL, writer integer NOT NULL, zone smallint NOT NULL CHECK(zone BETWEEN 0 AND 99),
  n bigint NOT NULL, PRIMARY KEY(epoch,writer,zone)
);
CREATE TABLE IF NOT EXISTS state_queue_zone_seed (
  epoch text COLLATE "C" PRIMARY KEY, zones jsonb NOT NULL, sampled_at timestamptz NOT NULL
);

CREATE OR REPLACE FUNCTION queue_count_zone(s state_compute_scheduling,c entity_compute_order_cuts,ignore_cancellation boolean DEFAULT false)
RETURNS smallint LANGUAGE sql STABLE SET search_path FROM CURRENT AS $$
  SELECT CASE WHEN s.state='open' AND (c.id IS NULL OR (
    (s.task_id>c.task_cutoff OR (coalesce(s.zone,factory_default_zone(s.producer)),coalesce(s.zone_position,s.task_id),s.id)
      <=(c.keep_zone,c.keep_position,c.keep_id) OR EXISTS(
      SELECT 1 FROM relation_compute_order_cut_protection p WHERE p.cut_id=c.id AND p.question=s.id))
    AND (ignore_cancellation OR NOT EXISTS(SELECT 1 FROM record_compute_order_cancellations r
      WHERE r.question=s.id AND r.task_id=s.task_id AND r.cut_id=c.id))))
  THEN coalesce(s.zone,factory_default_zone(s.producer))::smallint END
$$;

CREATE OR REPLACE FUNCTION queue_count_change() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE c entity_compute_order_cuts; epoch_at text; entry state_compute_scheduling; previous smallint; following smallint;
BEGIN
  IF TG_TABLE_NAME='state_compute_scheduling' AND TG_OP='UPDATE' THEN
    IF (OLD.state,OLD.id,OLD.task_id,coalesce(OLD.zone,factory_default_zone(OLD.producer)),coalesce(OLD.zone_position,OLD.task_id))
      IS NOT DISTINCT FROM
       (NEW.state,NEW.id,NEW.task_id,coalesce(NEW.zone,factory_default_zone(NEW.producer)),coalesce(NEW.zone_position,NEW.task_id))
    THEN RETURN NULL; END IF;
  END IF;
  PERFORM compute_order_cut_lock();
  SELECT * INTO c FROM compute_order_cut();
  epoch_at:=coalesce(compute_order_cut_epoch(),'');
  IF TG_TABLE_NAME='state_compute_scheduling' THEN
    IF TG_OP<>'INSERT' THEN previous:=queue_count_zone(OLD,c); END IF;
    IF TG_OP<>'DELETE' THEN following:=queue_count_zone(NEW,c); END IF;
  ELSE
    IF TG_OP<>'INSERT' AND OLD.cut_id=c.id THEN
      SELECT * INTO entry FROM state_compute_scheduling WHERE id=OLD.question AND task_id=OLD.task_id FOR UPDATE;
      following:=queue_count_zone(entry,c,true);
    END IF;
    IF TG_OP<>'DELETE' AND NEW.cut_id=c.id THEN
      SELECT * INTO entry FROM state_compute_scheduling WHERE id=NEW.question AND task_id=NEW.task_id FOR UPDATE;
      previous:=queue_count_zone(entry,c,true);
    END IF;
  END IF;
  IF previous IS DISTINCT FROM following THEN
    INSERT INTO state_queue_zone_changes(epoch,writer,zone,n)
      SELECT epoch_at,pg_backend_pid(),d.zone,d.n FROM (VALUES(previous,-1),(following,1)) d(zone,n) WHERE d.zone IS NOT NULL
      ON CONFLICT(epoch,writer,zone) DO UPDATE SET n=state_queue_zone_changes.n+excluded.n;
  END IF;
  RETURN NULL;
END $$;

CREATE OR REPLACE FUNCTION queue_count_reset() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  DELETE FROM state_queue_zone_seed;
  RETURN NULL;
END $$;

CREATE OR REPLACE TRIGGER queue_count_membership AFTER INSERT OR UPDATE OR DELETE ON state_compute_scheduling FOR EACH ROW EXECUTE FUNCTION queue_count_change();
CREATE OR REPLACE TRIGGER queue_count_cancel AFTER INSERT OR UPDATE OR DELETE ON record_compute_order_cancellations
  FOR EACH ROW EXECUTE FUNCTION queue_count_change();
CREATE OR REPLACE TRIGGER queue_count_truncated AFTER TRUNCATE ON state_compute_scheduling
  FOR EACH STATEMENT EXECUTE FUNCTION queue_count_reset();
CREATE OR REPLACE TRIGGER queue_cancel_truncated AFTER TRUNCATE ON record_compute_order_cancellations
  FOR EACH STATEMENT EXECUTE FUNCTION queue_count_reset();
REVOKE ALL ON FUNCTION queue_count_zone(state_compute_scheduling,entity_compute_order_cuts,boolean),queue_count_change(),queue_count_reset() FROM PUBLIC;
CREATE OR REPLACE TRIGGER queue_protection_truncated AFTER TRUNCATE ON relation_compute_order_cut_protection
  FOR EACH STATEMENT EXECUTE FUNCTION queue_count_reset();
CREATE OR REPLACE TRIGGER queue_activation_truncated AFTER TRUNCATE ON record_compute_order_cut_activations
  FOR EACH STATEMENT EXECUTE FUNCTION queue_count_reset();

-- Only a disconnected writer's stripes are folded into writer zero.
CREATE OR REPLACE FUNCTION queue_count_fold() RETURNS void
LANGUAGE sql VOLATILE SECURITY DEFINER SET search_path FROM CURRENT AS $$
WITH page AS MATERIALIZED (
    SELECT epoch,writer,zone FROM state_queue_zone_changes c WHERE writer<>0
      AND NOT EXISTS(SELECT 1 FROM pg_stat_activity a WHERE a.pid=c.writer)
      ORDER BY epoch,writer,zone LIMIT 128 FOR UPDATE SKIP LOCKED
  ), removed AS (DELETE FROM state_queue_zone_changes c USING page p
    WHERE (c.epoch,c.writer,c.zone)=(p.epoch,p.writer,p.zone) RETURNING c.epoch,c.zone,c.n)
  INSERT INTO state_queue_zone_changes(epoch,writer,zone,n)
    SELECT epoch,0,zone,sum(n)::bigint FROM removed GROUP BY epoch,zone HAVING sum(n)<>0
    ON CONFLICT(epoch,writer,zone) DO UPDATE SET n=state_queue_zone_changes.n+excluded.n
$$;
REVOKE ALL ON FUNCTION queue_count_fold() FROM PUBLIC;
