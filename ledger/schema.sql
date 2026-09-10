-- THE MONEY: ONE TABLE, EVERY CHANNEL, ONE ROW PER PAYMENT.
--
--   npx wrangler d1 execute wfsim --remote --file ledger/schema.sql
--
-- A MONTHLY PLEDGE IS ONE ROW PER MONTH, like everything else. There is no
-- subscription here, no supporter, no tier — a payment arrived, and that is the
-- whole of what this records.
--
-- ENTERED BY HAND, AND NOTHING READS IT. No endpoint serves a figure from these
-- tables and no webhook writes one: each channel reports differently enough
-- that an intake is a decision per channel, and one channel's webhook wired
-- straight in is an automation for a tenth of the money and a schema shaped
-- around that tenth. The page offers channels and publishes no total.

-- WHO A PAYMENT WAS FROM, when that is a PERSON rather than an account. One
-- human with a Bilibili account and a Ko-fi account is one row here and two
-- accounts on their payments, which is the only reason this table exists.
--
-- AN ACCOUNT IS NOT A TABLE. `(channel, donor)` on a payment already names one,
-- and a table of them would hold nothing a payment does not already carry.
CREATE TABLE IF NOT EXISTS donors (
  id           INTEGER PRIMARY KEY,
  -- WHAT THE THANKS LIST PRINTS — the name they chose. NULL means they were
  -- never asked, and the list falls back to whichever handle paid.
  display_name TEXT,
  -- Anything worth remembering about them, in prose, read by a person.
  note         TEXT
);

-- AMOUNTS ARE WRITTEN THE WAY THE CHANNEL SHOWS THEM: 10.00 and 6.72, in the
-- currency the row names. A currency is ISO 4217 and a real one — `CNY`,
-- `USD`, `EUR` — never a channel's own token: a Bilibili charge is the ￥10 the
-- supporter paid and the ￥6.72 that landed, and how the platform moved them is
-- not a fact about the money.
--
-- ROUND A SUM TO 2. A decimal is stored as a float, so a total over many rows
-- lands a fraction of a cent out and prints it. That is a display rule and not
-- an accounting one; every query below carries it.
CREATE TABLE IF NOT EXISTS donations (
  id            INTEGER PRIMARY KEY,
  -- BEIJING WALL CLOCK, ISO 8601 with the offset spelled out. Bilibili prints
  -- `2026-09-02 16:40:06`, which is `2026-09-02T16:40:06+08:00` here. ONE
  -- OFFSET IN THE COLUMN is what makes the accounting month
  -- `substr(paid_at, 1, 7)` and correct by construction; a row in another
  -- offset moves silently between two months, and the check stops one.
  paid_at       TEXT NOT NULL
                  CHECK (paid_at LIKE '____-__-__T__:__:__+08:00'),
  -- `bilibili`, `kofi`, `patreon`, `afdian` — the same spelling the page's
  -- `SUPPORT_CHANNELS` uses where the channel is on it.
  channel       TEXT NOT NULL,
  -- WHAT THE CHANNEL CALLS THEM, verbatim (`-卢卡斯的斯-`). Never tidied: it
  -- is how the row is found again in the channel's own list. With `channel` it
  -- is the ACCOUNT, which is what a payment actually arrives from.
  donor         TEXT NOT NULL,
  -- WHICH PERSON THAT ACCOUNT IS, and NULL until somebody says. A batch can be
  -- entered without deciding, and attributing it later is one UPDATE — the
  -- unattributed are one query, below. What it must never be is GUESSED: two
  -- accounts with the same handle are two people until they are known not to
  -- be, and a wrong link is a total that adds up perfectly and is wrong.
  donor_id      INTEGER REFERENCES donors(id),
  -- WHAT BOTH AMOUNTS BELOW ARE IN, and the key into `rates`.
  currency      TEXT NOT NULL REFERENCES rates(currency),
  -- WHAT THEY PAID, and WHAT LANDED after the channel's cut. The gap is the
  -- channel's, which is the number worth knowing per channel; the books read
  -- the net.
  gross_amount  REAL NOT NULL,
  net_amount    REAL NOT NULL,
  -- ONE PAYMENT, ONE ROW. Nothing is typed to say which payment this is, so
  -- this is what stops the same Bilibili list being entered twice.
  UNIQUE (channel, paid_at, donor)
);

