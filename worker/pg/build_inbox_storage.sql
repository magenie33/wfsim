CREATE TABLE IF NOT EXISTS state_build_inbox_storage (
  id integer PRIMARY KEY CHECK(id=1),
  enabled boolean NOT NULL DEFAULT false,
  adopted boolean NOT NULL DEFAULT false,
  automatic boolean NOT NULL DEFAULT false,
  closed_before bigint NOT NULL DEFAULT 0,
  tablespace oid NOT NULL,
  readiness text CHECK(readiness~'^[0-9a-f]{64}$')
);
INSERT INTO state_build_inbox_storage(id,tablespace)
  SELECT 1,coalesce(nullif(c.reltablespace,0),(SELECT dattablespace FROM pg_database WHERE datname=current_database()))
  FROM pg_class c WHERE c.oid='record_build_inbox_payloads'::regclass ON CONFLICT DO NOTHING;
CREATE TABLE IF NOT EXISTS state_build_inbox_generations (
  generation bigint PRIMARY KEY CHECK(generation>0),
  original oid,
  retained oid,
  CHECK(original IS NOT NULL OR retained IS NOT NULL)
);

CREATE OR REPLACE FUNCTION build_inbox_current_generation() RETURNS bigint
LANGUAGE sql VOLATILE SET search_path FROM CURRENT AS $$
  SELECT floor(extract(epoch FROM clock_timestamp())/900)::bigint
$$;

CREATE OR REPLACE FUNCTION build_inbox_hash(jsonb) RETURNS bytea
LANGUAGE sql IMMUTABLE SET search_path FROM CURRENT AS $$
  SELECT sha256(convert_to(jsonb_build_array($1->>'id',$1->>'at',$1->>'record',
    $1->>'initiator',$1->>'family_source',$1->>'observation')::text,'UTF8'))
$$;

CREATE OR REPLACE FUNCTION build_inbox_prepare(bigint) RETURNS void
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE name text; current_generation bigint:=build_inbox_current_generation(); storage state_build_inbox_storage%ROWTYPE;
BEGIN
  SELECT * INTO STRICT storage FROM state_build_inbox_storage WHERE id=1 FOR UPDATE NOWAIT;
  IF $1 IS NULL OR $1<greatest(storage.closed_before,current_generation-1) OR $1>current_generation+2 THEN
    RAISE EXCEPTION 'prepare only the current retry window' USING ERRCODE='22023'; END IF;
  IF EXISTS(SELECT FROM state_build_inbox_generations WHERE generation=$1) THEN RETURN; END IF;
  LOCK TABLE ONLY record_build_inbox_payloads IN ACCESS EXCLUSIVE MODE NOWAIT;
  name:='record_build_inbox_payloads_g'||$1;
  EXECUTE format('CREATE TABLE %I PARTITION OF record_build_inbox_payloads FOR VALUES IN (%s) TABLESPACE %I',
    name,$1,(SELECT spcname FROM pg_tablespace WHERE oid=storage.tablespace));
  INSERT INTO state_build_inbox_generations(generation,original) VALUES($1,to_regclass(format('%I.%I',current_schema(),name))::oid);
END $$;

CREATE OR REPLACE FUNCTION build_inbox_enable(text,boolean) RETURNS void
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE current_generation bigint:=build_inbox_current_generation();
BEGIN
  PERFORM FROM state_build_inbox_storage WHERE id=1 FOR UPDATE NOWAIT;
  IF NOT coalesce($1~'^[0-9a-f]{64}$',false) OR $2 IS NULL
    OR 3<>(SELECT count(*) FROM state_build_inbox_generations WHERE generation BETWEEN current_generation-1 AND current_generation+1
      AND original IS NOT NULL) THEN
    RAISE EXCEPTION 'prepare the retry window and prove the deployed readers and writers' USING ERRCODE='55000'; END IF;
  UPDATE state_build_inbox_storage SET enabled=true,adopted=true,automatic=$2,readiness=$1 WHERE id=1;
