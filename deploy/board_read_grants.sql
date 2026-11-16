-- The reader derives rankings; canonical inputs remain read-only to it.
DO $$BEGIN
  IF NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname='wfsim_board_read') THEN
    CREATE ROLE wfsim_board_read NOLOGIN;
  END IF;
END $$;
GRANT CONNECT ON DATABASE wfsim TO wfsim_board_read;
GRANT SELECT ON entity_weapon_builds,board_facts,entity_measurement_reviews,state_board_input_source TO wfsim_board_read;
GRANT SELECT,INSERT,UPDATE,DELETE ON state_board_read_routes,record_board_read_snapshots,state_board_read_rows TO wfsim_board_read;
GRANT SELECT,UPDATE(revision) ON state_board_read_partitions TO wfsim_board_read;
GRANT EXECUTE ON FUNCTION board_input_claim(bigint,text,bigint,text),board_input_read(text,text,text,bigint,bigint),
  board_input_lock(text,text,text,bigint),board_input_ack(text,text,text,bigint),
  board_input_retry(text,text,text,bigint,text,bigint),board_input_put(text,text,text) TO wfsim_board_read;
