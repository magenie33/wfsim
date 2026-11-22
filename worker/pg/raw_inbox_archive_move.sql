-- The storage owner installs both archive modules in the data tablespace.
CREATE TABLE state_raw_archive_move (
  id integer PRIMARY KEY CHECK(id=1),
  phase text NOT NULL CHECK(phase IN('idle','copying','cutting','complete')),
  generation uuid REFERENCES state_raw_archive_generation(generation),
  source_oid oid NOT NULL,
  after_id text COLLATE "C",
  upload_digest text CHECK(upload_digest~'^[0-9a-f]{64}$')
);
INSERT INTO state_raw_archive_move VALUES(1,'idle',NULL,'record_build_submissions'::regclass::oid,NULL,NULL);

CREATE TABLE record_raw_archive_hot (LIKE record_build_submissions INCLUDING ALL);
CREATE TABLE record_raw_archive_arrivals (LIKE record_build_submissions INCLUDING ALL);
CREATE TRIGGER archive_hot_position AFTER DELETE ON record_raw_archive_hot
  FOR EACH ROW EXECUTE FUNCTION build_inbox_position();
CREATE TRIGGER archive_arrival_position AFTER INSERT OR DELETE ON record_raw_archive_arrivals
  FOR EACH ROW EXECUTE FUNCTION build_inbox_position();

CREATE FUNCTION raw_archive_move_identity() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.id IS DISTINCT FROM OLD.id THEN RAISE EXCEPTION 'raw archive source identity is immutable' USING ERRCODE='55000'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER archive_hot_identity BEFORE UPDATE OF id ON record_raw_archive_hot
  FOR EACH ROW EXECUTE FUNCTION raw_archive_move_identity();
CREATE TRIGGER archive_arrival_identity BEFORE UPDATE OF id ON record_raw_archive_arrivals
  FOR EACH ROW EXECUTE FUNCTION raw_archive_move_identity();

CREATE FUNCTION raw_archive_move_route() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE mode text;
BEGIN
  SELECT phase INTO STRICT mode FROM state_raw_archive_move WHERE id=1;
  IF mode='idle' AND NOT EXISTS(SELECT FROM pg_inherits WHERE inhrelid='record_raw_archive_arrivals'::regclass) THEN RETURN NEW; END IF;
  IF raw_archive_legacy_id(NEW.id) THEN RAISE EXCEPTION 'old raw archive ID cannot be written again' USING ERRCODE='23505'; END IF;
  PERFORM FROM record_build_submissions WHERE id=NEW.id FOR KEY SHARE;
  IF FOUND THEN RETURN NULL; END IF;
  INSERT INTO record_raw_archive_arrivals SELECT NEW.* ON CONFLICT(id) DO NOTHING;
  RETURN NULL;
END $$;

CREATE FUNCTION raw_archive_move_freeze() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE mode text;
BEGIN
  SELECT phase INTO STRICT mode FROM state_raw_archive_move WHERE id=1;
  IF TG_OP='TRUNCATE' THEN
    IF mode<>'cutting' THEN RAISE EXCEPTION 'raw archive source truncation requires its cut'; END IF;
    RETURN NULL;
  END IF;
  IF mode='copying' THEN RAISE EXCEPTION 'raw archive source generation is frozen' USING ERRCODE='55000'; END IF;
  IF TG_OP='DELETE' THEN RETURN OLD; END IF;
  RETURN NEW;
END $$;

CREATE FUNCTION raw_archive_move_owner_fence() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE source_id text; mode text;
BEGIN
  IF TG_TABLE_NAME='relation_task_consumers' THEN
    IF NEW.state<>'live' THEN RETURN NEW; END IF;
    source_id:=NEW.ref;
  ELSE source_id:=NEW.id; END IF;
  IF NOT coalesce(raw_archive_legacy_id(source_id),false) THEN RETURN NEW; END IF;
  LOCK TABLE ONLY record_build_submissions IN ACCESS SHARE MODE;
  SELECT phase INTO STRICT mode FROM state_raw_archive_move WHERE id=1;
  IF mode='copying' AND EXISTS(SELECT FROM ONLY record_build_submissions WHERE id=source_id) THEN
    RAISE EXCEPTION 'cannot create an owner for a frozen archive input' USING ERRCODE='55000';
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER zz_raw_archive_route BEFORE INSERT ON record_build_submissions
  FOR EACH ROW EXECUTE FUNCTION raw_archive_move_route();
