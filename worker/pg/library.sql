-- THE LIBRARY (database `wfsim`; docs/BOARD.md §"The fact factory"): every table
-- besides the fact factory's, which are in schema.sql, in the same database.
-- The library is the one thing here that cannot be regenerated — the boards are
-- derived from it, the site is generated, the code is in git — so it is kept
-- where it is easiest to inspect, count and dump.
--
-- Every text column is COLLATE "C": it compares byte by byte, whatever the
-- database's locale. Every whole number is BIGINT. Applied by the role that
-- owns the tables:
--   sudo -u postgres psql -d wfsim -v ON_ERROR_STOP=1 -c 'SET ROLE wfsim' -f library.sql

-- WHERE A SUBMISSION LANDS, AND THE ONLY DOOR THERE IS. The endpoint has no game
-- data, so it cannot say what a build IS: telling two builds apart needs the mod
-- POOL — an elemental card enters the element sequence and a plain one does not.
-- A key derived without it would be a second answer to a question that must
-- have one. So the door stores the record VERBATIM, and `wfsim-intake` — which
-- has the engine — canonicalises it, resolves a riven's numbers and writes the
-- builds that come out; the row is deleted the moment they land.
--
-- A QUEUE, NOT A LIBRARY: `id` is random and exists so one row can be deleted;
-- two submissions of one build are two rows here and one in `builds`. Nothing
-- reads this table but intake.
CREATE TABLE IF NOT EXISTS inbox (
  id     TEXT COLLATE "C" PRIMARY KEY,
  at     TEXT COLLATE "C" NOT NULL,
  record TEXT COLLATE "C" NOT NULL
);
-- WHO ASKED (docs/BOARD.md §"Who asked"): 'user:<code>' signed in, 'anonymous'
-- signed out, NULL for the bot's own; carried to `queue` by intake.
ALTER TABLE inbox ADD COLUMN IF NOT EXISTS initiator TEXT COLLATE "C";

-- ONE ROW PER BUILD, keyed by a HASH of the canonical build
-- `engine::board::builds::identity` states — its riven's ROLLS included — hashed
-- by `wfsim-intake`. DERIVED AND NOT ALLOCATED, so the same build always keys
-- the same: nothing looks the row up before writing it.
--
-- The RECORD is kept whole as json rather than exploded into columns: a build's
-- axes are declared once (`engine::board::builds::BUILD_AXES`), and a schema
-- that spelled them out would be one more place to forget one. Anything worth
-- indexing gets a generated column instead, `weapon` the first, because "how
-- well covered is this weapon" is the question the board is run on.
CREATE TABLE IF NOT EXISTS builds (
  id     TEXT COLLATE "C" PRIMARY KEY,
  -- The submission DAY, and nothing finer. The store records nothing about
  -- submitters — no IP, no token, no timestamp that could order one person's
  -- submissions against another's — and the schema states that promise too.
  at     TEXT COLLATE "C" NOT NULL,
  record TEXT COLLATE "C" NOT NULL,
  weapon TEXT COLLATE "C" GENERATED ALWAYS AS ((record::jsonb ->> 'weapon')) STORED
);
CREATE INDEX IF NOT EXISTS builds_weapon ON builds (weapon);
CREATE INDEX IF NOT EXISTS builds_at ON builds (at);

