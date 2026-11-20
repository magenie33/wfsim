-- Queue age is independent of the next retry or lease expiry.
ALTER TABLE IF EXISTS state_board_measurement_selection
 ADD COLUMN IF NOT EXISTS publication_zone smallint;
ALTER TABLE state_board_rank_new_scopes ADD COLUMN IF NOT EXISTS zone smallint NOT NULL DEFAULT 3;
ALTER TABLE state_board_rank_new_scopes ADD COLUMN IF NOT EXISTS zone_position bigint;
ALTER TABLE state_board_rank_recent_inputs ADD COLUMN IF NOT EXISTS zone smallint NOT NULL DEFAULT 3;
ALTER TABLE state_board_rank_recent_inputs ADD COLUMN IF NOT EXISTS zone_position bigint;
ALTER TABLE state_board_rank_dirty ADD COLUMN IF NOT EXISTS zone smallint NOT NULL DEFAULT 99;
ALTER TABLE state_board_rank_dirty ADD COLUMN IF NOT EXISTS zone_position bigint;
ALTER TABLE state_board_rank_dirty ADD COLUMN IF NOT EXISTS queued_at text COLLATE "C";
ALTER TABLE state_board_rank_dirty ALTER COLUMN queued_at SET DEFAULT board_input_timestamp(clock_timestamp());
ALTER TABLE state_board_rank_publication_ready ADD COLUMN IF NOT EXISTS zone smallint NOT NULL DEFAULT 99;
ALTER TABLE state_board_rank_publication_ready ADD COLUMN IF NOT EXISTS zone_position bigint;
ALTER TABLE state_board_rank_publication_ready ADD COLUMN IF NOT EXISTS queued_at text COLLATE "C";
ALTER TABLE state_board_rank_publication_ready ALTER COLUMN queued_at SET DEFAULT board_input_timestamp(clock_timestamp());
CREATE INDEX IF NOT EXISTS board_rank_new_scopes_zone ON state_board_rank_new_scopes(zone,coalesce(zone_position,0),queued_at,weapon,ruler);
CREATE INDEX IF NOT EXISTS board_rank_recent_zone ON state_board_rank_recent_inputs(zone,coalesce(zone_position,0),queued_at,weapon,ruler,identity);
CREATE INDEX IF NOT EXISTS board_rank_publication_zone ON state_board_rank_publication_ready(zone,coalesce(zone_position,0),coalesce(queued_at,available_at),weapon,ruler)
 WHERE version>applied_version AND available_at IS NOT NULL;
