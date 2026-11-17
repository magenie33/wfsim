-- Derived statistics never admit work or replace canonical eligibility.
CREATE SEQUENCE IF NOT EXISTS statistics_revision;
CREATE TABLE IF NOT EXISTS state_statistics_generation (
  id bigint PRIMARY KEY CHECK(id=1), revision bigint NOT NULL DEFAULT 0,
  blocked text COLLATE "C"
);
INSERT INTO state_statistics_generation(id) VALUES(1) ON CONFLICT DO NOTHING;
CREATE TABLE IF NOT EXISTS state_statistics_journal (
  owner text COLLATE "C" NOT NULL,kind text COLLATE "C" NOT NULL, key text COLLATE "C" NOT NULL,
  revision bigint NOT NULL, PRIMARY KEY(owner,kind,key)
);
CREATE TABLE IF NOT EXISTS state_statistics_invalidations (
  id text COLLATE "C" PRIMARY KEY,reason text COLLATE "C" NOT NULL
);
CREATE TABLE IF NOT EXISTS state_statistics_namespaces (
  id text COLLATE "C" PRIMARY KEY, identity jsonb NOT NULL,
  phase bigint NOT NULL DEFAULT 0, cursor jsonb NOT NULL DEFAULT '[]',
  counts_at text COLLATE "C", published jsonb
);
CREATE TABLE IF NOT EXISTS state_statistics_head (
  id bigint PRIMARY KEY CHECK(id=1), namespace text COLLATE "C" NOT NULL
);
CREATE TABLE IF NOT EXISTS state_statistics_obligations (
  namespace text COLLATE "C" NOT NULL, kind text COLLATE "C" NOT NULL,
  key text COLLATE "C" NOT NULL,
  cursor jsonb NOT NULL DEFAULT '{}', PRIMARY KEY(namespace,kind,key)
);
CREATE TABLE IF NOT EXISTS state_statistics_checkpoints (
  namespace text COLLATE "C" NOT NULL,owner text COLLATE "C" NOT NULL, kind text COLLATE "C" NOT NULL,
  key text COLLATE "C" NOT NULL, revision bigint NOT NULL,
  PRIMARY KEY(namespace,owner,kind,key)
);
CREATE TABLE IF NOT EXISTS state_statistics_contributions (
  namespace text COLLATE "C" NOT NULL, kind text COLLATE "C" NOT NULL,
  key text COLLATE "C" NOT NULL, body jsonb NOT NULL,
  PRIMARY KEY(namespace,kind,key)
);
CREATE TABLE IF NOT EXISTS state_statistics_rollup (
  namespace text COLLATE "C" NOT NULL, key text COLLATE "C" NOT NULL,
  n bigint NOT NULL, PRIMARY KEY(namespace,key)
);
CREATE TABLE IF NOT EXISTS state_statistics_expiry (
  namespace text COLLATE "C" NOT NULL, question text COLLATE "C" NOT NULL,
  at text COLLATE "C" NOT NULL, PRIMARY KEY(namespace,question)
);
CREATE INDEX IF NOT EXISTS statistics_expiry_due ON state_statistics_expiry(namespace,at,question);

CREATE OR REPLACE FUNCTION statistics_invalidate(text DEFAULT NULL) RETURNS void
LANGUAGE sql VOLATILE SECURITY DEFINER SET search_path FROM CURRENT AS $$
  UPDATE state_statistics_generation SET revision=nextval('statistics_revision'),blocked=coalesce($1,blocked)
    WHERE id=1 AND (blocked IS NULL OR $1 IS NULL)
$$;
CREATE OR REPLACE FUNCTION statistics_touch(text,jsonb,text DEFAULT 'manual') RETURNS void
LANGUAGE plpgsql VOLATILE SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  IF (SELECT blocked IS NOT NULL FROM state_statistics_generation WHERE id=1) OR EXISTS(SELECT 1 FROM state_statistics_invalidations) THEN RETURN; END IF;
  IF octet_length($2::text)>1024 OR octet_length($3)>1024 OR pg_total_relation_size('state_statistics_journal')>268435456 THEN
    INSERT INTO state_statistics_invalidations(id,reason) VALUES(pg_current_xact_id()::text,'statistics capture storage budget') ON CONFLICT DO NOTHING; RETURN;
  END IF;
  INSERT INTO state_statistics_journal(owner,kind,key,revision) VALUES($3,$1,$2::text,nextval('statistics_revision'))
    ON CONFLICT(owner,kind,key) DO UPDATE SET revision=nextval('statistics_revision');
