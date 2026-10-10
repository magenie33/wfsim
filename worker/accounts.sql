-- THE ACCOUNTS DATABASE — `wfsim-accounts`, apart from `wfsim` (docs/ACCOUNTS.md).
--
--   npx wrangler d1 create wfsim-accounts
--   npx wrangler d1 execute wfsim-accounts --remote --file worker/accounts.sql
--
-- APART, because D1 restores a whole database to a point in time, and a restore
-- of one must not roll the other back; and because nothing in here may ever
-- reach the public library backup, which reads `wfsim`.

-- A PERSON: when the account began, and the name it goes by on the site.
-- Everything that says who they are elsewhere is a slot below.
--
-- `username` is the site-wide handle: lowercase letters, digits and `_`,
-- unique, born `user_` and six random characters, and changed by its owner at
-- will — the first change at once, each later one a day after the last
-- (`username_changed_at`). `display_name` is free text, unique to nobody, and
-- null shows the username. Anything signed by an account stores its id, so a
-- rename reaches everywhere at once.
CREATE TABLE IF NOT EXISTS accounts (
  id                  TEXT PRIMARY KEY,
  created_at          TEXT NOT NULL,
  username            TEXT NOT NULL,
  display_name        TEXT,
  username_changed_at TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS accounts_username ON accounts (username);

-- A NAME JUST GIVEN UP, held for its old owner until `held_until`, so nobody
-- can take it the moment it is dropped and pass as them. The old owner may
-- take it back meanwhile. Not tied to the account: a deleted account's name
-- is held the same.
CREATE TABLE IF NOT EXISTS username_holds (
  username   TEXT PRIMARY KEY,
  account    TEXT NOT NULL,
  held_until TEXT NOT NULL
);

-- THE FOUR WAYS IN, at most one of each per account. `subject` is the
-- provider's own id for the person — for `email`, the address, lowercased —
-- and (provider, subject) belongs to one account at most, which is what makes a
-- merge impossible to do by accident. `label` is only what the account page
-- shows the person: a name, or the address. `password_hash` is the email
-- slot's password (`worker/accounts.js` §passwords), null on a third-party
-- slot and on an email slot linked before passwords, until one is set.
CREATE TABLE IF NOT EXISTS identities (
  provider      TEXT NOT NULL CHECK (provider IN ('google', 'discord', 'github', 'email')),
  subject       TEXT NOT NULL,
  account       TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  label         TEXT NOT NULL,
  linked_at     TEXT NOT NULL,
  password_hash TEXT,
  PRIMARY KEY (provider, subject),
  UNIQUE (account, provider)
);

-- AN ACCOUNT LIVES WHILE A SLOT IS FILLED. Stated here rather than in the
-- worker, so every path that empties the last slot takes the account with it —
-- and, by cascade, its sessions and its documents.
CREATE TRIGGER IF NOT EXISTS identities_last_slot AFTER DELETE ON identities
WHEN NOT EXISTS (SELECT 1 FROM identities WHERE account = OLD.account)
BEGIN
  DELETE FROM accounts WHERE id = OLD.account;
END;

-- A SIGNED-IN BROWSER. The cookie carries a random token; this keeps its hash,
-- so a copy of the table signs nobody in.
CREATE TABLE IF NOT EXISTS sessions (
  token_hash TEXT PRIMARY KEY,
  account    TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  expires_at TEXT NOT NULL
);

-- A CODE MAILED AND NOT YET USED, one per address; its HMAC, never the code.
-- Mail goes out for three things only — `purpose` says which: `register` a new
-- account, `link` the address to the signed-in `account`, `reset` a password.
-- A password chosen before the code arrives waits here as its hash, never as
-- itself. Deleted when it is used.
CREATE TABLE IF NOT EXISTS email_codes (
  email         TEXT PRIMARY KEY,
  code_hash     TEXT NOT NULL,
  expires_at    TEXT NOT NULL,
  attempts      INTEGER NOT NULL DEFAULT 0,
  sent_at       TEXT NOT NULL,
  purpose       TEXT NOT NULL DEFAULT 'register' CHECK (purpose IN ('register', 'link', 'reset')),
  password_hash TEXT,
  account       TEXT
);

-- WRONG PASSWORDS AGAINST ONE ADDRESS, in a window that restarts when it
-- lapses. Enough of them close that address to passwords for the rest of the
-- window; a right one, or a reset, clears the row.
CREATE TABLE IF NOT EXISTS login_failures (
  email    TEXT PRIMARY KEY,
  failures INTEGER NOT NULL,
  first_at TEXT NOT NULL
);

-- AN AGENT'S KEY (docs/ACCOUNTS.md §"Agents"). An agent registers with no
-- account and gets a key at once; this keeps the key's hash, never the key.
-- Claiming it — a code mailed to an address an account holds, read back by the
-- agent — sets `account`, and the key then acts for that account in what the
-- account may do. `claim_*` is a claim in flight: the address, the code's
-- HMAC, when it lapses and how many wrong codes it has had. Deleting the
-- account deletes its agents' keys.
CREATE TABLE IF NOT EXISTS agent_keys (
  id               TEXT PRIMARY KEY,
  key_hash         TEXT NOT NULL UNIQUE,
  name             TEXT NOT NULL,
  created_at       TEXT NOT NULL,
  last_used_at     TEXT,
  account          TEXT REFERENCES accounts (id) ON DELETE CASCADE,
  claimed_at       TEXT,
  claim_email      TEXT,
  claim_code_hash  TEXT,
  claim_expires_at TEXT,
  claim_sent_at    TEXT,
  claim_attempts   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS agent_keys_by_account ON agent_keys (account);

-- A MACHINE THAT COMPUTES THE BOARD, claimed by the account signed in on it
-- (docs/BOARD.md §"Contribution"). `verifier` is the random id the browser
-- made for itself (`verifiers` in `wfsim`); a device has one owner, the last
-- account to claim it, and its work goes with it. Only its owner's name is
-- joined to it — never a submission. Deleting the account releases its devices.
CREATE TABLE IF NOT EXISTS devices (
  verifier   TEXT PRIMARY KEY,
  account    TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  claimed_at TEXT NOT NULL,
  -- What the owner calls it: the browser's own coarse guess ("Mac · Chrome") until renamed.
  label      TEXT
);
-- WHOSE A DEVICE'S WORK WAS, hour by hour (worker/contribution.js §"Spans"):
-- `verifier_hours` from `from_hour` (empty: its first) up to `until_hour`
-- (null: still theirs). A deleted account's spans keep their hours, unowned,
-- so the next claim cannot take them.
CREATE TABLE IF NOT EXISTS device_spans (
  verifier   TEXT NOT NULL,
  account    TEXT REFERENCES accounts (id) ON DELETE SET NULL,
  from_hour  TEXT NOT NULL,
  until_hour TEXT,
  PRIMARY KEY (verifier, from_hour)
);
CREATE INDEX IF NOT EXISTS device_spans_by_account ON device_spans (account);
CREATE INDEX IF NOT EXISTS devices_by_account ON devices (account);

-- WHETHER AN ACCOUNT AGREED TO SHOW ITS NAME ON THE CONTRIBUTION RANKING. An
-- account with a claimed device is on it anonymously; `named` 1 shows its name,
-- 0 is a no that stops the asking, and no row is not asked yet.
CREATE TABLE IF NOT EXISTS contribution_choice (
  account   TEXT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
  named     INTEGER NOT NULL,
  chosen_at TEXT NOT NULL
);