END $$;

CREATE OR REPLACE FUNCTION build_inbox_submit(text,text,text,text,text,text,bigint) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE birth bigint; location bigint; made boolean; held record_build_inbox_payloads%ROWTYPE;
  input jsonb:=jsonb_build_object('id',$1,'at',$2,'record',$3,'initiator',$4,'family_source',$5,'observation',$6);
  fingerprint bytea; storage state_build_inbox_storage%ROWTYPE;
BEGIN
  IF NOT coalesce($1~'^[0-9a-f]{8}-[0-9a-f]{4}-8[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$',false)
    OR $2 IS NULL OR $3 IS NULL OR octet_length($3)>65536 OR $5 IS NOT NULL OR $6 IS NOT NULL THEN
    RAISE EXCEPTION 'ordinary input requires its generated source and bounded payload' USING ERRCODE='22023'; END IF;
  birth:=$7;
  SELECT * INTO STRICT storage FROM state_build_inbox_storage WHERE id=1 FOR SHARE;
  IF birth IS NULL OR NOT storage.enabled OR birth<greatest(storage.closed_before,build_inbox_current_generation()-1)
    OR birth>build_inbox_current_generation()+1 OR NOT EXISTS(
      SELECT FROM state_build_inbox_generations WHERE generation=birth AND original IS NOT NULL) THEN
    RAISE EXCEPTION 'this submission generation cannot accept retries' USING ERRCODE='55000'; END IF;
  fingerprint:=build_inbox_hash(input);
  LOOP
    INSERT INTO state_build_inbox_positions(id,generation,payload_generation) VALUES($1,birth,birth) ON CONFLICT DO NOTHING;
    made:=FOUND;
    SELECT p.payload_generation INTO location FROM state_build_inbox_positions p WHERE p.id=$1 AND p.generation=birth FOR UPDATE;
    EXIT WHEN FOUND;
    IF EXISTS(SELECT FROM state_build_inbox_positions WHERE id=$1) THEN
      RAISE EXCEPTION 'source belongs to another inbox' USING ERRCODE='23505'; END IF;
  END LOOP;
  SELECT * INTO held FROM record_build_inbox_payloads WHERE generation=location AND id=$1 FOR UPDATE;
  IF FOUND THEN
    IF held.input_hash IS DISTINCT FROM fingerprint THEN
      RAISE EXCEPTION 'source already owns a different input' USING ERRCODE='23505'; END IF;
    IF held.state<>'pending' THEN DELETE FROM state_build_inbox_positions WHERE id=$1; END IF;
    RETURN false;
  END IF;
  IF NOT made OR EXISTS(SELECT FROM relation_build_intake_sources WHERE source_id=$1)
    OR EXISTS(SELECT FROM record_build_inbox_payloads WHERE id=$1 AND generation<>location) THEN
    RAISE EXCEPTION 'source custody changed' USING ERRCODE='23505'; END IF;
  INSERT INTO record_build_inbox_payloads VALUES(birth,$1,$2,$3,$4,$5,$6,fingerprint,'pending');
  RETURN true;
END $$;

