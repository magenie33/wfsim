-- THE ACCOUNTS IN POSTGRES: a database of their own (`wfsim_accounts`, owned by
-- the role `wfsim_accounts`), apart from the library as D1's `wfsim-accounts`
-- is apart from `wfsim`, so the library's role and backups never reach personal
-- data. Every table D1's holds, the paid half's (private/cloud) included, column
-- for column; text compares byte by byte (COLLATE "C") and an integer is a
-- bigint. Applied once, by its owner:
--   sudo -u postgres psql -d wfsim_accounts -v ON_ERROR_STOP=1 -c 'SET ROLE wfsim_accounts' -f accounts.sql

CREATE TABLE IF NOT EXISTS accounts (
  id                  TEXT COLLATE "C" PRIMARY KEY,
  created_at          TEXT COLLATE "C" NOT NULL,
  username            TEXT COLLATE "C",
  display_name        TEXT COLLATE "C",
  username_changed_at TEXT COLLATE "C",
  delete_after        TEXT COLLATE "C"
);
CREATE UNIQUE INDEX IF NOT EXISTS accounts_username ON accounts (username);

CREATE TABLE IF NOT EXISTS identities (
  provider      TEXT COLLATE "C" NOT NULL CHECK (provider IN ('google', 'discord', 'github', 'email')),
  subject       TEXT COLLATE "C" NOT NULL,
  account       TEXT COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  label         TEXT COLLATE "C" NOT NULL,
  linked_at     TEXT COLLATE "C" NOT NULL,
  password_hash TEXT COLLATE "C",
  PRIMARY KEY (provider, subject),
  UNIQUE (account, provider)
);

-- AN ACCOUNT LIVES WHILE A SLOT IS FILLED: the last identity gone takes the
-- account, and by cascade its sessions and data — D1's `identities_last_slot`.
CREATE OR REPLACE FUNCTION identities_last_slot() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM identities WHERE account = OLD.account) THEN
    DELETE FROM accounts WHERE id = OLD.account;
  END IF;
  RETURN NULL;
END $$;
DROP TRIGGER IF EXISTS identities_last_slot ON identities;
CREATE TRIGGER identities_last_slot AFTER DELETE ON identities
  FOR EACH ROW EXECUTE FUNCTION identities_last_slot();

CREATE TABLE IF NOT EXISTS sessions (
  token_hash TEXT COLLATE "C" PRIMARY KEY,
  account    TEXT COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  expires_at TEXT COLLATE "C" NOT NULL
);
CREATE INDEX IF NOT EXISTS sessions_by_account ON sessions (account);

CREATE TABLE IF NOT EXISTS email_codes (
  email         TEXT COLLATE "C" PRIMARY KEY,
  code_hash     TEXT COLLATE "C" NOT NULL,
  expires_at    TEXT COLLATE "C" NOT NULL,
  attempts      BIGINT NOT NULL DEFAULT 0,
  sent_at       TEXT COLLATE "C" NOT NULL,
  purpose       TEXT COLLATE "C" NOT NULL DEFAULT 'register' CHECK (purpose IN ('register', 'link', 'reset')),
  password_hash TEXT COLLATE "C",
  account       TEXT COLLATE "C"
);

CREATE TABLE IF NOT EXISTS login_failures (
  email    TEXT COLLATE "C" PRIMARY KEY,
  failures BIGINT NOT NULL,
  first_at TEXT COLLATE "C" NOT NULL
);

