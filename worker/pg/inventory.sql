-- Transactional invalidation keys; the engine's local catalog holds derived rows.
CREATE SEQUENCE IF NOT EXISTS inventory_revision;
CREATE SEQUENCE IF NOT EXISTS inventory_generation;
CREATE TABLE IF NOT EXISTS state_compute_inventory_invalidations (
  scope TEXT COLLATE "C" NOT NULL CHECK (scope IN ('build','question')),
  key TEXT COLLATE "C" NOT NULL,
  revision BIGINT NOT NULL DEFAULT nextval('inventory_revision'),
  first_at TEXT COLLATE "C" NOT NULL DEFAULT to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
  PRIMARY KEY (scope,key)
);
CREATE INDEX IF NOT EXISTS inventory_dirty_revision ON state_compute_inventory_invalidations (revision,scope,key);
CREATE TABLE IF NOT EXISTS state_compute_inventory_generation (
  id BIGINT PRIMARY KEY CHECK (id=1),
  generation BIGINT NOT NULL,
  engine TEXT COLLATE "C" NOT NULL,
  contract JSONB NOT NULL,
  status TEXT COLLATE "C" NOT NULL CHECK (status IN ('initializing','active'))
);
CREATE SEQUENCE IF NOT EXISTS inventory_epoch_token;
CREATE TABLE IF NOT EXISTS record_inventory_captures (
  epoch BIGINT PRIMARY KEY DEFAULT nextval('inventory_epoch_token'),
  generation BIGINT NOT NULL,
  engine TEXT COLLATE "C" NOT NULL,
  contract JSONB NOT NULL,
  snapshot TEXT COLLATE "C" NOT NULL,
  started_at TEXT COLLATE "C" NOT NULL,
  capture_tx TEXT COLLATE "C" NOT NULL,
  status TEXT COLLATE "C" NOT NULL CHECK (status IN ('capturing','sealed')),
  changes BIGINT,
  manifest_sha TEXT COLLATE "C"
);
CREATE UNIQUE INDEX IF NOT EXISTS inventory_epochs_capture ON record_inventory_captures (capture_tx);
REVOKE ALL ON state_compute_inventory_invalidations,state_compute_inventory_generation,record_inventory_captures FROM PUBLIC;
REVOKE ALL ON SEQUENCE inventory_revision,inventory_generation,inventory_epoch_token FROM PUBLIC;
SELECT set_config('search_path',quote_ident(current_schema())||',pg_catalog,pg_temp',false);

CREATE OR REPLACE FUNCTION inventory_check_source_write() RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  IF current_setting('transaction_isolation') IN ('repeatable read','serializable') AND EXISTS(
    SELECT 1 FROM record_inventory_captures e WHERE e.capture_tx=txid_current()::text)
    THEN RAISE EXCEPTION 'inventory epoch may write receipts only'; END IF;
END $$;
REVOKE ALL ON FUNCTION inventory_check_source_write() FROM PUBLIC;

CREATE OR REPLACE FUNCTION inventory_epoch_update_guard() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  PERFORM inventory_check_source_write();
  RETURN NULL;
END $$;
REVOKE ALL ON FUNCTION inventory_epoch_update_guard() FROM PUBLIC;

CREATE OR REPLACE FUNCTION inventory_hot_changed() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE old_key text; new_key text;
BEGIN
  IF TG_TABLE_NAME='state_compute_scheduling' THEN old_key:=OLD.id; new_key:=NEW.id;
  ELSE old_key:=OLD.question; new_key:=NEW.question; END IF;
  INSERT INTO state_compute_inventory_invalidations(scope,key) SELECT 'question',key FROM (VALUES(old_key),(new_key)) k(key)
    WHERE key IS NOT NULL GROUP BY key ON CONFLICT(scope,key) DO UPDATE SET revision=EXCLUDED.revision;
  RETURN NULL;
END $$;
REVOKE ALL ON FUNCTION inventory_hot_changed() FROM PUBLIC;

