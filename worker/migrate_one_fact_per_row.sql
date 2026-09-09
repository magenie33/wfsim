-- ONE FACT PER ROW — drop `data_fp` from the key of an existing `scores`.
--
--   npx wrangler d1 execute wfsim --remote -y --file worker/migrate_one_fact_per_row.sql
--
-- A ONE-OFF. Delete this file once every database has run it; `schema.sql` is
-- the shape a fresh one gets and is the only lasting statement of it.
--
-- WHY A FILE. It is the one migration that DROPS facts, and a destructive
-- statement against the only copy of 132 CPU-hours should be reviewable before
-- it is run rather than pasted from a chat window.
--
-- WHAT IT DROPS IS ALREADY UNREACHABLE. The reader asks a row for its own
-- fingerprint, so a second row under the same `(identity, ruler, mode)` could
-- never be returned — it was there so that reverting a data file restored its
-- answer without recomputing, and that bought less than an unbounded table
-- cost (docs/BOARD.md §"A fact is durable the instant it is computed").
--
-- THE NEWEST SURVIVES, by `finished_at`. Two measurements of one row differ
-- only in what they read and when, and the later one is the one this pipeline
-- would have written.
--
-- IF IT STOPS HALFWAY, `scores` is untouched and `scores_one` exists: nothing
-- is lost, and the retry is `DROP TABLE scores_one` and run this again. The
-- destructive statement is the fifth, and by then every row it needs has been
-- copied.

-- THE COLUMNS ARE SPELLED OUT, and the key with them. `CREATE TABLE … AS
-- SELECT` carries neither, so a rebuild that used it would leave a table with
-- NO primary key — and `INSERT OR REPLACE` against no key inserts, so the
-- shipper would quietly grow a duplicate per measurement for ever. Nothing
-- would fail; `schema.sql`'s `CREATE TABLE IF NOT EXISTS` is a no-op on a table
-- that already exists, so it would not repair it either.
CREATE TABLE scores_one (
  identity     TEXT NOT NULL,
  ruler        TEXT NOT NULL,
  mode         TEXT NOT NULL,
  data_fp      TEXT NOT NULL,
  measured_by  TEXT NOT NULL,
  score        REAL NOT NULL,
  metric       TEXT NOT NULL,
  rolls        TEXT,
  cost_seconds REAL NOT NULL,
  started_at   TEXT NOT NULL,
  finished_at  TEXT NOT NULL,
  PRIMARY KEY (identity, ruler, mode)
);

INSERT INTO scores_one
SELECT identity, ruler, mode, data_fp, measured_by, score, metric, rolls,
       cost_seconds, started_at, finished_at
FROM (
  SELECT *,
         ROW_NUMBER() OVER (
           PARTITION BY identity, ruler, mode
           ORDER BY finished_at DESC
         ) AS rn
  FROM scores
)
WHERE rn = 1;

DROP TABLE scores;
ALTER TABLE scores_one RENAME TO scores;

CREATE INDEX scores_ruler ON scores (ruler);
CREATE INDEX scores_oldest ON scores (finished_at);
