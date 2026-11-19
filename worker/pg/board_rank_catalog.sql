-- A held partition verifies the sealed output before replaying current canonical inputs.
CREATE TABLE state_board_rank_catalog_inputs (
 weapon text COLLATE "C" NOT NULL, ruler text COLLATE "C" NOT NULL, identity text COLLATE "C" NOT NULL,
 PRIMARY KEY(weapon,ruler,identity)
);
CREATE TABLE state_board_rank_catalog_scan (
 id integer PRIMARY KEY CHECK(id=1), source text NOT NULL, epoch text NOT NULL, floor bigint NOT NULL CHECK(floor>=0),
 upper_scope text COLLATE "C" NOT NULL, upper_key text COLLATE "C" NOT NULL,
 last_scope text COLLATE "C" NOT NULL DEFAULT '', last_key text COLLATE "C" NOT NULL DEFAULT '',
 complete boolean NOT NULL DEFAULT false, closed boolean NOT NULL DEFAULT false
);
CREATE FUNCTION board_rank_catalog_input() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE identity_key text; ruler_key text:=''; parts jsonb;
BEGIN
 IF NEW.scope='measurement' THEN parts:=NEW.key::jsonb; identity_key:=parts->>0; ruler_key:=parts->>1;
 ELSE identity_key:=NEW.key; END IF;
 INSERT INTO state_board_rank_catalog_inputs
  SELECT b.weapon,ruler_key,b.id FROM entity_weapon_builds b WHERE b.id=identity_key ON CONFLICT DO NOTHING;
 RETURN NULL;
END $$;
REVOKE ALL ON FUNCTION board_rank_catalog_input() FROM PUBLIC;
ALTER TABLE state_board_rank_partitions
 ADD COLUMN catalog_phase text CHECK(catalog_phase IN('frozen','replay','active')),
 ADD COLUMN catalog_proof jsonb,
 ADD CHECK ((catalog_phase IS NULL)=(catalog_proof IS NULL));

CREATE FUNCTION board_rank_catalog_activate(text,text,text,text) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM state_board_rank_partitions p
  JOIN record_board_rank_snapshot_refs r USING(weapon,ruler)
  JOIN state_board_rank_generations g ON g.token=r.token
  WHERE p.weapon=$1 AND p.ruler=$2 AND r.revision=$3 AND p.source=$4 AND p.complete
  AND p.catalog_phase='replay' AND p.catalog_proof ? 'checked'
  AND (g.source,g.native,g.cut)=(p.source,p.native,p.cut)) THEN
  RAISE EXCEPTION 'catalog generation has no qualified handoff';
 END IF;
 UPDATE state_board_read_partitions s SET enabled=true,revision=$3 FROM state_board_rank_partitions p
  WHERE s.weapon=$1 AND s.ruler=$2 AND (s.weapon,s.ruler)=(p.weapon,p.ruler)
   AND s.scene=p.catalog_proof->>'scene' AND NOT s.enabled AND s.canonical_sources AND s.revision IS NULL;
 IF NOT FOUND THEN RAISE EXCEPTION 'catalog public selection changed'; END IF;
 IF EXISTS(SELECT FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2 AND catalog_proof ? 'request_position') THEN
  UPDATE state_board_requested_partitions q SET activated_at=board_input_timestamp(clock_timestamp())
   FROM state_board_rank_partitions p WHERE (q.weapon,q.ruler)=(p.weapon,p.ruler) AND q.weapon=$1 AND q.ruler=$2
    AND q.enqueued AND q.activated_at IS NULL AND q.position::text=p.catalog_proof->>'request_position';
  IF NOT FOUND THEN RAISE EXCEPTION 'catalog player request changed'; END IF;
 END IF;
 UPDATE state_board_rank_partitions SET catalog_phase='active' WHERE weapon=$1 AND ruler=$2;
END $$;
REVOKE ALL ON FUNCTION board_rank_catalog_activate(text,text,text,text) FROM PUBLIC;
