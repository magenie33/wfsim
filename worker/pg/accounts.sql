-- THE ACCOUNTS (database `wfsim_accounts`, owned by the role `wfsim_accounts`;
-- docs/ACCOUNTS.md), apart from the library's `wfsim` so the library's role and
-- backups never reach personal data. The paid half's tables (wfsim-cloud)
-- live here too. Every text column is COLLATE "C": it compares byte by byte,
-- whatever the database's locale. Every whole number is BIGINT. Applied by its
-- owner:
--   sudo -u postgres psql -d wfsim_accounts -v ON_ERROR_STOP=1 -c 'SET ROLE wfsim_accounts' -f accounts.sql

-- A PERSON: when the account began, and the name it goes by on the site.
-- Everything that says who they are elsewhere is a slot below.
--
-- `username` is the site-wide handle: lowercase letters, digits and `_`,
-- unique, born `user_` and six random characters, and changed by its owner at
-- will — the first change at once, each later one a day after the last
-- (`username_changed_at`). `display_name` is free text, unique to nobody, and
-- null shows the username. Anything signed by an account stores its id, so a
-- rename reaches everywhere at once. `delete_after` is a deletion asked for,
-- carried out at that instant unless cancelled first.
CREATE TABLE IF NOT EXISTS entity_accounts (
  id                  TEXT COLLATE "C" PRIMARY KEY,
  created_at          TEXT COLLATE "C" NOT NULL,
  username            TEXT COLLATE "C",
  display_name        TEXT COLLATE "C",
  username_changed_at TEXT COLLATE "C",
  delete_after        TEXT COLLATE "C"
);
CREATE UNIQUE INDEX IF NOT EXISTS accounts_username ON entity_accounts (username);
ALTER TABLE entity_accounts ADD COLUMN IF NOT EXISTS profile_id TEXT COLLATE "C";
CREATE UNIQUE INDEX IF NOT EXISTS accounts_profile_id ON entity_accounts (profile_id);

-- A ten-minute browser claim for any volunteer client, without an account key.
CREATE TABLE IF NOT EXISTS state_device_claim_challenges (
  token_hash TEXT COLLATE "C" PRIMARY KEY,
  code_hash TEXT COLLATE "C" NOT NULL UNIQUE,
  verifier TEXT COLLATE "C" NOT NULL,
  label TEXT COLLATE "C" NOT NULL,
  expires_at TEXT COLLATE "C" NOT NULL,
  account TEXT COLLATE "C" REFERENCES entity_accounts (id) ON DELETE CASCADE,
  claimed_at TEXT COLLATE "C"
);
CREATE INDEX IF NOT EXISTS device_links_expires ON state_device_claim_challenges (expires_at);