CREATE OR REPLACE FUNCTION build_inbox_ack(jsonb) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE location bigint; held record_build_inbox_payloads%ROWTYPE; outcome text;
BEGIN
  SELECT p.payload_generation INTO location FROM state_build_inbox_positions p WHERE p.id=$1->>'id' AND p.generation IS NOT NULL FOR UPDATE;
  IF NOT FOUND THEN
    SELECT * INTO held FROM record_build_inbox_payloads WHERE id=$1->>'id' AND state<>'pending' LIMIT 1;
    IF FOUND THEN
      IF held.input_hash IS DISTINCT FROM build_inbox_hash($1) THEN
        RAISE EXCEPTION 'captured build submission changed' USING ERRCODE='22023'; END IF;
      RETURN true;
    END IF;
    RETURN false;
  END IF;
  SELECT * INTO STRICT held FROM record_build_inbox_payloads WHERE generation=location AND id=$1->>'id' FOR UPDATE;
  IF held.state<>'pending' OR held.input_hash IS DISTINCT FROM build_inbox_hash($1) THEN
    RAISE EXCEPTION 'captured build submission changed' USING ERRCODE='22023'; END IF;
  IF EXISTS(SELECT FROM relation_build_intake_sources s JOIN entity_build_intake_intents i ON i.id=s.intent WHERE s.source_id=held.id) THEN
    outcome:='accepted';
  ELSIF $1->'rejected'='true'::jsonb THEN outcome:='refused';
  ELSE RAISE EXCEPTION 'an inbox acknowledgement needs committed custody or explicit Native refusal' USING ERRCODE='55000'; END IF;
  UPDATE record_build_inbox_payloads SET at=NULL,record=NULL,initiator=NULL,family_source=NULL,observation=NULL,state=outcome
    WHERE generation=location AND id=held.id;
  DELETE FROM state_build_inbox_positions WHERE id=held.id;
  RETURN true;
END $$;

CREATE OR REPLACE FUNCTION build_inbox_submit_family(text,text,text,text,text) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE parent entity_family_search_requests%ROWTYPE; queued state_build_inbox_positions%ROWTYPE;
  held record_build_inbox_payloads%ROWTYPE; destination bigint:=build_inbox_current_generation();
  fingerprint bytea:=build_inbox_hash(jsonb_build_object('id',$1,'at',$2,'record',$3,'initiator',$4,'family_source',$5,'observation',NULL));
BEGIN
  SELECT * INTO STRICT parent FROM entity_family_search_requests WHERE id=$5 FOR UPDATE;
  IF $1 IS DISTINCT FROM 'family:'||parent.id OR $4 IS DISTINCT FROM 'system'
    OR octet_length($3)>65536
    OR coalesce($3::jsonb->>'weapon','') IS DISTINCT FROM parent.weapon THEN
    RAISE EXCEPTION 'family input changed its authorized owner or weapon' USING ERRCODE='22023'; END IF;
  SELECT * INTO queued FROM state_build_inbox_positions WHERE id=$1 AND generation IS NOT NULL FOR UPDATE;
  IF FOUND THEN
    SELECT * INTO STRICT held FROM record_build_inbox_payloads WHERE generation=queued.payload_generation AND id=$1 FOR UPDATE;
    IF (held.record,held.initiator,held.family_source,held.observation) IS DISTINCT FROM ($3,$4,$5,NULL::text) THEN
      RAISE EXCEPTION 'family source owns a different input' USING ERRCODE='23505'; END IF;
    RETURN false;
  END IF;
  IF EXISTS(SELECT FROM relation_build_intake_sources WHERE source_id=$1) THEN
    RAISE EXCEPTION 'the family input already has formal custody' USING ERRCODE='23505'; END IF;
  IF parent.state<>'waiting' OR NOT compute_order_family_wanted(parent.id) OR EXISTS(SELECT FROM record_build_submissions WHERE id=$1) THEN
    RAISE EXCEPTION 'family retry eligibility is closed or still belongs to the bridge' USING ERRCODE='55000'; END IF;
  PERFORM FROM state_build_inbox_storage WHERE id=1 AND enabled AND closed_before<=destination FOR SHARE;
  IF NOT FOUND OR NOT EXISTS(SELECT FROM state_build_inbox_generations WHERE generation=destination AND original IS NOT NULL) THEN
    RAISE EXCEPTION 'prepare and enable the family destination' USING ERRCODE='55000'; END IF;
  INSERT INTO state_build_inbox_positions(id,generation,payload_generation) VALUES($1,destination,destination);
  INSERT INTO record_build_inbox_payloads VALUES(destination,$1,$2,$3,$4,$5,NULL,fingerprint,'pending');
  UPDATE entity_family_search_requests SET state='succeeded',completed_at=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
    WHERE id=parent.id;
  RETURN true;
