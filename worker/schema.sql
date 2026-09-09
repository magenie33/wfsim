-- THE LIBRARY, AS A DATABASE.
--
--   npx wrangler d1 create wfsim
--   npx wrangler d1 execute wfsim --remote --file worker/schema.sql
--
-- …then declare the binding in `wrangler.jsonc` (see docs/BOARD.md §Setup) and
-- deploy. Without the binding every endpoint that reads or writes answers 503,
-- which is the honest state rather than a silent one.
--
-- WHY A DATABASE. The library is the one thing here that cannot be regenerated
-- — the boards are derived from it, the site is generated, the code is in git —
-- so it has to be the thing that is easiest to inspect, count and dump. A key
-- store is the hardest: no queries, no transactions, no bulk read, and listing
-- as the only index (docs/BOARD.md §"One database").

-- ONE ROW PER BUILD, keyed by `identity(rec)` — a function of the canonical
-- build, so a resubmission is the same row and there is nothing to keep in step.
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
-- `(build, ruler, mode) -> score` is true for ever once computed, so it is
-- written down the moment it is computed rather than when a batch finishes. It
-- does not stop being true later: neither the clock nor a thousand commits of
-- distance is evidence that a measurement was wrong, and the only thing that
-- retires one is its INPUTS moving. `scripts/ship_facts.sh` writes it beside the
-- running scorer; `scripts/fetch_facts.sh` reads them back out. This
-- table is the only source the publisher has.
CREATE TABLE IF NOT EXISTS scores (
  identity     TEXT NOT NULL,
  ruler        TEXT NOT NULL,
  -- A ROW IS (build, ruler, MODE). A mode is a property of the WEAPON, not of
  -- the build -- every melee carries seven and the Ballistica Prime four -- and
  -- the cards that win one do not win another, so each is an independent
  -- ranking. Without this column a melee build's seven measurements collapse
  -- into one row and six of them are lost on write.
  mode         TEXT NOT NULL,
  -- WHAT THIS ROW READ, and it is the only thing here that decides anything.
  -- A data change dirties exactly the rows that read the file that moved, which
  -- is asked per row and is the whole of the invalidation this pipeline can
  -- derive.
  --
  -- NOT IN THE KEY. A row has ONE fact, the last measurement of it, and this
  -- travels ON that fact — so the same row measured under three generations of
  -- data is one row and not three. Keeping the older ones bought one thing,
  -- that reverting a data file restored its answer without recomputing, and
  -- cost an unbounded table: every edit to a file no entity owns mints a fresh
  -- copy of every row on the board.
  data_fp      TEXT NOT NULL,
  -- WHICH BUILD MEASURED IT, AND IT DECIDES NOTHING. An engine version being
  -- older does not make a score wrong -- the two are a REFERENCE relation, not a
  -- validity one. What a row READS is enumerable from the row, so that is
  -- checked exactly above; what the code DOES to it is not, and no hash can
  -- answer it. Only a MEASUREMENT can, which is the audit's job, and this is
  -- what says which rows a build wrote once one is found to be broken.
  measured_by  TEXT NOT NULL,
  score        REAL NOT NULL,
  -- WHAT `score` IS IN — the ruler's CORE metric, an id from
  -- `engine::metrics::ALL` and never a label, which is translated on the page.
  --
  -- A TEST HAS ONE CORE, and that one is what ranks. A ruler may read more than
  -- one metric off the same fight; those are readings OF this row and belong
  -- beside it in a column of their own, never in rows of their own — a second
  -- row per metric would put two answers under one key and hand the publisher a
  -- ranking with two units in it.
  --
  -- IT IS NOT IN THE KEY AND DECIDES NOTHING, the same terms as `measured_by`.
  -- A ruler that changes its core changes its file, which moves `data_fp`,
  -- which is already the whole of invalidation; branching on this as well would
  -- be a second answer to a question that has one. What it is for is the ROW
  -- OUTLIVING THAT FILE: a fact is kept until its inputs move, so a row can be
  -- older than the ruler's current terms, and reading one back without this
  -- means checking out the commit that produced it.
  metric       TEXT NOT NULL,
  -- The riven corner the search settled on, when there is one: a score alone
  -- cannot publish a riven row, because the reader has to be able to BUILD that
  -- riven and the page cannot re-derive it without paying for the search again.
  rolls        TEXT,
  -- WHAT THE ROW COST, so the bill is read off the rows rather than estimated,
  -- and so the next run can pack its shards by work rather than by count. NOT
  -- derivable from the two clocks below: a row paid for in sittings spans a wall
  -- clock much longer than the fight it contains.
  cost_seconds REAL NOT NULL,
  -- WHEN THE FIGHT STARTED AND WHEN IT ENDED. Provenance, and nothing branches
  -- on either: a fact does not decay, so while `data_fp` matches the score is
  -- right however old it is. What they are for is showing a reader how old a ROW
  -- is rather than how old the board is, ordering repairs oldest first, and
  -- saying afterwards which rows a bad build wrote and when.
  --
  -- A ROW MIGRATED FROM BEFORE THEY EXISTED CARRIES THE SAME VALUE IN BOTH,
  -- which is how "we do not know when this started" is spelled. Deriving a
  -- start by subtracting the cost would invent precision the old row never had.
  started_at   TEXT NOT NULL,
  finished_at  TEXT NOT NULL,
  -- ONE FACT PER ROW: the last measurement of it. Neither the clock nor the
  -- build that wrote it says anything about whether the number is right, so
  -- neither is here; what a stored score can still be asked is whether its
  -- INPUTS hold, and that rides on the row as `data_fp`.
  PRIMARY KEY (identity, ruler, mode)
);

