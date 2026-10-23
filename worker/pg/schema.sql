-- THE FACT FACTORY (database `wfsim`; docs/BOARD.md §"The fact factory"): the
-- volunteers, the work they are credited, and the questions they answer. The
-- library's other tables are in library.sql, in the same database.
--
-- Instants are ISO-ms TEXT (docs/NAMING.md §9) and every text column is
-- COLLATE "C", so they compare byte by byte whatever the database's locale.
-- Every whole number is BIGINT: work sums past 2^31. Applied by the role that
-- owns the tables:
--
--   psql -d wfsim -f worker/pg/schema.sql

-- A VERIFYING CLIENT, by the random id its browser made for itself — joined to
-- no submission, and to an account only where its owner claimed it (`devices`,
-- in `wfsim_accounts`). Kept so a client caught once can be refused and
-- everything it agreed to re-checked. `compute_ms` is the time its fights took
-- on its machine, summed; `work` the work of every fact it measured, summed
-- (docs/BOARD.md §"Contribution").
CREATE TABLE IF NOT EXISTS verifiers (
  id            TEXT COLLATE "C" PRIMARY KEY,
  agreed        BIGINT NOT NULL DEFAULT 0,
  banned        BIGINT NOT NULL DEFAULT 0,
  seen          TEXT COLLATE "C" NOT NULL,
  compute_ms    BIGINT NOT NULL DEFAULT 0,
  work          BIGINT NOT NULL DEFAULT 0,
  last_at       TEXT COLLATE "C",
  -- THE READER'S YES: the statement version they agreed to, and when.
  consent_v     BIGINT,
  consent_at    TEXT COLLATE "C",
  -- A REFUSAL IS A COOL-DOWN: how many this client has had, and when the last
  -- one ends (scripts/live_orders.mjs `ban`).
  refusals      BIGINT NOT NULL DEFAULT 0,
  refused_until TEXT COLLATE "C",
  -- …and the facts it has been part of since its last refusal, which forgive
  -- one each `FACTS_PER_REFUSAL_FORGIVEN` (worker/tasks.js).
  clean         BIGINT NOT NULL DEFAULT 0,
  -- WHEN IT LAST ASKED FOR WORK, on which engine and with how many cores free,
  -- to the minute — what tells an idle page from a closed one (verify.js `askedOf`).
  asked_at      TEXT COLLATE "C",
  asked_engine  TEXT COLLATE "C",
  asked_lanes   BIGINT,
  -- THE SOONEST A PROTOCOL 8 SYNC IS HANDED WORK AGAIN (worker/sync.js): a sync
  -- before it is served its results and renewals and nothing new.
  next_at       TEXT COLLATE "C"
);
CREATE INDEX IF NOT EXISTS verifiers_last ON verifiers (last_at);
CREATE INDEX IF NOT EXISTS verifiers_asked ON verifiers (asked_at);

-- THE OFFICIAL POOL: work that was credited and then left its holder — a
-- deleted account's (`deleted_account`), or taken back from a refused client
-- (`refused`). Points are compute and stay counted; the pool holds no name.
CREATE TABLE IF NOT EXISTS points_pool (
  at     TEXT COLLATE "C" NOT NULL,
  source TEXT COLLATE "C" NOT NULL,
  work   BIGINT NOT NULL
);

-- THE SAME WORK BY THE HOUR IT WAS CREDITED, for the rankings' recent periods
-- (docs/BOARD.md §"Contribution"). A refused client's hours count for nothing,
-- as its total does: the ranking reads them through `verifiers.banned`.
CREATE TABLE IF NOT EXISTS verifier_hours (
  verifier TEXT COLLATE "C" NOT NULL,
  -- THE UTC HOUR, `YYYY-MM-DDTHH`: a reader's own "today" is whole hours of it
  -- in their time zone, and the rankings' days are its prefix.
  hour     TEXT COLLATE "C" NOT NULL,
  work     BIGINT NOT NULL DEFAULT 0,
  -- …AND WHAT THE BROWSER SENT IN THAT HOUR: results, and what their fights took.
  tasks    BIGINT NOT NULL DEFAULT 0,
  ms       BIGINT NOT NULL DEFAULT 0,
  PRIMARY KEY (verifier, hour)
);
CREATE INDEX IF NOT EXISTS verifier_hours_by_hour ON verifier_hours (hour);