END $$;

CREATE OR REPLACE FUNCTION build_inbox_family_owned(text) RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
  SELECT EXISTS(SELECT FROM relation_build_intake_sources WHERE source_id='family:'||$1)
$$;

CREATE OR REPLACE FUNCTION build_inbox_close_generations() RETURNS bigint
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE boundary bigint:=build_inbox_current_generation()-1;
BEGIN
  PERFORM FROM state_build_inbox_storage WHERE id=1 FOR UPDATE NOWAIT;
  UPDATE state_build_inbox_storage SET closed_before=greatest(closed_before,boundary) WHERE id=1 RETURNING closed_before INTO boundary;
  RETURN boundary;
END $$;

CREATE OR REPLACE FUNCTION build_inbox_legacy_write() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  IF EXISTS(SELECT FROM state_build_inbox_storage WHERE id=1 AND adopted) THEN
    RAISE EXCEPTION 'use the generation-bound submission entry' USING ERRCODE='55000'; END IF;
  RETURN NEW;
END $$;
CREATE OR REPLACE TRIGGER zy_build_inbox_legacy BEFORE INSERT ON record_build_submissions
  FOR EACH ROW EXECUTE FUNCTION build_inbox_legacy_write();

CREATE OR REPLACE FUNCTION build_inbox_adopt_arrivals(integer) RETURNS integer
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE legacy_raw record_build_submissions%ROWTYPE; queued state_build_inbox_positions%ROWTYPE;
  destination bigint:=build_inbox_current_generation(); moved integer:=0;
BEGIN
  IF $1 IS NULL OR $1 NOT BETWEEN 1 AND 64 THEN RAISE EXCEPTION 'invalid bridge budget' USING ERRCODE='22023'; END IF;
  PERFORM FROM state_build_inbox_storage WHERE id=1 AND enabled FOR SHARE;
  IF NOT FOUND OR NOT EXISTS(SELECT FROM state_build_inbox_generations WHERE generation=destination AND original IS NOT NULL) THEN
    RAISE EXCEPTION 'prepare and enable the destination first' USING ERRCODE='55000'; END IF;
  IF to_regclass('record_raw_archive_arrivals') IS NULL THEN RETURN 0; END IF;
  FOR legacy_raw IN SELECT b.* FROM ONLY record_raw_archive_arrivals b WHERE b.family_source IS NULL AND b.observation IS NULL
    AND octet_length(b.record)<=65536 ORDER BY b.id LIMIT $1 FOR UPDATE OF b SKIP LOCKED LOOP
    SELECT * INTO STRICT queued FROM state_build_inbox_positions p WHERE p.id=legacy_raw.id FOR UPDATE;
    IF queued.generation IS NOT NULL THEN RAISE EXCEPTION 'bridge input has another queue owner' USING ERRCODE='55000'; END IF;
    INSERT INTO record_build_inbox_payloads VALUES(destination,legacy_raw.id,legacy_raw.at,legacy_raw.record,legacy_raw.initiator,
      legacy_raw.family_source,legacy_raw.observation,build_inbox_hash(to_jsonb(legacy_raw)),'pending');
    DELETE FROM ONLY record_raw_archive_arrivals WHERE id=legacy_raw.id;
    INSERT INTO state_build_inbox_positions(id,position,generation,payload_generation)
      VALUES(queued.id,queued.position,destination,destination);
    moved:=moved+1;
  END LOOP;
  RETURN moved;
END $$;

CREATE OR REPLACE FUNCTION build_inbox_library_count() RETURNS bigint
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE legacy_count bigint;
BEGIN
  IF to_regprocedure('raw_inbox_library_count()') IS NOT NULL THEN
    legacy_count:=raw_inbox_library_count();
  ELSE legacy_count:=(SELECT count(*) FROM entity_weapon_builds)+(SELECT count(*) FROM record_build_submissions); END IF;
  RETURN legacy_count+(SELECT count(*) FROM state_build_inbox_positions WHERE generation IS NOT NULL);
