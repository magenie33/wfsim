-- ONE-OFF: the official pool (`points_pool` in schema.sql). Applied once, before
-- the worker that writes it is live; a fresh database takes schema.sql alone.
--
--   npx wrangler d1 execute wfsim --remote --file worker/schema-points-pool.sql
CREATE TABLE IF NOT EXISTS points_pool (
  at     TEXT NOT NULL,
  source TEXT NOT NULL,
  work   INTEGER NOT NULL
);
