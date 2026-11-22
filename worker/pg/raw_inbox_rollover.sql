-- Installed by the storage owner on the archive tablespace.
CREATE TABLE state_raw_inbox_rollover (
  id integer PRIMARY KEY CHECK(id=1),
  generation uuid,
  phase text NOT NULL CHECK(phase IN ('idle','copying','draining')),
  source_oid oid NOT NULL,
  source_fingerprint jsonb
);
INSERT INTO state_raw_inbox_rollover(id,phase,source_oid) VALUES(1,'idle','record_build_submissions'::regclass::oid);
CREATE TABLE state_raw_inbox_archive_cursor (
  generation uuid PRIMARY KEY,
  after_id text COLLATE "C",
  next_part bigint NOT NULL DEFAULT 1,
  cut_at text COLLATE "C",
  completed_at text COLLATE "C",
  aborted_at text COLLATE "C"
);
CREATE TABLE record_raw_inbox_archive_chunks (
  generation uuid NOT NULL REFERENCES state_raw_inbox_archive_cursor(generation),
  part bigint NOT NULL,
  digest text NOT NULL CHECK(digest~'^[a-f0-9]{64}$'),
  rows integer NOT NULL,
  loaded_at text COLLATE "C",
  PRIMARY KEY(generation,part)
);
CREATE TABLE state_raw_inbox_archive_claims (
  id text COLLATE "C" PRIMARY KEY,
  generation uuid NOT NULL,
  part bigint NOT NULL,
  digest bytea NOT NULL,
  materializing boolean NOT NULL DEFAULT false,
  FOREIGN KEY(generation,part) REFERENCES record_raw_inbox_archive_chunks(generation,part)
);
CREATE INDEX raw_inbox_claims_part ON state_raw_inbox_archive_claims(generation,part);
CREATE TABLE state_raw_inbox_hot_spool (
  id text COLLATE "C" PRIMARY KEY,
  input jsonb NOT NULL,
  arrival boolean NOT NULL
);

CREATE FUNCTION raw_inbox_fingerprint(jsonb) RETURNS bytea
LANGUAGE sql IMMUTABLE STRICT SET search_path FROM CURRENT AS $$
  SELECT sha256(convert_to($1::text,'UTF8'))
$$;

CREATE FUNCTION raw_inbox_portable(record_build_submissions,bigint) RETURNS boolean
LANGUAGE sql STABLE SET search_path FROM CURRENT AS $$
  SELECT $2 IS NULL AND $1.observation IS NULL
    AND $1.id~'^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
    AND split_part(coalesce($1.family_source,''),'|',1)<>'workflow'
    AND NOT EXISTS(SELECT FROM state_build_family_inbox f WHERE f.id=$1.id)
    AND NOT EXISTS(SELECT FROM entity_board_requests r WHERE r.id=$1.id)
    AND NOT EXISTS(SELECT FROM relation_task_consumers r WHERE r.ref=$1.id AND r.state='live')
$$;

CREATE FUNCTION raw_inbox_rollover_guard() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE mode text; held state_raw_inbox_archive_claims%ROWTYPE;
BEGIN
  SELECT phase INTO STRICT mode FROM state_raw_inbox_rollover WHERE id=1 FOR SHARE;
  IF TG_OP='TRUNCATE' THEN
    IF mode<>'idle' THEN RAISE EXCEPTION 'raw inbox has an active archive'; END IF;
    RETURN NULL;
  END IF;
  IF mode='idle' THEN RETURN NEW; END IF;
  IF TG_OP='UPDATE' THEN RAISE EXCEPTION 'raw inbox is owned by an active archive'; END IF;
  IF mode='draining' THEN
    SELECT * INTO held FROM state_raw_inbox_archive_claims WHERE id=NEW.id FOR SHARE;
    IF FOUND THEN
      IF held.digest<>raw_inbox_fingerprint(to_jsonb(NEW)) THEN
        RAISE EXCEPTION 'archived raw inbox ID owns different input' USING ERRCODE='23505'; END IF;
      IF NOT held.materializing THEN RETURN NULL; END IF;
    END IF;
  END IF;
  RETURN NEW;
END $$;

