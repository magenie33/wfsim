-- Native-derived indexes and their own durable preparation obligations.
CREATE SEQUENCE board_rank_generation;
CREATE FUNCTION board_rank_source_lock(text) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
 PERFORM 1 FROM state_board_input_source WHERE id=1 AND source=$1 FOR SHARE;
 IF NOT FOUND THEN RAISE EXCEPTION 'board source changed' USING ERRCODE='55000';END IF;
END $$;
REVOKE ALL ON FUNCTION board_rank_source_lock(text) FROM PUBLIC;
CREATE TABLE state_board_rank_partitions (
 weapon TEXT COLLATE "C" NOT NULL,
 ruler TEXT COLLATE "C" NOT NULL,
 source TEXT COLLATE "C" NOT NULL,
 native TEXT COLLATE "C" NOT NULL,
 cut TEXT COLLATE "C" NOT NULL,
 path TEXT NOT NULL DEFAULT '',
 bytes BIGINT NOT NULL DEFAULT 0,
 total BIGINT NOT NULL DEFAULT 0,
 cursor BIGINT NOT NULL DEFAULT 0,
 loaded BIGINT NOT NULL DEFAULT 0,
 seeded BOOLEAN NOT NULL DEFAULT false,
 reconcile_cursor TEXT COLLATE "C" NOT NULL DEFAULT '',
 reconciled BOOLEAN NOT NULL DEFAULT false,
 complete BOOLEAN NOT NULL DEFAULT false,
 available_at TEXT COLLATE "C" DEFAULT board_input_timestamp(clock_timestamp()),
 error TEXT,
 revision BIGINT NOT NULL DEFAULT 0,
 input_sha256 TEXT COLLATE "C" NOT NULL,
 PRIMARY KEY(weapon,ruler)
);
CREATE TABLE state_board_rank_sources (
 weapon TEXT COLLATE "C" NOT NULL,
 ruler TEXT COLLATE "C" NOT NULL,
 identity TEXT COLLATE "C" NOT NULL,
 record_sha256 TEXT COLLATE "C" NOT NULL,
 descriptor JSONB NOT NULL,
 input_sha256 TEXT COLLATE "C" NOT NULL,
 live BOOLEAN NOT NULL,
 PRIMARY KEY(weapon,ruler,identity),
 FOREIGN KEY(weapon,ruler) REFERENCES state_board_rank_partitions
);
CREATE TABLE state_board_rank_dependencies (
 weapon TEXT COLLATE "C" NOT NULL,
 ruler TEXT COLLATE "C" NOT NULL,
 identity TEXT COLLATE "C" NOT NULL,
 scope TEXT COLLATE "C" NOT NULL CHECK(scope IN('build','measurement','review')),
 key TEXT COLLATE "C" NOT NULL,
 version BIGINT,
 PRIMARY KEY(weapon,ruler,identity,scope,key),
 FOREIGN KEY(weapon,ruler,identity) REFERENCES state_board_rank_sources ON DELETE CASCADE
);
CREATE INDEX board_rank_dependency_input ON state_board_rank_dependencies(scope,key,weapon,ruler,identity);
CREATE TABLE state_board_rank_route_inputs (
 scope TEXT COLLATE "C" NOT NULL,
 key TEXT COLLATE "C" NOT NULL,
 version BIGINT NOT NULL,
 PRIMARY KEY(scope,key)
);
CREATE TABLE state_board_rank_rows (
 weapon TEXT COLLATE "C" NOT NULL,
 ruler TEXT COLLATE "C" NOT NULL,
 identity TEXT COLLATE "C" NOT NULL,
 mode TEXT COLLATE "C" NOT NULL,
 mode_order BIGINT NOT NULL,
 shape TEXT COLLATE "C" NOT NULL,
 floor TEXT COLLATE "C" NOT NULL,
 def BOOLEAN NOT NULL,
 score DOUBLE PRECISION NOT NULL,
 riven BOOLEAN NOT NULL,
 page TEXT COLLATE "C" NOT NULL CHECK(octet_length(page)<=65536),
 PRIMARY KEY(weapon,ruler,identity,mode),
 FOREIGN KEY(weapon,ruler,identity) REFERENCES state_board_rank_sources
);
CREATE INDEX board_rank_shape_order ON state_board_rank_rows(weapon,ruler,shape,score DESC,def DESC,identity,mode_order);
CREATE TABLE state_board_rank_winners (
 LIKE state_board_rank_rows INCLUDING CONSTRAINTS,
 PRIMARY KEY(weapon,ruler,shape)
);
CREATE INDEX board_rank_winner_order ON state_board_rank_winners(weapon,ruler,score DESC,def DESC,identity,mode_order);
CREATE INDEX board_rank_winner_floor ON state_board_rank_winners(weapon,ruler,floor,score DESC);
CREATE TABLE state_board_rank_dirty (
 weapon TEXT COLLATE "C" NOT NULL,
 ruler TEXT COLLATE "C" NOT NULL,
 identity TEXT COLLATE "C" NOT NULL,
 version BIGINT NOT NULL DEFAULT nextval('board_rank_generation'),
 applied_version BIGINT NOT NULL DEFAULT -1,
 lease TEXT COLLATE "C",
 lease_until TEXT COLLATE "C",
 available_at TEXT COLLATE "C" DEFAULT board_input_timestamp(clock_timestamp()),
 error TEXT,
 PRIMARY KEY(weapon,ruler,identity),
 FOREIGN KEY(weapon,ruler) REFERENCES state_board_rank_partitions
);
CREATE INDEX board_rank_dirty_ready ON state_board_rank_dirty(available_at,weapon,ruler,identity)
 WHERE version>applied_version AND available_at IS NOT NULL;