END $$;
CREATE OR REPLACE FUNCTION statistics_ref(text,text DEFAULT 'manual') RETURNS void
LANGUAGE plpgsql VOLATILE SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  PERFORM statistics_touch('k',jsonb_build_array(split_part($1,'|',1),split_part($1,'|',2),split_part($1,'|',3)),$2);
  PERFORM statistics_touch('r',jsonb_build_array($1),$2);
END $$;
CREATE OR REPLACE FUNCTION statistics_capture() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE prior jsonb; following jsonb; old_owner text; new_owner text;
BEGIN
  IF TG_OP='TRUNCATE' THEN PERFORM statistics_invalidate(); RETURN NULL; END IF;
  IF TG_TABLE_NAME IN('entity_compute_order_cuts','record_compute_order_cut_activations') THEN
    PERFORM statistics_invalidate(); RETURN NULL;
  ELSIF TG_TABLE_NAME='state_factory_findings' THEN
    IF TG_OP<>'INSERT' AND OLD.kind='task_inventory' THEN
      prior:=jsonb_build_array(OLD.body::jsonb->>'engine',OLD.body::jsonb->>'source',OLD.body::jsonb->>'snapshot'); END IF;
    IF TG_OP<>'DELETE' AND NEW.kind='task_inventory' THEN
      following:=jsonb_build_array(NEW.body::jsonb->>'engine',NEW.body::jsonb->>'source',NEW.body::jsonb->>'snapshot'); END IF;
    IF prior IS DISTINCT FROM following THEN PERFORM statistics_invalidate(); END IF;
  ELSIF TG_TABLE_NAME='relation_task_consumers' THEN
    IF TG_OP<>'INSERT' THEN
      old_owner:=TG_TABLE_NAME||':'||jsonb_build_array(OLD.producer,OLD.ref)::text;
      PERFORM statistics_touch('q',jsonb_build_array(OLD.question),old_owner); PERFORM statistics_ref(OLD.ref,old_owner);
    END IF;
    IF TG_OP<>'DELETE' THEN
      new_owner:=TG_TABLE_NAME||':'||jsonb_build_array(NEW.producer,NEW.ref)::text;
      PERFORM statistics_touch('q',jsonb_build_array(NEW.question),new_owner); PERFORM statistics_ref(NEW.ref,new_owner);
    END IF;
  ELSIF TG_TABLE_NAME='state_board_pending_work' THEN
    IF TG_OP<>'INSERT' THEN prior:=jsonb_build_array(OLD.batch,OLD.build_id,OLD.ruler,OLD.mode);PERFORM statistics_touch('p',prior,TG_TABLE_NAME||':'||prior::text); END IF;
    IF TG_OP<>'DELETE' THEN following:=jsonb_build_array(NEW.batch,NEW.build_id,NEW.ruler,NEW.mode);PERFORM statistics_touch('p',following,TG_TABLE_NAME||':'||following::text); END IF;
  ELSIF TG_TABLE_NAME='entity_compute_batches' THEN
    IF TG_OP<>'INSERT' THEN PERFORM statistics_touch('b',jsonb_build_array(OLD.id),TG_TABLE_NAME||':'||OLD.id); END IF;
    IF TG_OP<>'DELETE' THEN PERFORM statistics_touch('b',jsonb_build_array(NEW.id),TG_TABLE_NAME||':'||NEW.id); END IF;
  ELSIF TG_TABLE_NAME IN('entity_compute_tasks','entity_compute_results') THEN
    IF TG_OP<>'INSERT' THEN PERFORM statistics_touch('q',jsonb_build_array(OLD.computation),TG_TABLE_NAME||':'||OLD.id); END IF;
    IF TG_OP<>'DELETE' THEN PERFORM statistics_touch('q',jsonb_build_array(NEW.computation),TG_TABLE_NAME||':'||NEW.id); END IF;
  ELSIF TG_TABLE_NAME='state_compute_scheduling' THEN
    IF TG_OP<>'INSERT' THEN PERFORM statistics_touch('q',jsonb_build_array(OLD.id),TG_TABLE_NAME||':'||OLD.id); END IF;
    IF TG_OP<>'DELETE' THEN PERFORM statistics_touch('q',jsonb_build_array(NEW.id),TG_TABLE_NAME||':'||NEW.id); END IF;
  ELSE
    IF TG_OP<>'INSERT' THEN
      IF TG_TABLE_NAME='record_candidate_answers' THEN prior:=jsonb_build_array(OLD.id);
      ELSIF TG_TABLE_NAME='state_pending_verifications' THEN prior:=jsonb_build_array(OLD.question,OLD.engine,OLD.peer);
      ELSIF TG_TABLE_NAME='record_compute_order_cancellations' THEN prior:=jsonb_build_array(OLD.question,OLD.task_id);
      ELSE prior:=jsonb_build_array(OLD.cut_id,OLD.question); END IF;
      PERFORM statistics_touch('q',jsonb_build_array(OLD.question),TG_TABLE_NAME||':'||prior::text);
    END IF;
    IF TG_OP<>'DELETE' THEN
      IF TG_TABLE_NAME='record_candidate_answers' THEN following:=jsonb_build_array(NEW.id);
      ELSIF TG_TABLE_NAME='state_pending_verifications' THEN following:=jsonb_build_array(NEW.question,NEW.engine,NEW.peer);
      ELSIF TG_TABLE_NAME='record_compute_order_cancellations' THEN following:=jsonb_build_array(NEW.question,NEW.task_id);
      ELSE following:=jsonb_build_array(NEW.cut_id,NEW.question); END IF;
      PERFORM statistics_touch('q',jsonb_build_array(NEW.question),TG_TABLE_NAME||':'||following::text);
    END IF;
  END IF;
  RETURN NULL;