END $$;

CREATE OR REPLACE FUNCTION build_inbox_finish_bridge() RETURNS bigint
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE bytes bigint;
BEGIN
  IF to_regclass('record_raw_archive_arrivals') IS NULL THEN RETURN 0; END IF;
  LOCK TABLE ONLY record_build_submissions IN ACCESS EXCLUSIVE MODE NOWAIT;
  LOCK TABLE ONLY record_raw_archive_arrivals IN ACCESS EXCLUSIVE MODE NOWAIT;
  IF NOT EXISTS(SELECT FROM state_build_inbox_storage WHERE id=1 AND enabled)
    OR EXISTS(SELECT FROM ONLY record_raw_archive_arrivals) THEN
    RAISE EXCEPTION 'only an empty closed arrivals bridge can release its files' USING ERRCODE='55000'; END IF;
  bytes:=pg_total_relation_size('record_raw_archive_arrivals');
  TRUNCATE ONLY record_raw_archive_arrivals;
  RETURN bytes;
END $$;

CREATE OR REPLACE FUNCTION build_inbox_prepare_retained(bigint) RETURNS void
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE child text; held state_build_inbox_generations%ROWTYPE; space text;
BEGIN
  PERFORM FROM state_build_inbox_storage WHERE id=1 AND closed_before>$1 FOR UPDATE NOWAIT;
  IF NOT FOUND THEN RAISE EXCEPTION 'close retry eligibility before carrying input' USING ERRCODE='55000'; END IF;
  SELECT * INTO STRICT held FROM state_build_inbox_generations WHERE generation=$1 FOR UPDATE;
  IF held.retained IS NOT NULL THEN RETURN; END IF;
  LOCK TABLE ONLY record_build_inbox_payloads IN ACCESS EXCLUSIVE MODE NOWAIT;
  child:='record_build_inbox_payloads_r'||$1;
  SELECT spcname INTO STRICT space FROM pg_tablespace WHERE oid=(SELECT tablespace FROM state_build_inbox_storage WHERE id=1);
  EXECUTE format('CREATE TABLE %I PARTITION OF record_build_inbox_payloads FOR VALUES IN (%s) TABLESPACE %I',child,-$1,space);
  UPDATE state_build_inbox_generations SET retained=to_regclass(format('%I.%I',current_schema(),child))::oid WHERE generation=$1;
END $$;

CREATE OR REPLACE FUNCTION build_inbox_carry(bigint,integer) RETURNS integer
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE queued record; moved integer:=0;
BEGIN
  IF $2 IS NULL OR $2 NOT BETWEEN 1 AND 64 THEN RAISE EXCEPTION 'invalid carry budget' USING ERRCODE='22023'; END IF;
  PERFORM FROM state_build_inbox_storage WHERE id=1 AND closed_before>$1 FOR SHARE;
  IF NOT FOUND OR NOT EXISTS(SELECT FROM state_build_inbox_generations WHERE generation=$1 AND retained IS NOT NULL) THEN
    RAISE EXCEPTION 'prepare a retained destination after closing retries' USING ERRCODE='55000'; END IF;
  FOR queued IN SELECT p.id FROM state_build_inbox_positions p WHERE p.generation=$1 AND p.payload_generation=$1
    ORDER BY p.id LIMIT $2 FOR UPDATE OF p SKIP LOCKED LOOP
    UPDATE record_build_inbox_payloads SET generation=-$1 WHERE generation=$1 AND id=queued.id AND state='pending';
    IF NOT FOUND THEN RAISE EXCEPTION 'queued input has no original payload' USING ERRCODE='55000'; END IF;
    UPDATE state_build_inbox_positions SET payload_generation=-$1 WHERE id=queued.id;
    moved:=moved+1;
  END LOOP;
  RETURN moved;