CREATE TABLE state_board_rank_baseline_obligations (
 weapon TEXT COLLATE "C" NOT NULL,
 ruler TEXT COLLATE "C" NOT NULL,
 identity TEXT COLLATE "C" NOT NULL,
 version BIGINT NOT NULL,
 PRIMARY KEY(weapon,ruler,identity),
 FOREIGN KEY(weapon,ruler,identity) REFERENCES state_board_rank_dirty
);
CREATE TABLE state_board_rank_publication_ready (
 weapon TEXT COLLATE "C" NOT NULL,
 ruler TEXT COLLATE "C" NOT NULL,
 version BIGINT NOT NULL DEFAULT nextval('board_rank_generation'),
 applied_version BIGINT NOT NULL DEFAULT -1,
 lease TEXT COLLATE "C",
 lease_version BIGINT,
 lease_until TEXT COLLATE "C",
 available_at TEXT COLLATE "C" DEFAULT board_input_timestamp(clock_timestamp()),
 error TEXT,
 delivered_revision BIGINT NOT NULL DEFAULT 0,
 delivered_output TEXT COLLATE "C",
 PRIMARY KEY(weapon,ruler),
 FOREIGN KEY(weapon,ruler) REFERENCES state_board_rank_partitions
);
CREATE INDEX board_rank_publication_ready ON state_board_rank_publication_ready(available_at,weapon,ruler)
 WHERE version>applied_version AND available_at IS NOT NULL;
CREATE TABLE state_board_rank_generations (
 token UUID PRIMARY KEY,
 weapon TEXT COLLATE "C" NOT NULL,
 ruler TEXT COLLATE "C" NOT NULL,
 source TEXT COLLATE "C" NOT NULL,
 native TEXT COLLATE "C" NOT NULL,
 cut TEXT COLLATE "C" NOT NULL,
 engine TEXT COLLATE "C" NOT NULL,
 index_revision BIGINT NOT NULL,
 ready_version BIGINT NOT NULL,
 phase TEXT NOT NULL CHECK(phase IN('floor','upload','published','abandoned')),
 directory TEXT NOT NULL,
 winners_sha256 TEXT COLLATE "C" NOT NULL,
 winners_cursor BIGINT NOT NULL DEFAULT 0,
 output_size BIGINT NOT NULL DEFAULT 0,
 output_sha256 TEXT COLLATE "C" NOT NULL,
 output_rows BIGINT NOT NULL DEFAULT 0,
 upload_cursor BIGINT NOT NULL DEFAULT 0,
 uploaded_rows BIGINT NOT NULL DEFAULT 0,
 leaders JSONB NOT NULL DEFAULT '[]',
 captured_at TEXT COLLATE "C" NOT NULL,
 input_sha256 TEXT COLLATE "C" NOT NULL,
 created_at TEXT COLLATE "C" NOT NULL DEFAULT board_input_timestamp(clock_timestamp()),
 published_at TEXT COLLATE "C",
 FOREIGN KEY(weapon,ruler) REFERENCES state_board_rank_partitions
);
CREATE INDEX board_rank_generation_pending ON state_board_rank_generations(weapon,ruler,created_at,token)
 WHERE phase IN('floor','upload');
CREATE INDEX board_rank_generation_cleanup ON state_board_rank_generations(created_at,token) WHERE phase IN('published','abandoned');
CREATE TABLE state_board_rank_snapshot_rows (
 token UUID NOT NULL REFERENCES state_board_rank_generations ON DELETE CASCADE,
 ordinal BIGINT NOT NULL CHECK(ordinal>0),
 body TEXT COLLATE "C" NOT NULL CHECK(octet_length(body)<=65536),
 PRIMARY KEY(token,ordinal)
);
CREATE TABLE record_board_rank_snapshot_refs (
 weapon TEXT COLLATE "C" NOT NULL,
 ruler TEXT COLLATE "C" NOT NULL,
 revision TEXT COLLATE "C" NOT NULL,
 token UUID NOT NULL REFERENCES state_board_rank_generations,
 PRIMARY KEY(weapon,ruler,revision),
 FOREIGN KEY(weapon,ruler,revision) REFERENCES record_board_read_snapshots ON DELETE CASCADE
);
CREATE INDEX board_rank_snapshot_ref_token ON record_board_rank_snapshot_refs(token);
REVOKE ALL ON state_board_rank_partitions,state_board_rank_sources,state_board_rank_dependencies,
 state_board_rank_route_inputs,state_board_rank_rows,state_board_rank_winners,state_board_rank_dirty,state_board_rank_baseline_obligations,
 state_board_rank_publication_ready,state_board_rank_generations,state_board_rank_snapshot_rows,record_board_rank_snapshot_refs FROM PUBLIC;
REVOKE ALL ON SEQUENCE board_rank_generation FROM PUBLIC;
