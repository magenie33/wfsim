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
  IF length($1)-length(replace($1,'|',''))<>2 THEN
    PERFORM statistics_touch('r',jsonb_build_array($1),$2);
  END IF;
END $$;
CREATE OR REPLACE FUNCTION statistics_capture() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE prior jsonb; following jsonb; old_owner text; new_owner text; evidence_question text;
BEGIN
  IF TG_OP='TRUNCATE' THEN PERFORM statistics_invalidate(); RETURN NULL; END IF;
  IF TG_OP='INSERT' AND TG_TABLE_NAME IN('record_candidate_answers','entity_compute_results') THEN
    -- The order write guard holds the cut fence and locks the affected question.
    evidence_question:=coalesce(to_jsonb(NEW)->>'question',to_jsonb(NEW)->>'computation');
    IF NOT EXISTS(SELECT 1 FROM compute_order_cut()) OR EXISTS(
      SELECT 1 FROM state_compute_scheduling s CROSS JOIN compute_order_cut() c
      WHERE s.id=evidence_question AND (s.state NOT IN('open','withdrawn') OR s.task_id>c.task_cutoff
        OR (coalesce(s.zone,factory_default_zone(s.producer)),coalesce(s.zone_position,s.task_id),s.id)
          <=(c.keep_zone,c.keep_position,c.keep_id))) THEN RETURN NULL;
    END IF;
  END IF;
  IF TG_OP='UPDATE' THEN
    CASE TG_TABLE_NAME
      WHEN 'record_candidate_answers' THEN
        IF (OLD.id,OLD.question) IS NOT DISTINCT FROM (NEW.id,NEW.question) THEN RETURN NULL; END IF;
      WHEN 'state_pending_verifications' THEN
        IF (OLD.question,OLD.engine,OLD.peer) IS NOT DISTINCT FROM (NEW.question,NEW.engine,NEW.peer) THEN RETURN NULL; END IF;
      WHEN 'entity_compute_results' THEN
        IF (OLD.id,OLD.computation) IS NOT DISTINCT FROM (NEW.id,NEW.computation) THEN RETURN NULL; END IF;
      WHEN 'entity_compute_tasks' THEN
        IF (OLD.id,OLD.computation,OLD.generation) IS NOT DISTINCT FROM (NEW.id,NEW.computation,NEW.generation) THEN RETURN NULL; END IF;
      WHEN 'entity_compute_batches' THEN
        IF (OLD.id,OLD.at) IS NOT DISTINCT FROM (NEW.id,NEW.at) THEN RETURN NULL; END IF;
      WHEN 'state_board_pending_work' THEN
        IF (OLD.batch,OLD.build_id,OLD.ruler,OLD.mode,OLD.initiator)
           IS NOT DISTINCT FROM (NEW.batch,NEW.build_id,NEW.ruler,NEW.mode,NEW.initiator) THEN RETURN NULL; END IF;
      WHEN 'relation_task_consumers' THEN
        IF (OLD.producer,OLD.ref,OLD.question,OLD.state,OLD.asked_at,OLD.served_at,OLD.delivery_at,OLD.payload)
           IS NOT DISTINCT FROM (NEW.producer,NEW.ref,NEW.question,NEW.state,NEW.asked_at,NEW.served_at,NEW.delivery_at,NEW.payload)
        THEN RETURN NULL; END IF;
      ELSE NULL;
    END CASE;
  END IF;
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
    IF TG_OP='DELETE' OR (TG_OP='UPDATE' AND OLD.id IS DISTINCT FROM NEW.id) THEN
      PERFORM statistics_touch('q',jsonb_build_array(OLD.id),TG_TABLE_NAME||':'||OLD.id);
    END IF;
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
CREATE OR REPLACE FUNCTION statistics_capture_relations() RETURNS text[] LANGUAGE sql IMMUTABLE AS $$
 SELECT ARRAY['state_compute_scheduling','relation_task_consumers','state_board_pending_work',
    'entity_compute_batches','entity_compute_tasks','entity_compute_results','record_candidate_answers',
    'state_pending_verifications','record_compute_order_cancellations','relation_compute_order_cut_protection',
    'entity_compute_order_cuts','record_compute_order_cut_activations','state_factory_findings']::text[]
$$;
DO $capture$ DECLARE name text; BEGIN
  FOREACH name IN ARRAY statistics_capture_relations() LOOP
    EXECUTE format('DROP TRIGGER IF EXISTS statistics_capture ON %I',name);
    EXECUTE format('CREATE TRIGGER statistics_capture AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION statistics_capture()',name);
    EXECUTE format('DROP TRIGGER IF EXISTS statistics_truncate ON %I',name);
    EXECUTE format('CREATE TRIGGER statistics_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION statistics_capture()',name);
  END LOOP;
END $capture$;
DROP TRIGGER IF EXISTS pipeline_statistics_paused ON state_task_pipeline_controls;
CREATE TRIGGER pipeline_statistics_paused AFTER INSERT OR UPDATE OF paused ON state_task_pipeline_controls
 FOR EACH ROW WHEN(NEW.name='statistics' AND NEW.paused) EXECUTE FUNCTION task_pipeline_notify();

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

CREATE OR REPLACE FUNCTION statistics_preflight(text DEFAULT current_user) RETURNS jsonb LANGUAGE plpgsql STABLE AS $$
DECLARE tables text[]:=ARRAY['state_statistics_generation','state_statistics_journal','state_statistics_invalidations',
 'state_statistics_namespaces','state_statistics_head','state_statistics_obligations','state_statistics_checkpoints',
 'state_statistics_contributions','state_statistics_rollup','state_statistics_expiry'];
