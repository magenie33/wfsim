ALTER TABLE state_board_rank_generations ADD COLUMN IF NOT EXISTS riven_policy text COLLATE "C";

CREATE TABLE IF NOT EXISTS state_board_rank_riven_policy (
 id integer PRIMARY KEY CHECK(id=1),
 digest text COLLATE "C" NOT NULL CHECK(digest~'^[a-f0-9]{64}$')
);
CREATE TABLE IF NOT EXISTS state_board_rank_riven_repairs (
 weapon text COLLATE "C" NOT NULL,
 ruler text COLLATE "C" NOT NULL,
 revision text COLLATE "C" NOT NULL,
 inputs jsonb NOT NULL CHECK(jsonb_typeof(inputs)='array'),
 PRIMARY KEY(weapon,ruler),
 FOREIGN KEY(weapon,ruler) REFERENCES state_board_rank_partitions
);

CREATE OR REPLACE FUNCTION board_rank_guard_riven_policy() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE expected text;
BEGIN
 SELECT digest INTO expected FROM state_board_rank_riven_policy WHERE id=1 FOR SHARE;
 IF expected IS NOT NULL AND NOT EXISTS(SELECT 1 FROM state_board_rank_generations g
   WHERE g.token=NEW.token AND (g.weapon,g.ruler)=(NEW.weapon,NEW.ruler) AND g.riven_policy=expected) THEN
  RAISE EXCEPTION 'ranking publication riven policy changed' USING ERRCODE='55000';
 END IF;
 RETURN NEW;
END $$;
CREATE OR REPLACE TRIGGER board_rank_guard_riven_policy BEFORE INSERT OR UPDATE OF token ON record_board_rank_snapshot_refs
 FOR EACH ROW EXECUTE FUNCTION board_rank_guard_riven_policy();

CREATE OR REPLACE FUNCTION board_rank_finish_riven_repair(text,text,text,uuid) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE repair state_board_rank_riven_repairs%ROWTYPE;
BEGIN
 SELECT * INTO repair FROM state_board_rank_riven_repairs WHERE (weapon,ruler)=($1,$2) FOR UPDATE;
 IF NOT FOUND THEN RETURN false; END IF;
 IF $3=repair.revision OR NOT EXISTS(SELECT 1 FROM state_board_rank_partitions p JOIN state_board_read_partitions s USING(weapon,ruler)
      JOIN state_board_public_heads h ON (h.weapon,h.ruler,h.revision)=(s.weapon,s.ruler,s.revision)
      JOIN state_board_rank_generations g ON (g.weapon,g.ruler,g.index_revision)=(p.weapon,p.ruler,p.revision)
      JOIN state_board_rank_riven_policy policy ON policy.id=1 AND policy.digest=g.riven_policy
      JOIN record_board_rank_snapshot_refs r ON (r.weapon,r.ruler,r.revision)=(p.weapon,p.ruler,$3)
      WHERE (p.weapon,p.ruler)=($1,$2) AND p.complete AND s.enabled AND s.revision=repair.revision
       AND g.token=$4 AND g.phase='upload' AND g.source=p.source AND g.native=p.native AND g.cut=p.cut
       AND g.uploaded_rows=g.output_rows AND g.upload_cursor=g.output_size)
    OR EXISTS(SELECT 1 FROM jsonb_to_recordset(repair.inputs) i(identity text,version bigint)
      LEFT JOIN state_board_rank_dirty d ON (d.weapon,d.ruler,d.identity)=($1,$2,i.identity)
      WHERE d.applied_version IS NULL OR d.applied_version<i.version) THEN
  RAISE EXCEPTION 'riven repair ranking or public revision advanced' USING ERRCODE='40001';
 END IF;
 INSERT INTO state_board_public_withdrawn_revisions VALUES($1,$2,repair.revision) ON CONFLICT DO NOTHING;
 DELETE FROM state_board_rank_riven_repairs WHERE (weapon,ruler)=($1,$2);
 RETURN true;
END $$;
REVOKE ALL ON FUNCTION board_rank_guard_riven_policy(),board_rank_finish_riven_repair(text,text,text,uuid) FROM PUBLIC;
REVOKE ALL ON state_board_rank_riven_policy,state_board_rank_riven_repairs FROM PUBLIC;
