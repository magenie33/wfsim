-- THE LIBRARY, AS A DATABASE.
--
--   npx wrangler d1 create wfsim
--   npx wrangler d1 execute wfsim --remote --file worker/schema.sql
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
-- `(build, ruler, mode, what it read, what measured it) -> score` is true for
-- ever once computed, so it is written down the moment it is computed rather
-- than when a batch finishes. Nothing writes this table yet — the scorer keeps
-- its numbers in R2 blobs and the board yaml — and it is here so the shape is
-- settled before anything depends on it.
CREATE TABLE IF NOT EXISTS scores (
  identity     TEXT NOT NULL,
  ruler        TEXT NOT NULL,
  -- A ROW IS (build, ruler, MODE). A mode is a property of the WEAPON, not of
  -- the build — every melee carries seven and the Ballistica Prime four — and
  -- the cards that win one do not win another, so each is an independent
  -- ranking. Without this column a melee build's seven measurements collapse
  -- into one row and six of them are lost on write.
  mode         TEXT NOT NULL,
  -- WHAT THIS ROW READ. A data change dirties exactly the rows that read the
  -- file that moved, which is the cheap half of invalidation and is asked per
  -- row.
  data_fp      TEXT NOT NULL,
  -- WHICH GENERATION IT BELONGS TO, AND A GENERATION IS OPENED DELIBERATELY.
  -- Not a source hash: 55.6% of commits touch the engine, and a hash in this
  -- key would make each of them a full rescore — 8,008 CPU minutes against a
  -- day's budget of 960 CPU hours, thirteen times over on a working day. So
  -- most commits ride in the generation that is open, and one is opened when
  -- the AUDIT measures that a change actually moved numbers. The board
  -- publishes the newest COMPLETE generation, never a mixture of two.
  generation   TEXT NOT NULL,
  -- …AND WHICH BUILD ACTUALLY MEASURED IT, which is a different question and
  -- not part of the key. It is forensics: when a generation turns out to have
  -- been measured by something broken, this is what says which rows it wrote.
  measured_by  TEXT NOT NULL,
  score        REAL NOT NULL,
  -- The riven corner the search settled on, when there is one: a score alone
  -- cannot publish a riven row, because the reader has to be able to BUILD that
  -- riven and the page cannot re-derive it without paying for the search again.
  rolls        TEXT,
  -- WHAT THE ROW COST, so the bill is read off the rows rather than estimated.
  -- The spread is four orders of magnitude wide, so which rows are expensive is
  -- a question that has to be asked of the data and not guessed.
  cost_seconds REAL NOT NULL,
  -- WHEN IT WAS MEASURED, AND IT IS PROVENANCE — NEVER A TEST. A fact does not
  -- decay: while the fingerprints match, the score is right however old it is,
  -- and a rule that rescored by age would pay for rows that cannot have moved.
  -- What this is for: showing a reader how old a ROW is rather than how old the
  -- board is, ordering repairs oldest first, and saying afterwards which rows a
  -- bad engine wrote. Nothing branches on it.
  computed_at  TEXT NOT NULL,
  PRIMARY KEY (identity, ruler, mode, data_fp, generation)
);

-- "How complete is this generation" is one query rather than a walk, which is
-- the whole of the generation rule's cost.
CREATE INDEX IF NOT EXISTS scores_generation ON scores (generation, ruler);

-- …and "what has gone longest without being measured" is the order to repair
-- in, which is the one thing `computed_at` is allowed to decide.
CREATE INDEX IF NOT EXISTS scores_oldest ON scores (computed_at);