CREATE TABLE IF NOT EXISTS username_holds (
  username   TEXT COLLATE "C" PRIMARY KEY,
  account    TEXT COLLATE "C" NOT NULL,
  held_until TEXT COLLATE "C" NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_keys (
  id               TEXT COLLATE "C" PRIMARY KEY,
  key_hash         TEXT COLLATE "C" NOT NULL UNIQUE,
  name             TEXT COLLATE "C" NOT NULL,
  created_at       TEXT COLLATE "C" NOT NULL,
  last_used_at     TEXT COLLATE "C",
  account          TEXT COLLATE "C" REFERENCES accounts (id) ON DELETE CASCADE,
  claimed_at       TEXT COLLATE "C",
  claim_email      TEXT COLLATE "C",
  claim_code_hash  TEXT COLLATE "C",
  claim_expires_at TEXT COLLATE "C",
  claim_sent_at    TEXT COLLATE "C",
  claim_attempts   BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS agent_keys_by_account ON agent_keys (account);

CREATE TABLE IF NOT EXISTS devices (
  verifier   TEXT COLLATE "C" PRIMARY KEY,
  account    TEXT COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  claimed_at TEXT COLLATE "C" NOT NULL,
  label      TEXT COLLATE "C"
);
CREATE INDEX IF NOT EXISTS devices_by_account ON devices (account);

CREATE TABLE IF NOT EXISTS device_spans (
  verifier   TEXT COLLATE "C" NOT NULL,
  account    TEXT COLLATE "C" REFERENCES accounts (id) ON DELETE SET NULL,
  from_hour  TEXT COLLATE "C" NOT NULL,
  until_hour TEXT COLLATE "C",
  PRIMARY KEY (verifier, from_hour)
);
CREATE INDEX IF NOT EXISTS device_spans_by_account ON device_spans (account);

CREATE TABLE IF NOT EXISTS contribution_choice (
  account   TEXT COLLATE "C" PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
  named     BIGINT NOT NULL,
  chosen_at TEXT COLLATE "C" NOT NULL
);

-- THE PAID HALF'S TABLES (private/cloud), read and written there alone.
CREATE TABLE IF NOT EXISTS account_titles (
  account    TEXT COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  title      TEXT COLLATE "C" NOT NULL,
  granted_at TEXT COLLATE "C" NOT NULL,
  note       TEXT COLLATE "C",
  PRIMARY KEY (account, title)
);
CREATE TABLE IF NOT EXISTS billing_attempts (
  who TEXT COLLATE "C" NOT NULL,
  ip  TEXT COLLATE "C",
  at  TEXT COLLATE "C" NOT NULL
);
CREATE INDEX IF NOT EXISTS billing_attempts_by_at ON billing_attempts (at);
CREATE TABLE IF NOT EXISTS billing_card_refunds (
  card TEXT COLLATE "C" PRIMARY KEY,
  at   TEXT COLLATE "C" NOT NULL
);
CREATE TABLE IF NOT EXISTS billing_customers (
  account   TEXT COLLATE "C" PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
  customer  TEXT COLLATE "C" NOT NULL UNIQUE,
  synced_at TEXT COLLATE "C"
);
CREATE TABLE IF NOT EXISTS billing_endings (
  account TEXT COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  channel TEXT COLLATE "C" NOT NULL,
  ref     TEXT COLLATE "C" NOT NULL,
  amount  BIGINT NOT NULL CHECK (amount > 0),
  at      TEXT COLLATE "C" NOT NULL,
  done_at TEXT COLLATE "C",
  card    TEXT COLLATE "C",
  PRIMARY KEY (account, channel, ref)
);
CREATE TABLE IF NOT EXISTS billing_events (
  id   TEXT COLLATE "C" PRIMARY KEY,
  type TEXT COLLATE "C" NOT NULL,
  at   TEXT COLLATE "C" NOT NULL
);
CREATE TABLE IF NOT EXISTS billing_grants (
  id        BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  account   TEXT COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  offer     TEXT COLLATE "C" NOT NULL,
  starts_at TEXT COLLATE "C" NOT NULL,
  ends_at   TEXT COLLATE "C" NOT NULL,
  note      TEXT COLLATE "C"
);
CREATE TABLE IF NOT EXISTS billing_orders (
  id           TEXT COLLATE "C" PRIMARY KEY,
  account      TEXT COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  channel      TEXT COLLATE "C" NOT NULL,
  offer        TEXT COLLATE "C" NOT NULL,
  price        TEXT COLLATE "C" NOT NULL,
  amount       BIGINT NOT NULL,
  currency     TEXT COLLATE "C" NOT NULL,
  days         BIGINT,
  status       TEXT COLLATE "C" NOT NULL CHECK (status IN ('pending', 'paid', 'closed', 'refunded', 'disputed')),
  channel_ref  TEXT COLLATE "C",
  created_at   TEXT COLLATE "C" NOT NULL,
  paid_at      TEXT COLLATE "C",
  months       BIGINT,
  starts_after TEXT COLLATE "C"
);
CREATE INDEX IF NOT EXISTS billing_orders_by_account ON billing_orders (account);
CREATE UNIQUE INDEX IF NOT EXISTS billing_orders_by_ref ON billing_orders (channel, channel_ref);
CREATE TABLE IF NOT EXISTS billing_periods (
  invoice   TEXT COLLATE "C" NOT NULL,
  line      BIGINT NOT NULL,
  account   TEXT COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  offer     TEXT COLLATE "C" NOT NULL,
  price     TEXT COLLATE "C" NOT NULL,
  starts_at TEXT COLLATE "C" NOT NULL,
  ends_at   TEXT COLLATE "C" NOT NULL,
  status    TEXT COLLATE "C" NOT NULL CHECK (status IN ('paid', 'refunded', 'disputed')),
  PRIMARY KEY (invoice, line)
);
CREATE INDEX IF NOT EXISTS billing_periods_by_account ON billing_periods (account);
CREATE TABLE IF NOT EXISTS billing_subscriptions (
  id                   TEXT COLLATE "C" PRIMARY KEY,
  account              TEXT COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  price                TEXT COLLATE "C" NOT NULL,
  status               TEXT COLLATE "C" NOT NULL,
  period_start         TEXT COLLATE "C" NOT NULL,
  period_end           TEXT COLLATE "C" NOT NULL,
  cancel_at_period_end BIGINT NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS billing_usage (
  id      BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  account TEXT COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  meter   TEXT COLLATE "C" NOT NULL,
  pot     TEXT COLLATE "C" NOT NULL CHECK (pot IN ('included', 'pack')),
  amount  BIGINT NOT NULL CHECK (amount > 0),
  at      TEXT COLLATE "C" NOT NULL
);
CREATE INDEX IF NOT EXISTS billing_usage_by_account ON billing_usage (account, meter, pot, at);
CREATE TABLE IF NOT EXISTS discord_grants (
  discord_id TEXT COLLATE "C" PRIMARY KEY,
  account    TEXT COLLATE "C" NOT NULL,
  roles      TEXT COLLATE "C" NOT NULL,
  at         TEXT COLLATE "C" NOT NULL
);
CREATE TABLE IF NOT EXISTS patron_wall (
  account    TEXT COLLATE "C" PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
  shown      BIGINT NOT NULL DEFAULT 0,
  nickname   TEXT COLLATE "C",
  show_days  BIGINT NOT NULL DEFAULT 0,
  updated_at TEXT COLLATE "C" NOT NULL
);
CREATE TABLE IF NOT EXISTS share_signatures (
  id      TEXT COLLATE "C" PRIMARY KEY,
  share   TEXT COLLATE "C" NOT NULL,
  weapon  TEXT COLLATE "C" NOT NULL,
  account TEXT COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  at      TEXT COLLATE "C" NOT NULL,
  UNIQUE (account, share)
);
CREATE TABLE IF NOT EXISTS sync_devices (
  account  TEXT COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  id       TEXT COLLATE "C" NOT NULL,
  label    TEXT COLLATE "C" NOT NULL,
  seen_at  TEXT COLLATE "C" NOT NULL,
  round_at TEXT COLLATE "C",
  ok       BIGINT,
  reason   TEXT COLLATE "C",
  unsynced BIGINT,
  held     BIGINT,
  detail   TEXT COLLATE "C",
  PRIMARY KEY (account, id)
);
CREATE TABLE IF NOT EXISTS sync_entries (
  account      TEXT COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  id           TEXT COLLATE "C" NOT NULL,
  list         TEXT COLLATE "C" NOT NULL,
  body         TEXT COLLATE "C",
  updated_at   TEXT COLLATE "C" NOT NULL,
  synced_at    TEXT COLLATE "C" NOT NULL,
  version      BIGINT NOT NULL DEFAULT 0,
  deleted_body TEXT COLLATE "C",
  deleted_at   TEXT COLLATE "C",
  PRIMARY KEY (account, id)
);
CREATE INDEX IF NOT EXISTS sync_entries_by_synced ON sync_entries (account, synced_at, id);