END $$;
DO $capture$ DECLARE name text; BEGIN
  FOREACH name IN ARRAY ARRAY['state_compute_scheduling','relation_task_consumers','state_board_pending_work',
    'entity_compute_batches','entity_compute_tasks','entity_compute_results','record_candidate_answers',
    'state_pending_verifications','record_compute_order_cancellations','relation_compute_order_cut_protection',
    'entity_compute_order_cuts','record_compute_order_cut_activations','state_factory_findings'] LOOP
    EXECUTE format('DROP TRIGGER IF EXISTS statistics_capture ON %I',name);
    EXECUTE format('CREATE TRIGGER statistics_capture AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION statistics_capture()',name);
    EXECUTE format('DROP TRIGGER IF EXISTS statistics_truncate ON %I',name);
    EXECUTE format('CREATE TRIGGER statistics_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION statistics_capture()',name);
  END LOOP;
END $capture$;

CREATE OR REPLACE FUNCTION statistics_replace(text,text,text,jsonb) RETURNS void
LANGUAGE plpgsql AS $$
DECLARE old jsonb; item jsonb; delta bigint;
BEGIN
  SELECT body INTO old FROM state_statistics_contributions WHERE namespace=$1 AND kind=$2 AND key=$3;
  FOR item IN SELECT value FROM jsonb_array_elements(coalesce(old,'[]')) LOOP
    delta:=-(item->>1)::bigint;
    INSERT INTO state_statistics_rollup(namespace,key,n) VALUES($1,(item->0)::text,delta)
      ON CONFLICT(namespace,key) DO UPDATE SET n=state_statistics_rollup.n+excluded.n;
  END LOOP;
  FOR item IN SELECT value FROM jsonb_array_elements($4) LOOP
    delta:=(item->>1)::bigint;
    INSERT INTO state_statistics_rollup(namespace,key,n) VALUES($1,(item->0)::text,delta)
      ON CONFLICT(namespace,key) DO UPDATE SET n=state_statistics_rollup.n+excluded.n;
  END LOOP;
  DELETE FROM state_statistics_rollup WHERE namespace=$1 AND n=0 AND key IN(
    SELECT (value->0)::text FROM jsonb_array_elements(coalesce(old,'[]')||$4));
  IF $4='[]'::jsonb THEN
    DELETE FROM state_statistics_contributions WHERE namespace=$1 AND kind=$2 AND key=$3;
  ELSE
    INSERT INTO state_statistics_contributions(namespace,kind,key,body) VALUES($1,$2,$3,$4)
      ON CONFLICT(namespace,kind,key) DO UPDATE SET body=excluded.body;
  END IF;
END $$;

REVOKE ALL ON FUNCTION statistics_invalidate(text),statistics_touch(text,jsonb,text),statistics_ref(text,text),statistics_capture(),statistics_replace(text,text,text,jsonb) FROM PUBLIC;

-- Install this separately with CONCURRENTLY in a reviewed production window.
-- Runtime/bootstrap does not create an index on a canonical source relation.
-- CREATE INDEX CONCURRENTLY IF NOT EXISTS statistics_consumer_pages ON relation_task_consumers(question,producer,ref) WHERE state='live' AND producer IN('new_build','survey_build','refresh','sweep');
