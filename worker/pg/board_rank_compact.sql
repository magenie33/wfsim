-- One immutable Native description per source record, with each partition's observed versions kept together.
ALTER TABLE state_board_rank_dependencies RENAME TO state_board_rank_legacy_dependencies;
CREATE TABLE state_board_rank_descriptions (
 id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 native TEXT COLLATE "C" NOT NULL,
 identity TEXT COLLATE "C" NOT NULL,
 record_sha256 TEXT COLLATE "C" NOT NULL,
 descriptor JSONB NOT NULL,
 UNIQUE(native,identity,record_sha256)
);
CREATE TABLE state_board_rank_input_sets (
 identity TEXT COLLATE "C" NOT NULL,
 ruler TEXT COLLATE "C" NOT NULL,
 weapon TEXT COLLATE "C" NOT NULL,
 description BIGINT NOT NULL REFERENCES state_board_rank_descriptions,
 build_version BIGINT CHECK(build_version>=-1),
 modes JSONB NOT NULL CHECK(jsonb_typeof(modes)='object'),
 reviews JSONB NOT NULL CHECK(jsonb_typeof(reviews)='object'),
 PRIMARY KEY(identity,ruler,weapon),
 FOREIGN KEY(weapon,ruler,identity) REFERENCES state_board_rank_sources ON DELETE CASCADE
);
CREATE INDEX board_rank_description_refs ON state_board_rank_input_sets(description);
CREATE INDEX board_rank_review_refs ON state_board_rank_input_sets USING gin(reviews) WHERE reviews<>'{}'::jsonb;
CREATE VIEW state_board_rank_dependencies AS
 SELECT weapon,ruler,identity,scope,key,version FROM state_board_rank_legacy_dependencies
 UNION ALL SELECT s.weapon,s.ruler,s.identity,d.scope,d.key,d.version
 FROM state_board_rank_input_sets s CROSS JOIN LATERAL (
  SELECT 'build'::text scope,s.identity key,nullif(s.build_version,-1) version WHERE s.build_version IS NOT NULL
  UNION ALL SELECT 'measurement',jsonb_build_array(s.identity,s.ruler,m.key)::text,m.value::bigint FROM jsonb_each_text(s.modes) m
  UNION ALL SELECT 'review',r.key,r.value::bigint FROM jsonb_each_text(s.reviews) r
 ) d;
REVOKE ALL ON state_board_rank_descriptions,state_board_rank_input_sets,state_board_rank_dependencies FROM PUBLIC;

-- An offline operator can expand a bounded page before returning to a legacy writer.
CREATE FUNCTION board_rank_expand_inputs(integer) RETURNS integer
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE s RECORD; n integer:=0;
BEGIN
 IF $1 IS NULL OR $1<1 OR $1>64 THEN RAISE EXCEPTION 'invalid rank recovery page'; END IF;
 LOCK TABLE state_board_rank_sources,state_board_rank_input_sets IN SHARE ROW EXCLUSIVE MODE NOWAIT;
 FOR s IN SELECT i.identity,i.ruler,i.weapon,i.description,d.descriptor,d.record_sha256
   FROM state_board_rank_input_sets i JOIN state_board_rank_descriptions d ON d.id=i.description
   ORDER BY i.identity,i.ruler,i.weapon LIMIT $1 LOOP
  INSERT INTO state_board_rank_legacy_dependencies(weapon,ruler,identity,scope,key,version)
    SELECT weapon,ruler,identity,scope,key,version FROM state_board_rank_dependencies
     WHERE (identity,ruler,weapon)=(s.identity,s.ruler,s.weapon);
  UPDATE state_board_rank_sources SET descriptor=s.descriptor
    WHERE (identity,ruler,weapon,record_sha256)=(s.identity,s.ruler,s.weapon,s.record_sha256)
      AND descriptor='null'::jsonb AND EXISTS(SELECT 1 FROM state_board_rank_descriptions d
       JOIN state_board_rank_partitions p ON p.native=d.native AND (p.weapon,p.ruler)=(s.weapon,s.ruler)
       WHERE d.id=s.description AND d.identity=s.identity);
  IF NOT FOUND THEN RAISE EXCEPTION 'rank source recovery boundary changed'; END IF;
  DELETE FROM state_board_rank_input_sets WHERE (identity,ruler,weapon)=(s.identity,s.ruler,s.weapon);
  DELETE FROM state_board_rank_descriptions d WHERE d.id=s.description
    AND NOT EXISTS(SELECT 1 FROM state_board_rank_input_sets i WHERE i.description=d.id);
  n:=n+1;
 END LOOP;
 RETURN n;
END $$;
REVOKE ALL ON FUNCTION board_rank_expand_inputs(integer) FROM PUBLIC;