-- WHAT A DEVICE ASKED FOR, by the UTC hour (docs/BOARD.md §"Outages"): `asks`
-- the server saw; `idle` the asks that brought nothing and `idle_lanes` the
-- cores they left waiting; `failed` those that never reached the server. The
-- last three are the page's own count, sent with its next ask that lands.
CREATE TABLE IF NOT EXISTS verifier_asks (
  verifier   TEXT COLLATE "C" NOT NULL,
  hour       TEXT COLLATE "C" NOT NULL,
  asks       BIGINT NOT NULL DEFAULT 0,
  idle       BIGINT NOT NULL DEFAULT 0,
  idle_lanes BIGINT NOT NULL DEFAULT 0,
  failed     BIGINT NOT NULL DEFAULT 0,
  -- …AND BY ROAD (docs/DISTRIBUTION.md §"The relay"): the asks the relay
  -- carried, and the page's asks each road dropped before the other was tried.
  relayed      BIGINT NOT NULL DEFAULT 0,
  relay_failed BIGINT NOT NULL DEFAULT 0,
  direct_failed BIGINT NOT NULL DEFAULT 0,
  PRIMARY KEY (verifier, hour)
);
CREATE INDEX IF NOT EXISTS verifier_asks_by_hour ON verifier_asks (hour);

-- WHEN THE FACTORY WAS PAUSED OR ITS WATCHDOG FOUND SOMETHING WRONG, from the
-- first sight to the last (docs/BOARD.md §"Outages"): `pause`, or `alert:<cause>`.
CREATE TABLE IF NOT EXISTS factory_events (
  kind       TEXT COLLATE "C" NOT NULL,
  started_at TEXT COLLATE "C" NOT NULL,
  last_at    TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY (kind, started_at)
);

-- POINTS GIVEN BY HAND, an outage's compensation among them: each is ALSO in
-- `verifier_hours`, written in the same transaction, and this says which part
-- of an hour it was and why (`reason`, an id the ledger names).
CREATE TABLE IF NOT EXISTS adjustments (
  id       BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  verifier TEXT COLLATE "C" NOT NULL,
  hour     TEXT COLLATE "C" NOT NULL,
  work     BIGINT NOT NULL,
  reason   TEXT COLLATE "C" NOT NULL,
  at       TEXT COLLATE "C" NOT NULL
);
CREATE INDEX IF NOT EXISTS adjustments_by_verifier ON adjustments (verifier, hour);

