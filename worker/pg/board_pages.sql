-- A complete native partition becomes readable atomically; pages pin its version.
CREATE TABLE IF NOT EXISTS state_board_read_partitions (
  weapon TEXT COLLATE "C" NOT NULL,
  ruler TEXT COLLATE "C" NOT NULL,
  scene TEXT COLLATE "C" NOT NULL,
  enabled BOOLEAN NOT NULL DEFAULT false,
  canonical_sources BOOLEAN NOT NULL DEFAULT false,
  revision TEXT COLLATE "C",
  CHECK(NOT enabled OR canonical_sources),
  PRIMARY KEY(weapon,ruler)
);
CREATE INDEX IF NOT EXISTS board_read_partitions_scene ON state_board_read_partitions(scene,weapon,ruler) WHERE enabled;
CREATE TABLE IF NOT EXISTS state_board_read_routes (
  scope TEXT COLLATE "C" NOT NULL,
  key TEXT COLLATE "C" NOT NULL,
  weapon TEXT COLLATE "C" NOT NULL,
  ruler TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY(scope,key)
);
CREATE INDEX IF NOT EXISTS board_read_measurement_routes ON state_board_read_routes(weapon,ruler,key) WHERE scope='measurement';
CREATE TABLE IF NOT EXISTS record_board_read_snapshots (
  weapon TEXT COLLATE "C" NOT NULL,
  ruler TEXT COLLATE "C" NOT NULL,
  revision TEXT COLLATE "C" NOT NULL,
  source TEXT COLLATE "C" NOT NULL,
  engine TEXT COLLATE "C" NOT NULL,
  inputs_sha256 TEXT COLLATE "C" NOT NULL,
  rows BIGINT NOT NULL CHECK(rows>=0),
  leaders JSONB NOT NULL,
  captured_at TEXT COLLATE "C" NOT NULL,
  published_at TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY(weapon,ruler,revision)
);
CREATE TABLE IF NOT EXISTS state_board_read_rows (
  weapon TEXT COLLATE "C" NOT NULL,
  ruler TEXT COLLATE "C" NOT NULL,
  revision TEXT COLLATE "C" NOT NULL,
  ordinal BIGINT NOT NULL CHECK(ordinal>0),
  body TEXT COLLATE "C" NOT NULL CHECK(octet_length(body)<=65536),
  PRIMARY KEY(weapon,ruler,revision,ordinal),
  FOREIGN KEY(weapon,ruler,revision) REFERENCES record_board_read_snapshots ON DELETE CASCADE
);
REVOKE ALL ON state_board_read_partitions,state_board_read_routes,record_board_read_snapshots,state_board_read_rows FROM PUBLIC;
SELECT set_config('search_path',quote_ident(current_schema())||',pg_catalog,pg_temp',false);

CREATE OR REPLACE FUNCTION board_read_catalog(text,text,text,bigint) RETURNS jsonb
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE items jsonb;
BEGIN
  IF $4 IS NULL OR $4<1 OR $4>128 THEN RAISE EXCEPTION 'invalid board catalog page size'; END IF;
  SELECT coalesce(jsonb_agg(to_jsonb(x) ORDER BY x.weapon,x.ruler),'[]') INTO items FROM (
    SELECT p.weapon,p.ruler,p.scene,p.revision,s.leaders,s.rows,s.captured_at,s.published_at
    FROM state_board_read_partitions p JOIN record_board_read_snapshots s USING(weapon,ruler,revision)
    WHERE p.enabled AND p.scene=$2 AND ($1 IS NULL OR p.weapon=$1)
      AND (p.weapon||'/'||p.ruler)>coalesce($3,'')
    ORDER BY p.weapon,p.ruler LIMIT $4+1
  )x;
  RETURN jsonb_build_object('ok',true,'partitions',items);
END $$;

CREATE OR REPLACE FUNCTION board_read_page(text,text,text,bigint,bigint) RETURNS jsonb
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held record; requested text; pages jsonb; last_ordinal bigint;
BEGIN
  IF $4 IS NULL OR $4<0 OR $5 IS NULL OR $5<1 OR $5>128 THEN RAISE EXCEPTION 'invalid board row page'; END IF;
  SELECT * INTO held FROM state_board_read_partitions WHERE weapon=$1 AND ruler=$2 AND enabled;
  IF NOT FOUND OR held.revision IS NULL THEN RETURN jsonb_build_object('ok',false,'error','not_ready'); END IF;
  requested:=coalesce($3,held.revision);
  SELECT * INTO held FROM record_board_read_snapshots WHERE weapon=$1 AND ruler=$2 AND revision=requested;
  IF NOT FOUND THEN RETURN jsonb_build_object('ok',false,'error','revision_unavailable'); END IF;
  SELECT coalesce(jsonb_agg(body::jsonb ORDER BY ordinal),'[]'),coalesce(max(ordinal),$4) INTO pages,last_ordinal
  FROM (SELECT ordinal,body FROM state_board_read_rows WHERE weapon=$1 AND ruler=$2 AND revision=requested
    AND ordinal>$4 ORDER BY ordinal LIMIT $5)x;
  RETURN jsonb_build_object('ok',true,'weapon',$1,'ruler',$2,'revision',requested,'source',held.source,
    'engine',held.engine,'captured_at',held.captured_at,'count',held.rows,'after',$4,'rows',pages,
    'next',CASE WHEN last_ordinal<held.rows THEN last_ordinal END);
END $$;
REVOKE ALL ON FUNCTION board_read_catalog(text,text,text,bigint),board_read_page(text,text,text,bigint,bigint) FROM PUBLIC;
