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

-- WHERE A SUBMISSION LANDS, AND THE ONLY DOOR THERE IS.
--
-- The endpoint that writes it runs on Cloudflare and has no game data, so it
-- cannot say what a build IS: telling two builds apart needs the mod POOL — an
-- elemental card enters the element sequence and a plain one does not, and the
-- sequence decides the pairing (Torid, six mods: 12,424 DPS against 46,583). A
-- key derived without it would be a SECOND answer to the one question that must
-- have one, and the endpoint would be the half with no evidence.
--
-- So the door stores the record VERBATIM and nothing more, and `wfsim-intake`
-- — which has the engine — canonicalises it, resolves what a riven's numbers
-- must be, and writes the builds that come out. The row is deleted the moment
-- they land.
--
-- A QUEUE, NOT A LIBRARY. `id` is random and means nothing: it exists so one
-- row can be deleted, and two submissions of the same build are two rows here
-- and one row in `builds`. Nothing reads this table but intake.
CREATE TABLE IF NOT EXISTS inbox (
  id     TEXT PRIMARY KEY,
  at     TEXT NOT NULL,
  record TEXT NOT NULL
);

-- ONE ROW PER BUILD, keyed by a HASH of what makes it one: the canonical build
-- `engine::builds::identity` states — its riven's ROLLS included, because two
-- ends of one shape are two builds with two numbers — hashed by `wfsim-intake`,
-- which has the engine where the door does not. DERIVED AND NOT ALLOCATED, so the same build
-- always keys the same: nothing looks the row up before writing it, and two
-- writers cannot disagree about whether they hold one build.
--
-- The RECORD is kept whole as json rather than exploded into columns. The axes
-- are declared once, in `AXES` in worker/index.js, and a build has gained an
-- axis four times — a schema that spelled them out would be a fifth place to
-- forget one, which is the exact bug that lost `mode` and then `valence`.
-- Anything worth indexing gets a generated column instead; `weapon` is the
-- first, because "how well covered is this weapon" is the question the board is
-- actually run on.
CREATE TABLE IF NOT EXISTS builds (
  id       TEXT PRIMARY KEY,
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
-- retires one is a person deleting the row. `scripts/ship_facts.sh` writes it
-- beside the running scorer; `scripts/fetch_facts.sh` reads them back out. This
-- table is the only source the publisher has.
CREATE TABLE IF NOT EXISTS scores (
  -- THE BUILD, BY ITS `builds.id`. The name is the one the column was created
  -- with and the wire keeps the names it has; what it holds is the id, because
  -- two spellings of "which build" is the one thing a foreign key may not have.
  identity     TEXT NOT NULL,
  ruler        TEXT NOT NULL,
  -- A ROW IS (build, ruler, MODE). A mode is a property of the WEAPON, not of
  -- the build -- every melee carries seven and the Ballistica Prime four -- and
  -- the cards that win one do not win another, so each is an independent
  -- ranking. Without this column a melee build's seven measurements collapse
  -- into one row and six of them are lost on write.
  mode         TEXT NOT NULL,
  -- WHICH BUILD MEASURED IT, AND IT DECIDES NOTHING. An engine version being
  -- older does not make a score wrong -- the two are a REFERENCE relation, not a
  -- validity one. What the code DOES to a row is not enumerable from the row and
  -- no hash can answer it, so this is FORENSICS: it says which rows a build
  -- wrote, once that build is found to have been broken.
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
  -- IT DECIDES NOTHING, the same terms as `measured_by`. What it is for is the
  -- ROW OUTLIVING THE FILE it was measured under: a fact is kept until somebody
  -- deletes it, so a row can be older than the ruler's current terms, and
  -- reading one back without this means checking out the commit that made it.
  metric       TEXT NOT NULL,
  -- WHAT THE ROW COST, so the bill is read off the rows rather than estimated,
  -- and so the next run can pack its shards by work rather than by count. NOT
  -- derivable from the two clocks below: a row paid for in sittings spans a wall
  -- clock much longer than the fight it contains.
  cost_seconds REAL NOT NULL,
  -- WHEN THE FIGHT STARTED AND WHEN IT ENDED. Provenance, and nothing branches
  -- on either: a fact does not decay, and age is not evidence that a number is
  -- wrong. What they are for is showing a reader how old a ROW is rather than
  -- how old the board is, and saying afterwards which rows a bad build wrote.
  --
  -- A ROW MIGRATED FROM BEFORE THEY EXISTED CARRIES THE SAME VALUE IN BOTH,
  -- which is how "we do not know when this started" is spelled. Deriving a
  -- start by subtracting the cost would invent precision the old row never had.
  started_at   TEXT NOT NULL,
  finished_at  TEXT NOT NULL,
  -- ONE FACT PER ROW: the last measurement of it. Nothing here says whether a
  -- number is right — not the clock, not the build that wrote it, and no hash of
  -- what it read. Only another measurement can, and a person deleting the row is
  -- what asks for one.
  PRIMARY KEY (identity, ruler, mode)
);

-- "What has this ruler measured" is one indexed query, which is what the set
-- difference is asked through.
CREATE INDEX IF NOT EXISTS scores_ruler ON scores (ruler);

-- …and "what has gone longest without being measured" is the order to repair
-- in, which is the one thing a clock here is allowed to decide.
CREATE INDEX IF NOT EXISTS scores_oldest ON scores (finished_at);

-- WHAT SOMEBODY ASKED TO BE COMPUTED, AND HOW FAR IT GOT.
--
-- THE ONE RULE THAT KEEPS TWO TABLES FROM DISAGREEING: the queue may only ever
-- CAUSE work. It may not prevent work, and it may not decide a number. So
-- losing it costs an ordering and never a fact, a stale row costs one
-- recomputation that produces the same number, and the publisher never reads
-- it at all.
--
-- A ROW IS DELETED WHEN ITS SCORE LANDS, and in that order — the score is
-- written first. Interrupted between the two, the row is computed again, which
-- is free: a score is a pure function of what it measured.
--
-- WHY IT IS NOT A COLUMN ON `scores`. That table is what the site is built
-- from: one row per (build, ruler, mode), the latest measurement, no history
-- and no state. A `pending` flag on it would make the data source carry the
-- work list, and every reader would have to know which rows are real.
CREATE TABLE IF NOT EXISTS batches (
  id    TEXT PRIMARY KEY,
  -- THE ORDER, AND THE ONLY THING THAT SETS IT. Sorted as text, so a batch
  -- jumps the line by being renamed rather than by a priority nobody can see.
  at    TEXT NOT NULL,
  -- WHAT THIS GROUP IS FOR, for the person who finds it a week later. A batch
  -- with no reason is a batch nobody can decide to cancel.
  why   TEXT,
  -- HOW MANY ROWS IT WAS EVER ASKED FOR WITH — a statement about the PAST, so
  -- it cannot drift: progress is this minus what is still in `queue`. A second
  -- pass into one batch ADDS to it, because two runs in a day both find
  -- arrivals and a replace would report the group shrinking as it worked.
  total INTEGER NOT NULL
);

-- ONE ROW PER (batch, build, ruler, mode). The same row may sit in two batches
-- at once — two people asking for one thing — and computing it deletes it from
-- both, which is why the delete does not name a batch.
CREATE TABLE IF NOT EXISTS queue (
  batch    TEXT NOT NULL,
  build_id TEXT NOT NULL,
  ruler    TEXT NOT NULL,
  mode     TEXT NOT NULL,
  PRIMARY KEY (batch, build_id, ruler, mode)
);

-- "WHAT IS LEFT, IN ORDER" is one indexed read, and it is the only question a
-- run asks of this table.
CREATE INDEX IF NOT EXISTS queue_batch ON queue (batch);

-- "IS THIS ROW STILL OWED", asked by every lease of a compute order.
CREATE INDEX IF NOT EXISTS queue_row ON queue (build_id, ruler, mode);

-- A COMPUTE ORDER: one owed row, handed to the machines that have the site
-- open — docs/BOARD.md §"Compute orders". Opened beside the `queue` row that
-- owes it (`ship_queue.sh`), and leasable while that row is still owed; a row
-- reaches `scores` when two different clients produced the same bits for it,
-- or the server did.
--
-- `state`: todo (nobody has measured it), fresh (one result, not yet ranked),
-- open (one result, waiting for a second client), arbiter (in its group's top
-- ten: the server's alone), dispute, spot (verified, recomputed by the server
-- too), verified, rejected, and scoring:todo / scoring:open (claimed by a
-- `scores.yml` run, so no lease seeks it; released when the run ends). `slot`
-- is a random number a lease seeks from, which is what keeps one lease a few
-- rows read however long the book grows. `engine`
-- is the first result's `ENGINE_ID`, and only that engine verifies it. `at` is
-- when the order was opened: the scorer leaves a young one to clients.
-- `clients` is every client that measured it, comma-separated, in the order
-- their results came (`produced_by` first, `verifier` last), and
-- `clients_compute_ms` what each one's fight took on its machine, in the same
-- order ("" where a client did not say). Added to the live table with:
--   ALTER TABLE orders ADD COLUMN clients TEXT NOT NULL DEFAULT '';
--   ALTER TABLE orders ADD COLUMN clients_compute_ms TEXT NOT NULL DEFAULT '';
-- `work` is the first result's `Shard::work`, which every later one must equal
-- (docs/BOARD.md §"Contribution"):
--   ALTER TABLE orders ADD COLUMN work INTEGER;
-- `priority` is 0 for the rows a new build owes and 1 for a rescore or a sweep;
-- a lease takes 0 first (`ship_queue.sh` sets it from the batch). `carried_from`
-- is the engine a result was measured by when a release carried it to the next
-- engine, which a further result then confirms or replaces:
--   ALTER TABLE orders ADD COLUMN priority INTEGER NOT NULL DEFAULT 1;
--   ALTER TABLE orders ADD COLUMN carried_from TEXT;
--   DROP INDEX orders_pick;
-- `clients_nets` is the salted hashes of the networks its clients answered
-- from (worker/verify.js `netOf`), cleared when it becomes a fact:
--   ALTER TABLE orders ADD COLUMN clients_nets TEXT NOT NULL DEFAULT '';
CREATE TABLE IF NOT EXISTS orders (
  identity    TEXT NOT NULL,
  ruler       TEXT NOT NULL,
  mode        TEXT NOT NULL,
  record      TEXT NOT NULL,
  state       TEXT NOT NULL,
  engine      TEXT NOT NULL DEFAULT '',
  slot        INTEGER NOT NULL,
  lease       TEXT,
  lease_until TEXT,
  leased_to   TEXT,
  score       REAL,
  metric      TEXT,
  produced_by TEXT,
  verifier    TEXT,
  disputed    REAL,
  at          TEXT NOT NULL,
  clients     TEXT NOT NULL DEFAULT '',
  clients_compute_ms TEXT NOT NULL DEFAULT '',
  work        INTEGER,
  priority    INTEGER NOT NULL DEFAULT 1,
  carried_from TEXT,
  clients_nets TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (identity, ruler, mode)
);

CREATE INDEX IF NOT EXISTS orders_pick ON orders (state, engine, priority, slot);
CREATE INDEX IF NOT EXISTS orders_lease ON orders (lease);
CREATE INDEX IF NOT EXISTS orders_holder ON orders (leased_to);
CREATE INDEX IF NOT EXISTS orders_verifier ON orders (verifier);

-- A VERIFYING CLIENT, by the random id its browser made for itself — joined to
-- no submission, and to an account only where its owner claimed it (the
-- `devices` table, in the ACCOUNTS database). Kept so a client caught once can
-- be refused and everything it agreed to re-checked. `compute_ms` is the time
-- its fights took on its machine, summed; `work` the work of every fact it
-- measured, summed (docs/BOARD.md §"Contribution"). Added to the live table with:
--   ALTER TABLE verifiers ADD COLUMN compute_ms INTEGER NOT NULL DEFAULT 0;
--   ALTER TABLE verifiers ADD COLUMN work INTEGER NOT NULL DEFAULT 0;
--   ALTER TABLE verifiers ADD COLUMN last_at TEXT;
--   ALTER TABLE verifiers ADD COLUMN consent_v INTEGER;
--   ALTER TABLE verifiers ADD COLUMN consent_at TEXT;
CREATE TABLE IF NOT EXISTS verifiers (
  id     TEXT PRIMARY KEY,
  agreed INTEGER NOT NULL DEFAULT 0,
  banned INTEGER NOT NULL DEFAULT 0,
  seen   TEXT NOT NULL,
  compute_ms INTEGER NOT NULL DEFAULT 0,
  work   INTEGER NOT NULL DEFAULT 0,
  last_at TEXT,
  -- THE READER'S YES: the statement version they agreed to, and when.
  consent_v  INTEGER,
  consent_at TEXT,
  -- A REFUSAL IS A COOL-DOWN: how many this client has had, and when the last
  -- one ends (scripts/live_orders.mjs `ban`):
  --   ALTER TABLE verifiers ADD COLUMN refusals INTEGER NOT NULL DEFAULT 0;
  --   ALTER TABLE verifiers ADD COLUMN refused_until TEXT;
  refusals      INTEGER NOT NULL DEFAULT 0,
  refused_until TEXT,
  -- …and the facts it has been part of since its last refusal, which forgive
  -- one each `FACTS_PER_REFUSAL_FORGIVEN` (worker/verify.js):
  --   ALTER TABLE verifiers ADD COLUMN clean INTEGER NOT NULL DEFAULT 0;
  clean         INTEGER NOT NULL DEFAULT 0
);

-- THE SAME WORK BY THE DAY IT WAS CREDITED, for the ranking's last thirty days
-- (docs/BOARD.md §"Contribution"). A refused client's days count for nothing,
-- as its total does: the ranking reads them through `verifiers.banned`.
CREATE TABLE IF NOT EXISTS verifier_hours (
  verifier TEXT NOT NULL,
  -- THE UTC HOUR, `YYYY-MM-DDTHH`: a reader's own "today" is whole hours of it
  -- in their time zone, and the rankings' days are its prefix.
  hour     TEXT NOT NULL,
  work     INTEGER NOT NULL DEFAULT 0,
  -- …AND WHAT THE BROWSER SENT IN THAT HOUR: results, and what their fights took.
  tasks    INTEGER NOT NULL DEFAULT 0,
  ms       INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (verifier, hour)
);

-- SHORT SHARE LINKS: `/weapons/<weapon>/s/<id>` names a stored share code.
-- `id` is a hash of (weapon, code) computed by the worker, so a row never
-- changes, the same build is one row, and no client chooses an id. `at` is the
-- day, like every table here. worker/index.js §"SHORT SHARE LINKS".
-- `claim` is the sharer's measurement when they chose to include one (JSON,
-- validated by `shareClaim`), and it is in the id's hash; NULL otherwise.
-- Added to a live table with:
--   ALTER TABLE shares ADD COLUMN claim TEXT;
CREATE TABLE IF NOT EXISTS shares (
  id     TEXT PRIMARY KEY,
  weapon TEXT NOT NULL,
  code   TEXT NOT NULL,
  at     TEXT NOT NULL,
  claim  TEXT
);

