-- Keyed state handoffs coalesce values; accounting keeps its own immutable occurrences.
CREATE SEQUENCE IF NOT EXISTS board_input_revision;
CREATE TABLE IF NOT EXISTS state_board_input_mailbox (
  scope TEXT COLLATE "C" NOT NULL,
  key TEXT COLLATE "C" NOT NULL,
  version BIGINT NOT NULL,
  body JSONB,
  removed BOOLEAN NOT NULL,
  body_hash TEXT COLLATE "C" NOT NULL,
  body_bytes BIGINT GENERATED ALWAYS AS (coalesce(octet_length(body::text),0)) STORED,
  applied_version BIGINT NOT NULL DEFAULT -1,
  available_at TEXT COLLATE "C",
  lease_token TEXT COLLATE "C",
  lease_version BIGINT,
  lease_until TEXT COLLATE "C",
  last_error TEXT,
  last_error_version BIGINT,
  last_failed_at TEXT COLLATE "C",
  PRIMARY KEY(scope,key),
  CHECK (scope IN ('build','measurement','review','partition')),
  CHECK (octet_length(key) BETWEEN 1 AND 512),
  CHECK (version>=0 AND applied_version>=-1 AND applied_version<=version),
  CHECK (body IS NULL OR jsonb_typeof(body)='object'),
  CHECK (body_hash ~ '^[a-f0-9]{64}$'),
  CHECK (version<=applied_version OR removed OR body IS NOT NULL),
  CHECK ((lease_token IS NULL AND lease_version IS NULL AND lease_until IS NULL)
    OR (lease_token IS NOT NULL AND lease_version IS NOT NULL AND lease_until IS NOT NULL))
);
CREATE INDEX IF NOT EXISTS board_input_ready ON state_board_input_mailbox((version=0),available_at,scope,key)
  WHERE version>applied_version AND available_at IS NOT NULL;
CREATE TABLE IF NOT EXISTS state_board_input_source(id INTEGER PRIMARY KEY CHECK(id=1),source TEXT COLLATE "C" NOT NULL);
DO $$BEGIN
  IF NOT EXISTS(SELECT 1 FROM state_board_input_source) THEN
    IF EXISTS(SELECT 1 FROM state_board_input_mailbox LIMIT 1) THEN RAISE EXCEPTION 'board input source metadata requires restore'; END IF;
    INSERT INTO state_board_input_source VALUES(1,gen_random_uuid()::text);
  END IF;
END $$;
REVOKE ALL ON state_board_input_mailbox FROM PUBLIC;
REVOKE ALL ON state_board_input_source FROM PUBLIC;
REVOKE ALL ON SEQUENCE board_input_revision FROM PUBLIC;
SELECT set_config('search_path',quote_ident(current_schema())||',pg_catalog,pg_temp',false);

