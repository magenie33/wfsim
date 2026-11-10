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
CREATE TABLE IF NOT EXISTS record_build_submissions (
  id     TEXT COLLATE "C" PRIMARY KEY,
  at     TEXT COLLATE "C" NOT NULL,
  record TEXT COLLATE "C" NOT NULL
);
-- WHO ASKED (docs/BOARD.md §"Who asked"): 'user:<code>' signed in, 'anonymous'
-- signed out, NULL for the bot's own; carried to `queue` by intake.
ALTER TABLE record_build_submissions ADD COLUMN IF NOT EXISTS initiator TEXT COLLATE "C";
-- Trusted derivation provenance, outside the build and never accepted from a client.
ALTER TABLE record_build_submissions ADD COLUMN IF NOT EXISTS family_source TEXT COLLATE "C";

-- Arrivals alone create transfers. Retaining their keys prevents a later
-- resubmission from repeating a completed sibling search.
CREATE TABLE IF NOT EXISTS entity_family_search_requests (
  id TEXT COLLATE "C" PRIMARY KEY,
  source TEXT COLLATE "C" NOT NULL,
  weapon TEXT COLLATE "C" NOT NULL,
  ruler TEXT COLLATE "C" NOT NULL,
  request TEXT COLLATE "C" NOT NULL,
  context TEXT COLLATE "C" NOT NULL,
  engine TEXT COLLATE "C" NOT NULL,
  asked_engine TEXT COLLATE "C",
  state TEXT COLLATE "C" NOT NULL,
  created_at TEXT COLLATE "C" NOT NULL,
  completed_at TEXT COLLATE "C"
);
CREATE INDEX IF NOT EXISTS family_transfers_waiting ON entity_family_search_requests (created_at, id) WHERE state = 'waiting';

-- ONE ROW PER BUILD, keyed by a HASH of the canonical build
-- `engine::board::entity_weapon_builds::identity` states — its riven's ROLLS included — hashed
-- by `wfsim-intake`. DERIVED AND NOT ALLOCATED, so the same build always keys
-- the same: nothing looks the row up before writing it.
--
-- The RECORD is kept whole as json rather than exploded into columns: a build's
-- axes are declared once (`engine::board::entity_weapon_builds::BUILD_AXES`), and a schema
-- that spelled them out would be one more place to forget one. Anything worth
-- indexing gets a generated column instead, `weapon` the first, because "how
-- well covered is this weapon" is the question the board is run on.
CREATE TABLE IF NOT EXISTS entity_weapon_builds (
  id     TEXT COLLATE "C" PRIMARY KEY,
  -- The submission DAY, and nothing finer. The store records nothing about
  -- submitters — no IP, no token, no timestamp that could order one person's
  -- submissions against another's — and the schema states that promise too.
  at     TEXT COLLATE "C" NOT NULL,
  record TEXT COLLATE "C" NOT NULL,
  weapon TEXT COLLATE "C" GENERATED ALWAYS AS ((record::jsonb ->> 'weapon')) STORED
);
CREATE INDEX IF NOT EXISTS builds_weapon ON entity_weapon_builds (weapon);
CREATE INDEX IF NOT EXISTS builds_at ON entity_weapon_builds (at);

-- Warframe and companion builds share intake and canonical ids, without weapon scores.
CREATE TABLE IF NOT EXISTS record_equipment_submissions (
  id TEXT COLLATE "C" PRIMARY KEY,
  at TEXT COLLATE "C" NOT NULL,
  record TEXT COLLATE "C" NOT NULL
);
CREATE TABLE IF NOT EXISTS entity_equipment_builds (
  id TEXT COLLATE "C" PRIMARY KEY,
  at TEXT COLLATE "C" NOT NULL,
  record TEXT COLLATE "C" NOT NULL,
  kind TEXT COLLATE "C" GENERATED ALWAYS AS ((record::jsonb ->> 'kind')) STORED,
  equipment TEXT COLLATE "C" GENERATED ALWAYS AS ((record::jsonb ->> 'equipment')) STORED
);
CREATE INDEX IF NOT EXISTS equipment_builds_scope ON entity_equipment_builds (kind, equipment, id);

