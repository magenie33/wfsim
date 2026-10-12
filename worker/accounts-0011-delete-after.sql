-- ONE-OFF: a deletion waits `DELETE_GRACE_SECONDS` (worker/accounts.js) before
-- it is carried out, and this is when. Applied once, before the worker that
-- reads it is live; a fresh database takes accounts.sql alone.
--
--   npx wrangler d1 execute wfsim-accounts --remote --file worker/accounts-0011-delete-after.sql
ALTER TABLE accounts ADD COLUMN delete_after TEXT;
