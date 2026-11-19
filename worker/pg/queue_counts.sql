-- Each writer owns its zone counters. A queue change and its small signed
-- difference share a commit. Sampling and compaction never lock queue rows.
CREATE TABLE IF NOT EXISTS state_queue_zone_changes (
  epoch text COLLATE "C" NOT NULL, writer integer NOT NULL, zone smallint NOT NULL CHECK(zone BETWEEN 0 AND 99),
  n bigint NOT NULL, PRIMARY KEY(epoch,writer,zone)
);
CREATE TABLE IF NOT EXISTS state_queue_zone_seed (
  epoch text COLLATE "C" PRIMARY KEY, zones jsonb NOT NULL, sampled_at timestamptz NOT NULL
);

-- The empty verb identifies pre-classification balances still used by older readers.
ALTER TABLE state_queue_zone_changes ADD COLUMN IF NOT EXISTS verb text COLLATE "C" NOT NULL DEFAULT '';
ALTER TABLE state_queue_zone_changes ADD COLUMN IF NOT EXISTS completed bigint NOT NULL DEFAULT 0;
ALTER TABLE state_queue_zone_seed ADD COLUMN IF NOT EXISTS workload jsonb;
DO $$ BEGIN
  IF (SELECT cardinality(conkey) FROM pg_constraint WHERE conrelid='state_queue_zone_changes'::regclass AND contype='p')=3 THEN
    ALTER TABLE state_queue_zone_changes DROP CONSTRAINT state_queue_zone_changes_pkey,
      ADD PRIMARY KEY(epoch,writer,zone,verb);
  END IF;
END $$;
CREATE TABLE IF NOT EXISTS state_queue_clearance_days (
  day date PRIMARY KEY, epoch text COLLATE "C" NOT NULL, observed_at timestamptz NOT NULL, forecast jsonb NOT NULL
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
DECLARE c entity_compute_order_cuts; epoch_at text; entry state_compute_scheduling; previous smallint; following smallint; old_verb text; new_verb text; finished bigint:=0;
BEGIN
  IF TG_TABLE_NAME='state_compute_scheduling' AND TG_OP='UPDATE' THEN
    IF (OLD.state,OLD.id,OLD.task_id,OLD.verb,coalesce(OLD.zone,factory_default_zone(OLD.producer)),coalesce(OLD.zone_position,OLD.task_id))
      IS NOT DISTINCT FROM
       (NEW.state,NEW.id,NEW.task_id,NEW.verb,coalesce(NEW.zone,factory_default_zone(NEW.producer)),coalesce(NEW.zone_position,NEW.task_id))
    THEN RETURN NULL; END IF;
  END IF;
  PERFORM compute_order_cut_lock();
  SELECT * INTO c FROM compute_order_cut();
  epoch_at:=coalesce(compute_order_cut_epoch(),'');
  IF TG_TABLE_NAME='state_compute_scheduling' THEN
    IF TG_OP='UPDATE' AND OLD.state='open' AND NEW.state IN('fact','spot')
       AND NEW.task_id=OLD.task_id AND NEW.result_id IS NOT NULL THEN finished:=1; END IF;
    IF TG_OP<>'INSERT' THEN previous:=queue_count_zone(OLD,c); old_verb:=OLD.verb; END IF;
    IF TG_OP<>'DELETE' THEN following:=queue_count_zone(NEW,c); new_verb:=NEW.verb; END IF;
  ELSE
    IF TG_OP<>'INSERT' AND OLD.cut_id=c.id THEN
      SELECT * INTO entry FROM state_compute_scheduling WHERE id=OLD.question AND task_id=OLD.task_id FOR UPDATE;
      following:=queue_count_zone(entry,c,true); new_verb:=entry.verb;
    END IF;
    IF TG_OP<>'DELETE' AND NEW.cut_id=c.id THEN
      SELECT * INTO entry FROM state_compute_scheduling WHERE id=NEW.question AND task_id=NEW.task_id FOR UPDATE;
      previous:=queue_count_zone(entry,c,true); old_verb:=entry.verb;
    END IF;
  END IF;
  IF (previous,old_verb) IS DISTINCT FROM (following,new_verb) THEN
    INSERT INTO state_queue_zone_changes(epoch,writer,zone,verb,n,completed)
      SELECT epoch_at,pg_backend_pid(),zone,verb,n,done FROM
        (VALUES(previous,old_verb,-1,finished),(following,new_verb,1,0)) d(zone,verb,n,done) WHERE zone IS NOT NULL
      ON CONFLICT(epoch,writer,zone,verb) DO UPDATE SET n=state_queue_zone_changes.n+excluded.n,
        completed=state_queue_zone_changes.completed+excluded.completed;
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
    SELECT epoch,writer,zone,verb FROM state_queue_zone_changes c WHERE writer<>0
      AND NOT EXISTS(SELECT 1 FROM pg_stat_activity a WHERE a.pid=c.writer)
      ORDER BY epoch,writer,zone,verb LIMIT 128 FOR UPDATE SKIP LOCKED
  ), removed AS (DELETE FROM state_queue_zone_changes c USING page p
    WHERE (c.epoch,c.writer,c.zone,c.verb)=(p.epoch,p.writer,p.zone,p.verb)
    RETURNING c.epoch,c.zone,c.verb,c.n,c.completed)
  INSERT INTO state_queue_zone_changes(epoch,writer,zone,verb,n,completed)
    SELECT epoch,0,zone,verb,sum(n)::bigint,sum(completed)::bigint FROM removed GROUP BY epoch,zone,verb
    HAVING sum(n)<>0 OR sum(completed)<>0
    ON CONFLICT(epoch,writer,zone,verb) DO UPDATE SET n=state_queue_zone_changes.n+excluded.n,
      completed=state_queue_zone_changes.completed+excluded.completed
$$;
REVOKE ALL ON FUNCTION queue_count_fold() FROM PUBLIC;