CREATE TRIGGER raw_archive_freeze BEFORE UPDATE OR DELETE ON record_build_submissions
  FOR EACH ROW EXECUTE FUNCTION raw_archive_move_freeze();
CREATE TRIGGER raw_archive_truncate BEFORE TRUNCATE ON record_build_submissions
  FOR EACH STATEMENT EXECUTE FUNCTION raw_archive_move_freeze();
CREATE TRIGGER raw_archive_position_owner BEFORE INSERT OR UPDATE OF id ON state_build_inbox_positions
  FOR EACH ROW EXECUTE FUNCTION raw_archive_move_owner_fence();
CREATE TRIGGER raw_archive_family_owner BEFORE INSERT OR UPDATE OF id ON state_build_family_inbox
  FOR EACH ROW EXECUTE FUNCTION raw_archive_move_owner_fence();
CREATE TRIGGER raw_archive_request_owner BEFORE INSERT OR UPDATE OF id ON entity_board_requests
  FOR EACH ROW EXECUTE FUNCTION raw_archive_move_owner_fence();
CREATE TRIGGER raw_archive_task_owner BEFORE INSERT OR UPDATE OF ref,state ON relation_task_consumers
  FOR EACH ROW EXECUTE FUNCTION raw_archive_move_owner_fence();

CREATE FUNCTION raw_archive_move_begin(uuid,text) RETURNS void LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
BEGIN
  LOCK TABLE ONLY record_build_submissions IN ACCESS EXCLUSIVE MODE NOWAIT;
  IF to_regclass('state_raw_inbox_rollover') IS NOT NULL
    OR EXISTS(SELECT FROM pg_constraint WHERE contype='f' AND confrelid='record_build_submissions'::regclass)
    OR NOT EXISTS(SELECT FROM state_raw_archive_move WHERE id=1 AND phase='idle'
      AND source_oid='record_build_submissions'::regclass::oid) THEN
    RAISE EXCEPTION 'raw archive requires the detached source table' USING ERRCODE='55000';
  END IF;
  PERFORM raw_archive_begin($1,$2);
  UPDATE state_raw_archive_move SET generation=$1,phase='copying',after_id=NULL WHERE id=1;
END $$;

CREATE FUNCTION raw_archive_move_portable(record_build_submissions) RETURNS boolean
LANGUAGE sql STABLE SET search_path FROM CURRENT AS $$
  SELECT raw_archive_legacy_id($1.id) AND $1.observation IS NULL
    AND split_part(coalesce($1.family_source,''),'|',1)<>'workflow'
    AND coalesce(CASE WHEN pg_input_is_valid($1.record,'jsonb') THEN $1.record::jsonb->>'kind' END,'') NOT IN('warframe','companion')
    AND NOT EXISTS(SELECT FROM state_build_inbox_positions WHERE id=$1.id)
    AND NOT EXISTS(SELECT FROM state_build_family_inbox WHERE id=$1.id)
    AND NOT EXISTS(SELECT FROM entity_board_requests WHERE id=$1.id)
    AND NOT EXISTS(SELECT FROM relation_task_consumers WHERE ref=$1.id AND state='live')
$$;