CREATE FUNCTION raw_inbox_rollover_track() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE mode text;
BEGIN
  SELECT phase INTO STRICT mode FROM state_raw_inbox_rollover WHERE id=1 FOR SHARE;
  IF TG_OP='DELETE' THEN
    IF mode='copying' THEN
      DELETE FROM state_raw_inbox_hot_spool WHERE id=OLD.id;
      DELETE FROM state_raw_inbox_archive_claims WHERE id=OLD.id;
    END IF;
    RETURN OLD;
  END IF;
  IF mode='copying' THEN
    INSERT INTO state_raw_inbox_hot_spool VALUES(NEW.id,to_jsonb(NEW),true);
  END IF;
  RETURN NEW;
END $$;

CREATE FUNCTION raw_inbox_rollover_position() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  IF EXISTS(SELECT FROM state_raw_inbox_archive_claims WHERE id=NEW.id AND NOT materializing) THEN
    RAISE EXCEPTION 'a cold raw inbox input cannot acquire a new FIFO position'; END IF;
  RETURN NEW;
END $$;

CREATE FUNCTION raw_inbox_core_fingerprint() RETURNS jsonb
LANGUAGE sql STABLE SET search_path FROM CURRENT AS $$
  SELECT jsonb_object_agg(w.signature,encode(sha256(convert_to(p.prosrc,'UTF8')),'hex'))
  FROM (VALUES ('build_ack_submissions(text)'),('build_inbox_position()'),('build_family_capture_output()'),
        ('board_request_capture_review()')) w(signature) JOIN pg_proc p ON p.oid=to_regprocedure(w.signature)
$$;
CREATE FUNCTION raw_inbox_rollover_shape(boolean) RETURNS void
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE expected text[]:=ARRAY['board_requested_review','build_family_output_ready','build_inbox_positioned'];
BEGIN
  IF $1 THEN expected:=ARRAY['aa_raw_inbox_rollover_guard','board_requested_review','build_family_output_ready',
    'build_inbox_positioned','raw_inbox_rollover_truncate','zz_raw_inbox_rollover_track']; END IF;
  IF current_setting('session_replication_role')<>'origin'
    OR (SELECT source_oid FROM state_raw_inbox_rollover WHERE id=1)<>'record_build_submissions'::regclass::oid
    OR (SELECT source_fingerprint FROM state_raw_inbox_rollover WHERE id=1) IS DISTINCT FROM raw_inbox_core_fingerprint()
    OR EXISTS(SELECT FROM pg_constraint WHERE contype='f' AND confrelid='record_build_submissions'::regclass)
    OR (SELECT array_agg(tgname::text ORDER BY tgname) FROM pg_trigger
        WHERE tgrelid='record_build_submissions'::regclass AND NOT tgisinternal AND tgenabled='O') IS DISTINCT FROM expected
    OR EXISTS(SELECT FROM pg_trigger WHERE tgrelid='record_build_submissions'::regclass
        AND NOT tgisinternal AND tgenabled<>'O')
    OR NOT EXISTS(SELECT FROM pg_trigger WHERE tgrelid='record_build_submissions'::regclass
        AND tgname='build_inbox_positioned' AND tgfoid='build_inbox_position()'::regprocedure AND tgtype=13)
    OR NOT EXISTS(SELECT FROM pg_trigger WHERE tgrelid='record_build_submissions'::regclass
        AND tgname='board_requested_review' AND tgfoid='board_request_capture_review()'::regprocedure AND tgtype=7)
    OR NOT EXISTS(SELECT FROM pg_trigger WHERE tgrelid='record_build_submissions'::regclass
        AND tgname='build_family_output_ready' AND tgfoid='build_family_capture_output()'::regprocedure AND tgtype=7)
    OR ($1 AND NOT EXISTS(SELECT FROM pg_trigger WHERE tgrelid='state_build_inbox_positions'::regclass
      AND tgname='raw_inbox_rollover_position' AND tgfoid='raw_inbox_rollover_position()'::regprocedure AND tgenabled='O')) THEN
    RAISE EXCEPTION 'raw inbox archive requires the detached trigger and foreign key profile';
  END IF;
END $$;