CREATE OR REPLACE FUNCTION board_input_timestamp(timestamptz) RETURNS text
LANGUAGE sql IMMUTABLE SET search_path=pg_catalog,pg_temp AS $$
  SELECT to_char($1 AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
$$;
REVOKE ALL ON FUNCTION board_input_timestamp(timestamptz) FROM PUBLIC;

CREATE OR REPLACE FUNCTION board_input_put(text,text,text) RETURNS bigint
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE value jsonb:=$3::jsonb; revision bigint; digest text;
BEGIN
  IF $1 IS NULL OR $1 NOT IN ('build','measurement','review','partition') OR $2 IS NULL OR octet_length($2) NOT BETWEEN 1 AND 512
    OR (value IS NOT NULL AND jsonb_typeof(value)<>'object') THEN RAISE EXCEPTION 'invalid board input'; END IF;
  digest:=encode(sha256(convert_to(coalesce(value::text,'null'),'UTF8')),'hex');
  INSERT INTO state_board_input_mailbox(scope,key,version,body,removed,body_hash,available_at)
    VALUES($1,$2,nextval('board_input_revision'),value,value IS NULL,digest,board_input_timestamp(clock_timestamp()))
    ON CONFLICT(scope,key) DO UPDATE SET version=nextval('board_input_revision'),body=EXCLUDED.body,
      removed=EXCLUDED.removed,body_hash=EXCLUDED.body_hash,
      available_at=CASE WHEN state_board_input_mailbox.lease_until>board_input_timestamp(clock_timestamp())
        THEN state_board_input_mailbox.lease_until ELSE board_input_timestamp(clock_timestamp()) END
    WHERE state_board_input_mailbox.body_hash IS DISTINCT FROM EXCLUDED.body_hash
    RETURNING version INTO revision;
  IF revision IS NULL THEN SELECT version INTO revision FROM state_board_input_mailbox WHERE scope=$1 AND key=$2; END IF;
  RETURN revision;
END $$;

CREATE OR REPLACE FUNCTION board_input_seed(text,text,text) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE value jsonb:=$3::jsonb; inserted bigint;
BEGIN
  IF $1 IS NULL OR $1 NOT IN ('build','measurement','review','partition') OR $2 IS NULL OR octet_length($2) NOT BETWEEN 1 AND 512
    OR (value IS NOT NULL AND jsonb_typeof(value)<>'object') THEN RAISE EXCEPTION 'invalid board input'; END IF;
  INSERT INTO state_board_input_mailbox(scope,key,version,body,removed,body_hash,available_at)
    VALUES($1,$2,0,value,value IS NULL,encode(sha256(convert_to(coalesce(value::text,'null'),'UTF8')),'hex'),board_input_timestamp(clock_timestamp()))
    ON CONFLICT(scope,key) DO NOTHING;
  GET DIAGNOSTICS inserted=ROW_COUNT;
  RETURN inserted=1;
END $$;

CREATE OR REPLACE FUNCTION board_input_claim(bigint,text,bigint,text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE at text:=board_input_timestamp(clock_timestamp()); until text; claimed jsonb;
BEGIN
  IF $4 IS DISTINCT FROM (SELECT source FROM state_board_input_source WHERE id=1)
    THEN RAISE EXCEPTION 'board input source changed'; END IF;
  IF $1 IS NULL OR $1<1 OR $1>256 OR $2 IS NULL OR octet_length($2) NOT BETWEEN 1 AND 128 OR $3 IS NULL OR $3<1
    THEN RAISE EXCEPTION 'invalid board input claim'; END IF;
  until:=board_input_timestamp(at::timestamptz+make_interval(secs=>$3));
  -- Seek both bounds of each lane in board_input_ready, including for a generic plan.
  WITH live AS MATERIALIZED (
    SELECT scope,key FROM state_board_input_mailbox
    WHERE version>applied_version AND available_at IS NOT NULL AND available_at<=at
     AND ROW((version=0),available_at) BETWEEN ROW(false,''::text) AND ROW(false,at)
    ORDER BY (version=0),available_at,scope,key LIMIT 256
  ), candidates AS MATERIALIZED (
    SELECT scope,key FROM (
      SELECT scope,key,0 AS priority FROM live
      UNION ALL
      (SELECT scope,key,1 AS priority FROM state_board_input_mailbox
        WHERE version>applied_version AND available_at IS NOT NULL AND available_at<=at
         AND ROW((version=0),available_at) BETWEEN ROW(true,''::text) AND ROW(true,at)
        ORDER BY (version=0),available_at,scope,key LIMIT (256-(SELECT count(*) FROM live)))
    ) inputs ORDER BY priority
  ), ready AS MATERIALIZED (
    SELECT m.scope,m.key FROM candidates c CROSS JOIN LATERAL (
      SELECT m.scope,m.key FROM state_board_input_mailbox m WHERE (m.scope,m.key)=(c.scope,c.key)
        AND m.version>m.applied_version AND m.available_at<=at FOR UPDATE SKIP LOCKED OFFSET 0
    ) m LIMIT $1
  ), owned AS (
    UPDATE state_board_input_mailbox m SET lease_token=$2,lease_version=m.version,lease_until=until,available_at=until
    FROM ready r WHERE (m.scope,m.key)=(r.scope,r.key)
    RETURNING m.scope,m.key,m.version,m.body_bytes,m.removed AS deleted
  )
  SELECT coalesce(jsonb_agg(jsonb_build_object('scope',scope,'key',key,'version',version::text,
    'body_bytes',body_bytes,'deleted',deleted) ORDER BY scope,key),'[]') INTO claimed FROM owned;
  RETURN jsonb_build_object('source',(SELECT source FROM state_board_input_source WHERE id=1),
    'token',$2,'lease_until',until,'inputs',claimed);
END $$;

CREATE OR REPLACE FUNCTION board_input_read(text,text,text,bigint,bigint) RETURNS jsonb
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held record; status text;
BEGIN
  IF $5 IS NULL OR $5<1 OR $5>8388608 THEN RAISE EXCEPTION 'invalid board input byte budget'; END IF;
  SELECT version,body_bytes,body_hash,removed AS deleted,lease_token,lease_version,lease_until,
    CASE WHEN lease_token=$3 AND lease_version=$4 AND version=$4 AND lease_until>board_input_timestamp(statement_timestamp())
      AND body_bytes<=$5 THEN body END AS captured INTO held
    FROM state_board_input_mailbox WHERE scope=$1 AND key=$2;
  IF NOT FOUND THEN RETURN jsonb_build_object('status','missing'); END IF;
  status:=CASE WHEN held.lease_token IS DISTINCT FROM $3 OR held.lease_version IS DISTINCT FROM $4
      OR held.lease_until IS NULL OR held.lease_until<=board_input_timestamp(statement_timestamp()) THEN 'stale'
    WHEN held.version<>$4 THEN 'superseded' WHEN held.body_bytes>$5 THEN 'oversized' ELSE 'input' END;
  IF status<>'input' THEN RETURN jsonb_build_object('status',status,'version',held.version::text,'body_bytes',held.body_bytes); END IF;
  RETURN jsonb_build_object('status',status,'source',(SELECT source FROM state_board_input_source WHERE id=1),
    'scope',$1,'key',$2,'version',held.version::text,
    'payload',held.captured::text,'payload_sha256',held.body_hash,'deleted',held.deleted);
END $$;

CREATE OR REPLACE FUNCTION board_input_ack(text,text,text,bigint) RETURNS text
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held record;
BEGIN
  IF $4 IS NULL OR $4<0 THEN RAISE EXCEPTION 'invalid board input version'; END IF;
  SELECT scope,version,applied_version,lease_token,lease_version,lease_until INTO held
    FROM state_board_input_mailbox WHERE scope=$1 AND key=$2 FOR UPDATE;
  IF held.scope IS NULL THEN RETURN 'missing'; END IF;
  IF held.applied_version>=$4 THEN RETURN 'already_applied'; END IF;
  IF $4 IS NULL OR held.lease_token IS DISTINCT FROM $3 OR held.lease_version IS DISTINCT FROM $4
    OR held.lease_until IS NULL OR held.lease_until<=board_input_timestamp(clock_timestamp()) THEN RETURN 'stale'; END IF;
  UPDATE state_board_input_mailbox SET applied_version=$4,lease_token=NULL,lease_version=NULL,lease_until=NULL,
    body=CASE WHEN version=$4 THEN NULL ELSE body END,
    available_at=CASE WHEN version>$4 THEN board_input_timestamp(clock_timestamp()) ELSE NULL END WHERE scope=$1 AND key=$2;
  RETURN 'acknowledged';
END $$;

CREATE OR REPLACE FUNCTION board_input_lock(text,text,text,bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held record; status text;
BEGIN
  IF $4 IS NULL OR $4<0 THEN RAISE EXCEPTION 'invalid board input version'; END IF;
  SELECT version,body_hash,applied_version,lease_token,lease_version,lease_until INTO held
    FROM state_board_input_mailbox WHERE scope=$1 AND key=$2 FOR UPDATE;
  status:=CASE WHEN NOT FOUND THEN 'missing' WHEN held.applied_version>=$4 THEN 'already_applied'
    WHEN held.lease_token IS DISTINCT FROM $3 OR held.lease_version IS DISTINCT FROM $4
      OR held.lease_until IS NULL OR held.lease_until<=board_input_timestamp(clock_timestamp()) THEN 'stale'
    WHEN held.version<>$4 THEN 'superseded' ELSE 'owned' END;
  RETURN jsonb_build_object('source',(SELECT source FROM state_board_input_source WHERE id=1),
    'status',status,'version',held.version::text,'payload_sha256',held.body_hash);
END $$;

CREATE OR REPLACE FUNCTION board_input_retry(text,text,text,bigint,text,bigint) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held record;
BEGIN
  IF $5 IS NULL OR $5='' OR ($6 IS NOT NULL AND $6<0) THEN RAISE EXCEPTION 'invalid board input retry'; END IF;
  SELECT scope,version,lease_token,lease_version,lease_until INTO held
    FROM state_board_input_mailbox WHERE scope=$1 AND key=$2 FOR UPDATE;
  IF held.scope IS NULL OR held.lease_token IS DISTINCT FROM $3 OR held.lease_version IS DISTINCT FROM $4
    OR held.lease_until IS NULL OR held.lease_until<=board_input_timestamp(clock_timestamp()) THEN RETURN false; END IF;
  UPDATE state_board_input_mailbox SET lease_token=NULL,lease_version=NULL,lease_until=NULL,
    available_at=CASE WHEN version>$4 THEN board_input_timestamp(clock_timestamp()) WHEN $6 IS NULL THEN NULL
      ELSE board_input_timestamp(clock_timestamp()+make_interval(secs=>$6)) END,
    last_error=left($5,2048),last_error_version=$4,last_failed_at=board_input_timestamp(clock_timestamp())
    WHERE scope=$1 AND key=$2;
  RETURN true;
END $$;

REVOKE ALL ON FUNCTION board_input_put(text,text,text),board_input_seed(text,text,text),
  board_input_claim(bigint,text,bigint,text),board_input_ack(text,text,text,bigint),
  board_input_read(text,text,text,bigint,bigint),
  board_input_lock(text,text,text,bigint),
  board_input_retry(text,text,text,bigint,text,bigint) FROM PUBLIC;