-- A FINISHED DAY'S USAGE TOTALS, kept past the Analytics Engine's three months
-- by the worker's daily cron (worker/usage_days.js, docs/ANALYTICS.md
-- §Retention). Totals only, never a visitor id. `event` is a usage event, or
-- `visitors` (subject `all`, `result`, `returning`) or `visitors.country`;
-- `market` is `china` or `overseas`; `sampled` is 1 when the dataset sampled
-- that day, so `visitors` is a floor. Added to a live database with this
-- statement alone.
CREATE TABLE IF NOT EXISTS usage_days (
  day      TEXT NOT NULL,
  event    TEXT NOT NULL,
  subject  TEXT NOT NULL,
  market   TEXT NOT NULL,
  visitors INTEGER NOT NULL,
  points   INTEGER NOT NULL,
  sampled  INTEGER NOT NULL,
  PRIMARY KEY (day, event, subject, market)
);

-- WHAT A CHAT SENT THE BOT, until the bot server has answered it
-- (worker/qq.js, docs/AGENT.md §"The QQ bot"). `id` is the platform's own message id, so a
-- retried callback lands on the row it already wrote; `body` is the event as
-- the platform sent it. The server claims a row, answers from its whitelisted
-- address and marks it done; a row is deleted a day after it arrived, answered
-- or not. Added to a live database with this statement alone.
CREATE TABLE IF NOT EXISTS bot_inbox (
  id         TEXT PRIMARY KEY,
  channel    TEXT NOT NULL,
  kind       TEXT NOT NULL,
  body       TEXT NOT NULL,
  at         TEXT NOT NULL,
  claimed_at TEXT,
  done_at    TEXT
);

