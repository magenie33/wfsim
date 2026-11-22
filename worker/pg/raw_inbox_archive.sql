CREATE TABLE IF NOT EXISTS state_raw_archive_generation (
  generation uuid PRIMARY KEY,
  fence text NOT NULL CHECK(fence~'^[0-9a-f]{64}$'),
  phase text NOT NULL CHECK(phase IN('sealing','consuming','complete','abandoned')),
  sealed_parts bigint NOT NULL DEFAULT 0,
  sealed_rows bigint NOT NULL DEFAULT 0,
  consumed_rows bigint NOT NULL DEFAULT 0,
  last_id text COLLATE "C" NOT NULL DEFAULT '',
  next_part bigint NOT NULL DEFAULT 1,
  retired_part bigint NOT NULL DEFAULT 0,
  CHECK(next_part BETWEEN 1 AND sealed_parts+1),
  CHECK(retired_part BETWEEN 0 AND CASE WHEN phase='abandoned' THEN sealed_parts ELSE next_part-1 END)
);
CREATE UNIQUE INDEX IF NOT EXISTS raw_archive_single_generation ON state_raw_archive_generation((true)) WHERE phase IN('sealing','consuming');

CREATE TABLE IF NOT EXISTS record_raw_archive_parts (
  generation uuid NOT NULL REFERENCES state_raw_archive_generation(generation),
  part bigint NOT NULL CHECK(part>0),
  first_id text COLLATE "C" NOT NULL,
  last_id text COLLATE "C" NOT NULL CHECK(last_id>=first_id),
  rows integer NOT NULL CHECK(rows BETWEEN 1 AND 512),
  digest text NOT NULL CHECK(digest~'^[0-9a-f]{64}$'),
  PRIMARY KEY(generation,part)
);
CREATE INDEX IF NOT EXISTS raw_archive_part_range ON record_raw_archive_parts(generation,last_id);

-- Keep exact keys that may have reached R2 until a sealed part takes custody.
CREATE TABLE IF NOT EXISTS record_raw_archive_uploads (
  generation uuid NOT NULL REFERENCES state_raw_archive_generation(generation),
  part bigint NOT NULL CHECK(part>0),
  digest text NOT NULL CHECK(digest~'^[0-9a-f]{64}$'),
  PRIMARY KEY(generation,part,digest)
);

-- A copied input may settle or gain a live owner before the cut.
CREATE TABLE IF NOT EXISTS record_raw_archive_skips (
  generation uuid NOT NULL,
  part bigint NOT NULL,
  id uuid NOT NULL,
  PRIMARY KEY(generation,part,id),
  FOREIGN KEY(generation,part) REFERENCES record_raw_archive_parts(generation,part)
);

CREATE OR REPLACE FUNCTION raw_archive_legacy_id(text) RETURNS boolean
LANGUAGE sql IMMUTABLE STRICT SET search_path FROM CURRENT AS $$
  SELECT $1 COLLATE "C"~'^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
$$;

CREATE OR REPLACE FUNCTION raw_archive_begin(uuid,text) RETURNS void
LANGUAGE sql SET search_path FROM CURRENT AS $$
  INSERT INTO state_raw_archive_generation(generation,fence,phase) VALUES($1,$2,'sealing')
$$;

CREATE OR REPLACE FUNCTION raw_archive_seal_part(uuid,bigint,text,text,integer,text) RETURNS void
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE active state_raw_archive_generation%ROWTYPE;
BEGIN
  SELECT * INTO STRICT active FROM state_raw_archive_generation WHERE generation=$1 AND phase='sealing' FOR UPDATE;
  IF $2 IS DISTINCT FROM active.sealed_parts+1 OR $3 COLLATE "C"<=active.last_id
    OR NOT coalesce(raw_archive_legacy_id($3) AND raw_archive_legacy_id($4),false) THEN
    RAISE EXCEPTION 'raw archive part does not follow the sealed range' USING ERRCODE='55000';
  END IF;
  INSERT INTO record_raw_archive_parts VALUES($1,$2,$3,$4,$5,$6);
  UPDATE state_raw_archive_generation SET sealed_parts=$2,sealed_rows=sealed_rows+$5,last_id=$4 WHERE generation=$1;
END $$;

CREATE OR REPLACE FUNCTION raw_archive_activate(uuid,text,bigint) RETURNS void
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
BEGIN
  UPDATE state_raw_archive_generation SET phase=CASE WHEN sealed_parts=0 THEN 'complete' ELSE 'consuming' END
    WHERE generation=$1 AND fence=$2 AND sealed_parts=$3 AND phase='sealing';
  IF NOT FOUND THEN RAISE EXCEPTION 'raw archive activation does not match its sealed fence' USING ERRCODE='55000'; END IF;
END $$;