-- A SCORE IS A FACT, NOT A STEP IN A PIPELINE (docs/BOARD.md).
--
-- `(build, ruler, mode) -> score` is true for ever once computed, so it is
-- written down the moment it is computed rather than when a batch finishes.
-- Neither the clock nor a thousand commits of distance is evidence that a
-- measurement was wrong. Explicit corrections and retracted proofs retire
-- their captured baseline. `scripts/ship_facts.sh` writes measured rows;
-- `scripts/fetch_facts.sh` reads them back out. This table is the only source
-- the publisher has.
CREATE TABLE IF NOT EXISTS state_board_measurement_selection (
  -- THE BUILD, BY ITS `entity_weapon_builds.id`. The name is the one the column was created
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
CREATE INDEX IF NOT EXISTS scores_ruler ON state_board_measurement_selection (ruler);
-- THE TIME ORDER AND ROLLING PACE share one leading timestamp; the included
-- provenance lets the pace read who measured a score without its heap row.
CREATE INDEX IF NOT EXISTS scores_pace ON state_board_measurement_selection (finished_at) INCLUDE (measured_by);
DROP INDEX IF EXISTS scores_oldest;

-- One current measurement's witnesses; a newer score clock makes older attribution unavailable.
CREATE TABLE IF NOT EXISTS relation_measurement_witnesses (
  identity TEXT COLLATE "C" NOT NULL,
  ruler TEXT COLLATE "C" NOT NULL,
  mode TEXT COLLATE "C" NOT NULL,
  measured_at TEXT COLLATE "C" NOT NULL,
  engine TEXT COLLATE "C" NOT NULL,
  owner TEXT COLLATE "C" NOT NULL,
  device TEXT COLLATE "C" NOT NULL,
  official BIGINT NOT NULL,
  completed_at TEXT COLLATE "C" NOT NULL,
  credited_at TEXT COLLATE "C" NOT NULL,
  position BIGINT NOT NULL,
  PRIMARY KEY (identity, ruler, mode, device),
  FOREIGN KEY (identity, ruler, mode) REFERENCES state_board_measurement_selection(identity, ruler, mode) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS score_witnesses_owner ON relation_measurement_witnesses (owner, measured_at DESC, identity, ruler, mode);
CREATE INDEX IF NOT EXISTS score_witnesses_device ON relation_measurement_witnesses (device, measured_at DESC, identity, ruler, mode);
CREATE OR REPLACE FUNCTION score_witnesses_replaced() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  DELETE FROM relation_measurement_witnesses WHERE identity=OLD.identity AND ruler=OLD.ruler AND mode=OLD.mode;
  RETURN NEW;
END $$;
DROP TRIGGER IF EXISTS scores_replace_witnesses ON state_board_measurement_selection;
CREATE TRIGGER scores_replace_witnesses AFTER UPDATE OF finished_at ON state_board_measurement_selection
  FOR EACH ROW WHEN (OLD.finished_at IS DISTINCT FROM NEW.finished_at) EXECUTE FUNCTION score_witnesses_replaced();

-- WHAT SOMEBODY ASKED TO BE COMPUTED, AND HOW FAR IT GOT. The queue may only
-- ever CAUSE work: it may not prevent work and it may not decide a number, so
-- losing it costs an ordering and never a fact, and the publisher never reads
-- it. A row is deleted when its score lands, the score written first.
--
-- NOT A COLUMN ON `scores`: that table is what the site is built from, and a
-- `pending` flag on it would make every reader know which rows are real.
CREATE TABLE IF NOT EXISTS entity_compute_batches (
  id    TEXT COLLATE "C" PRIMARY KEY,
  -- THE ORDER, AND THE ONLY THING THAT SETS IT. Sorted as text, so a batch
  -- jumps the line by being renamed rather than by a priority nobody can see.
  at    TEXT COLLATE "C" NOT NULL,
  -- WHAT THIS GROUP IS FOR, for the person who finds it a week later. A batch
  -- with no reason is a batch nobody can decide to cancel.
  why   TEXT COLLATE "C",
  -- Historical admitted work. Pipeline retries add only newly queued
  -- calculations; progress is this minus the batch's remaining queue rows.
  total BIGINT NOT NULL
);

-- One pending calculation per (build, ruler, mode), owned by its first batch.
-- Later batches share it; completion removes it before another round can enter.
CREATE TABLE IF NOT EXISTS state_board_pending_work (
  batch    TEXT COLLATE "C" NOT NULL,
  build_id TEXT COLLATE "C" NOT NULL,
  ruler    TEXT COLLATE "C" NOT NULL,
  mode     TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY (batch, build_id, ruler, mode)
);
-- WHO ASKED for an arrival's row (`record_build_submissions.initiator`, the first one intake saw),
-- read when it is asked of the factory; gone with the row once it is computed.
ALTER TABLE state_board_pending_work ADD COLUMN IF NOT EXISTS initiator TEXT COLLATE "C";
-- "WHAT IS LEFT, IN ORDER" is one indexed read.
CREATE INDEX IF NOT EXISTS queue_batch ON state_board_pending_work (batch);
-- "IS THIS ROW STILL OWED" (worker/board_tasks.js `owedSql`).
CREATE UNIQUE INDEX IF NOT EXISTS queue_calculation ON state_board_pending_work (build_id, ruler, mode);

-- Repeated requests share the pending row and preserve its original batch.
CREATE OR REPLACE FUNCTION queue_once() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE held state_board_pending_work%ROWTYPE;
BEGIN
  PERFORM pg_advisory_xact_lock(hashtextextended(NEW.build_id || '|' || NEW.ruler || '|' || NEW.mode, 1));
  SELECT * INTO held FROM state_board_pending_work WHERE build_id=NEW.build_id AND ruler=NEW.ruler AND mode=NEW.mode ORDER BY batch LIMIT 1;
  IF FOUND THEN
    IF held.initiator IS NULL AND NEW.initiator IS NOT NULL THEN
      UPDATE state_board_pending_work SET initiator=NEW.initiator WHERE batch=held.batch AND build_id=held.build_id AND ruler=held.ruler AND mode=held.mode;
    END IF;
    RETURN NULL;
  END IF;
  RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS queue_once ON state_board_pending_work;
CREATE TRIGGER queue_once BEFORE INSERT ON state_board_pending_work FOR EACH ROW EXECUTE FUNCTION queue_once();

-- THE BOARD'S ORDERS BEFORE THE FACTORY, read by `/api/board/adopt` alone
-- (worker/board_tasks.js `adopt`): one owed row each. `state` is where its
-- lease left it, or `settled` once adopted; `engine` the first result's
-- `ENGINE_ID`; `clients` every client that measured it, comma-separated in the
-- order their results came, with `clients_compute_ms` (each one's fight time)
-- and `clients_nets` (each one's salted network hash, verify.js `netOf`) in the
-- same order; `work` the first result's `Shard::work`; `priority` 0 for a new
-- build's row and 1 for a rescore; `carried_from` the engine a result was
-- measured by when a release carried it to the next.
CREATE TABLE IF NOT EXISTS entity_compute_orders (
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
CREATE INDEX IF NOT EXISTS orders_pick ON entity_compute_orders (state, engine, priority, slot);

-- SHORT SHARE LINKS: `/weapons/<weapon>/s/<id>` names a stored share code
-- (worker/index.js §"SHORT SHARE LINKS"). `id` is a hash of (weapon, code,
-- claim) computed by the worker (`shareId`), so a row never changes, the same
-- build is one row, and no client chooses an id. `at` is the day. `claim` is
-- the sharer's measurement when they chose to include one (JSON, validated by
-- `shareClaim`); NULL otherwise.
CREATE TABLE IF NOT EXISTS entity_short_shares (
  id     TEXT COLLATE "C" PRIMARY KEY,
  weapon TEXT COLLATE "C" NOT NULL,
  code   TEXT COLLATE "C" NOT NULL,
  at     TEXT COLLATE "C" NOT NULL,
  claim  TEXT COLLATE "C"
);

-- A FINISHED DAY'S USAGE TOTALS, kept past the usage log's ninety days
-- by the worker's daily cron (worker/usage_days.js, docs/ANALYTICS.md
-- §Retention). Totals only, never a visitor id. `event` is a usage event, or
-- `visitors` (subject `all`, `result`, `returning`) or `visitors.country`;
-- `market` is `china`, `overseas` or `unknown`; `sampled` is 1 when the dataset sampled
-- that day, so `visitors` is a floor.
CREATE TABLE IF NOT EXISTS record_daily_usage_summaries (
  day      TEXT COLLATE "C" NOT NULL,
  event    TEXT COLLATE "C" NOT NULL,
  subject  TEXT COLLATE "C" NOT NULL,
  market   TEXT COLLATE "C" NOT NULL,
  visitors BIGINT NOT NULL,
  points   BIGINT NOT NULL,
  sampled  BIGINT NOT NULL,
  PRIMARY KEY (day, event, subject, market)
);
CREATE INDEX IF NOT EXISTS usage_days_by_event ON record_daily_usage_summaries (event, subject, day);

-- EVERY USAGE POINT, UNSAMPLED (worker/usage_log.js, docs/ANALYTICS.md §"The
-- log"): the source of every visitor count. `visitor` is the SHA-256 of the
-- browser's id, never the id itself, so a row is found only by the browser that
-- holds it — which is how `/api/e/forget` deletes them. Kept 90 days, then only
-- `usage_days` remains. No pipeline role is granted it: the public backup
-- cannot read it.
CREATE TABLE IF NOT EXISTS record_usage_events (
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
CREATE INDEX IF NOT EXISTS usage_points_by_day ON record_usage_events (day);
CREATE INDEX IF NOT EXISTS usage_points_by_visitor ON record_usage_events (visitor);

-- WHAT A CHAT SENT THE BOT, until the bot server has answered it (worker/qq.js,
-- docs/AGENT.md §"The QQ bot"). `id` is the platform's own message id, so a
-- retried callback lands on the row it already wrote; `body` is the event as
-- the platform sent it. The server claims a row, answers from its whitelisted
-- address and marks it done; a row is deleted a day after it arrived, answered
-- or not.
CREATE TABLE IF NOT EXISTS record_bot_messages (
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
CREATE TABLE IF NOT EXISTS entity_appraisal_requests (
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
CREATE INDEX IF NOT EXISTS appraisals_by_asker ON entity_appraisal_requests (channel, asker, at);
CREATE INDEX IF NOT EXISTS appraisals_by_channel ON entity_appraisal_requests (channel, at);

CREATE TABLE IF NOT EXISTS record_appraisal_candidates (
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
  -- THE ENGINE IT RAN ON, which the cap on record_candidate_answers counts by.
  engine     TEXT COLLATE "C"
);
CREATE INDEX IF NOT EXISTS appraisal_results_by_code ON record_appraisal_candidates (code);
-- ONE COMPUTER'S RESULTS IN TIME ORDER (worker/appraise.js `/api/appraise/mine`).
CREATE INDEX IF NOT EXISTS appraisal_results_by_verifier ON record_appraisal_candidates (verifier, at);
ALTER TABLE record_appraisal_candidates ADD COLUMN IF NOT EXISTS source TEXT COLLATE "C";

-- EVERY VOID FISSURE THE GAME OPENS, from the world state the bot server relays
-- each minute (worker/world.js `fissureLog`). `id` is DE's own, so the minutes
-- that see one again write nothing. `list` is normal, steel_path or railjack;
-- `mission` is NULL for a Void Storm. Which combinations a reminder may hold,
-- and how often each comes, is read from here; a node's planet is not stored,
-- it is the node's own (worker/world_names.json).
CREATE TABLE IF NOT EXISTS record_world_fissures (
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
CREATE TABLE IF NOT EXISTS entity_donors (
  id           BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  display_name TEXT COLLATE "C",
  note         TEXT COLLATE "C"
);
CREATE TABLE IF NOT EXISTS state_exchange_rate_quotes (
  currency     TEXT COLLATE "C" PRIMARY KEY,
  cny_per_unit DOUBLE PRECISION NOT NULL,
  updated_at   TEXT COLLATE "C" NOT NULL,
  source       TEXT COLLATE "C" NOT NULL
);
-- CNY IS THE UNIT THE BOOKS ARE IN, so its rate is 1 and stays 1.
INSERT INTO state_exchange_rate_quotes (currency, cny_per_unit, updated_at, source)
VALUES ('CNY', 1.0, '2026-09-09T16:00:00.000Z', 'the accounting currency, by definition')
ON CONFLICT (currency) DO NOTHING;
CREATE TABLE IF NOT EXISTS record_donation_payments (
  id           BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  paid_at      TEXT COLLATE "C" NOT NULL CHECK (paid_at LIKE '____-__-__T__:__:__.___Z'),
  channel      TEXT COLLATE "C" NOT NULL,
  account      TEXT COLLATE "C" NOT NULL,
  donor_id     BIGINT NOT NULL REFERENCES entity_donors (id),
  currency     TEXT COLLATE "C" NOT NULL REFERENCES state_exchange_rate_quotes (currency),
  gross_amount DOUBLE PRECISION,
  net_amount   DOUBLE PRECISION NOT NULL,
  UNIQUE (channel, paid_at, account)
);
CREATE INDEX IF NOT EXISTS donations_donor_id ON record_donation_payments (donor_id);
CREATE INDEX IF NOT EXISTS donations_paid_at ON record_donation_payments (paid_at);

-- One independently verified review per published row; the submitted score is never input.
CREATE TABLE IF NOT EXISTS entity_measurement_reviews (
  ref TEXT COLLATE "C" NOT NULL,
  id TEXT COLLATE "C" PRIMARY KEY,
  round_at TEXT COLLATE "C" NOT NULL,
  identity TEXT COLLATE "C" NOT NULL,
  ruler TEXT COLLATE "C" NOT NULL,
  mode TEXT COLLATE "C" NOT NULL,
  engine TEXT COLLATE "C" NOT NULL,
  question TEXT COLLATE "C" NOT NULL,
  record TEXT COLLATE "C" NOT NULL,
  state TEXT COLLATE "C" NOT NULL,
  baseline_score DOUBLE PRECISION NOT NULL,
  baseline_metric TEXT COLLATE "C" NOT NULL,
  baseline_cost_seconds DOUBLE PRECISION NOT NULL,
  baseline_review_id TEXT COLLATE "C",
  baseline_by TEXT COLLATE "C" NOT NULL,
  baseline_started_at TEXT COLLATE "C" NOT NULL,
  baseline_finished_at TEXT COLLATE "C" NOT NULL,
  created_at TEXT COLLATE "C" NOT NULL,
  completed_at TEXT COLLATE "C"
);
CREATE UNIQUE INDEX IF NOT EXISTS board_reviews_round ON entity_measurement_reviews (ref, engine, round_at);
CREATE UNIQUE INDEX IF NOT EXISTS board_reviews_pending ON entity_measurement_reviews (ref, engine) WHERE state = 'waiting';
CREATE INDEX IF NOT EXISTS board_reviews_waiting ON entity_measurement_reviews (engine, state, created_at);
CREATE INDEX IF NOT EXISTS board_reviews_question ON entity_measurement_reviews (question);
ALTER TABLE state_board_measurement_selection ADD COLUMN IF NOT EXISTS review_id TEXT COLLATE "C";
ALTER TABLE entity_measurement_reviews ADD COLUMN IF NOT EXISTS invalidated_at TEXT COLLATE "C";
ALTER TABLE entity_measurement_reviews ADD COLUMN IF NOT EXISTS invalidation_reason TEXT COLLATE "C";
CREATE INDEX IF NOT EXISTS scores_review ON state_board_measurement_selection (review_id) WHERE review_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS board_reviews_invalidated ON entity_measurement_reviews (invalidated_at) WHERE invalidated_at IS NOT NULL;

-- A retired measurement remains auditable and travels as a removal event.
CREATE OR REPLACE VIEW board_facts AS
SELECT s.identity,s.ruler,s.mode,s.measured_by,s.score,s.metric,s.cost_seconds,s.started_at,s.finished_at,s.review_id,
  CASE WHEN r.invalidated_at IS NOT NULL AND r.state <> 'succeeded' THEN r.invalidated_at END AS retired_at
FROM state_board_measurement_selection s LEFT JOIN entity_measurement_reviews r ON r.id = s.review_id;

-- Older daemons and native receipts cannot revive an explicitly retired row.
CREATE OR REPLACE FUNCTION protect_retired_board_score() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE held entity_measurement_reviews%ROWTYPE; replacement entity_measurement_reviews%ROWTYPE;
BEGIN
  IF OLD.review_id IS NULL THEN RETURN NEW; END IF;
  SELECT * INTO held FROM entity_measurement_reviews WHERE id=OLD.review_id;
  IF held.invalidated_at IS NULL OR held.state='succeeded' THEN RETURN NEW; END IF;
  IF NEW.review_id=held.id AND NEW.measured_by='verified:'||held.engine
     AND NEW.started_at=NEW.finished_at AND NEW.finished_at>held.invalidated_at
     AND OLD.score=held.baseline_score AND OLD.measured_by=held.baseline_by AND OLD.finished_at=held.baseline_finished_at
  THEN RETURN NEW; END IF;
  IF NEW.score=OLD.score AND NEW.measured_by=OLD.measured_by AND NEW.finished_at=OLD.finished_at
     AND NEW.started_at=OLD.started_at AND NEW.review_id IS DISTINCT FROM OLD.review_id THEN
    SELECT * INTO replacement FROM entity_measurement_reviews WHERE id=NEW.review_id;
    IF replacement.state='waiting' AND replacement.invalidated_at IS NOT NULL AND replacement.baseline_review_id=held.id
    THEN RETURN NEW; END IF;
  END IF;
  RETURN OLD;
END $$;
DROP TRIGGER IF EXISTS protect_retired_score ON state_board_measurement_selection;
CREATE TRIGGER protect_retired_score BEFORE UPDATE ON state_board_measurement_selection FOR EACH ROW EXECUTE FUNCTION protect_retired_board_score();

-- HOT TABLES RECYCLE SMALLER BATCHES: a fixed floor bounds the work on small
-- tables, while fractional thresholds keep dead versions and newly inserted
-- pages from waiting for a fifth of a large table before vacuuming.
DO $$
DECLARE relation text;
BEGIN
  FOREACH relation IN ARRAY ARRAY['state_compute_scheduling', 'record_candidate_answers', 'relation_task_consumers', 'state_board_measurement_selection', 'state_board_pending_work'] LOOP
    EXECUTE format('ALTER TABLE %I SET (
      autovacuum_vacuum_scale_factor = 0.02,
      autovacuum_vacuum_threshold = 50000,
      autovacuum_vacuum_insert_scale_factor = 0.01,
      autovacuum_vacuum_insert_threshold = 50000)', relation);
  END LOOP;
END $$;
