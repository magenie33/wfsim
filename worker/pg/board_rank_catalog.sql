-- A held partition verifies the sealed output before replaying current canonical inputs.
DO $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM pg_index i WHERE i.indexrelid=to_regclass('board_catalog_build_order')
  AND i.indrelid='entity_weapon_builds'::regclass AND i.indisvalid AND i.indisready
  AND pg_get_indexdef(i.indexrelid,1,true)='weapon' AND pg_get_indexdef(i.indexrelid,2,true)='id'
  AND i.indpred IS NULL) THEN RAISE EXCEPTION 'install the ordered catalog build index first'; END IF;
END $$;
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
 UPDATE state_board_rank_partitions SET catalog_phase='active' WHERE weapon=$1 AND ruler=$2;
END $$;
REVOKE ALL ON FUNCTION board_rank_catalog_activate(text,text,text,text) FROM PUBLIC;