-- THE FACT FACTORY (worker/tasks.js): every computed answer the volunteers and
-- the official machines give, whatever asked for it. A QUESTION is a verb on a
-- canonical request, its id the hash of both, so two producers asking one thing
-- share it. A DEMAND is one producer's ask for it, with the deadline that
-- orders the dispatch. An ANSWER is one witness's canonical result.
CREATE TABLE IF NOT EXISTS questions (
  id               TEXT COLLATE "C" PRIMARY KEY,
  verb             TEXT COLLATE "C" NOT NULL,
  request          TEXT COLLATE "C" NOT NULL,
  -- open | fact | spot | disputed | nondeterministic | withdrawn (worker/tasks.js §"States").
  state            TEXT COLLATE "C" NOT NULL,
  -- THE EARLIEST DEADLINE of its live demands, and 1 when a person waits on one
  -- now: dispatch is the waited-on first, then earliest deadline first.
  due_at           TEXT COLLATE "C" NOT NULL,
  waits            BIGINT NOT NULL DEFAULT 0,
  opened_at        TEXT COLLATE "C" NOT NULL,
  -- The cores a first answer needs, relaxing with age: [{ lanes, after_ms }], or null.
  lanes            TEXT COLLATE "C",
  lease            TEXT COLLATE "C",
  lease_until      TEXT COLLATE "C",
  leased_to        TEXT COLLATE "C",
  -- WHEN A VOLUNTEER LEASED IT, and the holder before, still computing, when
  -- another took it past `ROW_CEILING_MS` (worker/tasks.js `SUPERSEDE`).
  leased_at        TEXT COLLATE "C",
  lease_prev       TEXT COLLATE "C",
  leased_to_prev   TEXT COLLATE "C",
  lease_prev_until TEXT COLLATE "C",
  -- THE WITNESSES IT NEEDS, when a recheck raised them past the threshold
  -- (worker/tasks.js `decide`); null is the threshold.
  needs            BIGINT,
  -- THE FACT: the canonical answer, the engine it holds for, and when.
  canon            TEXT COLLATE "C",
  fact_engine      TEXT COLLATE "C",
  fact_at          TEXT COLLATE "C",
  -- 1 when its answers were found not deterministic: settled, nobody refused, said aloud.
  odd              BIGINT NOT NULL DEFAULT 0
);
-- IN THE ORDER A WITNESS IS HANDED WORK (worker/pg/sync.sql `factory_head`).
CREATE INDEX IF NOT EXISTS questions_order ON questions (state, waits DESC, due_at, id);
CREATE INDEX IF NOT EXISTS questions_holder ON questions (leased_to);
CREATE INDEX IF NOT EXISTS questions_lease ON questions (lease);
CREATE INDEX IF NOT EXISTS questions_fact_at ON questions (fact_at);
-- THE WITHDRAWN PAST THE WINDOW (worker/pg/sync.sql `factory_forget_past`):
-- the window's whole withdrawn read row by row every minute, without it.
CREATE INDEX IF NOT EXISTS questions_withdrawn_at ON questions (opened_at) WHERE state = 'withdrawn';

CREATE TABLE IF NOT EXISTS demands (
  producer  TEXT COLLATE "C" NOT NULL,
  ref       TEXT COLLATE "C" NOT NULL,
  question  TEXT COLLATE "C" NOT NULL,
  -- live | served | withdrawn
  state     TEXT COLLATE "C" NOT NULL,
  asked_at  TEXT COLLATE "C" NOT NULL,
  due_at    TEXT COLLATE "C" NOT NULL,
  waits     BIGINT NOT NULL DEFAULT 0,
  -- What the producer's consumer needs back, which the factory never reads.
  payload   TEXT COLLATE "C" NOT NULL DEFAULT '{}',
  served_at TEXT COLLATE "C",
  PRIMARY KEY (producer, ref)
);
CREATE INDEX IF NOT EXISTS demands_by_question ON demands (question, state);
-- A ROW'S DEMAND BY ITS REF ALONE, whichever producer (worker/verify.js `mine`).
CREATE INDEX IF NOT EXISTS demands_by_ref ON demands (ref);
CREATE INDEX IF NOT EXISTS demands_live ON demands (state, producer);
-- THE QUESTIONS SOMEBODY STILL WAITS ON, a tenth of every demand: a redelivery
-- starts here, not from every settled question (worker/tasks.js `redeliver`).
CREATE INDEX IF NOT EXISTS demands_live_question ON demands (question) WHERE state = 'live';
-- THE LIVE DEMANDS PAST DUE, a thousand of a million and a half (worker/tasks.js
-- `audit`): the planner prices `due_at` over every state, where served rows are
-- all past due, so without its own index it scans every live row.
CREATE INDEX IF NOT EXISTS demands_live_due ON demands (due_at) WHERE state = 'live';