-- Transition relations coalesce a bulk statement and ignore lease-only updates.
CREATE OR REPLACE FUNCTION inventory_source_changed() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE scope_name text; key_column text; projection text; changed text; targets text;
BEGIN
  PERFORM inventory_check_source_write();
  CASE TG_TABLE_NAME
    WHEN 'entity_weapon_builds' THEN scope_name:='build'; key_column:='id'; projection:='id,at,record';
    WHEN 'state_board_measurement_selection' THEN scope_name:='build'; key_column:='identity'; projection:='identity,ruler,mode,score,measured_by,started_at,finished_at,review_id';
    WHEN 'state_board_pending_work' THEN scope_name:='build'; key_column:='build_id'; projection:='batch,build_id,ruler,mode';
    WHEN 'entity_compute_batches' THEN scope_name:='build'; key_column:='id'; projection:='id';
    WHEN 'entity_measurement_reviews' THEN scope_name:='build'; key_column:='identity'; projection:='id,identity,state,invalidated_at';
    WHEN 'state_compute_scheduling' THEN scope_name:='question'; key_column:='id'; projection:='id,verb,state,fact_engine';
    WHEN 'relation_task_consumers' THEN scope_name:='question'; key_column:='question'; projection:='producer,ref,question,state';
    WHEN 'record_candidate_answers' THEN scope_name:='question'; key_column:='question'; projection:='question,engine';
    WHEN 'record_refusal_withdrawals' THEN scope_name:='question'; key_column:='question'; projection:='question,engine';
    ELSE RAISE EXCEPTION 'unsupported inventory source';
  END CASE;
  IF TG_OP='TRUNCATE' THEN
    changed:=format('SELECT DISTINCT %I AS key FROM %I',key_column,TG_TABLE_NAME);
  ELSIF TG_OP='UPDATE' THEN
    changed:=format('SELECT DISTINCT %I AS key FROM ((SELECT %s FROM inventory_new EXCEPT SELECT %s FROM inventory_old)
      UNION ALL (SELECT %s FROM inventory_old EXCEPT SELECT %s FROM inventory_new)) changed',
      key_column,projection,projection,projection,projection);
  ELSE
    changed:=format('SELECT DISTINCT %I AS key FROM %I',key_column,
      CASE WHEN TG_OP='INSERT' THEN 'inventory_new' ELSE 'inventory_old' END);
  END IF;
  targets:=CASE WHEN TG_TABLE_NAME='entity_compute_batches'
    THEN 'SELECT DISTINCT q.build_id AS key FROM state_board_pending_work q JOIN changed c ON c.key=q.batch'
    ELSE 'SELECT key FROM changed' END;
  EXECUTE format('WITH changed AS (%s),targets AS (%s)
    INSERT INTO state_compute_inventory_invalidations(scope,key) SELECT %L,key FROM targets WHERE key IS NOT NULL
    ON CONFLICT(scope,key) DO UPDATE SET revision=EXCLUDED.revision',changed,targets,scope_name);
  RETURN NULL;
END $$;
REVOKE ALL ON FUNCTION inventory_source_changed() FROM PUBLIC;

DO $triggers$
DECLARE source text; fields text; comparison text;
BEGIN
  FOREACH source IN ARRAY ARRAY['entity_weapon_builds','state_board_measurement_selection','state_board_pending_work','entity_compute_batches','entity_measurement_reviews','state_compute_scheduling','relation_task_consumers','record_candidate_answers','record_refusal_withdrawals'] LOOP
    EXECUTE format('DROP TRIGGER IF EXISTS inventory_insert ON %I',source);
    EXECUTE format('DROP TRIGGER IF EXISTS inventory_update ON %I',source);
    EXECUTE format('DROP TRIGGER IF EXISTS inventory_delete ON %I',source);
    EXECUTE format('DROP TRIGGER IF EXISTS inventory_truncate ON %I',source);
    EXECUTE format('DROP TRIGGER IF EXISTS inventory_receipt_update ON %I',source);
    EXECUTE format('CREATE TRIGGER inventory_insert AFTER INSERT ON %I
      REFERENCING NEW TABLE AS inventory_new FOR EACH STATEMENT EXECUTE FUNCTION inventory_source_changed()',source);
    IF source IN ('state_compute_scheduling','relation_task_consumers','record_candidate_answers') THEN
      CASE source
        WHEN 'state_compute_scheduling' THEN fields:='id,verb,state,fact_engine';
          comparison:='(OLD.id,OLD.verb,OLD.state,OLD.fact_engine) IS DISTINCT FROM (NEW.id,NEW.verb,NEW.state,NEW.fact_engine)';
        WHEN 'relation_task_consumers' THEN fields:='producer,ref,question,state';
          comparison:='(OLD.producer,OLD.ref,OLD.question,OLD.state) IS DISTINCT FROM (NEW.producer,NEW.ref,NEW.question,NEW.state)';
        WHEN 'record_candidate_answers' THEN fields:='question,engine'; comparison:='(OLD.question,OLD.engine) IS DISTINCT FROM (NEW.question,NEW.engine)';
      END CASE;
      EXECUTE format('CREATE TRIGGER inventory_receipt_update BEFORE UPDATE ON %I
        FOR EACH STATEMENT EXECUTE FUNCTION inventory_epoch_update_guard()',source);
      EXECUTE format('CREATE TRIGGER inventory_update AFTER UPDATE OF %s ON %I
        FOR EACH ROW WHEN (%s) EXECUTE FUNCTION inventory_hot_changed()',fields,source,comparison);
    ELSE
      EXECUTE format('CREATE TRIGGER inventory_update AFTER UPDATE ON %I
        REFERENCING OLD TABLE AS inventory_old NEW TABLE AS inventory_new FOR EACH STATEMENT EXECUTE FUNCTION inventory_source_changed()',source);
    END IF;
    EXECUTE format('CREATE TRIGGER inventory_delete AFTER DELETE ON %I
      REFERENCING OLD TABLE AS inventory_old FOR EACH STATEMENT EXECUTE FUNCTION inventory_source_changed()',source);
    EXECUTE format('CREATE TRIGGER inventory_truncate BEFORE TRUNCATE ON %I
      FOR EACH STATEMENT EXECUTE FUNCTION inventory_source_changed()',source);
  END LOOP;
END $triggers$;

CREATE OR REPLACE FUNCTION inventory_status() RETURNS jsonb
LANGUAGE sql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
  SELECT jsonb_build_object('generation',generation::text,'engine',engine,'contract',contract,'status',status)
    FROM state_compute_inventory_generation WHERE id=1
$$;
REVOKE ALL ON FUNCTION inventory_status() FROM PUBLIC;

CREATE OR REPLACE FUNCTION inventory_begin(text,text,text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held state_compute_inventory_generation%ROWTYPE; wanted jsonb; current_engine text;
BEGIN
  wanted:=$2::jsonb;
  IF $1 IS NULL OR $1 !~ '^[a-f0-9]{12}$' OR jsonb_typeof(wanted) IS DISTINCT FROM 'object'
    OR wanted->'v' IS DISTINCT FROM '1'::jsonb OR jsonb_typeof(wanted->'covers') IS DISTINCT FROM 'array'
    THEN RAISE EXCEPTION 'invalid inventory contract'; END IF;
  IF EXISTS(SELECT 1 FROM jsonb_array_elements(wanted->'covers') x WHERE jsonb_typeof(x)<>'string')
    THEN RAISE EXCEPTION 'invalid inventory covers'; END IF;
  PERFORM set_config('lock_timeout','1s',true);
  -- Generation changes fence receipts, not source writers; dirty tokens span generations.
  LOCK TABLE state_compute_inventory_generation IN SHARE ROW EXCLUSIVE MODE;
  SELECT * INTO held FROM state_compute_inventory_generation WHERE id=1 FOR UPDATE;
  IF held.generation::text IS DISTINCT FROM $3 THEN RAISE EXCEPTION 'inventory generation changed'; END IF;
  SELECT engine INTO current_engine FROM state_factory_snapshot WHERE id=1;
  IF current_engine IS NOT NULL AND current_engine<>$1 THEN RAISE EXCEPTION 'inventory engine changed'; END IF;
  INSERT INTO state_compute_inventory_generation(id,generation,engine,contract,status)
    VALUES (1,nextval('inventory_generation'),$1,wanted,'initializing')
    ON CONFLICT(id) DO UPDATE SET generation=EXCLUDED.generation,engine=EXCLUDED.engine,
      contract=EXCLUDED.contract,status=EXCLUDED.status;
  RETURN inventory_status();
END $$;
REVOKE ALL ON FUNCTION inventory_begin(text,text,text) FROM PUBLIC;

-- Revisions are equality tokens; sorting schedules work and never advances a cursor.
CREATE OR REPLACE FUNCTION inventory_changes(bigint)
RETURNS TABLE(scope text,key text,revision text)
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  IF $1 IS NULL OR $1<1 OR $1>256 THEN RAISE EXCEPTION 'inventory change limit must be 1..256'; END IF;
  RETURN QUERY SELECT d.scope,d.key,d.revision::text FROM state_compute_inventory_invalidations d ORDER BY d.revision,d.scope,d.key LIMIT $1;
END $$;
REVOKE ALL ON FUNCTION inventory_changes(bigint) FROM PUBLIC;

CREATE OR REPLACE FUNCTION inventory_change_keys(text) RETURNS jsonb
LANGUAGE plpgsql IMMUTABLE SET search_path FROM CURRENT AS $$
DECLARE changes jsonb;
BEGIN
  changes:=$1::jsonb;
  IF jsonb_typeof(changes) IS DISTINCT FROM 'array' THEN RAISE EXCEPTION 'invalid inventory changes'; END IF;
  IF jsonb_array_length(changes)>256 THEN RAISE EXCEPTION 'invalid inventory changes'; END IF;
  IF EXISTS(SELECT 1 FROM jsonb_array_elements(changes) x WHERE jsonb_typeof(x)<>'object'
    OR NOT(x ?& ARRAY['scope','key','revision'])
    OR x->>'scope' NOT IN ('build','question') OR jsonb_typeof(x->'scope')<>'string'
    OR jsonb_typeof(x->'key')<>'string' OR length(x->>'key')=0
    OR jsonb_typeof(x->'revision')<>'string' OR x->>'revision' !~ '^[1-9][0-9]{0,18}$')
    THEN RAISE EXCEPTION 'invalid inventory change key'; END IF;
  RETURN changes;
END $$;
REVOKE ALL ON FUNCTION inventory_change_keys(text) FROM PUBLIC;

CREATE OR REPLACE FUNCTION inventory_read_guard(text,text) RETURNS state_compute_inventory_generation
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held state_compute_inventory_generation%ROWTYPE; current_engine text;
BEGIN
  IF current_setting('transaction_isolation') NOT IN ('repeatable read','serializable')
    THEN RAISE EXCEPTION 'inventory inputs require a read-only repeatable-read transaction or owned epoch'; END IF;
  SELECT * INTO held FROM state_compute_inventory_generation WHERE id=1;
  IF current_setting('transaction_read_only')<>'on' AND NOT EXISTS(SELECT 1 FROM record_inventory_captures e
    WHERE e.capture_tx=txid_current()::text AND e.status='capturing' AND e.generation=held.generation
      AND e.snapshot=pg_current_snapshot()::text)
    THEN RAISE EXCEPTION 'inventory inputs require a read-only repeatable-read transaction or owned epoch'; END IF;
  SELECT engine INTO current_engine FROM state_factory_snapshot WHERE id=1;
  IF held.generation IS NULL OR held.engine IS DISTINCT FROM $1
    OR (current_engine IS NOT NULL AND current_engine<>$1)
    OR held.contract->'covers' IS DISTINCT FROM $2::jsonb THEN RAISE EXCEPTION 'inventory read fence changed'; END IF;
  RETURN held;
END $$;
REVOKE ALL ON FUNCTION inventory_read_guard(text,text) FROM PUBLIC;

CREATE OR REPLACE FUNCTION inventory_inputs(text,text,text) RETURNS jsonb
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held state_compute_inventory_generation%ROWTYPE; changes jsonb; build_keys text[]; question_keys text[];
  build_rows jsonb; fact_rows jsonb; queue_rows jsonb; task_rows jsonb;
BEGIN
  held:=inventory_read_guard($1,$2);
  changes:=inventory_change_keys($3);
  SELECT array_agg(DISTINCT x->>'key') FILTER(WHERE x->>'scope'='build'),
    array_agg(DISTINCT x->>'key') FILTER(WHERE x->>'scope'='question') INTO build_keys,question_keys FROM jsonb_array_elements(changes) x;
  SELECT COALESCE(jsonb_agg(CASE WHEN b.id IS NULL THEN jsonb_build_object('id',k.id,'deleted',true)
    ELSE jsonb_build_object('id',b.id,'at',b.at,'record',b.record::jsonb) END ORDER BY k.id),'[]'::jsonb)
    INTO build_rows FROM unnest(build_keys) k(id) LEFT JOIN entity_weapon_builds b ON b.id=k.id;
  SELECT COALESCE(jsonb_agg(to_jsonb(f) ORDER BY f.identity,f.ruler,f.mode),'[]'::jsonb)
    INTO fact_rows FROM board_facts f WHERE f.identity=ANY(build_keys);
  SELECT COALESCE(jsonb_agg(jsonb_build_object('batch',q.batch,'build_id',q.build_id,'ruler',q.ruler,'mode',q.mode)
    ORDER BY q.batch,q.build_id,q.ruler,q.mode),'[]'::jsonb) INTO queue_rows FROM state_board_pending_work q
    WHERE q.build_id=ANY(build_keys) AND EXISTS(SELECT 1 FROM entity_compute_batches b WHERE b.id=q.batch);
  SELECT COALESCE(jsonb_agg(CASE
    WHEN q.id IS NOT NULL AND q.state<>'withdrawn' AND ((q.state IN ('fact','spot') AND q.fact_engine=$1)
      OR q.state IN ('disputed','nondeterministic') OR live.kind IS NOT NULL)
      THEN jsonb_build_object('id',q.id,'verb',q.verb,'state',q.state,'fact_engine',q.fact_engine,'kind',live.kind,
        'served_answered',CASE WHEN q.state IN ('disputed','nondeterministic')
          THEN EXISTS(SELECT 1 FROM record_candidate_answers a WHERE a.question=q.id AND a.engine=$1) ELSE false END)
    WHEN refused.question IS NOT NULL AND (q.id IS NULL OR q.state='withdrawn')
      THEN jsonb_build_object('id',k.id,'verb','','state','refused','fact_engine',refused.engine,'kind',NULL,'served_answered',false)
    ELSE jsonb_build_object('id',k.id,'deleted',true) END ORDER BY k.id),'[]'::jsonb) INTO task_rows
    FROM unnest(question_keys) k(id) LEFT JOIN state_compute_work q ON q.id=k.id
    LEFT JOIN LATERAL (SELECT min(d.producer) kind FROM relation_task_consumers d WHERE d.question=k.id AND d.state='live'
      AND NOT(d.producer=ANY(ARRAY(SELECT jsonb_array_elements_text($2::jsonb))))) live ON true
    LEFT JOIN record_refusal_withdrawals refused ON refused.question=k.id AND refused.engine=$1;
  RETURN jsonb_build_object('v',1,'generation',held.generation::text,'engine',held.engine,'contract',held.contract,
    'snapshot',pg_current_snapshot()::text,'started_at',to_char(transaction_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    'changes',changes,'entity_weapon_builds',build_rows,'facts',fact_rows,'state_board_pending_work',queue_rows,'tasks',task_rows);
END $$;
REVOKE ALL ON FUNCTION inventory_inputs(text,text,text) FROM PUBLIC;

-- Complete cohort scopes stream through the caller's bounded cursor fetches.
CREATE OR REPLACE FUNCTION inventory_cohort_rows(text,text,text)
RETURNS TABLE(kind text,body jsonb)
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE scopes jsonb; weapons text[]; identities text[];
BEGIN
  PERFORM inventory_read_guard($1,$2);
  scopes:=$3::jsonb;
  IF jsonb_typeof(scopes) IS DISTINCT FROM 'object' OR jsonb_typeof(scopes->'weapons') IS DISTINCT FROM 'array'
    OR jsonb_typeof(scopes->'identities') IS DISTINCT FROM 'array' THEN RAISE EXCEPTION 'invalid inventory cohort scope'; END IF;
  IF jsonb_array_length(scopes->'weapons')>256
    OR jsonb_array_length(scopes->'identities')>256
    OR EXISTS(SELECT 1 FROM jsonb_array_elements((scopes->'weapons')||(scopes->'identities')) x
      WHERE jsonb_typeof(x)<>'string' OR length(x#>>'{}')=0)
    THEN RAISE EXCEPTION 'invalid inventory cohort scope'; END IF;
  SELECT ARRAY(SELECT jsonb_array_elements_text(scopes->'weapons')),
    ARRAY(SELECT jsonb_array_elements_text(scopes->'identities')) INTO weapons,identities;
  RETURN QUERY SELECT 'entity_weapon_builds'::text,jsonb_build_object('id',b.id,'at',b.at,'record',b.record::jsonb)
    FROM entity_weapon_builds b WHERE b.weapon=ANY(weapons);
  RETURN QUERY WITH selected AS MATERIALIZED (SELECT b.id FROM entity_weapon_builds b WHERE b.weapon=ANY(weapons)
    UNION SELECT unnest(identities)) SELECT 'facts'::text,to_jsonb(f) FROM selected k
    JOIN LATERAL (SELECT * FROM board_facts f WHERE f.identity=k.id OFFSET 0) f ON true;
  RETURN QUERY WITH selected AS MATERIALIZED (SELECT b.id FROM entity_weapon_builds b WHERE b.weapon=ANY(weapons)
    UNION SELECT unnest(identities)) SELECT 'state_board_pending_work'::text,jsonb_build_object('batch',q.batch,'build_id',q.build_id,'ruler',q.ruler,'mode',q.mode)
    FROM selected k JOIN LATERAL (SELECT * FROM state_board_pending_work q WHERE q.build_id=k.id OFFSET 0) q ON true
      WHERE EXISTS(SELECT 1 FROM entity_compute_batches b WHERE b.id=q.batch);
END $$;
REVOKE ALL ON FUNCTION inventory_cohort_rows(text,text,text) FROM PUBLIC;

CREATE OR REPLACE FUNCTION inventory_ack(text,text) RETURNS bigint
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE changes jsonb; held state_compute_inventory_generation%ROWTYPE; matched bigint;
BEGIN
  changes:=inventory_change_keys($2);
  LOCK TABLE state_compute_inventory_invalidations IN ROW EXCLUSIVE MODE;
  SELECT * INTO held FROM state_compute_inventory_generation WHERE id=1 FOR SHARE;
  IF held.generation::text IS DISTINCT FROM $1 THEN RAISE EXCEPTION 'inventory generation changed'; END IF;
  DELETE FROM state_compute_inventory_invalidations d USING jsonb_array_elements(changes) x
    WHERE d.scope=x->>'scope' AND d.key=x->>'key' AND d.revision::text=x->>'revision';
  GET DIAGNOSTICS matched=ROW_COUNT;
  RETURN matched;
END $$;
REVOKE ALL ON FUNCTION inventory_ack(text,text) FROM PUBLIC;

CREATE OR REPLACE FUNCTION inventory_validate_body(text) RETURNS jsonb
LANGUAGE plpgsql IMMUTABLE SET search_path FROM CURRENT AS $$
DECLARE body jsonb; order_total numeric; verb_total numeric;
BEGIN
  body:=$1::jsonb;
  IF jsonb_typeof(body) IS DISTINCT FROM 'object'
    OR NOT(body ?& ARRAY['v','generation','contract','source','total','registered','orders','verbs','engine'])
    OR body->'v' IS DISTINCT FROM '1'::jsonb OR jsonb_typeof(body->'generation')<>'string'
    OR jsonb_typeof(body->'contract')<>'object' OR jsonb_typeof(body->'source')<>'string'
    OR jsonb_typeof(body->'total')<>'number' OR jsonb_typeof(body->'registered')<>'number'
    OR body->>'total' !~ '^(0|[1-9][0-9]*)$' OR body->>'registered' !~ '^(0|[1-9][0-9]*)$'
    OR (body->>'total')::numeric>9007199254740991 OR (body->>'registered')::numeric>(body->>'total')::numeric
    OR jsonb_typeof(body->'orders')<>'array' OR jsonb_typeof(body->'verbs')<>'object'
    THEN RAISE EXCEPTION 'invalid inventory publication'; END IF;
  IF EXISTS(SELECT 1 FROM jsonb_array_elements(body->'orders') x WHERE jsonb_typeof(x)<>'object'
    OR NOT(x ?& ARRAY['kind','total'])
    OR jsonb_typeof(x->'kind')<>'string' OR length(x->>'kind')=0 OR jsonb_typeof(x->'total')<>'number'
    OR x->>'total' !~ '^(0|[1-9][0-9]*)$')
    OR EXISTS(SELECT 1 FROM jsonb_each(body->'verbs') x WHERE jsonb_typeof(x.value)<>'number' OR x.value#>>'{}' !~ '^(0|[1-9][0-9]*)$')
    THEN RAISE EXCEPTION 'invalid inventory counts'; END IF;
  SELECT COALESCE(sum((x->>'total')::numeric),0) INTO order_total FROM jsonb_array_elements(body->'orders') x;
  SELECT COALESCE(sum((x.value#>>'{}')::numeric),0) INTO verb_total FROM jsonb_each(body->'verbs') x;
  IF order_total<>(body->>'total')::numeric OR verb_total<>(body->>'total')::numeric THEN RAISE EXCEPTION 'inventory count mismatch'; END IF;
  RETURN body;
END $$;
REVOKE ALL ON FUNCTION inventory_validate_body(text) FROM PUBLIC;

CREATE OR REPLACE FUNCTION inventory_store_body(jsonb,text,text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE body jsonb; previous jsonb; stamped text; expires_at timestamptz; matched bigint;
BEGIN
  expires_at:=$2::timestamptz+interval '3 hours';
  IF expires_at<=clock_timestamp() THEN RAISE EXCEPTION 'inventory cut expired'; END IF;
  stamped:=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
  body:=$1||jsonb_build_object('at',stamped,'started_at',$2,'snapshot',$3,
    'expires_at',to_char(expires_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'));
  SELECT f.body::jsonb INTO previous FROM state_factory_findings f WHERE f.kind='task_inventory';
  IF previous->>'generation'=body->>'generation' AND previous ? 'epoch' AND body ? 'epoch'
    AND (previous->>'epoch')::bigint>(body->>'epoch')::bigint
    THEN RAISE EXCEPTION 'a newer inventory cut is already published'; END IF;
  INSERT INTO state_factory_findings(kind,body,at) VALUES ('task_inventory',body::text,stamped)
    ON CONFLICT(kind) DO UPDATE SET body=EXCLUDED.body,at=EXCLUDED.at
      WHERE COALESCE(state_factory_findings.body::jsonb->>'started_at','')<=EXCLUDED.body::jsonb->>'started_at';
  GET DIAGNOSTICS matched=ROW_COUNT;
  IF matched=0 THEN RAISE EXCEPTION 'a newer inventory cut is already published'; END IF;
  UPDATE state_compute_inventory_generation SET status='active' WHERE id=1;
  RETURN body;
END $$;
REVOKE ALL ON FUNCTION inventory_store_body(jsonb,text,text) FROM PUBLIC;

CREATE OR REPLACE FUNCTION inventory_publish(text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE body jsonb; held state_compute_inventory_generation%ROWTYPE; current_engine text; stamped text;
BEGIN
  IF current_setting('transaction_isolation')<>'read committed' THEN RAISE EXCEPTION 'inventory publication requires read committed'; END IF;
  body:=inventory_validate_body($1)-'epoch'-'manifest_sha';
  PERFORM set_config('lock_timeout','1s',true);
  LOCK TABLE state_compute_inventory_invalidations IN SHARE MODE;
  SELECT * INTO held FROM state_compute_inventory_generation WHERE id=1 FOR UPDATE;
  SELECT engine INTO current_engine FROM state_factory_snapshot WHERE id=1;
  IF held.generation::text IS DISTINCT FROM body->>'generation' OR held.contract IS DISTINCT FROM body->'contract'
    OR held.engine IS DISTINCT FROM body->>'engine' OR (current_engine IS NOT NULL AND current_engine<>held.engine)
    THEN RAISE EXCEPTION 'inventory publication fence changed'; END IF;
  IF EXISTS(SELECT 1 FROM state_compute_inventory_invalidations) THEN RAISE EXCEPTION 'inventory changes remain'; END IF;
  stamped:=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
  RETURN inventory_store_body(body,stamped,pg_current_snapshot()::text);
END $$;
REVOKE ALL ON FUNCTION inventory_publish(text) FROM PUBLIC;

CREATE OR REPLACE FUNCTION inventory_epoch_status(text) RETURNS jsonb
LANGUAGE sql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
  SELECT jsonb_build_object('epoch',epoch::text,'generation',generation::text,'engine',engine,'contract',contract,
    'snapshot',snapshot,'started_at',started_at,'status',status,'change_count',changes,'manifest_sha',manifest_sha)
    FROM record_inventory_captures WHERE epoch=$1::bigint
$$;
REVOKE ALL ON FUNCTION inventory_epoch_status(text) FROM PUBLIC;

CREATE OR REPLACE FUNCTION inventory_epoch_begin(text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held state_compute_inventory_generation%ROWTYPE; current_engine text; created bigint;
BEGIN
  IF current_setting('transaction_isolation') NOT IN ('repeatable read','serializable')
    OR current_setting('transaction_read_only')<>'off' THEN RAISE EXCEPTION 'inventory epoch requires repeatable read receipt transaction'; END IF;
  LOCK TABLE state_compute_inventory_invalidations IN ROW EXCLUSIVE MODE;
  SELECT * INTO held FROM state_compute_inventory_generation WHERE id=1 FOR SHARE;
  SELECT engine INTO current_engine FROM state_factory_snapshot WHERE id=1;
  IF held.generation::text IS DISTINCT FROM $1 OR (current_engine IS NOT NULL AND current_engine<>held.engine)
    THEN RAISE EXCEPTION 'inventory epoch fence changed'; END IF;
  INSERT INTO record_inventory_captures(generation,engine,contract,snapshot,started_at,capture_tx,status)
    VALUES (held.generation,held.engine,held.contract,pg_current_snapshot()::text,
      to_char(transaction_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),txid_current()::text,'capturing')
    RETURNING epoch INTO created;
  RETURN inventory_epoch_status(created::text);
END $$;
REVOKE ALL ON FUNCTION inventory_epoch_begin(text) FROM PUBLIC;

CREATE OR REPLACE FUNCTION inventory_epoch_changes(text)
RETURNS TABLE(scope text,key text,revision text)
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  IF NOT EXISTS(SELECT 1 FROM record_inventory_captures e WHERE e.epoch=$1::bigint AND e.capture_tx=txid_current()::text
    AND e.status='capturing' AND e.snapshot=pg_current_snapshot()::text)
    THEN RAISE EXCEPTION 'inventory epoch does not own this snapshot'; END IF;
  RETURN QUERY SELECT d.scope,d.key,d.revision::text FROM state_compute_inventory_invalidations d ORDER BY d.scope,d.key;
END $$;
REVOKE ALL ON FUNCTION inventory_epoch_changes(text) FROM PUBLIC;

CREATE OR REPLACE FUNCTION inventory_epoch_open(text) RETURNS refcursor
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE output refcursor := 'inventory_epoch_'||$1::bigint::text;
BEGIN
  IF NOT EXISTS(SELECT 1 FROM record_inventory_captures e WHERE e.epoch=$1::bigint AND e.capture_tx=txid_current()::text
    AND e.status='capturing' AND e.snapshot=pg_current_snapshot()::text)
    THEN RAISE EXCEPTION 'inventory epoch does not own this snapshot'; END IF;
  -- RETURN QUERY materializes the entire backlog before the first FETCH.
  OPEN output NO SCROLL FOR SELECT d.scope,d.key,d.revision::text FROM state_compute_inventory_invalidations d ORDER BY d.scope,d.key;
  RETURN output;
END $$;
REVOKE ALL ON FUNCTION inventory_epoch_open(text) FROM PUBLIC;

CREATE OR REPLACE FUNCTION inventory_epoch_seal(text,bigint,text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held record_inventory_captures%ROWTYPE; dirty_count bigint;
BEGIN
  SELECT * INTO held FROM record_inventory_captures WHERE epoch=$1::bigint;
  IF held.epoch IS NULL OR held.capture_tx<>txid_current()::text OR held.status<>'capturing'
    OR held.snapshot<>pg_current_snapshot()::text THEN RAISE EXCEPTION 'inventory epoch does not own this snapshot'; END IF;
  IF $2 IS NULL OR $2<0 OR $3 IS NULL OR $3 !~ '^[a-f0-9]{64}$' THEN RAISE EXCEPTION 'invalid inventory epoch manifest'; END IF;
  SELECT count(*) INTO dirty_count FROM state_compute_inventory_invalidations;
  IF dirty_count<>$2 THEN RAISE EXCEPTION 'inventory epoch changes incomplete'; END IF;
  UPDATE record_inventory_captures SET status='sealed',changes=$2,manifest_sha=$3 WHERE epoch=held.epoch;
  RETURN inventory_epoch_status($1);
END $$;
REVOKE ALL ON FUNCTION inventory_epoch_seal(text,bigint,text) FROM PUBLIC;

CREATE OR REPLACE FUNCTION inventory_publish_cut(text,text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE body jsonb; held state_compute_inventory_generation%ROWTYPE; cut record_inventory_captures%ROWTYPE; current_engine text;
BEGIN
  IF current_setting('transaction_isolation')<>'read committed' THEN RAISE EXCEPTION 'inventory publication requires read committed'; END IF;
  body:=inventory_validate_body($2);
  PERFORM set_config('lock_timeout','1s',true);
  -- A sealed MVCC cut has its own timestamp; later source writes remain dirty.
  SELECT * INTO held FROM state_compute_inventory_generation WHERE id=1 FOR UPDATE;
  SELECT * INTO cut FROM record_inventory_captures WHERE epoch=$1::bigint;
  SELECT engine INTO current_engine FROM state_factory_snapshot WHERE id=1;
  IF cut.status IS DISTINCT FROM 'sealed' OR cut.generation IS DISTINCT FROM held.generation
    OR cut.generation::text IS DISTINCT FROM body->>'generation' OR cut.contract IS DISTINCT FROM body->'contract'
    OR cut.engine IS DISTINCT FROM body->>'engine' OR (current_engine IS NOT NULL AND current_engine<>cut.engine)
    THEN RAISE EXCEPTION 'inventory cut publication fence changed'; END IF;
  RETURN inventory_store_body(body||jsonb_build_object('epoch',cut.epoch::text,'manifest_sha',cut.manifest_sha),cut.started_at,cut.snapshot);
END $$;
REVOKE ALL ON FUNCTION inventory_publish_cut(text,text) FROM PUBLIC;