-- RIVEN APPRAISAL (worker/appraise.js, docs/AGENT.md §"Riven appraisal"). One row
-- per appraisal a chat's bot opened; `chat` is that channel's own record of
-- where to answer and nothing else reads it. `winner` is the first handed-back
-- build the bot accepted. Both tables are emptied a day after a row arrived.
-- Added to a live database with these statements alone.
CREATE TABLE IF NOT EXISTS appraisals (
  code     TEXT PRIMARY KEY,
  channel  TEXT NOT NULL,
  chat     TEXT NOT NULL,
  asker    TEXT NOT NULL,
  room     TEXT NOT NULL,
  weapon   TEXT NOT NULL,
  ruler    TEXT NOT NULL,
  riven    TEXT NOT NULL,
  at       TEXT NOT NULL,
  winner   INTEGER,
  done_at  TEXT,
  told_at  TEXT,
  -- AS VOLUNTEER WORK (worker/appraise.js §"Volunteer work"): the search frozen
  -- once by the bot, the engine it is for, the lease of the computer running it,
  -- and when two owners' computers agreed on its answer.
  request     TEXT,
  engine      TEXT,
  lease       TEXT,
  lease_until TEXT,
  leased_to   TEXT,
  agreed_at   TEXT,
  -- WHEN A COMPUTER FIRST TOOK IT, and when the chat was told one had.
  started_at   TEXT,
  started_told TEXT
);
--   ALTER TABLE appraisals ADD COLUMN request TEXT;  (and engine, lease, leased_to TEXT;
--   lease_until, agreed_at, started_at, started_told TEXT) — the live table was made before them.
CREATE INDEX IF NOT EXISTS appraisals_by_asker ON appraisals (channel, asker, at);
-- `net` is the salted hash of the network an answer came from (verify.js `netOf`):
--   ALTER TABLE appraisal_results ADD COLUMN net TEXT NOT NULL DEFAULT '';
CREATE TABLE IF NOT EXISTS appraisal_results (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  code       TEXT NOT NULL,
  build      TEXT NOT NULL,
  thanks     TEXT NOT NULL DEFAULT '',
  at         TEXT NOT NULL,
  claimed_at TEXT,
  checked_at TEXT,
  verdict    TEXT,
  -- A VOLUNTEER'S ANSWER: whose computer, its winner's score, the search's work,
  -- and the build's canonical text, which two answers must share to agree.
  verifier   TEXT,
  score      REAL,
  work       INTEGER,
  key        TEXT,
  net        TEXT NOT NULL DEFAULT '',
  -- THE ENGINE IT RAN ON, which the cap on answers counts by (appraise.js `rivenTask`).
  engine     TEXT
);
--   ALTER TABLE appraisal_results ADD COLUMN verifier TEXT;  (and key TEXT; score REAL; work INTEGER; engine TEXT)
CREATE INDEX IF NOT EXISTS appraisal_results_by_code ON appraisal_results (code);

-- EVERY VOID FISSURE THE GAME OPENS, from the world state the bot server relays
-- each minute (worker/world.js `fissureLog`). `id` is DE's own, so the minutes
-- that see one again write nothing. `list` is normal, steel_path or railjack;
-- `mission` is NULL for a Void Storm. Which combinations a reminder may hold is
-- read from here. Added to a live database with this statement alone.
CREATE TABLE IF NOT EXISTS fissures (
  id            TEXT PRIMARY KEY,
  list          TEXT NOT NULL,
  tier          TEXT NOT NULL,
  mission       TEXT,
  node          TEXT NOT NULL,
  started_at    TEXT NOT NULL,
  ends_at       TEXT NOT NULL
);