CREATE TABLE IF NOT EXISTS answers (
  id         BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  question   TEXT COLLATE "C" NOT NULL,
  engine     TEXT COLLATE "C" NOT NULL,
  -- THE WITNESS: its device id, its owner then (the device itself when unclaimed),
  -- the salted hash of its network (verify.js `netOf`), and whether it is official.
  device     TEXT COLLATE "C" NOT NULL,
  owner      TEXT COLLATE "C" NOT NULL,
  net        TEXT COLLATE "C" NOT NULL DEFAULT '',
  official   BIGINT NOT NULL DEFAULT 0,
  -- The verb's canonical answer, which agreement compares byte for byte, and the
  -- whole result the consumers read (a build, a score).
  canon      TEXT COLLATE "C" NOT NULL,
  result     TEXT COLLATE "C" NOT NULL,
  work       BIGINT NOT NULL,
  compute_ms BIGINT,
  at         TEXT COLLATE "C" NOT NULL,
  -- 1 when it is one of a fact's witnesses, credited; 2 COVERED, an agreeing
  -- answer whose owner or network a witness already is, never to be paid
  -- (worker/tasks.js `repay`); `void` when its witness was refused, after
  -- which it counts for nothing.
  credited   BIGINT NOT NULL DEFAULT 0,
  void       BIGINT NOT NULL DEFAULT 0,
  -- WHAT IT COST, to fit what a point is worth and never credited: the engine's
  -- work counts (`WORK_COUNTERS`, and a search's own), and the cores it ran on.
  counts     TEXT COLLATE "C",
  lanes      BIGINT
);
CREATE INDEX IF NOT EXISTS answers_by_question ON answers (question, engine);
CREATE INDEX IF NOT EXISTS answers_by_device ON answers (device, at);
-- A DEVICE'S ANSWERS THAT EARNED NOTHING, the ledger's few among its many
-- (worker/contribution.js `ledgerOf`).
CREATE INDEX IF NOT EXISTS answers_unpaid_by_device ON answers (device, at) WHERE credited = 0 AND official = 0;
-- ONE VOLUNTEER ANSWER per question, device and engine: a send repeated or raced
-- is one witness, never two rows (worker/tasks.js `recordMany`). Official
-- witnesses may answer a question again, so they are left out.
CREATE UNIQUE INDEX IF NOT EXISTS answers_once ON answers (question, device, engine) WHERE official = 0;
-- WHEN AN ENGINE'S UNIT OF WORK BEGAN, one seek (worker/fair.js `tableOf`).
CREATE INDEX IF NOT EXISTS answers_by_engine ON answers (engine, at);
-- THE ANSWERS OF THE LAST HOURS, whatever engine, and their questions without
-- a visit to the table (worker/tasks.js `audit`): `answers_by_engine` holds `at`
-- second, so a time range alone walked all of it, past the role's timeout.
CREATE INDEX IF NOT EXISTS answers_by_at ON answers (at, question);
-- AN OPEN QUESTION A VOLUNTEER HAS ANSWERED, by engine: one more agreeing
-- answer makes it a fact, so a pack hands these out first (sync.sql
-- `factory_take`). Kept by the triggers in sync.sql, a row while its question
-- is open, so the pick reads a few thousand rows, never every answer.
CREATE TABLE IF NOT EXISTS led_questions (
  question TEXT COLLATE "C" NOT NULL,
  engine   TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY (question, engine)
);
CREATE INDEX IF NOT EXISTS led_questions_by_engine ON led_questions (engine, question);

-- WHAT THE WATCHDOG SAID, a round at most hourly, one row a finding under one
-- `at` (worker/factory.js `watchdog`): read here by whoever fixes it, never
-- sent to anyone. Kept thirty days.
CREATE TABLE IF NOT EXISTS watchdog_said (
  at    TEXT COLLATE "C" NOT NULL,
  cause TEXT COLLATE "C" NOT NULL,
  said  TEXT NOT NULL,
  PRIMARY KEY (cause, at)
);

-- THE FACTORY'S DIALS (worker/dials.js): a value the owner set, overriding the
-- code's default within the code's bounds, and every change ever made to one.
CREATE TABLE IF NOT EXISTS factory_dials (
  name   TEXT COLLATE "C" PRIMARY KEY,
  value  DOUBLE PRECISION NOT NULL,
  set_by TEXT COLLATE "C" NOT NULL,
  set_at TEXT COLLATE "C" NOT NULL
);
CREATE TABLE IF NOT EXISTS factory_dial_log (
  id           BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  name         TEXT COLLATE "C" NOT NULL,
  value_before DOUBLE PRECISION,
  value_after  DOUBLE PRECISION NOT NULL,
  set_by       TEXT COLLATE "C" NOT NULL,
  set_at       TEXT COLLATE "C" NOT NULL
);
CREATE INDEX IF NOT EXISTS factory_dial_log_by_name ON factory_dial_log (name, set_at);

-- THE SETTLER'S TABLES (worker/settler.js, scripts/factoryd.mjs; docs/BOARD.md
-- §"The settler"). The daemon beside the database writes them; while it runs in
-- shadow nothing the worker decides reads them.
-- A QUESTION WITH NEW ANSWERS, waiting for the settler (dial `settle_async`).
-- `claim` is the pass settling it; an answer landing meanwhile clears it, so the
-- pass that read before that answer does not take the row away unsettled.
CREATE TABLE IF NOT EXISTS settle_queue (
  question   TEXT COLLATE "C" NOT NULL,
  engine     TEXT COLLATE "C" NOT NULL,
  peer       TEXT COLLATE "C" NOT NULL DEFAULT '',
  joined     BIGINT NOT NULL DEFAULT 0,
  queued_at  TEXT COLLATE "C" NOT NULL,
  claim      TEXT COLLATE "C",
  claimed_at TEXT COLLATE "C",
  PRIMARY KEY (question, engine, peer)
);
ALTER TABLE settle_queue ADD COLUMN IF NOT EXISTS claim TEXT COLLATE "C";
ALTER TABLE settle_queue ADD COLUMN IF NOT EXISTS claimed_at TEXT COLLATE "C";
CREATE INDEX IF NOT EXISTS settle_queue_by_age ON settle_queue (queued_at);
-- THE FACTORY AT ONE MOMENT, one row: what is open and who is on, and the
-- daemon's own heartbeat and the commit it runs.
CREATE TABLE IF NOT EXISTS factory_snapshot (
  id                 BIGINT PRIMARY KEY CHECK (id = 1),
  at                 TEXT COLLATE "C" NOT NULL,
  engine             TEXT COLLATE "C",
  release            TEXT COLLATE "C",
  open               BIGINT NOT NULL,
  urgent             BIGINT NOT NULL,
  asking             BIGINT NOT NULL,
  computing          BIGINT NOT NULL,
  queue_rows         BIGINT NOT NULL,
  settle_queue_rows  BIGINT NOT NULL,
  daemon_at          TEXT COLLATE "C" NOT NULL,
  daemon_commit      TEXT COLLATE "C" NOT NULL
);
-- THE FAIR-SHARE TABLE (worker/fair.js `tableOf`) as the daemon worked it out,
-- per engine and account, and when.
CREATE TABLE IF NOT EXISTS fair_shares (
  engine  TEXT COLLATE "C" NOT NULL,
  account TEXT COLLATE "C" NOT NULL,
  target  DOUBLE PRECISION NOT NULL,
  share   DOUBLE PRECISION NOT NULL,
  ratio   DOUBLE PRECISION NOT NULL,
  at      TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY (engine, account)
);
-- WHAT THE SETTLER FOUND, one row per kind, its body JSON: the shadow's
-- comparisons, and later the findings the watchdog tells the owner.
CREATE TABLE IF NOT EXISTS factory_findings (
  kind TEXT COLLATE "C" PRIMARY KEY,
  body TEXT COLLATE "C" NOT NULL,
  at   TEXT COLLATE "C" NOT NULL
);
-- PROTOCOL 8 (worker/sync.js). A database laid before it gains the column here.
ALTER TABLE verifiers ADD COLUMN IF NOT EXISTS next_at TEXT COLLATE "C";
-- A QUESTION A CLIENT'S ENGINE REFUSED (`/api/board/order` answered ok: false),
-- by device and engine: not handed to that device again, and withdrawn once
-- `refusals_to_withdraw` owners refused it on the served engine.
CREATE TABLE IF NOT EXISTS engine_refusals (
  question TEXT COLLATE "C" NOT NULL,
  device   TEXT COLLATE "C" NOT NULL,
  owner    TEXT COLLATE "C" NOT NULL,
  engine   TEXT COLLATE "C" NOT NULL,
  step     TEXT COLLATE "C" NOT NULL,
  at       TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY (question, device, engine)
);
CREATE INDEX IF NOT EXISTS engine_refusals_by_engine ON engine_refusals (question, engine);
-- …AND A QUESTION THEY WITHDREW, which the watchdog tells the owner.
CREATE TABLE IF NOT EXISTS refusal_withdrawals (
  question TEXT COLLATE "C" PRIMARY KEY,
  engine   TEXT COLLATE "C" NOT NULL,
  owners   BIGINT NOT NULL,
  at       TEXT COLLATE "C" NOT NULL
);
-- FAIR SHARE BETWEEN PRODUCERS (worker/tasks.js §"Order", docs/BOARD.md
-- §"Fair share between producers"): a question's PRODUCER is its earliest-due
-- live demand's, kept where `due_at` is; each producer's head is one probe.
ALTER TABLE questions ADD COLUMN IF NOT EXISTS producer TEXT COLLATE "C" NOT NULL DEFAULT '';
-- WHO ASKED (docs/BOARD.md §"Who asked"): 'system', 'anonymous', 'user:<code>' or
-- 'bot:<code>', NULL where it was not recorded. A question's is its earliest-due
-- live demand's, as its producer is; a code is erased once the ask is over.
ALTER TABLE questions ADD COLUMN IF NOT EXISTS initiator TEXT COLLATE "C";
ALTER TABLE demands ADD COLUMN IF NOT EXISTS initiator TEXT COLLATE "C";
-- HOW FAR A PASS OVER TIME HAS READ, by name (worker/tasks.js `redeliver`).
CREATE TABLE IF NOT EXISTS factory_cursors (
  name    TEXT COLLATE "C" PRIMARY KEY,
  through TEXT COLLATE "C" NOT NULL
);