END $$;

CREATE OR REPLACE FUNCTION build_inbox_retire(bigint,boolean) RETURNS jsonb
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE held state_build_inbox_generations%ROWTYPE; relation oid; selected_generation bigint; bytes bigint; paths jsonb;
BEGIN
  PERFORM FROM state_build_inbox_storage WHERE id=1 AND closed_before>$1 FOR UPDATE NOWAIT;
  IF NOT FOUND OR $2 IS NULL THEN RAISE EXCEPTION 'an accepting generation cannot retire' USING ERRCODE='55000'; END IF;
  SELECT * INTO held FROM state_build_inbox_generations WHERE generation=$1 FOR UPDATE;
  IF NOT FOUND THEN RETURN jsonb_build_object('replay',true,'bytes',0); END IF;
  relation:=CASE WHEN $2 THEN held.retained ELSE held.original END;
  selected_generation:=CASE WHEN $2 THEN -$1 ELSE $1 END;
  IF relation IS NULL THEN RETURN jsonb_build_object('replay',true,'bytes',0); END IF;
  LOCK TABLE ONLY record_build_inbox_payloads IN ACCESS EXCLUSIVE MODE NOWAIT;
  IF NOT EXISTS(SELECT FROM pg_inherits i JOIN pg_class c ON c.oid=i.inhrelid
      JOIN pg_class parent ON parent.oid=i.inhparent WHERE parent.oid='record_build_inbox_payloads'::regclass AND c.oid=relation
      AND c.relowner=parent.relowner AND c.relnamespace=parent.relnamespace AND c.relkind='r' AND c.relispartition
      AND c.relname='record_build_inbox_payloads_'||CASE WHEN $2 THEN 'r' ELSE 'g' END||$1
      AND pg_get_expr(c.relpartbound,c.oid)=format('FOR VALUES IN (''%s'')',selected_generation))
    OR EXISTS(SELECT FROM state_build_inbox_positions p WHERE p.payload_generation=selected_generation)
    OR EXISTS(SELECT FROM record_build_inbox_payloads WHERE generation=selected_generation AND state='pending') THEN
    RAISE EXCEPTION 'unfinished input or changed partition ownership' USING ERRCODE='55000'; END IF;
  bytes:=pg_total_relation_size(relation);
  SELECT jsonb_agg(pg_relation_filepath(oid)) INTO paths FROM pg_class WHERE oid=relation OR oid IN(
    SELECT indexrelid FROM pg_index WHERE indrelid=relation);
  EXECUTE format('DROP TABLE %s',relation::regclass);
  IF $2 THEN UPDATE state_build_inbox_generations SET retained=NULL WHERE generation=$1 AND original IS NOT NULL;
  ELSE UPDATE state_build_inbox_generations SET original=NULL WHERE generation=$1 AND retained IS NOT NULL; END IF;
  IF NOT FOUND THEN DELETE FROM state_build_inbox_generations WHERE generation=$1; END IF;
  RETURN jsonb_build_object('replay',false,'bytes',bytes,'files',paths);
END $$;

REVOKE ALL ON state_build_inbox_storage,state_build_inbox_generations,record_build_inbox_payloads FROM PUBLIC;
REVOKE ALL ON FUNCTION build_inbox_current_generation(),build_inbox_hash(jsonb),build_inbox_prepare(bigint),build_inbox_enable(text,boolean),
  build_inbox_submit(text,text,text,text,text,text,bigint),build_inbox_submit_family(text,text,text,text,text),build_inbox_ack(jsonb),build_inbox_close_generations(),
  build_inbox_prepare_retained(bigint),build_inbox_carry(bigint,integer),build_inbox_retire(bigint,boolean),
  build_inbox_legacy_write(),build_inbox_adopt_arrivals(integer),build_inbox_library_count(),build_inbox_finish_bridge(),build_inbox_family_owned(text) FROM PUBLIC;
