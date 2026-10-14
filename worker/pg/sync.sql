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

-- WHO HAS HAD THEIR SHARE (docs/BOARD.md §"Who asked"): each coded initiator
-- holding more live leases at $1 than dial `initiator_share_pct` of all of them,
-- and more than `initiator_floor_rows`. Off (none) while the share is 0; the
-- dials are read here as worker/dials.js states them, an unset one its default.
CREATE OR REPLACE FUNCTION factory_capped(text) RETURNS SETOF text LANGUAGE sql STABLE AS $$
  WITH d AS (SELECT COALESCE((SELECT value FROM factory_dials WHERE name = 'initiator_share_pct'), 0) AS pct,
                    COALESCE((SELECT value FROM factory_dials WHERE name = 'initiator_floor_rows'), 50) AS floor),
  held AS MATERIALIZED (SELECT initiator FROM questions WHERE leased_to IS NOT NULL AND lease_until > $1)
  SELECT h.initiator FROM held h
  WHERE (SELECT pct FROM d) > 0 AND (h.initiator LIKE 'user:%' OR h.initiator LIKE 'bot:%')
  GROUP BY h.initiator
  HAVING count(*) > GREATEST((SELECT floor FROM d), (SELECT pct FROM d) / 100 * (SELECT count(*) FROM held))
$$;