CREATE OR REPLACE FUNCTION raw_archive_next() RETURNS jsonb
LANGUAGE sql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
  SELECT jsonb_build_object('generation',p.generation,'part',p.part::text,'digest',p.digest,'rows',p.rows,
    'first_id',p.first_id,'last_id',p.last_id)
  FROM state_raw_archive_generation g JOIN record_raw_archive_parts p ON (p.generation,p.part)=(g.generation,g.next_part)
  WHERE g.phase='consuming'
$$;

CREATE OR REPLACE FUNCTION raw_archive_pending(uuid,bigint,text) RETURNS jsonb
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE active state_raw_archive_generation%ROWTYPE;
BEGIN
  SELECT * INTO STRICT active FROM state_raw_archive_generation g WHERE g.generation=$1;
  IF active.phase NOT IN('consuming','complete') OR $2>active.next_part OR NOT EXISTS(
    SELECT FROM record_raw_archive_parts p WHERE (p.generation,p.part,p.digest)=($1,$2,$3)) THEN
    RAISE EXCEPTION 'raw archive digest or sequence changed' USING ERRCODE='55000';
  END IF;
  RETURN jsonb_build_object('replay',$2<active.next_part,'skip',ARRAY(
    SELECT id::text FROM record_raw_archive_skips s WHERE (s.generation,s.part)=($1,$2) ORDER BY id));
END $$;

CREATE OR REPLACE FUNCTION raw_archive_retire_next(uuid) RETURNS jsonb
LANGUAGE sql STABLE SET search_path FROM CURRENT AS $$
  WITH eligible AS (
    SELECT p.part,p.digest,true AS upload
    FROM state_raw_archive_generation g JOIN record_raw_archive_uploads p USING(generation)
    WHERE g.generation=$1 AND g.phase<>'sealing' AND NOT EXISTS(
      SELECT FROM record_raw_archive_parts s WHERE (s.generation,s.part,s.digest)=(p.generation,p.part,p.digest))
    UNION ALL
    SELECT p.part,p.digest,false
    FROM state_raw_archive_generation g JOIN record_raw_archive_parts p
      ON (p.generation,p.part)=(g.generation,g.retired_part+1)
    WHERE g.generation=$1 AND (g.phase='abandoned' OR (g.phase IN('consuming','complete') AND p.part<g.next_part))
  )
  SELECT jsonb_build_object('part',part::text,'digest',digest) FROM eligible
    ORDER BY upload DESC,part,digest LIMIT 1
$$;

CREATE OR REPLACE FUNCTION raw_archive_trim_skips() RETURNS void
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
BEGIN
  IF pg_relation_size('record_raw_archive_skips')=0
    OR EXISTS(SELECT FROM state_raw_archive_generation WHERE phase IN('sealing','consuming'))
    OR EXISTS(SELECT FROM record_raw_archive_skips) THEN RETURN; END IF;
  LOCK TABLE record_raw_archive_skips IN ACCESS EXCLUSIVE MODE NOWAIT;
  IF NOT EXISTS(SELECT FROM record_raw_archive_skips) THEN TRUNCATE record_raw_archive_skips; END IF;
EXCEPTION WHEN lock_not_available THEN
  RETURN;
END $$;

CREATE OR REPLACE FUNCTION raw_archive_retired(uuid,bigint,text) RETURNS void
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
BEGIN
  DELETE FROM record_raw_archive_uploads p USING state_raw_archive_generation g
  WHERE (p.generation,p.part,p.digest)=($1,$2,$3) AND g.generation=p.generation AND g.phase<>'sealing'
    AND NOT EXISTS(SELECT FROM record_raw_archive_parts s WHERE (s.generation,s.part,s.digest)=(p.generation,p.part,p.digest));
  IF FOUND THEN RETURN; END IF;
  UPDATE state_raw_archive_generation g SET retired_part=$2 WHERE g.generation=$1
    AND g.retired_part+1=$2 AND (g.phase='abandoned' OR (g.phase IN('consuming','complete') AND $2<g.next_part))
    AND EXISTS(SELECT FROM record_raw_archive_parts p WHERE (p.generation,p.part,p.digest)=($1,$2,$3));
  IF NOT FOUND THEN RAISE EXCEPTION 'raw archive retirement must follow committed custody' USING ERRCODE='55000'; END IF;
  DELETE FROM record_raw_archive_skips s WHERE (s.generation,s.part)=($1,$2);
  PERFORM raw_archive_trim_skips();
END $$;