CREATE FUNCTION raw_inbox_rollover_begin(uuid) RETURNS void
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
BEGIN
  LOCK TABLE record_build_submissions IN ACCESS EXCLUSIVE MODE NOWAIT;
  PERFORM raw_inbox_rollover_shape(false);
  UPDATE state_raw_inbox_rollover SET generation=$1,phase='copying' WHERE id=1 AND phase='idle';
  IF NOT FOUND THEN RAISE EXCEPTION 'a raw inbox archive is already active'; END IF;
  INSERT INTO state_raw_inbox_archive_cursor(generation) VALUES($1);
  CREATE TRIGGER aa_raw_inbox_rollover_guard BEFORE INSERT OR UPDATE ON record_build_submissions
    FOR EACH ROW EXECUTE FUNCTION raw_inbox_rollover_guard();
  CREATE TRIGGER raw_inbox_rollover_truncate BEFORE TRUNCATE ON record_build_submissions
    FOR EACH STATEMENT EXECUTE FUNCTION raw_inbox_rollover_guard();
  CREATE TRIGGER zz_raw_inbox_rollover_track AFTER INSERT OR DELETE ON record_build_submissions
    FOR EACH ROW EXECUTE FUNCTION raw_inbox_rollover_track();
  CREATE TRIGGER raw_inbox_rollover_position BEFORE INSERT OR UPDATE ON state_build_inbox_positions
    FOR EACH ROW EXECUTE FUNCTION raw_inbox_rollover_position();
END $$;

CREATE FUNCTION raw_inbox_archive_page(integer,integer) RETURNS jsonb
LANGUAGE plpgsql STABLE SET search_path FROM CURRENT AS $$
DECLARE active uuid; progress state_raw_inbox_archive_cursor%ROWTYPE; page jsonb; scanned integer;
BEGIN
  IF $1 NOT BETWEEN 1 AND 512 OR $2 NOT BETWEEN 4096 AND 8388608 OR $1 IS NULL OR $2 IS NULL THEN
    RAISE EXCEPTION 'invalid raw inbox archive page limits'; END IF;
  SELECT generation INTO STRICT active FROM state_raw_inbox_rollover WHERE id=1 AND phase='copying';
  SELECT * INTO STRICT progress FROM state_raw_inbox_archive_cursor WHERE generation=active;
  WITH source AS MATERIALIZED (
    SELECT b.id,jsonb_build_object('input',to_jsonb(b),'position',p.position::text,
      'portable',raw_inbox_portable(b,p.position)) AS item
    FROM record_build_submissions b LEFT JOIN state_build_inbox_positions p USING(id)
    WHERE (progress.after_id IS NULL OR b.id>progress.after_id)
      AND NOT EXISTS(SELECT FROM state_raw_inbox_hot_spool hot WHERE hot.id=b.id AND hot.arrival)
    ORDER BY b.id LIMIT $1
  ), bounded AS (
    SELECT *,sum(octet_length(item::text)+2) OVER(ORDER BY id) bytes FROM source
  )
  SELECT jsonb_build_object('generation',active,'part',progress.next_part,'after',progress.after_id,
    'rows',coalesce(jsonb_agg(item ORDER BY id) FILTER(WHERE bytes<=$2),'[]'::jsonb)),
    count(*) INTO page,scanned FROM bounded;
  IF jsonb_array_length(page->'rows')=0 AND scanned>0 THEN
    RAISE EXCEPTION 'one raw inbox input exceeds the archive page limit'; END IF;
  RETURN page;
END $$;