-- A SCORE IS A FACT, NOT A STEP IN A PIPELINE (docs/BOARD.md).
--
-- `(build, ruler, mode) -> score` is true for ever once computed, so it is
-- written down the moment it is computed rather than when a batch finishes.
-- Neither the clock nor a thousand commits of distance is evidence that a
-- measurement was wrong; the only thing that retires one is a person deleting
-- the row. `scripts/ship_facts.sh` writes it beside the running scorer;
-- `scripts/fetch_facts.sh` reads them back out. This table is the only source
-- the publisher has.
CREATE TABLE IF NOT EXISTS scores (
  -- THE BUILD, BY ITS `builds.id`. The name is the one the column was created
  -- with and the wire keeps the names it has; what it holds is the id.
  identity     TEXT COLLATE "C" NOT NULL,
  ruler        TEXT COLLATE "C" NOT NULL,
  -- A ROW IS (build, ruler, MODE). A mode is a property of the WEAPON — every
  -- melee carries seven and the Ballistica Prime four — and the cards that win
  -- one do not win another, so each is an independent ranking.
  mode         TEXT COLLATE "C" NOT NULL,
  -- WHICH BUILD MEASURED IT, AND IT DECIDES NOTHING. An older engine does not
  -- make a score wrong — the two are a REFERENCE relation, not a validity one.
  -- This is FORENSICS: it says which rows a build wrote, once that build is
  -- found to have been broken.
  measured_by  TEXT COLLATE "C" NOT NULL,
  score        DOUBLE PRECISION NOT NULL,
  -- WHAT `score` IS IN — the ruler's CORE metric, an id from
  -- `engine::metrics::ALL`, never a label. A test has one core, and that one
  -- ranks; another metric read off the same fight belongs in a column of its
  -- own beside it, never in a row of its own, or one key holds two answers.
  -- It decides nothing either: it lets a row outlive the ruler's terms it was
  -- measured under and still be read without checking out that commit.
  metric       TEXT COLLATE "C" NOT NULL,
  -- WHAT THE ROW COST, so the bill is read off the rows rather than estimated,
  -- and the next run packs its shards by work rather than by count. NOT
  -- derivable from the two clocks below: a row paid for in sittings spans a wall
  -- clock much longer than the fight it contains.
  cost_seconds DOUBLE PRECISION NOT NULL,
  -- WHEN THE FIGHT STARTED AND WHEN IT ENDED. Provenance, and nothing branches
  -- on either: they show how old a ROW is, and say afterwards which rows a bad
  -- build wrote. A row from before they existed carries the same value in
  -- both, which is how "we do not know when this started" is spelled; deriving
  -- a start by subtracting the cost would invent precision it never had.
  started_at   TEXT COLLATE "C" NOT NULL,
  finished_at  TEXT COLLATE "C" NOT NULL,
  -- ONE FACT PER ROW: the last measurement of it. Nothing here says whether a
  -- number is right; only another measurement can, and a person deleting the
  -- row is what asks for one.
  PRIMARY KEY (identity, ruler, mode)
);
-- "What has this ruler measured" is one indexed query, which is what the set
-- difference is asked through.
CREATE INDEX IF NOT EXISTS scores_ruler ON scores (ruler);
-- …and "what has gone longest without being measured" is the order to repair
-- in, which is the one thing a clock here is allowed to decide.
CREATE INDEX IF NOT EXISTS scores_oldest ON scores (finished_at);

-- WHAT SOMEBODY ASKED TO BE COMPUTED, AND HOW FAR IT GOT. The queue may only
-- ever CAUSE work: it may not prevent work and it may not decide a number, so
-- losing it costs an ordering and never a fact, and the publisher never reads
-- it. A row is deleted when its score lands, the score written first.
--
-- NOT A COLUMN ON `scores`: that table is what the site is built from, and a
-- `pending` flag on it would make every reader know which rows are real.
CREATE TABLE IF NOT EXISTS batches (
  id    TEXT COLLATE "C" PRIMARY KEY,
  -- THE ORDER, AND THE ONLY THING THAT SETS IT. Sorted as text, so a batch
  -- jumps the line by being renamed rather than by a priority nobody can see.
  at    TEXT COLLATE "C" NOT NULL,
  -- WHAT THIS GROUP IS FOR, for the person who finds it a week later. A batch
  -- with no reason is a batch nobody can decide to cancel.
  why   TEXT COLLATE "C",
  -- HOW MANY ROWS IT WAS EVER ASKED FOR WITH — a statement about the PAST, so
  -- it cannot drift: progress is this minus what is still in `queue`. A second
  -- pass into one batch ADDS to it.
  total BIGINT NOT NULL
);