CREATE OR REPLACE FUNCTION raw_archive_consume(uuid,bigint,text,text[],text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE active state_raw_archive_generation%ROWTYPE; sealed record_raw_archive_parts%ROWTYPE;
  packet jsonb; batch jsonb; ids text[]; remaining text[]; skipped text[];
BEGIN
  IF $3 IS NULL OR octet_length($3)>8392704 OR $5 IS NULL OR octet_length($5)>8392704 THEN
    RAISE EXCEPTION 'raw archive part exceeds its transfer bounds' USING ERRCODE='22023';
  END IF;
  SELECT * INTO STRICT active FROM state_raw_archive_generation WHERE generation=$1 FOR UPDATE;
  SELECT * INTO STRICT sealed FROM record_raw_archive_parts WHERE generation=$1 AND part=$2;
  IF encode(sha256(convert_to($3,'UTF8')),'hex')<>sealed.digest
    OR active.phase NOT IN('consuming','complete') OR $2>active.next_part THEN
    RAISE EXCEPTION 'raw archive digest or sequence changed' USING ERRCODE='55000';
  END IF;
  IF $2<active.next_part THEN RETURN jsonb_build_object('committed',true,'replay',true,'rows',sealed.rows); END IF;
  packet:=$3::jsonb;
  IF packet->>'generation' IS DISTINCT FROM $1::text OR packet->>'part' IS DISTINCT FROM $2::text
    OR jsonb_typeof(packet->'rows') IS DISTINCT FROM 'array' OR jsonb_array_length(packet->'rows')<>sealed.rows THEN
    RAISE EXCEPTION 'raw archive packet membership changed' USING ERRCODE='22023';
  END IF;
  SELECT array_agg(item#>>'{input,id}' COLLATE "C" ORDER BY ordinal) INTO ids
    FROM jsonb_array_elements(packet->'rows') WITH ORDINALITY parts(item,ordinal);
  IF ids[1] IS DISTINCT FROM sealed.first_id OR ids[sealed.rows] IS DISTINCT FROM sealed.last_id
    OR ids IS DISTINCT FROM ARRAY(SELECT DISTINCT id COLLATE "C" FROM unnest(ids) id ORDER BY id COLLATE "C")
    OR EXISTS(SELECT FROM jsonb_array_elements(packet->'rows') item WHERE
      NOT coalesce(raw_archive_legacy_id(item#>>'{input,id}'),false)
      OR item->'portable' IS DISTINCT FROM 'true'::jsonb OR item->'position' IS DISTINCT FROM 'null'::jsonb
      OR item->'input'->>'observation' IS NOT NULL OR split_part(item#>>'{input,family_source}','|',1)='workflow'
      OR jsonb_typeof(item#>'{input,record}') IS DISTINCT FROM 'string'
      OR jsonb_typeof(item#>'{input,at}') IS DISTINCT FROM 'string') THEN
    RAISE EXCEPTION 'raw archive acknowledgement does not match the sealed inputs' USING ERRCODE='22023';
  END IF;
  SELECT ARRAY(SELECT id::text FROM record_raw_archive_skips s WHERE (s.generation,s.part)=($1,$2))
    INTO skipped;
  remaining:=ARRAY(SELECT id FROM unnest(ids) WITH ORDINALITY AS source(id,ordinal) WHERE NOT id=ANY(skipped) ORDER BY ordinal);
  IF NOT (skipped<@ids) OR remaining IS DISTINCT FROM $4 THEN
    RAISE EXCEPTION 'raw archive acknowledgement does not match pending inputs' USING ERRCODE='22023';
  END IF;
  batch:=$5::jsonb;
  IF jsonb_typeof(batch) IS DISTINCT FROM 'array' OR jsonb_array_length(batch)>1000 OR EXISTS(
    SELECT FROM jsonb_array_elements(batch) item
      WHERE jsonb_typeof(item->'from') IS DISTINCT FROM 'array'
        OR jsonb_array_length(item->'from')=0 OR NOT (item->'from'<@to_jsonb(remaining))
        OR item#>>'{record,kind}' IN('warframe','companion')) THEN
    RAISE EXCEPTION 'raw archive canonical output changed its source' USING ERRCODE='22023';
  END IF;
  PERFORM build_intake_batch($5);
  DELETE FROM record_raw_archive_skips s WHERE (s.generation,s.part)=($1,$2);
  UPDATE state_raw_archive_generation SET next_part=$2+1,consumed_rows=consumed_rows+sealed.rows,
    phase=CASE WHEN sealed_parts=$2 THEN 'complete' ELSE 'consuming' END WHERE generation=$1;
  IF active.sealed_parts=$2 THEN PERFORM raw_archive_trim_skips(); END IF;
  RETURN jsonb_build_object('committed',true,'replay',false,'rows',sealed.rows);
END $$;

REVOKE ALL ON state_raw_archive_generation,record_raw_archive_parts,record_raw_archive_uploads,record_raw_archive_skips FROM PUBLIC;
REVOKE ALL ON FUNCTION raw_archive_legacy_id(text),raw_archive_begin(uuid,text),
  raw_archive_seal_part(uuid,bigint,text,text,integer,text),raw_archive_activate(uuid,text,bigint),
  raw_archive_next(),raw_archive_pending(uuid,bigint,text),raw_archive_consume(uuid,bigint,text,text[],text),
  raw_archive_retire_next(uuid),raw_archive_retired(uuid,bigint,text),raw_archive_trim_skips() FROM PUBLIC;
