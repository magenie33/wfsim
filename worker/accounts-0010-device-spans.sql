-- ONE-OFF: a device's work is the account's by span, every device claimed
-- today its owner's from its first hour, so every total stays what it was.
-- Applied once, before the worker that reads `device_spans` is live; a fresh
-- database takes accounts.sql alone.
--
--   npx wrangler d1 execute wfsim-accounts --remote --file worker/accounts-0010-device-spans.sql
CREATE TABLE IF NOT EXISTS device_spans (
  verifier   TEXT NOT NULL,
  account    TEXT REFERENCES accounts (id) ON DELETE SET NULL,
  from_hour  TEXT NOT NULL,
  until_hour TEXT,
  PRIMARY KEY (verifier, from_hour)
);
CREATE INDEX IF NOT EXISTS device_spans_by_account ON device_spans (account);
INSERT OR IGNORE INTO device_spans (verifier, account, from_hour) SELECT verifier, account, '' FROM devices;