-- MERIT SPENT ON A ROW (docs/BOARD.md §"Merit priority"): the demand it moved,
-- the code that paid, the price held, and the deadline it had (`natural_due`)
-- and was given (`boosted_due`). `state` held → charged | refunded, and a
-- settled hold keeps no code.
CREATE TABLE IF NOT EXISTS merit_holds (
  id          BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  producer    TEXT COLLATE "C" NOT NULL,
  ref         TEXT COLLATE "C" NOT NULL,
  code        TEXT COLLATE "C" NOT NULL,
  price       BIGINT NOT NULL,
  natural_due TEXT COLLATE "C" NOT NULL,
  boosted_due TEXT COLLATE "C" NOT NULL,
  at          TEXT COLLATE "C" NOT NULL,
  state       TEXT COLLATE "C" NOT NULL DEFAULT 'held'
);
CREATE INDEX IF NOT EXISTS merit_holds_held ON merit_holds (state, at) WHERE state = 'held';
-- …THE CODES STILL KEPT, found without a scan by the pass that erases them.
CREATE INDEX IF NOT EXISTS questions_coded ON questions (state) WHERE initiator LIKE 'user:%' OR initiator LIKE 'bot:%';
CREATE INDEX IF NOT EXISTS demands_coded ON demands (state) WHERE initiator LIKE 'user:%' OR initiator LIKE 'bot:%';
CREATE INDEX IF NOT EXISTS questions_by_producer ON questions (state, producer, waits DESC, due_at, id);
-- …AND WHAT EACH HAS USED: compute charged at hand-out (`used`, ms over
-- `weight`), at `answer_ms`, the running mean of its answers' fights.
CREATE TABLE IF NOT EXISTS producer_use (
  producer  TEXT COLLATE "C" PRIMARY KEY,
  used      DOUBLE PRECISION NOT NULL DEFAULT 0,
  answer_ms DOUBLE PRECISION NOT NULL DEFAULT 0,
  weight    DOUBLE PRECISION NOT NULL DEFAULT 1
);