-- Recommendation targets are library identities; personal votes stay in this database.
CREATE TABLE IF NOT EXISTS state_recommendation_target_snapshots (
  identity TEXT COLLATE "C" PRIMARY KEY,
  weapon TEXT COLLATE "C" NOT NULL,
  record TEXT COLLATE "C" NOT NULL,
  ruler TEXT COLLATE "C" NOT NULL,
  mode TEXT COLLATE "C" NOT NULL
);
CREATE INDEX IF NOT EXISTS recommendation_targets_weapon ON state_recommendation_target_snapshots (weapon);
CREATE TABLE IF NOT EXISTS record_recommendation_votes (
  account TEXT COLLATE "C" NOT NULL REFERENCES entity_accounts (id) ON DELETE CASCADE,
  identity TEXT COLLATE "C" NOT NULL REFERENCES state_recommendation_target_snapshots (identity),
  day DATE NOT NULL,
  PRIMARY KEY (account, identity, day)
);
CREATE INDEX IF NOT EXISTS recommendations_target_day ON record_recommendation_votes (identity, day);
CREATE INDEX IF NOT EXISTS recommendations_account_day ON record_recommendation_votes (account, day, identity);
CREATE OR REPLACE FUNCTION recommendation_day() RETURNS date LANGUAGE sql STABLE AS $$
  SELECT (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')::date
$$;

-- A DISPLAY NAME HELD BY HAND (worker/names.js `hiddenWhy`): hidden while the
-- account still shows exactly `display_name`, and `reason` (`NAME_REASONS`) is
-- what its owner is told. Written by the owner's tool in the paid half, and
-- deleted when the account sets a new name (`profile`).
CREATE TABLE IF NOT EXISTS state_account_name_holds (
  account      TEXT COLLATE "C" PRIMARY KEY REFERENCES entity_accounts (id) ON DELETE CASCADE,
  display_name TEXT COLLATE "C" NOT NULL,
  reason       TEXT COLLATE "C" NOT NULL,
  held_at      TEXT COLLATE "C" NOT NULL
);

-- THE FOUR WAYS IN, at most one of each per account. `subject` is the
-- provider's own id for the person — for `email`, the address, lowercased —
-- and (provider, subject) belongs to one account at most, which is what makes a
-- merge impossible to do by accident. `label` is only what the account page
-- shows the person: a name, or the address. `password_hash` is the email
-- slot's password (`worker/accounts.js` §passwords), null on a third-party
-- slot and on an email slot that has none set.
CREATE TABLE IF NOT EXISTS relation_account_login_identities (
  provider      TEXT COLLATE "C" NOT NULL CHECK (provider IN ('google', 'discord', 'github', 'email')),
  subject       TEXT COLLATE "C" NOT NULL,
  account       TEXT COLLATE "C" NOT NULL REFERENCES entity_accounts (id) ON DELETE CASCADE,
  label         TEXT COLLATE "C" NOT NULL,
  linked_at     TEXT COLLATE "C" NOT NULL,
  password_hash TEXT COLLATE "C",
  PRIMARY KEY (provider, subject),
  UNIQUE (account, provider)
);

-- AN ACCOUNT LIVES WHILE A SLOT IS FILLED. Stated here rather than in the
-- worker, so every path that empties the last slot takes the account with it —
-- and, by cascade, its sessions and its data.
CREATE OR REPLACE FUNCTION identities_last_slot() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM relation_account_login_identities WHERE account = OLD.account) THEN
    DELETE FROM entity_accounts WHERE id = OLD.account;
  END IF;
  RETURN NULL;
END $$;
DROP TRIGGER IF EXISTS identities_last_slot ON relation_account_login_identities;
CREATE TRIGGER identities_last_slot AFTER DELETE ON relation_account_login_identities
  FOR EACH ROW EXECUTE FUNCTION identities_last_slot();

-- A SIGNED-IN BROWSER. The cookie carries a random token; this keeps its hash,
-- so a copy of the table signs nobody in.
CREATE TABLE IF NOT EXISTS state_login_sessions (
  token_hash TEXT COLLATE "C" PRIMARY KEY,
  account    TEXT COLLATE "C" NOT NULL REFERENCES entity_accounts (id) ON DELETE CASCADE,
  expires_at TEXT COLLATE "C" NOT NULL
);
CREATE INDEX IF NOT EXISTS sessions_by_account ON state_login_sessions (account);

-- A CODE MAILED AND NOT YET USED, one per address; its HMAC, never the code.
-- Mail goes out for three things only — `purpose` says which: `register` a new
-- account, `link` the address to the signed-in `account`, `reset` a password.
-- A password chosen before the code arrives waits here as its hash, never as
-- itself. Deleted when it is used.
CREATE TABLE IF NOT EXISTS state_email_verification_challenges (
  email         TEXT COLLATE "C" PRIMARY KEY,
  code_hash     TEXT COLLATE "C" NOT NULL,
  expires_at    TEXT COLLATE "C" NOT NULL,
  attempts      BIGINT NOT NULL DEFAULT 0,
  sent_at       TEXT COLLATE "C" NOT NULL,
  purpose       TEXT COLLATE "C" NOT NULL DEFAULT 'register' CHECK (purpose IN ('register', 'link', 'reset')),
  password_hash TEXT COLLATE "C",
  account       TEXT COLLATE "C"
);

-- WRONG PASSWORDS AGAINST ONE ADDRESS, in a window that restarts when it
-- lapses. Enough of them close that address to passwords for the rest of the
-- window; a right one, or a reset, clears the row.
CREATE TABLE IF NOT EXISTS state_login_rate_limits (
  email    TEXT COLLATE "C" PRIMARY KEY,
  failures BIGINT NOT NULL,
  first_at TEXT COLLATE "C" NOT NULL
);

-- A NAME JUST GIVEN UP, held for its old owner until `held_until`, so nobody
-- can take it the moment it is dropped and pass as them. The old owner may
-- take it back meanwhile. Not tied to the account: a deleted account's name
-- is held the same.
CREATE TABLE IF NOT EXISTS state_username_reservations (
  username   TEXT COLLATE "C" PRIMARY KEY,
  account    TEXT COLLATE "C" NOT NULL,
  held_until TEXT COLLATE "C" NOT NULL
);