-- WHAT A WITNESS MAY ANSWER, whatever the state (the caller names it, so the
-- order's index is used): a question leased to nobody it must wait for, with
-- the cores a first answer needs at its age, not answered on this engine by any
-- of its owner's devices nor its network, not refused by its engine, due before
-- $8 when given, not asked by an initiator past its share (`factory_capped`),
-- and not just queued for the settler (dial `settle_grace_ms`).
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
    AND (q.initiator IS NULL OR q.initiator NOT IN (SELECT factory_capped($1)))
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
  locked AS (SELECT u.producer FROM producer_use u WHERE u.producer IN (SELECT t.producer FROM taken t) ORDER BY u.producer FOR UPDATE),
  charged AS (UPDATE producer_use u SET used = u.used + u.answer_ms * c.n / u.weight
    FROM (SELECT t.producer, count(*) AS n FROM taken t GROUP BY t.producer) c WHERE u.producer = c.producer
      AND u.producer IN (SELECT producer FROM locked))
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
-- larger wait and that deadline's producer and initiator — or withdrawn, nobody asking.
CREATE OR REPLACE FUNCTION factory_restate(text) RETURNS void LANGUAGE sql AS $$
  UPDATE questions q SET due_at = l.due, waits = l.waits, producer = l.producer, initiator = l.initiator
    FROM (SELECT min(due_at) AS due, max(waits) AS waits, count(*) AS n, (array_agg(producer ORDER BY due_at, producer))[1] AS producer,
            (array_agg(initiator ORDER BY due_at, producer))[1] AS initiator
          FROM demands WHERE question = $1 AND state = 'live') l
    WHERE q.id = $1 AND l.n > 0;
  UPDATE questions SET state = 'withdrawn', lease = NULL, lease_until = NULL, leased_to = NULL
    WHERE id = $1 AND state = 'open' AND NOT EXISTS (SELECT 1 FROM demands WHERE question = $1 AND state = 'live');
$$;

-- COVERED (`answers.credited = 2`): on the questions $1 (json), an agreeing answer
-- still unpaid whose owner or network a paid witness of the same answer already
-- is — no witness rule can ever pay it. Marked where the witnesses are paid
-- (worker/tasks.js `credit`) and by `repay` for the rows before it; returns how many.
CREATE OR REPLACE FUNCTION factory_cover(text) RETURNS bigint LANGUAGE sql AS $$
  WITH c AS (UPDATE answers a SET credited = 2
      WHERE a.question IN (SELECT jsonb_array_elements_text($1::jsonb)) AND a.credited = 0 AND a.void = 0 AND a.official = 0
        AND EXISTS (SELECT 1 FROM answers w WHERE w.question = a.question AND w.credited = 1 AND w.void = 0 AND w.official = 0
                    AND w.canon = a.canon AND (w.owner = a.owner OR (coalesce(a.net, '') <> '' AND w.net = a.net)))
      RETURNING 1)
  SELECT count(*) FROM c
$$;

-- AN ASK THAT IS OVER KEEPS NO CODE: a demand no longer live and a question no
-- longer open lose a user or bot initiator (the settler's pass, dial
-- `initiator_forget_ms`). The rows erased.
CREATE OR REPLACE FUNCTION factory_forget_codes() RETURNS bigint LANGUAGE sql AS $$
  WITH d AS (UPDATE demands SET initiator = NULL
               WHERE (initiator LIKE 'user:%' OR initiator LIKE 'bot:%') AND state <> 'live' RETURNING 1),
  q AS (UPDATE questions SET initiator = NULL
          WHERE (initiator LIKE 'user:%' OR initiator LIKE 'bot:%') AND state <> 'open' RETURNING 1)
  SELECT (SELECT count(*) FROM d) + (SELECT count(*) FROM q)
$$;

-- A BOARD ROW STILL OWED (worker/board_tasks.js): `queue` holds it.
CREATE OR REPLACE FUNCTION board_owed(text, text, text) RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT EXISTS (SELECT 1 FROM queue WHERE build_id = $1 AND ruler = $2 AND mode = $3)
$$;

-- A CLIENT THAT ASKS (worker/verify.js `/work`, worker/sync.js): remembered by
-- its id, its yes kept when given ($4, $5), a refusal lifted once over at $3,
-- its day ($2).
CREATE OR REPLACE FUNCTION verifier_seen(text, text, text, bigint, text) RETURNS void LANGUAGE sql AS $$
  INSERT INTO verifiers (id, seen) VALUES ($1, $2) ON CONFLICT DO NOTHING;
  UPDATE verifiers SET consent_v = $4, consent_at = $5
    WHERE id = $1 AND $4 IS NOT NULL AND (consent_v IS DISTINCT FROM $4 OR consent_at IS DISTINCT FROM $5);
  UPDATE verifiers SET banned = 0 WHERE id = $1 AND banned = 1 AND (refused_until IS NULL OR refused_until <= $3);
  UPDATE verifiers SET seen = $2 WHERE id = $1 AND seen IS DISTINCT FROM $2;
$$;

-- …ON, its engine and free cores ($3, $4) kept, written at $2 at most once
-- since $5 unless they changed (worker/verify.js `askedOf`).
CREATE OR REPLACE FUNCTION verifier_asked(text, text, text, bigint, text) RETURNS void LANGUAGE sql AS $$
  UPDATE verifiers SET asked_at = $2, asked_engine = $3, asked_lanes = coalesce($4, asked_lanes)
    WHERE id = $1 AND (asked_at IS NULL OR asked_at < $5 OR asked_engine IS DISTINCT FROM $3
      OR ($4 IS NOT NULL AND asked_lanes IS DISTINCT FROM $4))
$$;

-- …AND WHAT IT ASKED FOR, by the hour (worker/verify.js `asksOf`), added to.
CREATE OR REPLACE FUNCTION verifier_asks(text) RETURNS void LANGUAGE sql AS $$
  INSERT INTO verifier_asks AS h (verifier, hour, asks, idle, idle_lanes, failed, relayed, relay_failed, direct_failed)
  SELECT x.verifier, x.hour, x.asks, x.idle, x.idle_lanes, x.failed, x.relayed, x.relay_failed, x.direct_failed
  FROM jsonb_to_recordset($1::jsonb) AS x(verifier text, hour text, asks bigint, idle bigint, idle_lanes bigint, failed bigint,
    relayed bigint, relay_failed bigint, direct_failed bigint)
  ON CONFLICT (verifier, hour) DO UPDATE SET asks = h.asks + excluded.asks, idle = h.idle + excluded.idle,
    idle_lanes = h.idle_lanes + excluded.idle_lanes, failed = h.failed + excluded.failed,
    relayed = h.relayed + excluded.relayed, relay_failed = h.relay_failed + excluded.relay_failed,
    direct_failed = h.direct_failed + excluded.direct_failed
$$;

-- THE ROWS OF $3 (json) A PACK'S LEASE $1 STILL HOLDS for device $2 at $4 — or,
-- $5 = 1, held still though run out — or held before (worker/tasks.js `answerPack`).
CREATE OR REPLACE FUNCTION factory_held(text, text, text, text, bigint) RETURNS SETOF questions LANGUAGE sql STABLE AS $$
  SELECT * FROM questions WHERE id IN (SELECT value FROM jsonb_array_elements_text($3::jsonb))
    AND ((lease = $1 AND leased_to = $2 AND ($5 = 1 OR lease_until >= $4)) OR (lease_prev = $1 AND leased_to_prev = $2))
$$;

-- …AND THOSE DEVICE $1 ALREADY ANSWERED on engine $2: a send repeated is not a second witness.
CREATE OR REPLACE FUNCTION factory_given(text, text, text) RETURNS SETOF text LANGUAGE sql STABLE AS $$
  SELECT DISTINCT question FROM answers WHERE device = $1 AND engine = $2 AND question IN (SELECT jsonb_array_elements_text($3::jsonb))
$$;

-- ONE ANSWER WRITTEN, unless its device gave one on this engine already, the
-- unique index `answers_once` the last word on a race. Columns in table order.
CREATE OR REPLACE FUNCTION factory_answer(text, text, text, text, text, bigint, text, text, bigint, bigint, text, bigint, text)
RETURNS void LANGUAGE sql AS $$
  INSERT INTO answers (question, engine, device, owner, net, official, canon, result, work, compute_ms, counts, lanes, at)
  SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13
  WHERE $6 = 1 OR NOT EXISTS (SELECT 1 FROM answers x WHERE x.question = $1 AND x.device = $3 AND x.engine = $2)
  ON CONFLICT (question, device, engine) WHERE official = 0 DO NOTHING
$$;

-- EVERY WRITER OF PRODUCER_USE LOCKS ITS ROWS IN PRODUCER ORDER first (this,
-- the pack's charge, board_sync): one statement taking them in its plan's order
-- deadlocked against another taking them in its own.
-- WHAT ANSWERS COST, folded into each producer's mean: `n` answers of `ms` in
-- all move it as `n` folds of share $2 would ($1: [{ p, ms, n }]).
CREATE OR REPLACE FUNCTION factory_learn(text, double precision) RETURNS void LANGUAGE sql AS $$
  SELECT 1 FROM producer_use WHERE producer IN (SELECT p FROM jsonb_to_recordset($1::jsonb) AS x(p text)) ORDER BY producer FOR UPDATE;
  UPDATE producer_use u SET answer_ms = CASE WHEN u.answer_ms = 0 THEN x.ms / x.n
      ELSE u.answer_ms * power(1 - $2, x.n) + x.ms / x.n * (1 - power(1 - $2, x.n)) END
  FROM jsonb_to_recordset($1::jsonb) AS x(p text, ms double precision, n double precision) WHERE u.producer = x.p
$$;

-- QUESTIONS $1 (json) QUEUED FOR THE SETTLER on engine $2 beside peer $3; an
-- answer on a row a settler pass holds clears its claim.
CREATE OR REPLACE FUNCTION factory_queue(text, text, text, bigint, text) RETURNS void LANGUAGE sql AS $$
  INSERT INTO settle_queue (question, engine, peer, joined, queued_at)
  SELECT value, $2, $3, $4, $5 FROM jsonb_array_elements_text($1::jsonb)
  ON CONFLICT (question, engine, peer) DO UPDATE SET claim = NULL, claimed_at = NULL
$$;

-- WHEN A VOLUNTEER LAST ANSWERED and what it spent ($2 ms on $3 tasks), in its hour.
CREATE OR REPLACE FUNCTION verifier_worked(text, bigint, bigint, text) RETURNS void LANGUAGE sql AS $$
  UPDATE verifiers SET compute_ms = compute_ms + $2, last_at = $4 WHERE id = $1;
  INSERT INTO verifier_hours AS h (verifier, hour, tasks, ms) VALUES ($1, substr($4, 1, 13), $3, $2)
    ON CONFLICT (verifier, hour) DO UPDATE SET tasks = h.tasks + excluded.tasks, ms = h.ms + excluded.ms;
$$;

-- PROTOCOL 8'S RENEWAL: every row $1 ([{ lease, question }]) device $2 names
-- held until $3 at least, as its holder or as the holder before. The ids held.
-- Its rows are locked in id order first (worker/tasks.js `inIdOrder`): inside
-- `board_sync` it holds them while it takes others.
CREATE OR REPLACE FUNCTION factory_renew(text, text, text) RETURNS SETOF text LANGUAGE plpgsql AS $$
DECLARE x text;
BEGIN
  PERFORM 1 FROM questions q, jsonb_to_recordset($1::jsonb) AS r(lease text, question text)
    WHERE q.id = r.question AND ((q.lease = r.lease AND q.leased_to = $2) OR (q.lease_prev = r.lease AND q.leased_to_prev = $2))
    ORDER BY q.id FOR UPDATE OF q;
  FOR x IN UPDATE questions q SET lease_until = GREATEST(q.lease_until, $3) FROM jsonb_to_recordset($1::jsonb) AS r(lease text, question text)
      WHERE q.id = r.question AND q.lease = r.lease AND q.leased_to = $2 RETURNING q.id LOOP
    RETURN NEXT x;
  END LOOP;
  FOR x IN UPDATE questions q SET lease_prev_until = GREATEST(q.lease_prev_until, $3) FROM jsonb_to_recordset($1::jsonb) AS r(lease text, question text)
      WHERE q.id = r.question AND q.lease_prev = r.lease AND q.leased_to_prev = $2 RETURNING q.id LOOP
    RETURN NEXT x;
  END LOOP;
END $$;

-- …AND EVERY LIVE LEASE OF DEVICE $1 AT $2 ON A ROW IT NAMES NOWHERE ($3) goes back.
CREATE OR REPLACE FUNCTION factory_release_unnamed(text, text, text) RETURNS void LANGUAGE sql AS $$
  UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL
  WHERE id IN (SELECT id FROM questions WHERE leased_to = $1 AND lease_until > $2
    AND id NOT IN (SELECT value FROM jsonb_array_elements_text($3::jsonb)) ORDER BY id FOR UPDATE)
$$;

-- ROWS A CLIENT'S ENGINE REFUSED ($1: [{ question, step }]) by device $2 of
-- owner $3 on engine $4: kept and given back; on the served engine $5, one
-- refused by $6 owners is withdrawn and written down at $7. The ids withdrawn.
CREATE OR REPLACE FUNCTION factory_refuse(text, text, text, text, text, bigint, text) RETURNS SETOF text LANGUAGE plpgsql AS $$
DECLARE x record;
BEGIN
  INSERT INTO engine_refusals (question, device, owner, engine, step, at)
    SELECT r.question, $2, $3, $4, r.step, $7 FROM jsonb_to_recordset($1::jsonb) AS r(question text, step text) ON CONFLICT DO NOTHING;
  UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL
    WHERE id IN (SELECT id FROM questions WHERE id IN (SELECT r.question FROM jsonb_to_recordset($1::jsonb) AS r(question text))
      AND leased_to = $2 ORDER BY id FOR UPDATE);
  IF $4 IS DISTINCT FROM $5 THEN RETURN; END IF;
  FOR x IN SELECT e.question, count(DISTINCT e.owner) AS owners FROM engine_refusals e
      WHERE e.question IN (SELECT r.question FROM jsonb_to_recordset($1::jsonb) AS r(question text)) AND e.engine = $5
      GROUP BY e.question HAVING count(DISTINCT e.owner) >= $6 ORDER BY e.question LOOP
    UPDATE demands SET state = 'withdrawn' WHERE question = x.question AND state = 'live';
    INSERT INTO refusal_withdrawals (question, engine, owners, at) VALUES (x.question, $5, x.owners, $7) ON CONFLICT (question) DO NOTHING;
    PERFORM factory_restate(x.question);
    RETURN NEXT x.question;
  END LOOP;
END $$;

-- WHERE EACH OF $2 (json) STANDS for device $1: its answers counted and credited.
CREATE OR REPLACE FUNCTION factory_states(text, text)
RETURNS TABLE (id text, state text, fact_at text, credited bigint, mine bigint) LANGUAGE sql STABLE AS $$
  SELECT q.id, q.state, q.fact_at,
    (SELECT max(a.credited) FROM answers a WHERE a.question = q.id AND a.device = $1 AND a.void = 0),
    (SELECT count(*) FROM answers a WHERE a.question = q.id AND a.device = $1 AND a.void = 0)
  FROM questions q WHERE q.id IN (SELECT value FROM jsonb_array_elements_text($2::jsonb)) ORDER BY q.id
$$;

-- WHAT DEVICE $1 EARNED THAT NO ACCOUNT HOLDS, as worker/contribution.js
-- `devicePointsOf` reads it: from hour $2 ('' all of it; NULL while an account
-- holds it), since days $3 and $4, and since hour $5 when given; work, not points.
CREATE OR REPLACE FUNCTION device_points(text, text, text, text, text) RETURNS jsonb LANGUAGE sql STABLE AS $$
  SELECT jsonb_build_object('held', $2 IS NULL,
    'work', CASE WHEN $2 IS NULL OR v.id IS NULL THEN 0 WHEN $2 = '' THEN v.work
      ELSE COALESCE((SELECT sum(h.work) FROM verifier_hours h WHERE h.verifier = $1 AND h.hour >= $2), 0) END,
    'recent', CASE WHEN $2 IS NULL OR v.id IS NULL THEN 0
      ELSE COALESCE((SELECT sum(h.work) FROM verifier_hours h WHERE h.verifier = $1 AND h.hour >= $2 AND h.hour >= $3), 0) END,
    'week', CASE WHEN $2 IS NULL OR v.id IS NULL THEN 0
      ELSE COALESCE((SELECT sum(h.work) FROM verifier_hours h WHERE h.verifier = $1 AND h.hour >= $2 AND h.hour >= $4), 0) END,
    'today', CASE WHEN $5 IS NULL THEN NULL ELSE (SELECT jsonb_build_object('tasks', COALESCE(sum(h.tasks), 0),
      'ms', COALESCE(sum(h.ms), 0), 'work', COALESCE(sum(h.work), 0)) FROM verifier_hours h WHERE h.verifier = $1 AND h.hour >= $5) END)
  FROM (SELECT 1) one LEFT JOIN verifiers v ON v.id = $1 AND v.banned = 0
$$;

-- PROTOCOL 8'S SYNC, ONE CALL AND ONE TRANSACTION (worker/sync.js, which builds
-- `p` and reads `r`; docs/BOARD.md §"The sync in the database"). The order and
-- every gate are worker/sync.js's JS path, step for step, so the two answer
-- alike (scripts/check_sync_proc.mjs). Whatever needs JS raises WS001 and
-- nothing is written: an answer the settler may not take, or a pack demand of
-- a producer whose wanting is not `board_owed`. `outcome` says which reply:
-- idle, busy, banned, early, full, alone (a waited-on head: worker/sync.js
-- leases it), or pack.
CREATE OR REPLACE PROCEDURE board_sync(p jsonb, INOUT r jsonb) LANGUAGE plpgsql AS $$
DECLARE
  v_dev text := p ->> 'id';
  v_at text := p ->> 'at';
  v_engine text := p ->> 'engine';
  v_stale boolean := (p ->> 'stale')::boolean;
  v_owner text := p ->> 'owner';
  v_siblings text := p ->> 'siblings';
  v_net text := p ->> 'net';
  v_lanes bigint := (p ->> 'lanes')::bigint;
  v_ceiling bigint := (p ->> 'ceiling_s')::bigint;
  v_grace_s bigint := (p ->> 'grace_s')::bigint;
  v_grace boolean := jsonb_typeof(p -> 'grace') = 'object';
  v_lease text := p #>> '{pack,lease}';
  v_taken jsonb := COALESCE(p -> 'taken', '[]'::jsonb);
  v_dropped jsonb := COALESCE(p -> 'dropped', '[]'::jsonb);
  v_withdrawn jsonb := '[]'::jsonb;
  v_held jsonb := '[]'::jsonb;
  v_states jsonb := '[]'::jsonb;
  v_points jsonb;
  v_queued boolean := false;
  v_group jsonb;
  v_ids jsonb;
  v_ok jsonb;
  v_rows jsonb;
  v_kept jsonb := '[]'::jsonb;
  v_me record;
  v_holds record;
  v_head record;
  v_has_head boolean;
  v_packable boolean;
  v_fair record;
  v_short boolean;
  v_size bigint;
  v_got jsonb;
  v_ds jsonb;
  v_unwanted jsonb := '[]'::jsonb;
  v_wanted jsonb;
  v_gone jsonb;
  v_x record;
  v_use_locked boolean := false;
BEGIN
  -- ONE DEVICE'S SENDS ARE TAKEN ONE AT A TIME (worker/tasks.js `recordMany`).
  PERFORM pg_advisory_xact_lock(hashtextextended(v_dev, 0));
  IF (p ->> 'seen')::boolean THEN
    PERFORM verifier_seen(v_dev, p ->> 'day', v_at, (p ->> 'consent_v')::bigint, p ->> 'consent_at');
    PERFORM verifier_asked(v_dev, v_at, p #>> '{asked,engine}', (p #>> '{asked,lanes}')::bigint, p #>> '{asked,before}');
    IF jsonb_array_length(p -> 'asks') > 0 THEN PERFORM verifier_asks((p -> 'asks')::text); END IF;
  END IF;

  -- RESULTS, by the lease each was handed under (worker/tasks.js `answerPack`, `late`).
  FOR v_group IN SELECT value FROM jsonb_array_elements(COALESCE(p -> 'groups', '[]'::jsonb)) LOOP
    v_ids := (SELECT jsonb_agg(a ->> 'question') FROM jsonb_array_elements(v_group -> 'rows') a);
    v_ok := COALESCE((SELECT jsonb_agg(g) FROM factory_given(v_dev, v_engine, v_ids::text) g), '[]'::jsonb);
    v_rows := COALESCE((SELECT jsonb_agg(jsonb_build_object('a', x.a, 'id', q.id, 'verb', q.verb, 'producer', q.producer) ORDER BY x.k)
        FROM jsonb_array_elements(v_group -> 'rows') WITH ORDINALITY x(a, k)
        JOIN factory_held(v_group ->> 'lease', v_dev, v_ids::text, v_at, 1) q ON q.id = x.a ->> 'question'
        WHERE NOT v_ok ? q.id), '[]'::jsonb);
    IF jsonb_array_length(v_rows) > 0 THEN
      IF EXISTS (SELECT 1 FROM jsonb_array_elements(v_rows) i
          WHERE NOT (p -> 'packable') ? (i ->> 'verb') OR NOT (p -> 'queueable') ? (i ->> 'producer')) THEN
        RAISE EXCEPTION 'board_sync: an answer the settler may not take' USING ERRCODE = 'WS001';
      END IF;
      FOR v_x IN SELECT value AS i FROM jsonb_array_elements(v_rows) LOOP
        PERFORM factory_answer(v_x.i ->> 'id', v_engine, v_dev, v_owner, v_net, 0, v_x.i #>> ARRAY['a', 'canons', v_x.i ->> 'verb'],
          v_x.i #>> '{a,result}', (v_x.i #>> '{a,work}')::bigint, (v_x.i #>> '{a,compute_ms}')::bigint, v_x.i #>> '{a,counts}',
          (v_x.i #>> '{a,lanes}')::bigint, v_at);
      END LOOP;
      -- PRODUCER_USE IS LOCKED WHOLE, in producer order, before its first write
      -- here or at the pack's charge: two syncs taking its rows in opposite
      -- orders deadlocked.
      PERFORM 1 FROM producer_use ORDER BY producer FOR UPDATE;
      v_use_locked := true;
      PERFORM factory_learn(y.x::text, (p ->> 'share')::double precision)
        FROM (SELECT jsonb_agg(jsonb_build_object('p', l.producer, 'ms', l.ms, 'n', l.n) ORDER BY l.producer) AS x
          FROM (SELECT i ->> 'producer' AS producer, sum((i #>> '{a,compute_ms}')::bigint) AS ms, count(*) AS n
            FROM jsonb_array_elements(v_rows) i WHERE (i #>> '{a,compute_ms}')::bigint > 0 GROUP BY 1) l) y
        WHERE y.x IS NOT NULL;
      PERFORM factory_queue((SELECT jsonb_agg(i ->> 'id' ORDER BY k) FROM jsonb_array_elements(v_rows) WITH ORDINALITY x(i, k))::text,
        p #>> '{record,engine}', p #>> '{record,peer}', (p #>> '{record,joined}')::bigint, v_at);
      v_queued := true;
      PERFORM verifier_worked(v_dev, (SELECT COALESCE(sum((i #>> '{a,compute_ms}')::bigint), 0) FROM jsonb_array_elements(v_rows) i)::bigint,
        jsonb_array_length(v_rows)::bigint, v_at);
      v_ok := v_ok || (SELECT jsonb_agg(i ->> 'id') FROM jsonb_array_elements(v_rows) i);
    END IF;
    FOR v_x IN SELECT x.a ->> 'question' AS q FROM jsonb_array_elements(v_group -> 'rows') WITH ORDINALITY x(a, k) ORDER BY x.k LOOP
      IF v_ok ? v_x.q THEN v_taken := v_taken || to_jsonb(v_x.q);
      ELSE v_dropped := v_dropped || jsonb_build_object('question', v_x.q, 'why', 'not_held');
      END IF;
    END LOOP;
  END LOOP;

  -- ROWS ITS ENGINE REFUSED; RENEWED every row it names as held; ANY OTHER LIVE LEASE GOES BACK.
  IF NOT v_stale AND jsonb_array_length(p -> 'refused') > 0 THEN
    v_withdrawn := COALESCE((SELECT jsonb_agg(f.x ORDER BY f.k) FROM factory_refuse((p -> 'refused')::text, v_dev, v_owner, v_engine,
      p ->> 'served', (p ->> 'withdraw_at')::bigint, v_at) WITH ORDINALITY f(x, k)), '[]'::jsonb);
  END IF;
  v_rows := '[]'::jsonb;
  IF NOT v_stale THEN
    v_rows := COALESCE((SELECT jsonb_agg(x.h ORDER BY x.k) FROM jsonb_array_elements(p -> 'held') WITH ORDINALITY x(h, k)
      WHERE NOT v_taken ? (x.h ->> 'question')
        AND NOT EXISTS (SELECT 1 FROM jsonb_array_elements(p -> 'refused') f WHERE f ->> 'question' = x.h ->> 'question')), '[]'::jsonb);
  END IF;
  IF jsonb_array_length(v_rows) > 0 THEN
    v_kept := COALESCE((SELECT jsonb_agg(DISTINCT k) FROM factory_renew(v_rows::text, v_dev, p ->> 'renew_until') k), '[]'::jsonb);
  END IF;
  PERFORM factory_release_unnamed(v_dev, v_at, (v_kept || CASE WHEN NOT v_stale AND p ->> 'keep' IS NOT NULL
    THEN jsonb_build_array(p ->> 'keep') ELSE '[]'::jsonb END)::text);
  v_held := COALESCE((SELECT jsonb_agg(jsonb_build_object('question', x.h ->> 'question', 'held', v_kept ? (x.h ->> 'question')) ORDER BY x.k)
    FROM jsonb_array_elements(v_rows) WITH ORDINALITY x(h, k)), '[]'::jsonb);
  IF jsonb_array_length(p -> 'states') > 0 THEN
    v_states := COALESCE((SELECT jsonb_agg(to_jsonb(s)) FROM factory_states(v_dev, (p -> 'states')::text) s), '[]'::jsonb);
  END IF;
  IF jsonb_typeof(p -> 'points') = 'object' THEN
    v_points := device_points(v_dev, p #>> '{points,from}', p #>> '{points,recent_from}', p #>> '{points,week_from}', p #>> '{points,since_hour}');
  END IF;
  r := jsonb_build_object('taken', v_taken, 'dropped', v_dropped, 'withdrawn', v_withdrawn, 'held', v_held, 'states', v_states,
    'points', v_points, 'queued', v_queued);

  -- THE GATES, in worker/sync.js's order.
  SELECT banned, refused_until, last_at, next_at INTO v_me FROM verifiers WHERE id = v_dev;
  IF v_stale THEN
    UPDATE verifiers SET next_at = p ->> 'idle_at' WHERE id = v_dev;
    r := r || jsonb_build_object('outcome', 'stale');
    RETURN;
  END IF;
  IF v_me.banned <> 0 THEN
    r := r || jsonb_build_object('outcome', 'banned', 'until', v_me.refused_until);
    RETURN;
  END IF;
  IF NOT (p ->> 'consented')::boolean THEN
    UPDATE verifiers SET next_at = p ->> 'idle_at' WHERE id = v_dev;
    r := r || jsonb_build_object('outcome', 'idle');
    RETURN;
  END IF;
  IF v_me.next_at IS NOT NULL
      AND v_at < to_char((v_me.next_at::timestamptz - interval '2 seconds') AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') THEN
    r := r || jsonb_build_object('outcome', 'early', 'next_at', v_me.next_at);
    RETURN;
  END IF;
  SELECT count(*) AS questions, count(DISTINCT lease) AS packs INTO v_holds FROM questions WHERE leased_to = v_dev AND lease_until > v_at;
  IF (p ->> 'want')::bigint <= 0 OR v_holds.packs >= (p ->> 'packs_held')::bigint THEN
    r := r || jsonb_build_object('outcome', 'full');
    RETURN;
  END IF;
  SELECT * INTO v_head FROM factory_head(v_at, 0, v_lanes, v_engine, v_siblings, v_owner, v_net, NULL, v_ceiling, v_grace_s, p ->> 'official_states');
  v_has_head := FOUND;
  v_packable := v_has_head AND v_head.waits = 0 AND (p -> 'packable') ? v_head.verb;
  IF (NOT v_has_head AND NOT v_grace) OR (NOT v_grace AND NOT v_packable AND v_holds.questions > 0) THEN
    UPDATE verifiers SET next_at = p ->> 'idle_at' WHERE id = v_dev;
    r := r || jsonb_build_object('outcome', 'idle');
    RETURN;
  END IF;
  IF NOT v_grace AND NOT v_packable THEN
    r := r || jsonb_build_object('outcome', 'alone');
    RETURN;
  END IF;
  SELECT * INTO v_fair FROM factory_open(v_at, (p #>> '{fair,scan}')::bigint, p #>> '{fair,asking_since}', (p #>> '{fair,per}')::bigint);
  IF (p #>> '{fair,cap}')::bigint <> 0 AND v_fair.asking > (p #>> '{fair,cap}')::bigint
      AND NOT (v_me.last_at IS NOT NULL AND v_me.last_at > p #>> '{fair,cap_since}') THEN
    UPDATE verifiers SET next_at = p ->> 'idle_at' WHERE id = v_dev;
    r := r || jsonb_build_object('outcome', 'busy');
    RETURN;
  END IF;
  -- WHILE WORK IS SHORT an account past its fair share (the dice are JS's) is handed nothing.
  v_short := v_fair.open / GREATEST(1, v_fair.asking) < (p #>> '{fair,short_rows}')::bigint;
  IF v_short AND (p #>> '{fair,throttle}')::boolean THEN
    UPDATE verifiers SET next_at = p ->> 'idle_at' WHERE id = v_dev;
    r := r || jsonb_build_object('outcome', 'idle');
    RETURN;
  END IF;
  v_size := GREATEST(1, LEAST((p #>> '{pack,max}')::bigint, (p #>> '{pack,by_pace}')::bigint, v_fair.open / GREATEST(1, v_fair.asking)));
  IF NOT v_use_locked THEN PERFORM 1 FROM producer_use ORDER BY producer FOR UPDATE; END IF;
  v_got := COALESCE((SELECT jsonb_agg(to_jsonb(t)) FROM factory_lease_pack(v_at, v_lanes, v_engine, v_siblings, v_owner, v_net, NULL, v_ceiling,
    v_grace_s, (p -> 'packable')::text, p #>> '{grace,exclude}', p #>> '{grace,join}', CASE WHEN v_packable THEN v_head.producer END, v_size,
    v_lease, CASE WHEN v_short THEN p #>> '{pack,short_until}' ELSE p #>> '{pack,long_until}' END, v_dev, (p ->> 'packs_held')::bigint) t),
    '[]'::jsonb);

  -- THE PACK'S DEMANDS, in worker/tasks.js `nextPack`'s order: a row no longer
  -- owed is withdrawn where it is found, producer by producer, and left out.
  v_ds := COALESCE((SELECT jsonb_agg(to_jsonb(d) ORDER BY d.k) FROM (
      SELECT d.question, d.producer, d.ref, d.payload, row_number() OVER (ORDER BY d.question, d.producer, d.ref) AS k
      FROM demands d JOIN questions q ON q.id = d.question WHERE q.lease = v_lease AND q.leased_to = v_dev AND d.state = 'live') d), '[]'::jsonb);
  IF jsonb_array_length(v_got) > 0 AND EXISTS (SELECT 1 FROM jsonb_array_elements(v_ds) e WHERE NOT (p -> 'owed') ? (e ->> 'producer')) THEN
    RAISE EXCEPTION 'board_sync: a pack demand of a producer not owed in sql' USING ERRCODE = 'WS001';
  END IF;
  FOR v_x IN SELECT u.producer, u.ref, u.question FROM (
      SELECT e ->> 'producer' AS producer, e ->> 'ref' AS ref, e ->> 'question' AS question, (e ->> 'payload')::jsonb AS payload,
        (e ->> 'k')::bigint AS k, min((e ->> 'k')::bigint) OVER (PARTITION BY e ->> 'producer') AS first
      FROM jsonb_array_elements(v_ds) e) u
    WHERE NOT board_owed(u.payload ->> 'identity', u.payload ->> 'ruler', u.payload ->> 'mode') ORDER BY u.first, u.k LOOP
    UPDATE demands SET state = 'withdrawn' WHERE producer = v_x.producer AND ref = v_x.ref;
    PERFORM factory_restate(v_x.question);
    v_unwanted := v_unwanted || jsonb_build_array(jsonb_build_array(v_x.producer, v_x.ref));
  END LOOP;
  v_wanted := COALESCE((SELECT jsonb_agg(e ORDER BY (e ->> 'k')::bigint) FROM jsonb_array_elements(v_ds) e
    WHERE NOT v_unwanted @> jsonb_build_array(jsonb_build_array(e ->> 'producer', e ->> 'ref'))), '[]'::jsonb);
  v_gone := COALESCE((SELECT jsonb_agg(t ->> 'id') FROM jsonb_array_elements(v_got) t
    WHERE NOT EXISTS (SELECT 1 FROM jsonb_array_elements(v_wanted) e WHERE e ->> 'question' = t ->> 'id')), '[]'::jsonb);
  IF jsonb_array_length(v_gone) > 0 THEN
    UPDATE questions SET lease = NULL, lease_until = NULL, leased_to = NULL
      WHERE id IN (SELECT id FROM questions WHERE id IN (SELECT jsonb_array_elements_text(v_gone)) AND lease = v_lease ORDER BY id FOR UPDATE);
  END IF;
  v_got := COALESCE((SELECT jsonb_agg(jsonb_build_object('question', t ->> 'id', 'verb', t ->> 'verb', 'request', t ->> 'request',
      'due_at', t ->> 'due_at', 'demands', (SELECT COALESCE(jsonb_agg(jsonb_build_object('producer', e ->> 'producer', 'ref', e ->> 'ref',
        'payload', e ->> 'payload') ORDER BY (e ->> 'k')::bigint), '[]'::jsonb) FROM jsonb_array_elements(v_wanted) e WHERE e ->> 'question' = t ->> 'id'))
      ORDER BY t ->> 'due_at' COLLATE "C", t ->> 'id' COLLATE "C")
    FROM jsonb_array_elements(v_got) t WHERE NOT v_gone ? (t ->> 'id')), '[]'::jsonb);
  IF jsonb_array_length(v_got) = 0 THEN
    UPDATE verifiers SET next_at = p ->> 'idle_at' WHERE id = v_dev;
    r := r || jsonb_build_object('outcome', 'idle');
    RETURN;
  END IF;
  UPDATE verifiers SET next_at = p ->> 'floor_at' WHERE id = v_dev;
  r := r || jsonb_build_object('outcome', 'pack', 'short', v_short, 'lease', v_lease, 'questions', v_got);
END $$;