CREATE FUNCTION raw_archive_move_page(integer,integer) RETURNS jsonb
LANGUAGE plpgsql STABLE SET search_path FROM CURRENT AS $$
DECLARE active state_raw_archive_move%ROWTYPE; part bigint; packet jsonb; scanned integer;
BEGIN
  IF $1 IS NULL OR $1 NOT BETWEEN 1 AND 512 OR $2 IS NULL OR $2 NOT BETWEEN 4096 AND 8388608 THEN
    RAISE EXCEPTION 'invalid raw archive source bounds'; END IF;
  SELECT * INTO STRICT active FROM state_raw_archive_move WHERE id=1 AND phase='copying';
  SELECT sealed_parts+1 INTO STRICT part FROM state_raw_archive_generation WHERE generation=active.generation AND phase='sealing';
  WITH sizes AS MATERIALIZED (
    SELECT b.id,6::bigint*(octet_length(b.id)::bigint+octet_length(b.at)+octet_length(b.record)
      +coalesce(octet_length(b.initiator),0)+coalesce(octet_length(b.family_source),0)
      +coalesce(octet_length(b.observation),0))+192 AS bytes FROM ONLY record_build_submissions b
    WHERE active.after_id IS NULL OR b.id>active.after_id ORDER BY b.id LIMIT $1
  ), measured AS (
    SELECT id,sum(bytes) OVER(ORDER BY id) AS used FROM sizes
  ), source AS MATERIALIZED (
    SELECT b.id,to_jsonb(b) AS payload,raw_archive_move_portable(b) AS portable
    FROM measured m JOIN ONLY record_build_submissions b USING(id) WHERE m.used<=$2
  )
  SELECT jsonb_build_object('generation',active.generation,'part',part,'after',active.after_id,
    'last',max(id),'source_rows',count(*),
    'rows',coalesce(jsonb_agg(jsonb_build_object('input',payload,'portable',true,'position',NULL) ORDER BY id)
      FILTER(WHERE portable),'[]'::jsonb)),(SELECT count(*) FROM sizes) INTO packet,scanned FROM source;
  IF scanned=0 THEN RETURN NULL; END IF;
  IF packet->>'last' IS NULL THEN RAISE EXCEPTION 'one raw input exceeds the archive page limit'; END IF;
  RETURN packet;
END $$;

CREATE FUNCTION raw_archive_move_prepare(integer,integer) RETURNS text
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE active state_raw_archive_move%ROWTYPE; packet jsonb; body text; digest text;
BEGIN
  LOCK TABLE ONLY record_build_submissions IN ROW SHARE MODE;
  SELECT * INTO STRICT active FROM state_raw_archive_move WHERE id=1 AND phase='copying' FOR UPDATE;
  packet:=raw_archive_move_page($1,$2);
  IF packet IS NULL THEN RETURN NULL; END IF;
  body:=packet::text;
  IF jsonb_array_length(packet->'rows')>0 THEN
    digest:=encode(sha256(convert_to(body,'UTF8')),'hex');
    INSERT INTO record_raw_archive_uploads VALUES(active.generation,(packet->>'part')::bigint,digest) ON CONFLICT DO NOTHING;
  END IF;
  UPDATE state_raw_archive_move SET upload_digest=digest WHERE id=1;
  RETURN body;
END $$;