-- "What has this ruler measured" is one indexed query, which is what the set
-- difference is asked through.
CREATE INDEX IF NOT EXISTS scores_ruler ON scores (ruler);

-- …and "what has gone longest without being measured" is the order to repair
-- in, which is the one thing a clock here is allowed to decide.
CREATE INDEX IF NOT EXISTS scores_oldest ON scores (finished_at);

-- TWO MEASUREMENTS OF ONE ROW DISAGREE, and the board should look again.
--
-- The only EVENT in this system: nobody can derive it from anything, so it has
-- a row of its own. Everything else here is a fact or a build.
--
-- NOTHING HERE IS TRUSTED AS A SCORE. The numbers are a REPORT that two
-- measurements differ; the board answers by measuring again, and only its own
-- measurement moves a row. The worst a forged report buys is one wasted
-- rescore, which is why the endpoint needs no authentication.
--
-- KEYED BY THE ROW, so a thousand players finding one disagreement leave one
-- report. `at` is the DAY, and a report the board has acted on is swept by the
-- nightly job rather than expiring on its own.
CREATE TABLE IF NOT EXISTS disagreements (
  ruler    TEXT NOT NULL,
  identity TEXT NOT NULL,
  at       TEXT NOT NULL,
  client   REAL NOT NULL,
  board    REAL NOT NULL,
  record   TEXT NOT NULL,
  PRIMARY KEY (ruler, identity)
);

-- HOW MANY PEOPLE HAVE CHIPPED IN — a COUNT, and the schema cannot hold more.
--
-- One row per Ko-fi message id, a DAY, and nothing else: no amount, no name, no
-- email, no message. That is a property of the table rather than a promise
-- about the endpoint — asked for a total, this worker could not produce one.
--
-- IDEMPOTENT ON THE MESSAGE ID, because Ko-fi retries a delivery it did not see
-- acknowledged and a retry must not be a second supporter.
CREATE TABLE IF NOT EXISTS supporters (
  message_id TEXT PRIMARY KEY,
  at         TEXT NOT NULL
);
