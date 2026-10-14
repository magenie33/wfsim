-- THE FACT FACTORY'S CHOICES IN THE DATABASE (docs/BOARD.md §"The sync in the
-- database"): which question a witness may take and in what order, who is
-- asking, a question restated from its demands, and protocol 8's whole sync as
-- one call (`board_sync`). worker/tasks.js and worker/sync.js call these and
-- state none of it again, so the JS path and the procedure hand out by one rule.
-- Applied after schema.sql and library.sql, by the role that owns the tables,
-- and again whenever it changes — every statement replaces what it defines:
--
--   psql -d wfsim -f worker/pg/sync.sql
--
-- Instants are ISO-ms text (schema.sql); arguments are positional, as a column
-- of the same name would shadow a named one inside a SQL function.

-- A LEASE ANOTHER MAY TAKE: none, run out, or held past the ceiling (dial
-- `row_ceiling_ms`, here in seconds) at instant $3.
CREATE OR REPLACE FUNCTION factory_takeable(text, text, text, bigint) RETURNS boolean
LANGUAGE sql STABLE AS $$
  SELECT $1 IS NULL OR $1 < $3
    OR $2 < to_char(($3::timestamptz AT TIME ZONE 'UTC') - make_interval(secs => $4), 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
$$;

-- …AND TAKING ONE keeps the holder computing longest as the one before, its
-- answer still counted: one still beating there stays, else the one taken from
-- if it still beats; a lease run out keeps nobody. Arguments: the row's
-- lease_prev, leased_to_prev, lease_prev_until, lease, leased_to, lease_until,
-- and the instant; set as `(lease_prev, leased_to_prev, lease_prev_until)`.
CREATE OR REPLACE FUNCTION factory_supersede(text, text, text, text, text, text, text)
RETURNS TABLE (lease_prev text, leased_to_prev text, lease_prev_until text)
LANGUAGE sql IMMUTABLE AS $$
  SELECT CASE WHEN $3 >= $7 THEN $1 WHEN $6 >= $7 THEN $4 END,
         CASE WHEN $3 >= $7 THEN $2 WHEN $6 >= $7 THEN $5 END,
         CASE WHEN $3 >= $7 THEN $3 WHEN $6 >= $7 THEN $6 END
$$;

-- WHAT A WITNESS MAY ANSWER, whatever the state (the caller names it, so the
-- order's index is used): a question leased to nobody it must wait for, with
-- the cores a first answer needs at its age, not answered on this engine by any
-- of its owner's devices nor its network, not refused by its engine, due before
-- $8 when given, and not just queued for the settler (dial `settle_grace_ms`).
-- $1 at, $2 official, $3 lanes, $4 engine, $5 siblings (json), $6 owner,
-- $7 net, $8 before, $9 row ceiling and $10 settle grace in seconds.
CREATE OR REPLACE FUNCTION factory_eligible(text, bigint, bigint, text, text, text, text, text, bigint, bigint)
RETURNS SETOF questions LANGUAGE sql STABLE AS $$
  SELECT q.* FROM questions q
  WHERE factory_takeable(q.lease_until, q.leased_at, $1, $9)
    AND ($2 = 1 OR q.lanes IS NULL OR EXISTS (SELECT 1 FROM answers x WHERE x.question = q.id)
         OR $3 >= (SELECT min((t ->> 'lanes')::numeric) FROM jsonb_array_elements(q.lanes::jsonb) t
                   WHERE (t ->> 'after_ms')::numeric <= extract(epoch FROM ($1::timestamptz - q.opened_at::timestamptz)) * 1000))
    AND NOT EXISTS (SELECT 1 FROM answers a WHERE a.question = q.id AND a.engine = $4
          AND (a.device IN (SELECT value FROM jsonb_array_elements_text($5::jsonb)) OR a.owner = $6 OR ($7 <> '' AND a.net = $7)))
    AND NOT EXISTS (SELECT 1 FROM engine_refusals r WHERE r.question = q.id AND r.engine = $4
          AND r.device IN (SELECT value FROM jsonb_array_elements_text($5::jsonb)))
    AND ($8 IS NULL OR q.due_at < $8)
    AND NOT EXISTS (SELECT 1 FROM settle_queue s WHERE s.question = q.id
          AND s.queued_at > to_char(($1::timestamptz AT TIME ZONE 'UTC') - make_interval(secs => $10), 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'))
$$;

-- THE HEAD OF A WITNESS'S ORDER (docs/BOARD.md §"Fair share between producers"):
-- for a volunteer each producer's head, one index probe each, and of those the
-- waited-on first, then the producer that has used least; where no producer's
-- row finds one the plain order does. The official machines ($2 = 1) take what
-- is past its deadline and what only they settle ($11, json), in the plain order.
CREATE OR REPLACE FUNCTION factory_head(text, bigint, bigint, text, text, text, text, text, bigint, bigint, text)
RETURNS TABLE (id text, verb text, request text, state text, due_at text, waits bigint, producer text)
LANGUAGE sql STABLE AS $$
  SELECT z.id, z.verb, z.request, z.state, z.due_at, z.waits, z.producer FROM (
    (SELECT 0 AS fallback, x.* FROM producer_use u CROSS JOIN LATERAL (
       SELECT q.id, q.verb, q.request, q.state, q.due_at, q.waits, q.producer
       FROM factory_eligible($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) q
       WHERE q.state = 'open' AND q.producer = u.producer ORDER BY q.waits DESC, q.due_at, q.id LIMIT 1) x
     WHERE $2 = 0 ORDER BY x.waits DESC, u.used, x.due_at, x.id LIMIT 1)
    UNION ALL
    (SELECT 1, q.id, q.verb, q.request, q.state, q.due_at, q.waits, q.producer
     FROM factory_eligible($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) q
     WHERE $2 = 0 AND q.state = 'open' ORDER BY q.waits DESC, q.due_at, q.id LIMIT 1)
    UNION ALL
    (SELECT 2, q.id, q.verb, q.request, q.state, q.due_at, q.waits, q.producer
     FROM factory_eligible($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) q
     WHERE $2 = 1 AND ((q.state = 'open' AND q.due_at < $1) OR q.state IN (SELECT jsonb_array_elements_text($11::jsonb)))
     ORDER BY q.waits DESC, q.due_at, q.id LIMIT 1)
  ) z ORDER BY z.fallback LIMIT 1
$$;

-- A PACK'S ROWS, locked: up to $14 open questions nobody waits on, of a packable
-- verb ($10, json), in the order. AN OLDER ENGINE STILL ACCEPTED is handed only
-- rows of a weapon outside $11 (json); ONE WHOSE CODE MOVED only a row a served
-- answer ($12) of another owner and network leads; $13 keeps to one producer.
-- The pick skips rows another pick holds (two asks at once deadlocked), and the
-- row itself is checked again once locked, as another may have taken it since.
-- $1 at, $2 lanes, $3 engine, $4 siblings, $5 owner, $6 net, $7 before, $8, $9 as above.
CREATE OR REPLACE FUNCTION factory_take(text, bigint, text, text, text, text, text, bigint, bigint, text, text, text, text, bigint)
RETURNS SETOF text LANGUAGE sql AS $$
  SELECT q.id FROM factory_eligible($1, 0, $2, $3, $4, $5, $6, $7, $8, $9) e JOIN questions q ON q.id = e.id
  WHERE e.state = 'open' AND e.waits = 0 AND e.verb IN (SELECT jsonb_array_elements_text($10::jsonb))
    AND ($11 IS NULL OR (e.request::jsonb #>> '{record,weapon}') NOT IN (SELECT jsonb_array_elements_text($11::jsonb)))
    AND ($12 IS NULL OR EXISTS (SELECT 1 FROM answers s WHERE s.question = e.id AND s.engine = $12 AND s.void = 0
         AND s.owner <> $5 AND ($6 = '' OR s.net <> $6)))
    AND ($13 IS NULL OR e.producer = $13)
    AND q.state = 'open' AND q.waits = 0 AND factory_takeable(q.lease_until, q.leased_at, $1, $8)
  ORDER BY e.waits DESC, e.due_at, e.id LIMIT $14 FOR UPDATE OF q SKIP LOCKED
$$;

-- …LEASED TOGETHER as $15 until $16 to device $17 — unless it holds $18 packs
-- already — and each producer charged its rows at its mean fight over its weight.
CREATE OR REPLACE FUNCTION factory_lease_pack(text, bigint, text, text, text, text, text, bigint, bigint, text, text, text, text, bigint,
  text, text, text, bigint)
RETURNS TABLE (id text, verb text, request text, due_at text, producer text) LANGUAGE sql AS $$
  WITH taken AS (UPDATE questions q SET (lease_prev, leased_to_prev, lease_prev_until) = (SELECT * FROM factory_supersede(q.lease_prev,
        q.leased_to_prev, q.lease_prev_until, q.lease, q.leased_to, q.lease_until, $1)),
      lease = $15, lease_until = $16, leased_to = $17, leased_at = $1
    WHERE q.id IN (SELECT factory_take($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14))
      AND (SELECT count(DISTINCT h.lease) FROM questions h WHERE h.leased_to = $17 AND h.lease_until > $1 AND h.lease <> $15) < $18
    RETURNING q.id, q.verb, q.request, q.due_at, q.producer),
  charged AS (UPDATE producer_use u SET used = u.used + u.answer_ms * c.n / u.weight
    FROM (SELECT t.producer, count(*) AS n FROM taken t GROUP BY t.producer) c WHERE u.producer = c.producer)
  SELECT t.id, t.verb, t.request, t.due_at, t.producer FROM taken t
$$;

-- WHO IS ASKING: an id that asked since $1, OR one holding a live lease at $2 —
-- a protocol 8 client deep in a ten-minute pack asks only as it ends, and
-- counting it out would hand its share of the last rows to the fast.
CREATE OR REPLACE FUNCTION factory_asking(text, text) RETURNS SETOF text LANGUAGE sql STABLE AS $$
  SELECT id FROM verifiers WHERE asked_at > $1
  UNION SELECT leased_to FROM questions WHERE leased_to IS NOT NULL AND lease_until > $2
$$;

-- THE OPEN ROWS NOBODY WAITS ON, counted only as far as they can matter — past
-- the asking times $4 (and never past $2) a pack's size and `short` read the
-- same — and the asking since $3, at $1 (worker/verify.js `packSize`).
CREATE OR REPLACE FUNCTION factory_open(text, bigint, text, bigint) RETURNS TABLE (open bigint, asking bigint)
LANGUAGE sql STABLE AS $$
  WITH a AS (SELECT count(*) AS n FROM factory_asking($3, $1))
  SELECT (SELECT count(*) FROM (SELECT 1 FROM questions WHERE state = 'open' AND waits = 0
      AND (lease_until IS NULL OR lease_until < $1)
      LIMIT LEAST($2, GREATEST(1, (SELECT n FROM a)) * $4)) o), (SELECT n FROM a)
$$;

-- A QUESTION AFTER ITS DEMANDS MOVED: the earliest deadline still standing, the
-- larger wait and that deadline's producer — or withdrawn, nobody asking.
CREATE OR REPLACE FUNCTION factory_restate(text) RETURNS void LANGUAGE sql AS $$
  UPDATE questions q SET due_at = l.due, waits = l.waits, producer = l.producer
    FROM (SELECT min(due_at) AS due, max(waits) AS waits, count(*) AS n, (array_agg(producer ORDER BY due_at, producer))[1] AS producer
          FROM demands WHERE question = $1 AND state = 'live') l
    WHERE q.id = $1 AND l.n > 0;
  UPDATE questions SET state = 'withdrawn', lease = NULL, lease_until = NULL, leased_to = NULL
    WHERE id = $1 AND state = 'open' AND NOT EXISTS (SELECT 1 FROM demands WHERE question = $1 AND state = 'live');
$$;

-- A BOARD ROW STILL OWED (worker/board_tasks.js): `queue` holds it.
CREATE OR REPLACE FUNCTION board_owed(text, text, text) RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT EXISTS (SELECT 1 FROM queue WHERE build_id = $1 AND ruler = $2 AND mode = $3)
$$;
