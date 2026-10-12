-- THE LIBRARY'S OTHER TABLES IN POSTGRES (database `wfsim`, beside the fact
-- factory's in schema.sql): what D1's `wfsim` holds besides the factory, column
-- for column, so worker/factory_db.js can serve `LIBRARY` from here when
-- `DB = "pg"`. Text compares byte by byte (COLLATE "C"), as D1's does, and an
-- integer is a bigint. Applied once, by the role that owns the tables:
--   sudo -u postgres psql -d wfsim -v ON_ERROR_STOP=1 -c 'SET ROLE wfsim' -f library.sql

CREATE TABLE IF NOT EXISTS inbox (
  id     TEXT COLLATE "C" PRIMARY KEY,
  at     TEXT COLLATE "C" NOT NULL,
  record TEXT COLLATE "C" NOT NULL
);

-- A BUILD'S WEAPON is read out of its record, stored, as D1 reads it virtually.
CREATE TABLE IF NOT EXISTS builds (
  id     TEXT COLLATE "C" PRIMARY KEY,
  at     TEXT COLLATE "C" NOT NULL,
  record TEXT COLLATE "C" NOT NULL,
  weapon TEXT COLLATE "C" GENERATED ALWAYS AS ((record::jsonb ->> 'weapon')) STORED
);
CREATE INDEX IF NOT EXISTS builds_weapon ON builds (weapon);
CREATE INDEX IF NOT EXISTS builds_at ON builds (at);

CREATE TABLE IF NOT EXISTS scores (
  identity     TEXT COLLATE "C" NOT NULL,
  ruler        TEXT COLLATE "C" NOT NULL,
  mode         TEXT COLLATE "C" NOT NULL,
  measured_by  TEXT COLLATE "C" NOT NULL,
  score        DOUBLE PRECISION NOT NULL,
  metric       TEXT COLLATE "C" NOT NULL,
  cost_seconds DOUBLE PRECISION NOT NULL,
  started_at   TEXT COLLATE "C" NOT NULL,
  finished_at  TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY (identity, ruler, mode)
);
CREATE INDEX IF NOT EXISTS scores_ruler ON scores (ruler);
CREATE INDEX IF NOT EXISTS scores_oldest ON scores (finished_at);

CREATE TABLE IF NOT EXISTS batches (
  id    TEXT COLLATE "C" PRIMARY KEY,
  at    TEXT COLLATE "C" NOT NULL,
  why   TEXT COLLATE "C",
  total BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS queue (
  batch    TEXT COLLATE "C" NOT NULL,
  build_id TEXT COLLATE "C" NOT NULL,
  ruler    TEXT COLLATE "C" NOT NULL,
  mode     TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY (batch, build_id, ruler, mode)
);
CREATE INDEX IF NOT EXISTS queue_batch ON queue (batch);
CREATE INDEX IF NOT EXISTS queue_row ON queue (build_id, ruler, mode);

-- THE BOARD'S ORDERS BEFORE THE FACTORY, read by `/api/board/adopt` alone.
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

CREATE TABLE IF NOT EXISTS shares (
  id     TEXT COLLATE "C" PRIMARY KEY,
  weapon TEXT COLLATE "C" NOT NULL,
  code   TEXT COLLATE "C" NOT NULL,
  at     TEXT COLLATE "C" NOT NULL,
  claim  TEXT COLLATE "C"
);

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

CREATE TABLE IF NOT EXISTS bot_inbox (
  id         TEXT COLLATE "C" PRIMARY KEY,
  channel    TEXT COLLATE "C" NOT NULL,
  kind       TEXT COLLATE "C" NOT NULL,
  body       TEXT COLLATE "C" NOT NULL,
  at         TEXT COLLATE "C" NOT NULL,
  claimed_at TEXT COLLATE "C",
  done_at    TEXT COLLATE "C"
);

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
  request      TEXT COLLATE "C",
  engine       TEXT COLLATE "C",
  lease        TEXT COLLATE "C",
  lease_until  TEXT COLLATE "C",
  leased_to    TEXT COLLATE "C",
  agreed_at    TEXT COLLATE "C",
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
  verifier   TEXT COLLATE "C",
  score      DOUBLE PRECISION,
  work       BIGINT,
  key        TEXT COLLATE "C",
  net        TEXT COLLATE "C" NOT NULL DEFAULT '',
  engine     TEXT COLLATE "C"
);
CREATE INDEX IF NOT EXISTS appraisal_results_by_code ON appraisal_results (code);

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
-- repository; carried so nothing D1 holds is left behind.
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
