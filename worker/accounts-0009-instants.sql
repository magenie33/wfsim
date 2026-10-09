-- ONE-OFF: a device's claim and a ranking answer are instants (docs/NAMING.md
-- §9), stored before as the day alone. An old row becomes the later of that
-- day's start and the account's creation, the earliest it can have been.
-- Applied once, with the worker that writes them whole; a fresh database takes
-- accounts.sql alone.
--
--   npx wrangler d1 execute wfsim-accounts --remote --file worker/accounts-0009-instants.sql
UPDATE devices SET claimed_at = MAX(claimed_at || 'T00:00:00.000Z',
  (SELECT created_at FROM accounts WHERE accounts.id = devices.account)) WHERE length(claimed_at) = 10;
UPDATE contribution_choice SET chosen_at = MAX(chosen_at || 'T00:00:00.000Z',
  (SELECT created_at FROM accounts WHERE accounts.id = contribution_choice.account)) WHERE length(chosen_at) = 10;