-- ONE ROW PER (batch, build, ruler, mode). The same row may sit in two batches
-- at once — two people asking for one thing — and computing it deletes it from
-- both, which is why the delete does not name a batch.
CREATE TABLE IF NOT EXISTS queue (
  batch    TEXT COLLATE "C" NOT NULL,
  build_id TEXT COLLATE "C" NOT NULL,
  ruler    TEXT COLLATE "C" NOT NULL,
  mode     TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY (batch, build_id, ruler, mode)
);
-- WHO ASKED for an arrival's row (`inbox.initiator`, the first one intake saw),
-- read when it is asked of the factory; gone with the row once it is computed.
ALTER TABLE queue ADD COLUMN IF NOT EXISTS initiator TEXT COLLATE "C";
-- "WHAT IS LEFT, IN ORDER" is one indexed read.
CREATE INDEX IF NOT EXISTS queue_batch ON queue (batch);
-- "IS THIS ROW STILL OWED" (worker/board_tasks.js `owedSql`).
CREATE INDEX IF NOT EXISTS queue_row ON queue (build_id, ruler, mode);

-- THE BOARD'S ORDERS BEFORE THE FACTORY, read by `/api/board/adopt` alone
-- (worker/board_tasks.js `adopt`): one owed row each. `state` is where its
-- lease left it, or `settled` once adopted; `engine` the first result's
-- `ENGINE_ID`; `clients` every client that measured it, comma-separated in the
-- order their results came, with `clients_compute_ms` (each one's fight time)
-- and `clients_nets` (each one's salted network hash, verify.js `netOf`) in the
-- same order; `work` the first result's `Shard::work`; `priority` 0 for a new
-- build's row and 1 for a rescore; `carried_from` the engine a result was
-- measured by when a release carried it to the next.
CREATE TABLE IF NOT EXISTS orders (
  identity           TEXT COLLATE "C" NOT NULL,
  ruler              TEXT COLLATE "C" NOT NULL,
  mode               TEXT COLLATE "C" NOT NULL,
  record             TEXT COLLATE "C" NOT NULL,
  state              TEXT COLLATE "C" NOT NULL,
  engine             TEXT COLLATE "C" NOT NULL DEFAULT '',
  slot               BIGINT NOT NULL,
  lease              TEXT COLLATE "C",
  lease_until        TEXT COLLATE "C",
  leased_to          TEXT COLLATE "C",
  score              DOUBLE PRECISION,
  metric             TEXT COLLATE "C",
  produced_by        TEXT COLLATE "C",
  verifier           TEXT COLLATE "C",
  disputed           DOUBLE PRECISION,
  at                 TEXT COLLATE "C" NOT NULL,
  clients            TEXT COLLATE "C" NOT NULL DEFAULT '',
  clients_compute_ms TEXT COLLATE "C" NOT NULL DEFAULT '',
  work               BIGINT,
  priority           BIGINT NOT NULL DEFAULT 1,
  carried_from       TEXT COLLATE "C",
  clients_nets       TEXT COLLATE "C" NOT NULL DEFAULT '',
  PRIMARY KEY (identity, ruler, mode)
);
CREATE INDEX IF NOT EXISTS orders_pick ON orders (state, engine, priority, slot);

-- SHORT SHARE LINKS: `/weapons/<weapon>/s/<id>` names a stored share code
-- (worker/index.js §"SHORT SHARE LINKS"). `id` is a hash of (weapon, code,
-- claim) computed by the worker (`shareId`), so a row never changes, the same
-- build is one row, and no client chooses an id. `at` is the day. `claim` is
-- the sharer's measurement when they chose to include one (JSON, validated by
-- `shareClaim`); NULL otherwise.
CREATE TABLE IF NOT EXISTS shares (
  id     TEXT COLLATE "C" PRIMARY KEY,
  weapon TEXT COLLATE "C" NOT NULL,
  code   TEXT COLLATE "C" NOT NULL,
  at     TEXT COLLATE "C" NOT NULL,
  claim  TEXT COLLATE "C"
);

-- A FINISHED DAY'S USAGE TOTALS, kept past the Analytics Engine's three months
-- by the worker's daily cron (worker/usage_days.js, docs/ANALYTICS.md
-- §Retention). Totals only, never a visitor id. `event` is a usage event, or
-- `visitors` (subject `all`, `result`, `returning`) or `visitors.country`;
-- `market` is `china` or `overseas`; `sampled` is 1 when the dataset sampled
-- that day, so `visitors` is a floor.
CREATE TABLE IF NOT EXISTS usage_days (
  day      TEXT COLLATE "C" NOT NULL,
  event    TEXT COLLATE "C" NOT NULL,
  subject  TEXT COLLATE "C" NOT NULL,
  market   TEXT COLLATE "C" NOT NULL,
  visitors BIGINT NOT NULL,
  points   BIGINT NOT NULL,
  sampled  BIGINT NOT NULL,
  PRIMARY KEY (day, event, subject, market)
);
CREATE INDEX IF NOT EXISTS usage_days_by_event ON usage_days (event, subject, day);

