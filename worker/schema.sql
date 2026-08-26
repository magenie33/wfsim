-- THE LIBRARY, AS A DATABASE.
--
--   npx wrangler d1 create wfsim-library
--   npx wrangler d1 execute wfsim-library --remote --file worker/schema.sql
--
-- …then declare the binding in `wrangler.jsonc` (see docs/BOARD.md §Setup) and
-- deploy. Until the binding exists the mirror in `worker/index.js` is a no-op,
-- so this file can land long before the database does.
--
-- WHY THIS EXISTS. KV holds the library today and cannot be asked a question
-- about it: no queries, no transactions, no bulk read, and listing is the only
-- index. The library is the one thing here that cannot be regenerated — the
-- boards are derived from it, the site is generated, the code is in git — so it
-- has to be the thing that is easiest to inspect, count and dump, and in KV it
-- is the hardest (docs/BOARD.md, 2026-08-26).

-- ONE ROW PER BUILD, keyed by the same identity KV uses, so the two stores can
-- be compared row for row without a join key having to be invented.
--
-- The RECORD is kept whole as json rather than exploded into columns. The axes
-- are declared once, in `AXES` in worker/index.js, and a build has gained an
-- axis four times — a schema that spelled them out would be a fifth place to
-- forget one, which is the exact bug that lost `mode` and then `valence`.
-- Anything worth indexing gets a generated column instead; `weapon` is the
-- first, because "how well covered is this weapon" is the question the board is
-- actually run on.
CREATE TABLE IF NOT EXISTS builds (
  identity TEXT PRIMARY KEY,
  -- The submission DAY, and nothing finer. The store records nothing about
  -- submitters — no IP, no token, no timestamp that could order one person's
  -- submissions against another's — and a schema is a place that promise could
  -- quietly be broken, so it is stated here too.
  at       TEXT NOT NULL,
  record   TEXT NOT NULL,
  weapon   TEXT GENERATED ALWAYS AS (json_extract(record, '$.weapon')) VIRTUAL
);

CREATE INDEX IF NOT EXISTS builds_weapon ON builds (weapon);
CREATE INDEX IF NOT EXISTS builds_at ON builds (at);

-- A SCORE IS A FACT, NOT A STEP IN A PIPELINE (docs/BOARD.md).
--
-- `(identity, ruler, fingerprint) -> score` is true for ever once computed, so
-- it is written down the moment it is computed rather than when a batch
-- finishes. Nothing writes this table yet — the scorer still keeps its numbers
-- in the board yaml — and it is here so the shape is settled before anything
-- depends on it.
--
-- THE FINGERPRINT IS PART OF THE KEY, which is what makes a rescore a BACKFILL
-- instead of a rebuild: a code change does not invalidate rows, it means the
-- facts for the new fingerprint are simply missing, and the board keeps
-- publishing the newest generation that is COMPLETE while they fill in.
CREATE TABLE IF NOT EXISTS scores (
  identity    TEXT NOT NULL,
  ruler       TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  score       REAL NOT NULL,
  -- The riven corner the search settled on, when there is one: a score alone
  -- cannot publish a riven row, because the reader has to be able to BUILD that
  -- riven and the page cannot re-derive it without paying for the search again.
  rolls       TEXT,
  PRIMARY KEY (identity, ruler, fingerprint)
);

-- "How complete is this generation" is one query rather than a walk, which is
-- the whole of the generation rule's cost.
CREATE INDEX IF NOT EXISTS scores_generation ON scores (fingerprint, ruler);
