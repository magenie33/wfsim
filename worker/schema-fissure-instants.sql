-- ONE-OFF: a fissure's start and end are instants (docs/NAMING.md §9), stored
-- before as epoch milliseconds under `_ms`. Applied once, right before the
-- worker that writes `started_at`; a fresh database takes schema.sql alone.
--
--   npx wrangler d1 execute wfsim --remote --file worker/schema-fissure-instants.sql
CREATE TABLE fissures_instants (
  id            TEXT PRIMARY KEY,
  list          TEXT NOT NULL,
  tier          TEXT NOT NULL,
  mission       TEXT,
  node          TEXT NOT NULL,
  started_at    TEXT NOT NULL,
  ends_at       TEXT NOT NULL
);
INSERT INTO fissures_instants SELECT id, list, tier, mission, node,
  strftime('%Y-%m-%dT%H:%M:%S.', started_at_ms / 1000, 'unixepoch') || printf('%03dZ', started_at_ms % 1000),
  strftime('%Y-%m-%dT%H:%M:%S.', ends_at_ms / 1000, 'unixepoch') || printf('%03dZ', ends_at_ms % 1000) FROM fissures;
DROP TABLE fissures;
ALTER TABLE fissures_instants RENAME TO fissures;