CREATE INDEX IF NOT EXISTS donations_paid_at ON donations (paid_at);
CREATE INDEX IF NOT EXISTS donations_donor_id ON donations (donor_id);

-- WHAT ONE UNIT IS WORTH IN CNY, NOW. One row per currency, overwritten when
-- it moves — no history, so a total is what the money is worth TODAY rather
-- than what it was booked at. Changing a rate therefore restates every past
-- total in that currency, which is the point: the question this answers is
-- "what do I have", not "what did I have then".
CREATE TABLE IF NOT EXISTS rates (
  currency     TEXT PRIMARY KEY,
  -- 7.12 for a dollar — CNY for one unit, not its reciprocal.
  cny_per_unit REAL NOT NULL,
  -- WHEN IT WAS LAST SET, and WHERE IT CAME FROM: the bank that would settle
  -- it, or the feed it was read off. A rate with no source is a number nobody
  -- can check, and one with no date cannot be told from a stale one.
  updated_at   TEXT NOT NULL,
  source       TEXT NOT NULL
);

-- CNY IS THE UNIT THE BOOKS ARE IN, so its rate is 1 and stays 1.
INSERT OR IGNORE INTO rates (currency, cny_per_unit, updated_at, source)
VALUES ('CNY', 1.0, '2026-09-10', 'the accounting currency, by definition');

-- ENTERING A PAYMENT. Neither the rate nor the person is part of it:
--
--   INSERT INTO donations
--     (paid_at, channel, donor, currency, gross_amount, net_amount)
--   VALUES ('2026-09-02T16:40:06+08:00', 'bilibili', '-卢卡斯的斯-',
--           'CNY', 10.00, 6.72);
--
-- SAYING WHO AN ACCOUNT IS. The first line makes the person, the second points
-- every payment from that account at them — past and future both, since the
-- same statement run again picks up whatever has arrived since:
--
--   INSERT INTO donors (display_name) VALUES ('Lucas');
--   UPDATE donations SET donor_id = last_insert_rowid()
--    WHERE channel = 'bilibili' AND donor = '-卢卡斯的斯-';
--
-- A SECOND ACCOUNT OF THE SAME PERSON is that UPDATE again with their id, and
-- merging two people who turned out to be one is the same shape:
--
--   UPDATE donations SET donor_id = 3 WHERE donor_id = 7;
--   DELETE FROM donors WHERE id = 7;

-- UPDATING A RATE, which restates every total that reads it:
--
--   INSERT INTO rates (currency, cny_per_unit, updated_at, source)
--   VALUES ('USD', 7.12, '2026-09-10', 'BOC middle rate')
--   ON CONFLICT (currency) DO UPDATE SET
--     cny_per_unit = excluded.cny_per_unit,
--     updated_at   = excluded.updated_at,
--     source       = excluded.source;

-- READING IT BACK. Totals are not stored: a stored total is a second copy of
-- rows that already exist, and it falls behind the first payment entered late.
--
--   SELECT ROUND(SUM(d.net_amount * r.cny_per_unit), 2) AS net_cny
--     FROM donations d JOIN rates r ON r.currency = d.currency;
--
--   SELECT substr(d.paid_at, 1, 7) AS month, d.channel, COUNT(*) AS payments,
--          ROUND(SUM(d.net_amount * r.cny_per_unit), 2) AS net_cny
--     FROM donations d JOIN rates r ON r.currency = d.currency
--    GROUP BY month, d.channel ORDER BY month;
--
-- PER PERSON, with everyone still unattributed falling back to their account —
-- so the list is complete before the linking is:
--
--   SELECT COALESCE(p.display_name, d.donor) AS name,
--          COUNT(*) AS payments,
--          ROUND(SUM(d.net_amount * r.cny_per_unit), 2) AS net_cny
--     FROM donations d
--     JOIN rates r ON r.currency = d.currency
--     LEFT JOIN donors p ON p.id = d.donor_id
--    GROUP BY COALESCE(CAST(d.donor_id AS TEXT), d.channel || ':' || d.donor)
--    ORDER BY net_cny DESC;
--
-- WHICH ACCOUNTS ARE NOT ATTRIBUTED TO ANYONE YET:
--
--   SELECT DISTINCT channel, donor FROM donations WHERE donor_id IS NULL;