CREATE FUNCTION raw_archive_move_accept(text,text,integer,integer) RETURNS jsonb
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE active state_raw_archive_move%ROWTYPE; packet jsonb; kept integer; cold integer; tail text;
BEGIN
  LOCK TABLE ONLY record_build_submissions IN ROW SHARE MODE;
  SELECT * INTO STRICT active FROM state_raw_archive_move WHERE id=1 AND phase='copying' FOR UPDATE;
  packet:=raw_archive_move_page($3,$4);
  IF packet IS NULL OR $1 IS DISTINCT FROM packet::text OR $2 IS DISTINCT FROM encode(sha256(convert_to($1,'UTF8')),'hex') THEN
    RAISE EXCEPTION 'raw archive source or readback changed' USING ERRCODE='55000'; END IF;
  cold:=jsonb_array_length(packet->'rows');
  tail:=packet->>'last';
  INSERT INTO record_raw_archive_hot
    SELECT b.* FROM ONLY record_build_submissions b WHERE (active.after_id IS NULL OR b.id>active.after_id)
      AND b.id<=tail AND NOT raw_archive_move_portable(b);
  GET DIAGNOSTICS kept=ROW_COUNT;
  IF kept+cold<>(packet->>'source_rows')::int THEN RAISE EXCEPTION 'raw archive ownership did not cover the source range'; END IF;
  IF cold>0 THEN
    IF active.upload_digest IS DISTINCT FROM $2 THEN
      RAISE EXCEPTION 'raw archive object was not prepared for upload' USING ERRCODE='55000'; END IF;
    DELETE FROM record_raw_archive_uploads
    WHERE (generation,part,digest)=(active.generation,(packet->>'part')::bigint,$2);
    IF NOT FOUND THEN RAISE EXCEPTION 'raw archive upload receipt is missing' USING ERRCODE='55000'; END IF;
    PERFORM raw_archive_seal_part(active.generation,(packet->>'part')::bigint,packet#>>'{rows,0,input,id}',
      packet#>>'{rows,-1,input,id}',cold,$2);
  END IF;
  UPDATE state_raw_archive_move SET after_id=tail,upload_digest=NULL WHERE id=1;
  RETURN jsonb_build_object('after',tail,'hot',kept,'cold',cold);
END $$;

CREATE FUNCTION raw_archive_move_attach_arrivals() RETURNS void LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
BEGIN
  IF NOT EXISTS(SELECT FROM pg_inherits WHERE inhrelid='record_raw_archive_arrivals'::regclass
    AND inhparent='record_build_submissions'::regclass) THEN
    ALTER TABLE record_raw_archive_arrivals INHERIT record_build_submissions;
  END IF;
END $$;

CREATE FUNCTION raw_archive_move_cut(uuid,text) RETURNS bigint LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE active state_raw_archive_move%ROWTYPE; old_bytes bigint; parts bigint;
BEGIN
  LOCK TABLE ONLY record_build_submissions IN ACCESS EXCLUSIVE MODE NOWAIT;
  SELECT * INTO STRICT active FROM state_raw_archive_move WHERE id=1 AND phase='copying' AND generation=$1 FOR UPDATE;
  IF active.source_oid<>'record_build_submissions'::regclass::oid OR active.upload_digest IS NOT NULL
    OR EXISTS(SELECT FROM ONLY record_build_submissions b WHERE active.after_id IS NULL OR b.id>active.after_id) THEN
    RAISE EXCEPTION 'raw archive still has a source range to seal' USING ERRCODE='55000';
  END IF;
  SELECT sealed_parts INTO STRICT parts FROM state_raw_archive_generation WHERE generation=$1 AND phase='sealing';
  PERFORM raw_archive_activate($1,$2,parts);
  old_bytes:=pg_total_relation_size('record_build_submissions');
  UPDATE state_raw_archive_move SET phase='cutting' WHERE id=1;
  TRUNCATE ONLY record_build_submissions;
  ALTER TABLE record_raw_archive_hot INHERIT record_build_submissions;
  PERFORM raw_archive_move_attach_arrivals();
  UPDATE state_raw_archive_move SET phase='complete' WHERE id=1;
  RETURN old_bytes;
END $$;

CREATE FUNCTION raw_archive_move_abort(uuid) RETURNS void LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
BEGIN
  LOCK TABLE ONLY record_build_submissions IN ACCESS EXCLUSIVE MODE NOWAIT;
  UPDATE state_raw_archive_move SET phase='idle',generation=NULL,after_id=NULL,upload_digest=NULL
    WHERE id=1 AND phase='copying' AND generation=$1;
  IF NOT FOUND THEN RAISE EXCEPTION 'only a frozen source can abandon its move' USING ERRCODE='55000'; END IF;
  UPDATE state_raw_archive_generation SET phase='abandoned' WHERE generation=$1 AND phase='sealing';
  TRUNCATE record_raw_archive_hot;
  PERFORM raw_archive_move_attach_arrivals();
END $$;

CREATE FUNCTION raw_inbox_library_count() RETURNS bigint
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  IF EXISTS(SELECT FROM state_raw_archive_move WHERE id=1 AND phase='copying') THEN RETURN NULL; END IF;
  RETURN (SELECT count(*) FROM entity_weapon_builds)+(SELECT count(*) FROM record_build_submissions)
    +coalesce((SELECT sealed_rows-consumed_rows FROM state_raw_archive_generation WHERE phase='consuming'),0);
END $$;

REVOKE ALL ON state_raw_archive_move,record_raw_archive_hot,record_raw_archive_arrivals FROM PUBLIC;
REVOKE ALL ON FUNCTION raw_archive_move_identity(),raw_archive_move_route(),raw_archive_move_freeze(),raw_archive_move_owner_fence(),
  raw_archive_move_begin(uuid,text),raw_archive_move_portable(record_build_submissions),raw_archive_move_page(integer,integer),
  raw_archive_move_prepare(integer,integer),raw_archive_move_accept(text,text,integer,integer),
  raw_archive_move_attach_arrivals(),raw_archive_move_cut(uuid,text),
  raw_archive_move_abort(uuid),raw_inbox_library_count() FROM PUBLIC;
