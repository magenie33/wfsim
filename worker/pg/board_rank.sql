-- Native-derived indexes and their own durable preparation obligations.
CREATE SEQUENCE board_rank_generation;
CREATE TABLE state_board_rank_partitions (
 weapon TEXT COLLATE "C" NOT NULL,
 ruler TEXT COLLATE "C" NOT NULL,
 source TEXT COLLATE "C" NOT NULL,
 native TEXT COLLATE "C" NOT NULL,
 cut TEXT COLLATE "C" NOT NULL,
 cursor BIGINT NOT NULL DEFAULT 0,
 loaded BIGINT NOT NULL DEFAULT 0,
 complete BOOLEAN NOT NULL DEFAULT false,
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
REVOKE ALL ON state_board_rank_partitions,state_board_rank_sources,state_board_rank_dependencies,
 state_board_rank_rows,state_board_rank_winners,state_board_rank_dirty FROM PUBLIC;
REVOKE ALL ON SEQUENCE board_rank_generation FROM PUBLIC;
