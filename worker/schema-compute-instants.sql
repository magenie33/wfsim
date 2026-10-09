-- ONE-OFF: the compute tables' times become ISO 8601 to the millisecond
-- (docs/NAMING.md ¡ì9), from epoch milliseconds and from whole seconds. In
-- place, so no hot table is rebuilt: a live column declared INTEGER holds the
-- text as text. Run once the worker that writes them as text is live, and again
-- a minute later for a straggler; it changes only what is not converted yet.
--
--   npx wrangler d1 execute wfsim --remote --file worker/schema-compute-instants.sql
UPDATE orders SET lease_until = strftime('%Y-%m-%dT%H:%M:%S.', lease_until / 1000, 'unixepoch') || printf('%03dZ', lease_until % 1000) WHERE typeof(lease_until) = 'integer';
UPDATE orders SET at = strftime('%Y-%m-%dT%H:%M:%S.', at / 1000, 'unixepoch') || printf('%03dZ', at % 1000) WHERE typeof(at) = 'integer';
UPDATE bot_inbox SET at = strftime('%Y-%m-%dT%H:%M:%S.', at / 1000, 'unixepoch') || printf('%03dZ', at % 1000) WHERE typeof(at) = 'integer';
UPDATE bot_inbox SET claimed_at = strftime('%Y-%m-%dT%H:%M:%S.', claimed_at / 1000, 'unixepoch') || printf('%03dZ', claimed_at % 1000) WHERE typeof(claimed_at) = 'integer';
UPDATE bot_inbox SET done_at = strftime('%Y-%m-%dT%H:%M:%S.', done_at / 1000, 'unixepoch') || printf('%03dZ', done_at % 1000) WHERE typeof(done_at) = 'integer';
UPDATE appraisals SET at = strftime('%Y-%m-%dT%H:%M:%S.', at / 1000, 'unixepoch') || printf('%03dZ', at % 1000) WHERE typeof(at) = 'integer';
UPDATE appraisals SET done_at = strftime('%Y-%m-%dT%H:%M:%S.', done_at / 1000, 'unixepoch') || printf('%03dZ', done_at % 1000) WHERE typeof(done_at) = 'integer';
UPDATE appraisals SET told_at = strftime('%Y-%m-%dT%H:%M:%S.', told_at / 1000, 'unixepoch') || printf('%03dZ', told_at % 1000) WHERE typeof(told_at) = 'integer';
UPDATE appraisals SET lease_until = strftime('%Y-%m-%dT%H:%M:%S.', lease_until / 1000, 'unixepoch') || printf('%03dZ', lease_until % 1000) WHERE typeof(lease_until) = 'integer';
UPDATE appraisals SET agreed_at = strftime('%Y-%m-%dT%H:%M:%S.', agreed_at / 1000, 'unixepoch') || printf('%03dZ', agreed_at % 1000) WHERE typeof(agreed_at) = 'integer';
UPDATE appraisals SET started_at = strftime('%Y-%m-%dT%H:%M:%S.', started_at / 1000, 'unixepoch') || printf('%03dZ', started_at % 1000) WHERE typeof(started_at) = 'integer';
UPDATE appraisals SET started_told = strftime('%Y-%m-%dT%H:%M:%S.', started_told / 1000, 'unixepoch') || printf('%03dZ', started_told % 1000) WHERE typeof(started_told) = 'integer';
UPDATE appraisal_results SET at = strftime('%Y-%m-%dT%H:%M:%S.', at / 1000, 'unixepoch') || printf('%03dZ', at % 1000) WHERE typeof(at) = 'integer';
UPDATE appraisal_results SET claimed_at = strftime('%Y-%m-%dT%H:%M:%S.', claimed_at / 1000, 'unixepoch') || printf('%03dZ', claimed_at % 1000) WHERE typeof(claimed_at) = 'integer';
UPDATE appraisal_results SET checked_at = strftime('%Y-%m-%dT%H:%M:%S.', checked_at / 1000, 'unixepoch') || printf('%03dZ', checked_at % 1000) WHERE typeof(checked_at) = 'integer';
UPDATE scores SET started_at = substr(started_at, 1, 19) || '.000Z' WHERE started_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z';
UPDATE scores SET finished_at = substr(finished_at, 1, 19) || '.000Z' WHERE finished_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z';
UPDATE verifiers SET last_at = substr(last_at, 1, 19) || '.000Z' WHERE last_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z';