CREATE FUNCTION raw_inbox_archive_accept(jsonb,text) RETURNS bigint
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE active uuid; progress state_raw_inbox_archive_cursor%ROWTYPE; rows jsonb:=$1->'rows'; expected jsonb; last_id text;
BEGIN
  IF jsonb_typeof(rows) IS DISTINCT FROM 'array' OR jsonb_array_length(rows) NOT BETWEEN 1 AND 512
    OR octet_length($1::text)>8392704 OR $2 IS DISTINCT FROM encode(raw_inbox_fingerprint($1),'hex') THEN
    RAISE EXCEPTION 'raw inbox archive digest or bounds changed'; END IF;
  LOCK TABLE record_build_submissions IN ROW SHARE MODE;
  SELECT generation INTO STRICT active FROM state_raw_inbox_rollover WHERE id=1 AND phase='copying' FOR SHARE;
  SELECT * INTO STRICT progress FROM state_raw_inbox_archive_cursor WHERE generation=active FOR UPDATE;
  IF $1->>'generation' IS DISTINCT FROM active::text OR $1->>'part' IS DISTINCT FROM progress.next_part::text
    OR $1->>'after' IS DISTINCT FROM progress.after_id THEN
    RAISE EXCEPTION 'raw inbox archive cursor changed'; END IF;
  SELECT max(item#>>'{input,id}' COLLATE "C") INTO last_id FROM jsonb_array_elements(rows) item;
  SELECT jsonb_agg(item ORDER BY id) INTO expected FROM (
    SELECT b.id,jsonb_build_object('input',to_jsonb(b),'position',p.position::text,
      'portable',raw_inbox_portable(b,p.position)) AS item
    FROM record_build_submissions b LEFT JOIN state_build_inbox_positions p USING(id)
    WHERE (progress.after_id IS NULL OR b.id>progress.after_id) AND b.id<=last_id
      AND NOT EXISTS(SELECT FROM state_raw_inbox_hot_spool hot WHERE hot.id=b.id AND hot.arrival)
    ORDER BY b.id LIMIT jsonb_array_length(rows)+1 FOR SHARE OF b
  ) current_page;
  IF expected IS DISTINCT FROM rows THEN RAISE EXCEPTION 'raw inbox archive source changed before acknowledgement'; END IF;
  INSERT INTO record_raw_inbox_archive_chunks(generation,part,digest,rows)
    VALUES(active,progress.next_part,$2,jsonb_array_length(rows));
  INSERT INTO state_raw_inbox_hot_spool
    SELECT item#>>'{input,id}',item->'input',false FROM jsonb_array_elements(rows) item WHERE NOT (item->>'portable')::boolean;
  INSERT INTO state_raw_inbox_archive_claims(id,generation,part,digest)
    SELECT item#>>'{input,id}',active,progress.next_part,raw_inbox_fingerprint(item->'input')
    FROM jsonb_array_elements(rows) item WHERE (item->>'portable')::boolean;
  UPDATE state_raw_inbox_archive_cursor SET after_id=last_id,next_part=progress.next_part+1 WHERE generation=active;
  RETURN progress.next_part;
END $$;

CREATE FUNCTION raw_inbox_rollover_cut(uuid,bigint,bigint) RETURNS jsonb
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE progress state_raw_inbox_archive_cursor%ROWTYPE; hot_rows bigint; hot_bytes bigint; added bigint;
  old_bytes bigint;
BEGIN
  IF $2 IS NULL OR $2<0 OR $3 IS NULL OR $3<0 THEN RAISE EXCEPTION 'invalid raw inbox cut budget'; END IF;
  LOCK TABLE record_build_submissions IN ACCESS EXCLUSIVE MODE NOWAIT;
  PERFORM raw_inbox_rollover_shape(true);
  IF NOT EXISTS(SELECT FROM state_raw_inbox_rollover WHERE id=1 AND generation=$1 AND phase='copying') THEN
    RAISE EXCEPTION 'raw inbox generation is not available for cut'; END IF;
  SELECT * INTO STRICT progress FROM state_raw_inbox_archive_cursor WHERE generation=$1;
  IF EXISTS(SELECT FROM record_build_submissions b WHERE (progress.after_id IS NULL OR b.id>progress.after_id)
    AND NOT EXISTS(SELECT FROM state_raw_inbox_hot_spool h WHERE h.id=b.id AND h.arrival)) THEN
    RAISE EXCEPTION 'raw inbox archive has a source range that is still local'; END IF;
  IF EXISTS(SELECT FROM (SELECT 1 FROM state_raw_inbox_hot_spool LIMIT 1 OFFSET $2) excess) THEN
    RAISE EXCEPTION 'raw inbox hot spool exceeds the row budget'; END IF;
  SELECT count(*),coalesce(sum(octet_length(input::text)),0) INTO hot_rows,hot_bytes FROM state_raw_inbox_hot_spool;
  IF hot_bytes>$3 THEN RAISE EXCEPTION 'raw inbox hot spool exceeds the byte budget'; END IF;
  old_bytes:=pg_total_relation_size('record_build_submissions');
  ALTER TABLE record_build_submissions DISABLE TRIGGER USER;
  TRUNCATE record_build_submissions;
  INSERT INTO record_build_submissions
    SELECT (jsonb_populate_record(NULL::record_build_submissions,input)).* FROM state_raw_inbox_hot_spool;
  GET DIAGNOSTICS added=ROW_COUNT;
  IF added<>hot_rows THEN RAISE EXCEPTION 'raw inbox hot spool copy changed'; END IF;
  ALTER TABLE record_build_submissions ENABLE TRIGGER USER;
  TRUNCATE state_raw_inbox_hot_spool;
  UPDATE state_raw_inbox_archive_cursor
    SET cut_at=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') WHERE generation=$1;
  UPDATE state_raw_inbox_rollover SET phase='draining' WHERE id=1;
  RETURN jsonb_build_object('generation',$1,'old_bytes',old_bytes,'new_bytes',pg_total_relation_size('record_build_submissions'),
    'hot_rows',hot_rows,'hot_bytes',hot_bytes);
END $$;

CREATE FUNCTION raw_inbox_rollover_untrack() RETURNS void
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
BEGIN
  DROP TRIGGER aa_raw_inbox_rollover_guard ON record_build_submissions;
  DROP TRIGGER raw_inbox_rollover_truncate ON record_build_submissions;
  DROP TRIGGER zz_raw_inbox_rollover_track ON record_build_submissions;
  DROP TRIGGER raw_inbox_rollover_position ON state_build_inbox_positions;
END $$;

CREATE FUNCTION raw_inbox_rollover_abort(uuid) RETURNS void
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
BEGIN
  LOCK TABLE record_build_submissions IN ACCESS EXCLUSIVE MODE NOWAIT;
  UPDATE state_raw_inbox_rollover SET phase='idle' WHERE id=1 AND generation=$1 AND phase='copying';
  IF NOT FOUND THEN RAISE EXCEPTION 'only a raw inbox generation before cut can be abandoned'; END IF;
  PERFORM raw_inbox_rollover_untrack();
  TRUNCATE state_raw_inbox_hot_spool,state_raw_inbox_archive_claims;
  UPDATE state_raw_inbox_archive_cursor
    SET aborted_at=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') WHERE generation=$1;
END $$;

CREATE FUNCTION raw_inbox_rollover_ack(record_build_submissions) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held state_raw_inbox_archive_claims%ROWTYPE;
BEGIN
  LOCK TABLE record_build_submissions IN ROW SHARE MODE;
  PERFORM 1 FROM state_raw_inbox_rollover WHERE id=1 AND phase='draining' FOR SHARE;
  IF NOT FOUND THEN RETURN; END IF;
  SELECT * INTO held FROM state_raw_inbox_archive_claims WHERE id=$1.id FOR UPDATE;
  IF NOT FOUND THEN
    IF EXISTS(SELECT FROM record_build_submissions b WHERE b.id=$1.id AND to_jsonb(b)=to_jsonb($1)) THEN
      DELETE FROM record_build_submissions b WHERE b.id=$1.id AND to_jsonb(b)=to_jsonb($1);
    ELSIF EXISTS(SELECT FROM record_build_submissions b WHERE b.id=$1.id) THEN
      RAISE EXCEPTION 'captured archive input changed';
    END IF;
    RETURN;
  END IF;
  IF held.digest<>raw_inbox_fingerprint(to_jsonb($1)) THEN RAISE EXCEPTION 'captured archive input changed'; END IF;
  DELETE FROM state_raw_inbox_archive_claims WHERE id=held.id;
END $$;

CREATE FUNCTION raw_inbox_archive_load(jsonb) RETURNS jsonb
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE active uuid; part_id bigint:=($1->>'part')::bigint; chunk record_raw_inbox_archive_chunks%ROWTYPE;
  item jsonb; inbox record_build_submissions%ROWTYPE; inserted bigint:=0; prior bigint;
BEGIN
  IF jsonb_typeof($1->'rows') IS DISTINCT FROM 'array' OR octet_length($1::text)>8392704 THEN
    RAISE EXCEPTION 'invalid raw inbox recovery packet'; END IF;
  LOCK TABLE record_build_submissions IN ROW EXCLUSIVE MODE;
  SELECT generation INTO STRICT active FROM state_raw_inbox_rollover WHERE id=1 AND phase='draining' FOR SHARE;
  IF $1->>'generation' IS DISTINCT FROM active::text THEN RAISE EXCEPTION 'raw inbox generation changed'; END IF;
  SELECT * INTO STRICT chunk FROM record_raw_inbox_archive_chunks WHERE generation=active AND part=part_id FOR UPDATE;
  IF chunk.digest<>encode(raw_inbox_fingerprint($1),'hex') OR chunk.rows<>jsonb_array_length($1->'rows') THEN
    RAISE EXCEPTION 'raw inbox recovery packet changed'; END IF;
  IF chunk.loaded_at IS NOT NULL THEN RETURN jsonb_build_object('loaded',0,'replay',true); END IF;
  FOR item IN SELECT value FROM jsonb_array_elements($1->'rows') WHERE (value->>'portable')::boolean LOOP
    inbox:=jsonb_populate_record(NULL::record_build_submissions,item->'input');
    UPDATE state_raw_inbox_archive_claims SET materializing=true WHERE id=inbox.id AND generation=active AND part=part_id
      AND digest=raw_inbox_fingerprint(item->'input');
    IF NOT FOUND THEN
      IF EXISTS(SELECT FROM state_raw_inbox_archive_claims WHERE id=inbox.id) THEN RAISE EXCEPTION 'raw inbox recovery claim changed'; END IF;
      CONTINUE;
    END IF;
    IF NOT raw_inbox_portable(inbox,NULL)
      OR EXISTS(SELECT FROM state_build_inbox_positions WHERE id=inbox.id) THEN
      RAISE EXCEPTION 'an archived input acquired a live owner or queue position'; END IF;
    INSERT INTO record_build_submissions SELECT inbox.*;
    GET DIAGNOSTICS prior=ROW_COUNT;
    IF prior<>1 THEN RAISE EXCEPTION 'raw inbox recovery was redirected'; END IF;
    DELETE FROM state_build_inbox_positions WHERE id=inbox.id;
    DELETE FROM state_raw_inbox_archive_claims WHERE id=inbox.id AND materializing;
    inserted:=inserted+1;
  END LOOP;
  UPDATE record_raw_inbox_archive_chunks
    SET loaded_at=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
    WHERE generation=active AND part=part_id;
  RETURN jsonb_build_object('loaded',inserted,'replay',false);
END $$;

CREATE FUNCTION raw_inbox_rollover_finish(uuid) RETURNS void
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
BEGIN
  LOCK TABLE record_build_submissions IN ACCESS EXCLUSIVE MODE NOWAIT;
  IF NOT EXISTS(SELECT FROM state_raw_inbox_rollover WHERE id=1 AND phase='draining' AND generation=$1)
    OR EXISTS(SELECT FROM state_raw_inbox_archive_claims)
    OR EXISTS(SELECT FROM record_raw_inbox_archive_chunks WHERE generation=$1 AND loaded_at IS NULL) THEN
    RAISE EXCEPTION 'raw inbox generation still holds cold inputs'; END IF;
  TRUNCATE state_raw_inbox_archive_claims;
  PERFORM raw_inbox_rollover_untrack();
  UPDATE state_raw_inbox_archive_cursor
    SET completed_at=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') WHERE generation=$1;
  UPDATE state_raw_inbox_rollover SET phase='idle' WHERE id=1;
END $$;

CREATE FUNCTION raw_inbox_library_count() RETURNS bigint
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  IF EXISTS(SELECT FROM state_raw_inbox_rollover WHERE id=1 AND phase='draining') THEN RETURN NULL; END IF;
  RETURN (SELECT count(*) FROM entity_weapon_builds)+(SELECT count(*) FROM record_build_submissions);
END $$;

REVOKE ALL ON state_raw_inbox_rollover,state_raw_inbox_archive_cursor,record_raw_inbox_archive_chunks,
  state_raw_inbox_archive_claims,state_raw_inbox_hot_spool FROM PUBLIC;
REVOKE ALL ON FUNCTION raw_inbox_fingerprint(jsonb),raw_inbox_portable(record_build_submissions,bigint),
  raw_inbox_rollover_guard(),raw_inbox_rollover_track(),raw_inbox_rollover_position(),raw_inbox_core_fingerprint(),
  raw_inbox_rollover_shape(boolean),raw_inbox_rollover_begin(uuid),
  raw_inbox_archive_page(integer,integer),raw_inbox_archive_accept(jsonb,text),raw_inbox_rollover_cut(uuid,bigint,bigint),
  raw_inbox_rollover_abort(uuid),raw_inbox_rollover_untrack(),raw_inbox_rollover_ack(record_build_submissions),raw_inbox_archive_load(jsonb),
  raw_inbox_rollover_finish(uuid),raw_inbox_library_count() FROM PUBLIC;
