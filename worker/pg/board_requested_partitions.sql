-- Requested targets prepare their current database view before selecting an online partition.
CREATE TABLE state_board_requested_partitions (
  weapon TEXT COLLATE "C" NOT NULL,
  ruler TEXT COLLATE "C" NOT NULL,
  position BIGINT NOT NULL DEFAULT nextval('compute_queue_position'),
  enqueued BOOLEAN NOT NULL DEFAULT false,
  activated_at TEXT COLLATE "C",
  PRIMARY KEY(weapon,ruler)
);
CREATE INDEX board_requested_partitions_ready ON state_board_requested_partitions(position) WHERE NOT enqueued;
CREATE FUNCTION board_requested_partition_capture() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  IF OLD.phase='pending' AND NEW.phase='staged' THEN
    INSERT INTO state_board_requested_partitions(weapon,ruler)
      SELECT i.input#>>'{record,weapon}',r.ruler FROM entity_board_requests r
        JOIN entity_build_intake_intents i ON i.id=NEW.intent WHERE r.id=NEW.id
      ON CONFLICT DO NOTHING;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER board_requested_partition_ready AFTER UPDATE OF phase ON state_board_requests
  FOR EACH ROW EXECUTE FUNCTION board_requested_partition_capture();

CREATE FUNCTION board_requested_partition_enqueue(text,uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE target state_board_requested_partitions%ROWTYPE; scene text;
BEGIN
  IF NOT EXISTS(SELECT 1 FROM state_board_input_source WHERE id=1 AND source=$2::text) THEN RAISE EXCEPTION 'board source changed';END IF;
  IF NOT EXISTS(SELECT 1 FROM state_board_request_control WHERE id=1 AND enabled) THEN RETURN NULL;END IF;
  SELECT * INTO target FROM state_board_requested_partitions WHERE NOT enqueued ORDER BY position LIMIT 1 FOR UPDATE SKIP LOCKED;
  IF NOT FOUND THEN RETURN NULL;END IF;
  IF EXISTS(SELECT 1 FROM state_board_read_partitions WHERE weapon=target.weapon AND ruler=target.ruler) THEN
    UPDATE state_board_requested_partitions SET enqueued=true WHERE weapon=target.weapon AND ruler=target.ruler;
    RETURN jsonb_build_object('existing',true,'weapon',target.weapon,'ruler',target.ruler);
  END IF;
  SELECT item->>'scene' INTO scene FROM jsonb_array_elements($1::jsonb) item WHERE item->>'id'=target.ruler;
  IF scene IS NULL OR scene!~'^[a-z0-9_]{0,160}$' THEN RAISE EXCEPTION 'unknown requested partition ruler';END IF;
  INSERT INTO state_board_read_partitions(weapon,ruler,scene) VALUES(target.weapon,target.ruler,scene);
  PERFORM board_input_put('partition','["'||target.weapon||'","'||target.ruler||'"]',
    jsonb_build_object('weapon',target.weapon,'ruler',target.ruler,'qualify_requested',true)::text);
  UPDATE state_board_requested_partitions SET enqueued=true WHERE weapon=target.weapon AND ruler=target.ruler;
  RETURN jsonb_build_object('existing',false,'weapon',target.weapon,'ruler',target.ruler);
END $$;

CREATE OR REPLACE FUNCTION board_requested_partition_selected(text,text,boolean) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE part state_board_read_partitions%ROWTYPE;
BEGIN
  IF to_regclass('state_board_public_import') IS NOT NULL THEN
    IF EXISTS(SELECT FROM state_board_public_import WHERE id=1) THEN
      RAISE EXCEPTION 'public catalog requires a qualified first publication';
    END IF;
  END IF;
  IF NOT EXISTS(SELECT 1 FROM state_board_request_control WHERE id=1 AND enabled) THEN
    RAISE EXCEPTION 'requested partition preparation is paused';END IF;
  IF NOT EXISTS(SELECT 1 FROM state_board_requested_partitions WHERE weapon=$1 AND ruler=$2 AND enqueued) THEN
    RAISE EXCEPTION 'partition has no requested source';END IF;
  IF $3 THEN SELECT * INTO part FROM state_board_read_partitions WHERE weapon=$1 AND ruler=$2 FOR UPDATE;
  ELSE SELECT * INTO part FROM state_board_read_partitions WHERE weapon=$1 AND ruler=$2;END IF;
  IF part.weapon IS NULL OR part.enabled OR part.canonical_sources OR part.revision IS NOT NULL THEN
    RAISE EXCEPTION 'requested partition selection changed';END IF;
  RETURN to_jsonb(part);
END $$;

CREATE FUNCTION board_requested_partition_activate(text,text,text,uuid) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  PERFORM board_requested_partition_selected($1,$2,true);
  IF NOT EXISTS(SELECT 1 FROM state_board_input_source WHERE id=1 AND source=$4::text)
    OR NOT EXISTS(SELECT 1 FROM record_board_read_snapshots WHERE weapon=$1 AND ruler=$2 AND revision=$3 AND source=$4::text) THEN
    RAISE EXCEPTION 'requested partition has no committed projection';END IF;
  UPDATE state_board_read_partitions SET enabled=true,canonical_sources=true,revision=$3 WHERE weapon=$1 AND ruler=$2;
  UPDATE state_board_requested_partitions SET activated_at=board_input_timestamp(clock_timestamp()) WHERE weapon=$1 AND ruler=$2;
END $$;
REVOKE ALL ON FUNCTION board_requested_partition_capture(),board_requested_partition_enqueue(text,uuid),
  board_requested_partition_selected(text,text,boolean),board_requested_partition_activate(text,text,text,uuid) FROM PUBLIC;
