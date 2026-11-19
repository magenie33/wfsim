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

CREATE TABLE state_board_rank_new_scopes (
 weapon text COLLATE "C" NOT NULL,
 ruler text COLLATE "C" NOT NULL,
 queued_at text COLLATE "C" NOT NULL DEFAULT board_input_timestamp(clock_timestamp()),
 last_measured_at text COLLATE "C",
 PRIMARY KEY(weapon,ruler)
);
CREATE INDEX board_rank_new_scopes_ready ON state_board_rank_new_scopes(queued_at,weapon,ruler);
CREATE INDEX board_rank_new_scopes_latest ON state_board_rank_new_scopes(coalesce(last_measured_at,queued_at),weapon,ruler);
CREATE TABLE state_board_rank_scope_discovery (
 id integer PRIMARY KEY CHECK(id=1),
 source uuid NOT NULL,
 native text COLLATE "C" NOT NULL CHECK(native~'^[0-9a-f]{64}$'),
 archive text COLLATE "C" NOT NULL CHECK(archive~'^[0-9a-f]{64}$')
);

CREATE OR REPLACE FUNCTION board_rank_capture_recent_measurement() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE parts jsonb:=NEW.key::jsonb; target record;
BEGIN
 SELECT b.weapon,p.complete,r.enabled,r.canonical_sources,r.revision
 INTO target FROM entity_weapon_builds b
 LEFT JOIN state_board_rank_partitions p ON p.weapon=b.weapon AND p.ruler=parts->>1
 LEFT JOIN state_board_read_partitions r ON r.weapon=b.weapon AND r.ruler=parts->>1
 WHERE b.id=parts->>0;
 IF NOT FOUND THEN RETURN NULL; END IF;
 IF target.complete AND target.enabled AND target.canonical_sources AND target.revision IS NOT NULL THEN
  INSERT INTO state_board_rank_recent_inputs(weapon,ruler,identity) VALUES(target.weapon,parts->>1,parts->>0)
   ON CONFLICT DO NOTHING;
 ELSIF NOT NEW.removed AND jsonb_typeof(NEW.body->'score')='number' AND NEW.body->>'retired_at' IS NULL THEN
  INSERT INTO state_board_rank_new_scopes(weapon,ruler,last_measured_at) VALUES(target.weapon,parts->>1,board_input_timestamp(clock_timestamp()))
   ON CONFLICT(weapon,ruler) DO UPDATE SET last_measured_at=greatest(state_board_rank_new_scopes.last_measured_at,excluded.last_measured_at);
 END IF;
 RETURN NULL;
END $$;
REVOKE ALL ON FUNCTION board_rank_capture_recent_measurement() FROM PUBLIC;
CREATE TRIGGER board_rank_recent_measurement AFTER INSERT OR UPDATE OF version ON state_board_input_mailbox
 FOR EACH ROW WHEN(NEW.scope='measurement' AND NEW.version>0) EXECUTE FUNCTION board_rank_capture_recent_measurement();

CREATE FUNCTION board_rank_discover_scope(uuid,text,text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE candidate record;
BEGIN
 IF NOT EXISTS(SELECT 1 FROM state_board_rank_scope_discovery WHERE id=1 AND source=$1 AND native=$2)
  OR NOT EXISTS(SELECT 1 FROM state_board_request_control WHERE id=1 AND enabled) THEN RETURN NULL; END IF;
 PERFORM board_rank_source_lock($1::text);
 DELETE FROM state_board_rank_new_scopes s USING
  (SELECT weapon,ruler FROM state_board_rank_new_scopes ORDER BY queued_at,weapon,ruler LIMIT 1 FOR UPDATE SKIP LOCKED) head
  WHERE (s.weapon,s.ruler)=(head.weapon,head.ruler) RETURNING s.weapon,s.ruler INTO candidate;
 IF NOT FOUND THEN RETURN NULL; END IF;
 IF EXISTS(SELECT 1 FROM jsonb_array_elements_text($3::jsonb) allowed(id) WHERE id=candidate.ruler)
  AND NOT EXISTS(SELECT 1 FROM state_board_read_partitions WHERE weapon=candidate.weapon AND ruler=candidate.ruler) THEN
  INSERT INTO state_board_requested_partitions(weapon,ruler) VALUES(candidate.weapon,candidate.ruler) ON CONFLICT DO NOTHING;
 END IF;
 RETURN to_jsonb(candidate);
END $$;
REVOKE ALL ON FUNCTION board_rank_discover_scope(uuid,text,text) FROM PUBLIC;
