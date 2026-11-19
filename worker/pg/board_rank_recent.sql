-- New measured identities on an already public partition retain a separate, coalesced wakeup.
CREATE TABLE state_board_rank_recent_inputs (
 weapon text COLLATE "C" NOT NULL,
 ruler text COLLATE "C" NOT NULL,
 identity text COLLATE "C" NOT NULL,
 queued_at text COLLATE "C" NOT NULL DEFAULT board_input_timestamp(clock_timestamp()),
 PRIMARY KEY(weapon,ruler,identity)
);
CREATE INDEX board_rank_recent_inputs_ready ON state_board_rank_recent_inputs(queued_at,weapon,ruler,identity);
ALTER TABLE state_board_rank_dirty ADD COLUMN recent_version bigint;
ALTER TABLE state_board_rank_publication_ready ADD COLUMN recent_version bigint;
CREATE INDEX board_rank_recent_dirty ON state_board_rank_dirty(available_at,weapon,ruler,identity)
 WHERE recent_version>applied_version AND version>applied_version AND available_at IS NOT NULL;
CREATE INDEX board_rank_recent_publication ON state_board_rank_publication_ready(available_at,weapon,ruler)
 WHERE recent_version>applied_version AND version>applied_version AND available_at IS NOT NULL;

CREATE FUNCTION board_rank_capture_recent_measurement() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE parts jsonb:=NEW.key::jsonb;
BEGIN
 INSERT INTO state_board_rank_recent_inputs(weapon,ruler,identity)
 SELECT b.weapon,p.ruler,b.id FROM entity_weapon_builds b
 JOIN state_board_rank_partitions p ON p.weapon=b.weapon AND p.ruler=parts->>1
 JOIN state_board_read_partitions r ON r.weapon=p.weapon AND r.ruler=p.ruler
 WHERE b.id=parts->>0 AND p.complete AND r.enabled AND r.canonical_sources AND r.revision IS NOT NULL
 ON CONFLICT DO NOTHING;
 RETURN NULL;
END $$;
REVOKE ALL ON FUNCTION board_rank_capture_recent_measurement() FROM PUBLIC;
CREATE TRIGGER board_rank_recent_measurement AFTER INSERT OR UPDATE OF version ON state_board_input_mailbox
 FOR EACH ROW WHEN(NEW.scope='measurement' AND NEW.version>0) EXECUTE FUNCTION board_rank_capture_recent_measurement();