-- AN AGENT'S KEY (docs/ACCOUNTS.md §"Agents"). An agent registers with no
-- account and gets a key at once; this keeps the key's hash, never the key.
-- Claiming it — a code mailed to an address an account holds, read back by the
-- agent — sets `account`, and the key then acts for that account in what the
-- account may do. `claim_*` is a claim in flight: the address, the code's
-- HMAC, when it lapses and how many wrong codes it has had. Deleting the
-- account deletes its agents' keys.
CREATE TABLE IF NOT EXISTS entity_agent_keys (
  id               TEXT COLLATE "C" PRIMARY KEY,
  key_hash         TEXT COLLATE "C" NOT NULL UNIQUE,
  name             TEXT COLLATE "C" NOT NULL,
  created_at       TEXT COLLATE "C" NOT NULL,
  last_used_at     TEXT COLLATE "C",
  account          TEXT COLLATE "C" REFERENCES entity_accounts (id) ON DELETE CASCADE,
  claimed_at       TEXT COLLATE "C",
  claim_email      TEXT COLLATE "C",
  claim_code_hash  TEXT COLLATE "C",
  claim_expires_at TEXT COLLATE "C",
  claim_sent_at    TEXT COLLATE "C",
  claim_attempts   BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS agent_keys_by_account ON entity_agent_keys (account);

-- A MACHINE THAT COMPUTES THE BOARD, claimed by the account signed in on it
-- (docs/BOARD.md §"Contribution"). `verifier` is the random id the browser
-- made for itself (`verifiers` in `wfsim`); a device has one owner, the last
-- account to claim it, and its work goes with it. Only its owner's name is
-- joined to it — never a submission. Deleting the account releases its devices.
CREATE TABLE IF NOT EXISTS relation_account_compute_devices (
  verifier   TEXT COLLATE "C" PRIMARY KEY,
  account    TEXT COLLATE "C" NOT NULL REFERENCES entity_accounts (id) ON DELETE CASCADE,
  claimed_at TEXT COLLATE "C" NOT NULL,
  -- What the owner calls it: the browser's own coarse guess ("Mac · Chrome") until renamed.
  label      TEXT COLLATE "C"
);
CREATE INDEX IF NOT EXISTS devices_by_account ON relation_account_compute_devices (account);

-- WHOSE A DEVICE'S WORK WAS, hour by hour (worker/contribution.js §"Spans"):
-- `verifier_hours` from `from_hour` (empty: its first) up to `until_hour`
-- (null: still theirs). A deleted account's spans keep their hours, unowned,
-- so the next claim cannot take them.
CREATE TABLE IF NOT EXISTS record_compute_device_ownership_intervals (
  verifier   TEXT COLLATE "C" NOT NULL,
  account    TEXT COLLATE "C" REFERENCES entity_accounts (id) ON DELETE SET NULL,
  from_hour  TEXT COLLATE "C" NOT NULL,
  until_hour TEXT COLLATE "C",
  PRIMARY KEY (verifier, from_hour)
);
CREATE INDEX IF NOT EXISTS device_spans_by_account ON record_compute_device_ownership_intervals (account);

-- WHETHER AN ACCOUNT AGREED TO SHOW ITS NAME ON THE CONTRIBUTION RANKING. An
-- account with a claimed device is on it anonymously; `named` 1 shows its name,
-- 0 is a no that stops the asking, and no row is not asked yet.
CREATE TABLE IF NOT EXISTS state_account_public_attribution (
  account   TEXT COLLATE "C" PRIMARY KEY REFERENCES entity_accounts (id) ON DELETE CASCADE,
  named     BIGINT NOT NULL,
  chosen_at TEXT COLLATE "C" NOT NULL
);
ALTER TABLE state_account_public_attribution ADD COLUMN IF NOT EXISTS label TEXT COLLATE "C" NOT NULL DEFAULT 'none';

-- MERIT (docs/BOARD.md §"Merit"): minted from an account's contribution points,
-- spent on what community compute can give. `balance` never decays; `points_seen`
-- is the points it was minted from so far, so a point mints once or never;
-- `minted_today` counts toward the day's cap (`day`, UTC). `auto_priority` 1
-- spends it on the account's own submissions without asking.
CREATE TABLE IF NOT EXISTS state_account_priority_credit (
  account       TEXT COLLATE "C" PRIMARY KEY REFERENCES entity_accounts (id) ON DELETE CASCADE,
  balance       BIGINT NOT NULL DEFAULT 0,
  points_seen   BIGINT NOT NULL DEFAULT 0,
  day           TEXT COLLATE "C" NOT NULL,
  minted_today  BIGINT NOT NULL DEFAULT 0,
  last_mint     BIGINT NOT NULL DEFAULT 0,
  auto_priority BIGINT NOT NULL DEFAULT 0,
  since         TEXT COLLATE "C" NOT NULL,
  -- WHEN ITS FIRST GRANT WAS MADE: a row the toggle made first is granted still.
  granted_at    TEXT COLLATE "C",
  -- THE ROWS ITS MERIT MOVED in the UTC hour `prio_hour` (dial `merit_rows_per_hour`).
  prio_hour     TEXT COLLATE "C",
  prio_rows     BIGINT NOT NULL DEFAULT 0
);
-- …EVERY MOVEMENT: `grant` (the first, from all points before), `mint`, and what
-- spending writes. An account sees its own rows; the public sees sums by kind.
CREATE TABLE IF NOT EXISTS record_priority_credit_entries (
  id      BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  account TEXT COLLATE "C" NOT NULL REFERENCES entity_accounts (id) ON DELETE CASCADE,
  at      TEXT COLLATE "C" NOT NULL,
  kind    TEXT COLLATE "C" NOT NULL,
  amount  BIGINT NOT NULL,
  balance BIGINT NOT NULL,
  note    TEXT COLLATE "C"
);
CREATE INDEX IF NOT EXISTS merit_ledger_by_account ON record_priority_credit_entries (account, id);
-- …ONE REFUND A HOLD, however often the settling runs.
CREATE UNIQUE INDEX IF NOT EXISTS merit_ledger_refund_once ON record_priority_credit_entries (note) WHERE kind = 'refund';
-- WHOSE CODE IS WHOSE (docs/BOARD.md §"Who asked"): kept while the account's
-- priority switch is on and deleted with it, read only to spend its merit on its
-- rows. The board's database holds the code and never this.
CREATE TABLE IF NOT EXISTS relation_account_priority_codes (
  code    TEXT COLLATE "C" PRIMARY KEY,
  account TEXT COLLATE "C" NOT NULL REFERENCES entity_accounts (id) ON DELETE CASCADE
);
-- WHAT A MINT MAY ADD: the new points ($1 points, $2 seen), within what the day's
-- cap leaves ($3 cap, $4 minted today) and the balance cap leaves ($5, $6 balance).
CREATE OR REPLACE FUNCTION merit_room(bigint, bigint, bigint, bigint, bigint, bigint) RETURNS bigint
LANGUAGE sql IMMUTABLE AS $$ SELECT GREATEST(0, LEAST($1 - $2, $3 - $4, $5 - $6)) $$;

-- THE PAID HALF'S TABLES (wfsim-cloud), read and written there alone.
CREATE TABLE IF NOT EXISTS relation_account_titles (
  account    TEXT COLLATE "C" NOT NULL REFERENCES entity_accounts (id) ON DELETE CASCADE,
  title      TEXT COLLATE "C" NOT NULL,
  granted_at TEXT COLLATE "C" NOT NULL,
  note       TEXT COLLATE "C",
  PRIMARY KEY (account, title)
);
CREATE TABLE IF NOT EXISTS record_checkout_attempts (
  who TEXT COLLATE "C" NOT NULL,
  ip  TEXT COLLATE "C",
  at  TEXT COLLATE "C" NOT NULL
);
CREATE INDEX IF NOT EXISTS billing_attempts_by_at ON record_checkout_attempts (at);
CREATE TABLE IF NOT EXISTS state_payment_refund_cooldowns (
  card TEXT COLLATE "C" PRIMARY KEY,
  at   TEXT COLLATE "C" NOT NULL
);
CREATE TABLE IF NOT EXISTS entity_redemption_codes (
  id          TEXT COLLATE "C" PRIMARY KEY,
  price       TEXT COLLATE "C" NOT NULL,
  offer       TEXT COLLATE "C" NOT NULL,
  months      BIGINT NOT NULL CHECK (months > 0),
  amount      BIGINT NOT NULL CHECK (amount > 0),
  currency    TEXT COLLATE "C" NOT NULL,
  batch       TEXT COLLATE "C" NOT NULL,
  created_at  TEXT COLLATE "C" NOT NULL,
  redeemed_by TEXT COLLATE "C" REFERENCES entity_accounts (id) ON DELETE SET NULL,
  redeemed_at TEXT COLLATE "C",
  revoked_at  TEXT COLLATE "C"
);
CREATE INDEX IF NOT EXISTS billing_codes_by_batch ON entity_redemption_codes (batch);
CREATE TABLE IF NOT EXISTS relation_account_payment_customers (
  account   TEXT COLLATE "C" PRIMARY KEY REFERENCES entity_accounts (id) ON DELETE CASCADE,
  customer  TEXT COLLATE "C" NOT NULL UNIQUE,
  synced_at TEXT COLLATE "C"
);
CREATE TABLE IF NOT EXISTS state_payment_refund_jobs (
  account TEXT COLLATE "C" NOT NULL REFERENCES entity_accounts (id) ON DELETE CASCADE,
  channel TEXT COLLATE "C" NOT NULL,
  ref     TEXT COLLATE "C" NOT NULL,
  amount  BIGINT NOT NULL CHECK (amount > 0),
  at      TEXT COLLATE "C" NOT NULL,
  done_at TEXT COLLATE "C",
  card    TEXT COLLATE "C",
  PRIMARY KEY (account, channel, ref)
);
CREATE TABLE IF NOT EXISTS record_payment_event_receipts (
  id   TEXT COLLATE "C" PRIMARY KEY,
  type TEXT COLLATE "C" NOT NULL,
  at   TEXT COLLATE "C" NOT NULL
);
CREATE TABLE IF NOT EXISTS relation_account_offer_grants (
  id        BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  account   TEXT COLLATE "C" NOT NULL REFERENCES entity_accounts (id) ON DELETE CASCADE,
  offer     TEXT COLLATE "C" NOT NULL,
  starts_at TEXT COLLATE "C" NOT NULL,
  ends_at   TEXT COLLATE "C" NOT NULL,
  note      TEXT COLLATE "C"
);
CREATE TABLE IF NOT EXISTS entity_payment_orders (
  id           TEXT COLLATE "C" PRIMARY KEY,
  account      TEXT COLLATE "C" NOT NULL REFERENCES entity_accounts (id) ON DELETE CASCADE,
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
CREATE INDEX IF NOT EXISTS billing_orders_by_account ON entity_payment_orders (account);
CREATE UNIQUE INDEX IF NOT EXISTS billing_orders_by_ref ON entity_payment_orders (channel, channel_ref);
CREATE TABLE IF NOT EXISTS state_external_payment_periods (
  invoice   TEXT COLLATE "C" NOT NULL,
  line      BIGINT NOT NULL,
  account   TEXT COLLATE "C" NOT NULL REFERENCES entity_accounts (id) ON DELETE CASCADE,
  offer     TEXT COLLATE "C" NOT NULL,
  price     TEXT COLLATE "C" NOT NULL,
  starts_at TEXT COLLATE "C" NOT NULL,
  ends_at   TEXT COLLATE "C" NOT NULL,
  status    TEXT COLLATE "C" NOT NULL CHECK (status IN ('paid', 'refunded', 'disputed')),
  PRIMARY KEY (invoice, line)
);
CREATE INDEX IF NOT EXISTS billing_periods_by_account ON state_external_payment_periods (account);
CREATE TABLE IF NOT EXISTS state_external_subscriptions (
  id                   TEXT COLLATE "C" PRIMARY KEY,
  account              TEXT COLLATE "C" NOT NULL REFERENCES entity_accounts (id) ON DELETE CASCADE,
  price                TEXT COLLATE "C" NOT NULL,
  status               TEXT COLLATE "C" NOT NULL,
  period_start         TEXT COLLATE "C" NOT NULL,
  period_end           TEXT COLLATE "C" NOT NULL,
  cancel_at_period_end BIGINT NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS record_meter_consumption (
  id      BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  account TEXT COLLATE "C" NOT NULL REFERENCES entity_accounts (id) ON DELETE CASCADE,
  meter   TEXT COLLATE "C" NOT NULL,
  pot     TEXT COLLATE "C" NOT NULL CHECK (pot IN ('included', 'pack')),
  amount  BIGINT NOT NULL CHECK (amount > 0),
  at      TEXT COLLATE "C" NOT NULL
);
CREATE INDEX IF NOT EXISTS billing_usage_by_account ON record_meter_consumption (account, meter, pot, at);
CREATE TABLE IF NOT EXISTS state_discord_role_sync (
  discord_id TEXT COLLATE "C" PRIMARY KEY,
  account    TEXT COLLATE "C" NOT NULL,
  roles      TEXT COLLATE "C" NOT NULL,
  at         TEXT COLLATE "C" NOT NULL
);
CREATE TABLE IF NOT EXISTS state_supporter_visibility (
  account    TEXT COLLATE "C" PRIMARY KEY REFERENCES entity_accounts (id) ON DELETE CASCADE,
  shown      BIGINT NOT NULL DEFAULT 0,
  nickname   TEXT COLLATE "C",
  show_days  BIGINT NOT NULL DEFAULT 0,
  updated_at TEXT COLLATE "C" NOT NULL
);
CREATE TABLE IF NOT EXISTS entity_share_signatures (
  id      TEXT COLLATE "C" PRIMARY KEY,
  share   TEXT COLLATE "C" NOT NULL,
  weapon  TEXT COLLATE "C" NOT NULL,
  account TEXT COLLATE "C" NOT NULL REFERENCES entity_accounts (id) ON DELETE CASCADE,
  at      TEXT COLLATE "C" NOT NULL,
  UNIQUE (account, share)
);
CREATE TABLE IF NOT EXISTS entity_sync_clients (
  account  TEXT COLLATE "C" NOT NULL REFERENCES entity_accounts (id) ON DELETE CASCADE,
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
CREATE TABLE IF NOT EXISTS entity_user_documents (
  account      TEXT COLLATE "C" NOT NULL REFERENCES entity_accounts (id) ON DELETE CASCADE,
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
CREATE INDEX IF NOT EXISTS sync_entries_by_synced ON entity_user_documents (account, synced_at, id);

-- WHAT THE SITE TELLS AN ACCOUNT, or every reader when `account` is NULL
-- (worker/notices.js, docs/ACCOUNTS.md §"Notices"). One way: the site writes,
-- an account reads. A kind the page words itself carries `params` (JSON); one
-- the owner wrote carries its own `text_en`/`text_zh`. `dedupe_key` makes a
-- writer that runs twice write once. Kept 180 days, a broadcast until it expires.
CREATE TABLE IF NOT EXISTS entity_notices (
  id         BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  account    TEXT COLLATE "C" REFERENCES entity_accounts (id) ON DELETE CASCADE,
  kind       TEXT COLLATE "C" NOT NULL,
  text_en    TEXT COLLATE "C",
  text_zh    TEXT COLLATE "C",
  params     TEXT COLLATE "C",
  link       TEXT COLLATE "C",
  dedupe_key TEXT COLLATE "C",
  created_at TEXT COLLATE "C" NOT NULL,
  expires_at TEXT COLLATE "C"
);
CREATE INDEX IF NOT EXISTS notices_by_account ON entity_notices (account, created_at);
CREATE UNIQUE INDEX IF NOT EXISTS notices_once ON entity_notices (account, dedupe_key) WHERE dedupe_key IS NOT NULL;
-- HOW FAR AN ACCOUNT HAS READ: every notice made before `read_at` is read.
-- Opening the bell reads all of it, so one instant is the whole read state.
CREATE TABLE IF NOT EXISTS state_account_notice_reads (
  account TEXT COLLATE "C" PRIMARY KEY REFERENCES entity_accounts (id) ON DELETE CASCADE,
  read_at TEXT COLLATE "C" NOT NULL
);

-- WHAT A READER TOLD THE OWNER from the page (worker/feedback.js, docs/
-- ACCOUNTS.md §"Feedback"): their words as they wrote them and where they were
-- — the route, the build's share link, the release — with the count of game
-- screenshots kept in the private bucket under `feedback/<id>/<n>.jpg`. Read by
-- the owner and the account alone; kept with the account and deleted with it.
CREATE TABLE IF NOT EXISTS entity_feedback (
  id         BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  account    TEXT COLLATE "C" NOT NULL REFERENCES entity_accounts (id) ON DELETE CASCADE,
  created_at TEXT COLLATE "C" NOT NULL,
  text       TEXT COLLATE "C" NOT NULL,
  route      TEXT COLLATE "C",
  share      TEXT COLLATE "C",
  release    TEXT COLLATE "C",
  lang       TEXT COLLATE "C",
  images     BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS feedback_by_account ON entity_feedback (account, created_at);