schema_ready boolean; capture_ready boolean; index_ready boolean; grants_ready boolean; index_oid oid;
BEGIN
 SELECT bool_and(to_regclass(name) IS NOT NULL) INTO schema_ready FROM unnest(tables)name;
 schema_ready:=coalesce(schema_ready,false) AND EXISTS(SELECT 1 FROM pg_attribute
  WHERE attrelid=to_regclass('state_statistics_journal') AND attname='owner' AND NOT attisdropped);
 SELECT NOT EXISTS(SELECT 1 FROM unnest(statistics_capture_relations())r(name) WHERE
  NOT EXISTS(SELECT 1 FROM pg_trigger t WHERE t.tgrelid=to_regclass(r.name) AND t.tgname='statistics_capture'
    AND t.tgenabled IN('O','A') AND t.tgfoid=to_regprocedure('statistics_capture()')) OR
  NOT EXISTS(SELECT 1 FROM pg_trigger t WHERE t.tgrelid=to_regclass(r.name) AND t.tgname='statistics_truncate'
    AND t.tgenabled IN('O','A') AND t.tgfoid=to_regprocedure('statistics_capture()'))) INTO capture_ready;
 SELECT i.indexrelid INTO index_oid FROM pg_index i JOIN pg_class c ON c.oid=i.indexrelid JOIN pg_am am ON am.oid=c.relam
  WHERE i.indexrelid=to_regclass('statistics_consumer_pages') AND i.indrelid=to_regclass('relation_task_consumers')
   AND i.indisready AND i.indisvalid AND NOT i.indisunique AND i.indnatts=3 AND i.indnkeyatts=3 AND am.amname='btree'
   AND pg_get_indexdef(i.indexrelid,1,true)='question' AND pg_get_indexdef(i.indexrelid,2,true)='producer' AND pg_get_indexdef(i.indexrelid,3,true)='ref'
   AND regexp_replace(pg_get_expr(i.indpred,i.indrelid),'[[:space:]()]','','g')=
    regexp_replace($predicate$(state='live'::text) AND(producer=ANY(ARRAY['new_build'::text,'survey_build'::text,'refresh'::text,'sweep'::text]))$predicate$,'[[:space:]()]','','g');
 index_ready:=index_oid IS NOT NULL;
 IF schema_ready AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=$1::name) THEN
  SELECT bool_and(has_table_privilege($1::name,to_regclass(name),'SELECT')) INTO grants_ready FROM unnest(tables)name;
  grants_ready:=grants_ready AND has_table_privilege($1::name,'state_statistics_namespaces','INSERT')
   AND has_table_privilege($1::name,'state_statistics_namespaces','UPDATE')
   AND has_table_privilege($1::name,'state_statistics_head','INSERT')
   AND has_table_privilege($1::name,'state_statistics_head','UPDATE')
   AND has_table_privilege($1::name,'state_statistics_journal','DELETE')
   AND has_sequence_privilege($1::name,'statistics_revision','USAGE')
   AND has_function_privilege($1::name,'statistics_replace(text,text,text,jsonb)','EXECUTE')
   AND has_function_privilege($1::name,'statistics_capture_relations()','EXECUTE');
  SELECT grants_ready AND bool_and(has_table_privilege($1::name,to_regclass(name),privilege)) INTO grants_ready
   FROM unnest(ARRAY['state_statistics_obligations','state_statistics_checkpoints','state_statistics_contributions','state_statistics_rollup','state_statistics_expiry'])name
   CROSS JOIN unnest(ARRAY['INSERT','UPDATE','DELETE'])privilege;
  grants_ready:=grants_ready AND has_table_privilege($1::name,'state_compute_scheduling','SELECT')
   AND has_table_privilege($1::name,'relation_task_consumers','SELECT')
   AND has_table_privilege($1::name,'state_board_pending_work','SELECT')
   AND has_column_privilege($1::name,'entity_compute_batches','id','SELECT')
   AND has_column_privilege($1::name,'entity_compute_batches','at','SELECT')
   AND has_column_privilege($1::name,'relation_compute_order_cut_protection','cut_id','SELECT')
   AND has_column_privilege($1::name,'relation_compute_order_cut_protection','question','SELECT');
 ELSE grants_ready:=false; END IF;
 RETURN jsonb_build_object('contract','statistics-projection-1','role',$1,'schema',schema_ready,'capture',capture_ready,
  'index',index_ready,'index_oid',index_oid,'grants',coalesce(grants_ready,false),
  'ready',schema_ready AND capture_ready AND index_ready AND coalesce(grants_ready,false));
END $$;

REVOKE ALL ON FUNCTION statistics_invalidate(text),statistics_touch(text,jsonb,text),statistics_ref(text,text),statistics_capture(),statistics_replace(text,text,text,jsonb),statistics_capture_relations(),statistics_preflight(text) FROM PUBLIC;

-- Install this separately with CONCURRENTLY in a reviewed production window.
-- Runtime/bootstrap does not create an index on a canonical source relation.
-- CREATE INDEX CONCURRENTLY IF NOT EXISTS statistics_consumer_pages ON relation_task_consumers(question,producer,ref) WHERE state='live' AND producer IN('new_build','survey_build','refresh','sweep');