-- EVERY USAGE POINT, UNSAMPLED (worker/usage_log.js, docs/ANALYTICS.md §"The
-- log"): the source of every visitor count. `visitor` is the SHA-256 of the
-- browser's id, never the id itself, so a row is found only by the browser that
-- holds it — which is how `/api/e/forget` deletes them. Kept 90 days, then only
-- `usage_days` remains. No pipeline role is granted it: the public backup
-- cannot read it.
CREATE TABLE IF NOT EXISTS usage_points (
  at      TEXT COLLATE "C" NOT NULL,
  day     TEXT COLLATE "C" NOT NULL,
  visitor TEXT COLLATE "C" NOT NULL,
  event   TEXT COLLATE "C" NOT NULL,
  subject TEXT COLLATE "C" NOT NULL,
  route   TEXT COLLATE "C" NOT NULL,
  lang    TEXT COLLATE "C" NOT NULL,
  shell   TEXT COLLATE "C" NOT NULL,
  release TEXT COLLATE "C" NOT NULL,
  country TEXT COLLATE "C" NOT NULL,
  n       DOUBLE PRECISION NOT NULL
);
CREATE INDEX IF NOT EXISTS usage_points_by_day ON usage_points (day);
CREATE INDEX IF NOT EXISTS usage_points_by_visitor ON usage_points (visitor);

-- WHAT A CHAT SENT THE BOT, until the bot server has answered it (worker/qq.js,
-- docs/AGENT.md §"The QQ bot"). `id` is the platform's own message id, so a
-- retried callback lands on the row it already wrote; `body` is the event as
-- the platform sent it. The server claims a row, answers from its whitelisted
-- address and marks it done; a row is deleted a day after it arrived, answered
-- or not.
CREATE TABLE IF NOT EXISTS bot_inbox (
  id         TEXT COLLATE "C" PRIMARY KEY,
  channel    TEXT COLLATE "C" NOT NULL,
  kind       TEXT COLLATE "C" NOT NULL,
  body       TEXT COLLATE "C" NOT NULL,
  at         TEXT COLLATE "C" NOT NULL,
  claimed_at TEXT COLLATE "C",
  done_at    TEXT COLLATE "C"
);

-- RIVEN APPRAISAL (worker/appraise.js, docs/AGENT.md §"Riven appraisal"). One
-- row per appraisal a chat's bot opened; `chat` is that channel's own record of
-- where to answer and nothing else reads it. `winner` is the first handed-back
-- build the bot accepted. Both tables are emptied a day after a row arrived.
CREATE TABLE IF NOT EXISTS appraisals (
  code         TEXT COLLATE "C" PRIMARY KEY,
  channel      TEXT COLLATE "C" NOT NULL,
  chat         TEXT COLLATE "C" NOT NULL,
  asker        TEXT COLLATE "C" NOT NULL,
  room         TEXT COLLATE "C" NOT NULL,
  weapon       TEXT COLLATE "C" NOT NULL,
  ruler        TEXT COLLATE "C" NOT NULL,
  riven        TEXT COLLATE "C" NOT NULL,
  at           TEXT COLLATE "C" NOT NULL,
  winner       BIGINT,
  done_at      TEXT COLLATE "C",
  told_at      TEXT COLLATE "C",
  -- AS VOLUNTEER WORK (worker/appraise.js §"Volunteer work"): the search frozen
  -- once by the bot, the engine it is for, the lease of the computer running it,
  -- and when two owners' computers agreed on its answer.
  request      TEXT COLLATE "C",
  engine       TEXT COLLATE "C",
  lease        TEXT COLLATE "C",
  lease_until  TEXT COLLATE "C",
  leased_to    TEXT COLLATE "C",
  agreed_at    TEXT COLLATE "C",
  -- WHEN A COMPUTER FIRST TOOK IT, and when the chat was told one had.
  started_at   TEXT COLLATE "C",
  started_told TEXT COLLATE "C"
);
CREATE INDEX IF NOT EXISTS appraisals_by_asker ON appraisals (channel, asker, at);
CREATE INDEX IF NOT EXISTS appraisals_by_channel ON appraisals (channel, at);

CREATE TABLE IF NOT EXISTS appraisal_results (
  id         BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  code       TEXT COLLATE "C" NOT NULL,
  build      TEXT COLLATE "C" NOT NULL,
  thanks     TEXT COLLATE "C" NOT NULL DEFAULT '',
  at         TEXT COLLATE "C" NOT NULL,
  claimed_at TEXT COLLATE "C",
  checked_at TEXT COLLATE "C",
  verdict    TEXT COLLATE "C",
  -- A VOLUNTEER'S ANSWER: whose computer, its winner's score, the search's work,
  -- the build's canonical text, which two answers must share to agree, and the
  -- salted hash of the network it came from (verify.js `netOf`).
  verifier   TEXT COLLATE "C",
  score      DOUBLE PRECISION,
  work       BIGINT,
  key        TEXT COLLATE "C",
  net        TEXT COLLATE "C" NOT NULL DEFAULT '',
  -- THE ENGINE IT RAN ON, which the cap on answers counts by.
  engine     TEXT COLLATE "C"
);
CREATE INDEX IF NOT EXISTS appraisal_results_by_code ON appraisal_results (code);
-- ONE COMPUTER'S RESULTS IN TIME ORDER (worker/appraise.js `/api/appraise/mine`).
CREATE INDEX IF NOT EXISTS appraisal_results_by_verifier ON appraisal_results (verifier, at);

-- EVERY VOID FISSURE THE GAME OPENS, from the world state the bot server relays
-- each minute (worker/world.js `fissureLog`). `id` is DE's own, so the minutes
-- that see one again write nothing. `list` is normal, steel_path or railjack;
-- `mission` is NULL for a Void Storm. Which combinations a reminder may hold,
-- and how often each comes, is read from here; a node's planet is not stored,
-- it is the node's own (worker/world_names.json).
CREATE TABLE IF NOT EXISTS fissures (
  id         TEXT COLLATE "C" PRIMARY KEY,
  list       TEXT COLLATE "C" NOT NULL,
  tier       TEXT COLLATE "C" NOT NULL,
  mission    TEXT COLLATE "C",
  node       TEXT COLLATE "C" NOT NULL,
  started_at TEXT COLLATE "C" NOT NULL,
  ends_at    TEXT COLLATE "C" NOT NULL
);

-- THE OWNER'S DONATION LEDGER, kept by hand and read by nothing in this
-- repository.
CREATE TABLE IF NOT EXISTS donors (
  id           BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  display_name TEXT COLLATE "C",
  note         TEXT COLLATE "C"
);
CREATE TABLE IF NOT EXISTS rates (
  currency     TEXT COLLATE "C" PRIMARY KEY,
  cny_per_unit DOUBLE PRECISION NOT NULL,
  updated_at   TEXT COLLATE "C" NOT NULL,
  source       TEXT COLLATE "C" NOT NULL
);
-- CNY IS THE UNIT THE BOOKS ARE IN, so its rate is 1 and stays 1.
INSERT INTO rates (currency, cny_per_unit, updated_at, source)
VALUES ('CNY', 1.0, '2026-09-09T16:00:00.000Z', 'the accounting currency, by definition')
ON CONFLICT (currency) DO NOTHING;
CREATE TABLE IF NOT EXISTS donations (
  id           BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  paid_at      TEXT COLLATE "C" NOT NULL CHECK (paid_at LIKE '____-__-__T__:__:__.___Z'),
  channel      TEXT COLLATE "C" NOT NULL,
  account      TEXT COLLATE "C" NOT NULL,
  donor_id     BIGINT NOT NULL REFERENCES donors (id),
  currency     TEXT COLLATE "C" NOT NULL REFERENCES rates (currency),
  gross_amount DOUBLE PRECISION,
  net_amount   DOUBLE PRECISION NOT NULL,
  UNIQUE (channel, paid_at, account)
);
CREATE INDEX IF NOT EXISTS donations_donor_id ON donations (donor_id);
CREATE INDEX IF NOT EXISTS donations_paid_at ON donations (paid_at);
