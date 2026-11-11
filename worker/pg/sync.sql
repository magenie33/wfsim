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

-- Changing an accepted canonical output is a fresh round, including official corrections.
CREATE OR REPLACE FUNCTION compute_result_round() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE selected entity_compute_results%ROWTYPE;
BEGIN
  SELECT r.* INTO selected FROM state_compute_scheduling s JOIN entity_compute_results r ON r.id=s.result_id
    WHERE s.id=NEW.computation AND s.task_id=NEW.task_id AND r.task_id=NEW.task_id FOR UPDATE OF s;
  IF FOUND AND (selected.canon,selected.engine) IS DISTINCT FROM (NEW.canon,NEW.engine) THEN
    RAISE EXCEPTION 'changing an accepted output requires a new compute task';
  END IF;
  RETURN NEW;
END $$;
DROP TRIGGER IF EXISTS compute_result_round ON entity_compute_results;
CREATE TRIGGER compute_result_round BEFORE INSERT ON entity_compute_results
  FOR EACH ROW EXECUTE FUNCTION compute_result_round();

CREATE OR REPLACE FUNCTION compute_prepare_correction(text,bigint,text,text,text,text) RETURNS bigint LANGUAGE plpgsql AS $$
DECLARE held state_compute_scheduling%ROWTYPE; selected entity_compute_results%ROWTYPE; next_task bigint;
BEGIN
  SELECT * INTO held FROM state_compute_scheduling WHERE id=$1 FOR UPDATE;
  IF NOT FOUND OR held.task_id IS DISTINCT FROM $2 THEN
    RAISE EXCEPTION 'compute task changed before correction' USING ERRCODE='40001';
  END IF;
  SELECT * INTO selected FROM entity_compute_results WHERE id=held.result_id AND task_id=held.task_id;
  IF NOT FOUND OR (selected.canon,selected.engine) IS NOT DISTINCT FROM ($3,$4) THEN RETURN held.task_id; END IF;
  next_task:=compute_open_task($1,'official_correction',$4,$5,true);
  INSERT INTO relation_task_answer_evidence(task_id,answer_id,role)
    SELECT next_task,a.id,'accepted_correction' FROM record_candidate_answers a
      JOIN relation_task_answer_evidence e ON e.answer_id=a.id AND e.task_id=held.task_id
    WHERE a.question=$1 AND a.id IN(SELECT value::bigint FROM jsonb_array_elements_text($6::jsonb)) AND a.void=0
    ON CONFLICT DO NOTHING;
  RETURN next_task;
END $$;

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
         CASE WHEN $3 >= $7 THEN $3 WHEN $6 >= $7 THEN $6 END;
$$;

-- WHO HAS HAD THEIR SHARE (docs/BOARD.md §"Who asked"): each coded initiator
-- holding more live leases at $1 than dial `initiator_share_pct` of all of them,
-- and more than `initiator_floor_rows`. Off (none) while the share is 0; the
-- dials are read here as worker/dials.js states them, an unset one its default.
CREATE OR REPLACE FUNCTION factory_capped(text) RETURNS SETOF text LANGUAGE sql STABLE AS $$
  WITH d AS (SELECT COALESCE((SELECT value FROM state_factory_parameters WHERE name = 'initiator_share_pct'), 0) AS pct,
                    COALESCE((SELECT value FROM state_factory_parameters WHERE name = 'initiator_floor_rows'), 50) AS floor),
  held AS MATERIALIZED (SELECT initiator FROM state_compute_work WHERE leased_to IS NOT NULL AND lease_until > $1)
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
-- Keep the lane gate's answer lookup correlated: a hash of the entire answer
-- history must never precede a bounded task selection.
CREATE OR REPLACE FUNCTION factory_eligible(text, bigint, bigint, text, text, text, text, text, bigint, bigint)
RETURNS SETOF state_compute_work LANGUAGE sql STABLE AS $$
  SELECT q.* FROM state_compute_work q
  WHERE factory_takeable(q.lease_until, q.leased_at, $1, $9)
    AND (q.verb NOT IN ('simulate_report', 'optimize_report') OR $5::jsonb ? 'cap:calculation-reports-v1')
    AND ($2 = 1 OR q.lanes IS NULL OR EXISTS (SELECT 1 FROM (SELECT 1 FROM state_compute_candidate_answers x WHERE x.question = q.id OFFSET 0) answered)
         OR $3 >= (SELECT min((t ->> 'lanes')::numeric) FROM jsonb_array_elements(q.lanes::jsonb) t
                   WHERE (t ->> 'after_ms')::numeric <= extract(epoch FROM ($1::timestamptz - q.opened_at::timestamptz)) * 1000))
    -- Read this question before filtering its engine: a newly released engine
    -- absent from ANALYZE can otherwise scan its whole day of answers per pick.
    AND NOT EXISTS (SELECT 1 FROM (SELECT a.engine, a.device, a.owner, a.net FROM state_compute_candidate_answers a
          WHERE a.question = q.id OFFSET 0) a
          WHERE a.engine = $4
            AND (a.device IN (SELECT value FROM jsonb_array_elements_text($5::jsonb)) OR a.owner = $6 OR ($7 <> '' AND a.net = $7)))
    AND NOT EXISTS (SELECT 1 FROM record_engine_refusals r WHERE r.question = q.id AND r.engine = $4
          AND r.device IN (SELECT value FROM jsonb_array_elements_text($5::jsonb)))
    AND ($8 IS NULL OR q.due_at < $8)
    AND (q.initiator IS NULL OR q.initiator NOT IN (SELECT factory_capped($1)))
    AND NOT EXISTS (SELECT 1 FROM state_pending_verifications s WHERE s.question = q.id
          AND s.queued_at > to_char(($1::timestamptz AT TIME ZONE 'UTC') - make_interval(secs => $10), 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'))
$$;

-- THE HEAD OF A WITNESS'S ORDER (docs/BOARD.md §"Fair share between producers"):
-- for a volunteer each producer's head, one index probe each, and of those the
-- waited-on first, then the producer that has used least; where no producer's
-- row finds one the plain order does. The official machines ($2 = 1) take what
-- is past its deadline and what only they settle ($11, json), in the plain order.
CREATE OR REPLACE FUNCTION factory_head(text, bigint, bigint, text, text, text, text, text, bigint, bigint, text, text)
RETURNS TABLE (id text, verb text, request text, state text, due_at text, waits bigint, producer text)
LANGUAGE sql STABLE AS $$
  WITH eligible AS NOT MATERIALIZED (
    SELECT * FROM factory_eligible($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) q
    WHERE $12 IS NULL OR (q.waits = 0 AND q.verb IN (SELECT jsonb_array_elements_text($12::jsonb))))
  SELECT z.id, z.verb, z.request, z.state, z.due_at, z.waits, z.producer FROM (
    (SELECT 0 AS fallback, x.* FROM state_producer_compute_usage u CROSS JOIN LATERAL (
       SELECT q.id, q.verb, q.request, q.state, q.due_at, q.waits, q.producer
       FROM eligible q
       WHERE q.state = 'open' AND q.producer = u.producer ORDER BY q.waits DESC, q.due_at, q.id LIMIT 1) x
     WHERE $2 = 0 ORDER BY x.waits DESC, u.used, x.due_at, x.id LIMIT 1)
    UNION ALL
    (SELECT 1, q.id, q.verb, q.request, q.state, q.due_at, q.waits, q.producer
     FROM eligible q
     WHERE $2 = 0 AND q.state = 'open' ORDER BY q.waits DESC, q.due_at, q.id LIMIT 1)
    UNION ALL
    (SELECT 2, q.id, q.verb, q.request, q.state, q.due_at, q.waits, q.producer
     FROM eligible q
     WHERE $2 = 1 AND ((q.state = 'open' AND q.due_at < $1) OR q.state IN (SELECT jsonb_array_elements_text($11::jsonb)))
     ORDER BY q.waits DESC, q.due_at, q.id LIMIT 1)
  ) z ORDER BY z.fallback LIMIT 1
$$;
CREATE OR REPLACE FUNCTION factory_head(text, bigint, bigint, text, text, text, text, text, bigint, bigint, text)
RETURNS TABLE (id text, verb text, request text, state text, due_at text, waits bigint, producer text)
LANGUAGE sql STABLE AS $$
  SELECT * FROM factory_head($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, NULL::text)
$$;

-- A PACK'S ROWS, locked: up to $14 open questions nobody waits on, of a packable
-- verb ($10, json), in the order. AN OLDER ENGINE STILL ACCEPTED is handed only
-- rows of a weapon outside $11 (json); ONE WHOSE CODE MOVED only a row a served
-- answer ($12) of another owner and network leads; $13 keeps to one producer.
-- The pick skips rows another pick holds (two asks at once deadlocked), and the
-- row itself is checked again once locked, as another may have taken it since.
-- $1 at, $2 lanes, $3 engine, $4 siblings, $5 owner, $6 net, $7 before, $8, $9 as above.
-- THE JOIN PICK STARTS FROM THE SERVED ENGINE'S ANSWERS, in its own statement:
-- walked down the order, a new engine's few answers among a million open rows
-- took ten seconds and more, under the producer_use lock every sync waits on.
-- ITS CANDIDATES ARE THE SERVED ENGINE'S LED QUESTIONS, each then checked for
-- another owner's answer: a semi-join on the engine's answers read the day of
-- them, 8 s a pick and 25 s under load, and the syncs locked behind it failed.
-- A QUESTION ANOTHER VOLUNTEER HAS ANSWERED GOES FIRST, in the order, then the
-- rest: one more agreeing answer makes it a fact, where a fresh row needs two,
-- and its first witness waits for that answer to be paid. Taken by the deadline
-- alone, a batch due sooner asked after the first answer left it waiting behind
-- every row of that batch. WHATEVER ENGINE ANSWERED: an older engine's answer
-- joins a served one that reproduces it, and kept to the served engine, every
-- release left the half-answered behind the rest until someone copied them over.
CREATE OR REPLACE FUNCTION factory_take(text, bigint, text, text, text, text, text, bigint, bigint, text, text, text, text, bigint)
RETURNS SETOF text LANGUAGE plpgsql AS $$
DECLARE
  v_led text[];
BEGIN
  IF $12 IS NULL THEN
    SELECT coalesce(array_agg(t.id), '{}') INTO v_led FROM (
      SELECT q.id FROM factory_eligible($1, 0, $2, $3, $4, $5, $6, $7, $8, $9) e JOIN state_compute_work q ON q.id = e.id
      WHERE e.id IN (SELECT l.question FROM state_task_answer_heads l)
        AND e.state = 'open' AND e.waits = 0 AND e.verb IN (SELECT jsonb_array_elements_text($10::jsonb))
        AND ($11 IS NULL OR (e.request::jsonb #>> '{record,weapon}') NOT IN (SELECT jsonb_array_elements_text($11::jsonb)))
        AND ($13 IS NULL OR e.producer = $13)
        AND q.state = 'open' AND q.waits = 0 AND factory_takeable(q.lease_until, q.leased_at, $1, $8)
      ORDER BY e.waits DESC, e.due_at, e.id LIMIT $14 FOR UPDATE OF q SKIP LOCKED) t;
    RETURN QUERY SELECT unnest(v_led);
    IF cardinality(v_led) >= $14 THEN RETURN; END IF;
    RETURN QUERY SELECT q.id FROM factory_eligible($1, 0, $2, $3, $4, $5, $6, $7, $8, $9) e JOIN state_compute_work q ON q.id = e.id
    WHERE e.state = 'open' AND e.waits = 0 AND e.verb IN (SELECT jsonb_array_elements_text($10::jsonb))
      AND ($11 IS NULL OR (e.request::jsonb #>> '{record,weapon}') NOT IN (SELECT jsonb_array_elements_text($11::jsonb)))
      AND ($13 IS NULL OR e.producer = $13)
      AND q.state = 'open' AND q.waits = 0 AND factory_takeable(q.lease_until, q.leased_at, $1, $8)
      AND NOT (q.id = ANY (v_led))
    ORDER BY e.waits DESC, e.due_at, e.id LIMIT $14 - cardinality(v_led) FOR UPDATE OF q SKIP LOCKED;
  ELSE
    RETURN QUERY SELECT q.id FROM factory_eligible($1, 0, $2, $3, $4, $5, $6, $7, $8, $9) e JOIN state_compute_work q ON q.id = e.id
    WHERE e.id IN (SELECT l.question FROM state_task_answer_heads l WHERE l.engine = $12 AND EXISTS (SELECT 1 FROM state_compute_candidate_answers s
        WHERE s.question = l.question AND s.engine = $12 AND s.void = 0 AND s.owner <> $5 AND ($6 = '' OR s.net <> $6)))
      AND e.state = 'open' AND e.waits = 0 AND e.verb IN (SELECT jsonb_array_elements_text($10::jsonb))
      AND ($11 IS NULL OR (e.request::jsonb #>> '{record,weapon}') NOT IN (SELECT jsonb_array_elements_text($11::jsonb)))
      AND ($13 IS NULL OR e.producer = $13)
      AND q.state = 'open' AND q.waits = 0 AND factory_takeable(q.lease_until, q.leased_at, $1, $8)
    ORDER BY e.waits DESC, e.due_at, e.id LIMIT $14 FOR UPDATE OF q SKIP LOCKED;
  END IF;
END $$;

-- A consumer transaction may apply only the fact and demand it read.
CREATE OR REPLACE FUNCTION factory_delivery_guard(text,text,text,text,text,text,text) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  PERFORM 1 FROM state_compute_work WHERE id=$1 AND state IN ('fact','spot')
    AND canon IS NOT DISTINCT FROM $2 AND fact_engine IS NOT DISTINCT FROM $3 FOR UPDATE;
  IF NOT FOUND THEN RAISE EXCEPTION 'stale task delivery' USING ERRCODE='WS002'; END IF;
  PERFORM 1 FROM relation_task_consumers WHERE producer=$4 AND ref=$5 AND question=$1 AND asked_at=$6 AND payload=$7 AND state='live' FOR UPDATE;
  IF NOT FOUND THEN RAISE EXCEPTION 'stale task delivery' USING ERRCODE='WS002'; END IF;
END $$;

-- THE LED QUESTIONS KEPT (schema.sql `led_questions`): a volunteer's answer on an
-- open question adds its row, the question leaving `open` drops every row of it.
-- An answer voided later leaves its row: the pick reads no answers, which on
-- every pack scanned the engine's whole day of them, and a row it moves up
-- early costs only its place.
-- AS THE TABLES' OWNER (SECURITY DEFINER): a trigger runs as whoever wrote the
-- row, and the settler's role, denied the table, had every settlement refused.
CREATE OR REPLACE FUNCTION led_answered() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER AS $$
BEGIN
  INSERT INTO state_task_answer_heads (question, engine)
    SELECT NEW.question, NEW.engine WHERE EXISTS (SELECT 1 FROM state_compute_work q WHERE q.id = NEW.question AND q.state = 'open')
    ON CONFLICT DO NOTHING;
  RETURN NULL;
END $$;
DROP TRIGGER IF EXISTS answers_led ON record_candidate_answers;
CREATE TRIGGER answers_led AFTER INSERT ON record_candidate_answers
  FOR EACH ROW WHEN (NEW.void = 0 AND NEW.official = 0) EXECUTE FUNCTION led_answered();
CREATE OR REPLACE FUNCTION led_closed() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER AS $$
BEGIN
  DELETE FROM state_task_answer_heads WHERE question = NEW.id;
  RETURN NULL;
END $$;
DROP TRIGGER IF EXISTS questions_led ON state_compute_scheduling;
CREATE TRIGGER questions_led AFTER UPDATE OF state ON state_compute_scheduling
  FOR EACH ROW WHEN (OLD.state = 'open' AND NEW.state <> 'open') EXECUTE FUNCTION led_closed();

-- …LEASED TOGETHER as $15 until $16 to device $17 — unless it holds $18 packs
-- already.
CREATE OR REPLACE FUNCTION factory_take_pack(text, bigint, text, text, text, text, text, bigint, bigint, text, text, text, text, bigint,
  text, text, text, bigint)
RETURNS TABLE (id text, verb text, request text, due_at text, producer text) LANGUAGE sql AS $$
  UPDATE state_compute_work q SET (lease_prev, leased_to_prev, lease_prev_until) = (SELECT * FROM factory_supersede(q.lease_prev,
      q.leased_to_prev, q.lease_prev_until, q.lease, q.leased_to, q.lease_until, $1)),
    lease = $15, lease_until = $16, leased_to = $17, leased_at = $1
  WHERE q.id IN (SELECT factory_take($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14))
    AND (SELECT count(DISTINCT h.lease) FROM state_compute_work h WHERE h.leased_to = $17 AND h.lease_until > $1 AND h.lease <> $15) < $18
  RETURNING q.id, q.verb, q.request, q.due_at, q.producer
$$;

-- …EACH PRODUCER CHARGED ITS ROWS at its mean fight over its weight ($1: [{ p, n }]).
CREATE OR REPLACE FUNCTION factory_charge(text) RETURNS void LANGUAGE sql AS $$
  SELECT 1 FROM state_producer_compute_usage WHERE producer IN (SELECT p FROM jsonb_to_recordset($1::jsonb) AS x(p text)) ORDER BY producer FOR UPDATE;
  UPDATE state_producer_compute_usage u SET used = u.used + u.answer_ms * x.n / u.weight
  FROM jsonb_to_recordset($1::jsonb) AS x(p text, n bigint) WHERE u.producer = x.p
$$;

-- …BOTH IN ONE STATEMENT, the pick before the charge's lock.
CREATE OR REPLACE FUNCTION factory_lease_pack(text, bigint, text, text, text, text, text, bigint, bigint, text, text, text, text, bigint,
  text, text, text, bigint)
RETURNS TABLE (id text, verb text, request text, due_at text, producer text) LANGUAGE sql AS $$
  WITH taken AS MATERIALIZED (SELECT * FROM factory_take_pack($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)),
  charged AS MATERIALIZED (SELECT factory_charge((SELECT jsonb_agg(jsonb_build_object('p', c.producer, 'n', c.n) ORDER BY c.producer)
    FROM (SELECT t.producer, count(*) AS n FROM taken t GROUP BY t.producer) c)::text))
  SELECT t.id, t.verb, t.request, t.due_at, t.producer FROM taken t CROSS JOIN charged
$$;

-- WHO IS ASKING: an id that asked since $1, OR one holding a live lease at $2 —
-- a protocol 8 client deep in a ten-minute pack asks only as it ends, and
-- counting it out would hand its share of the last rows to the fast.
CREATE OR REPLACE FUNCTION factory_asking(text, text) RETURNS SETOF text LANGUAGE sql STABLE AS $$
  SELECT id FROM entity_compute_devices WHERE asked_at > $1
  UNION SELECT leased_to FROM state_compute_work WHERE leased_to IS NOT NULL AND lease_until > $2
$$;

-- THE OPEN ROWS NOBODY WAITS ON and the computers asking, at $1: the settler's
-- snapshot (worker/settler.js `snapshotTick`, every ten seconds, open counted as
-- far as any pack could need), read while under a minute old — six missed ticks
-- mean the daemon is down. Only then counted here: the open past the asking
-- since $3 times $4, the rows this pack would hold (worker/verify.js `packRows`).
-- $2 is unused, kept so the procedure that calls this stays as it is.
CREATE OR REPLACE FUNCTION factory_open(text, bigint, text, bigint) RETURNS TABLE (open bigint, asking bigint)
LANGUAGE sql STABLE AS $$
  WITH s AS (SELECT f.open, f.asking FROM state_factory_snapshot f WHERE f.id = 1
      AND f.at >= to_char(($1::timestamptz AT TIME ZONE 'UTC') - interval '60 seconds', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')),
  a AS (SELECT count(*) AS n FROM factory_asking($3, $1) WHERE NOT EXISTS (SELECT 1 FROM s))
  SELECT s.open, s.asking FROM s
  UNION ALL
  SELECT (SELECT count(*) FROM (SELECT 1 FROM state_compute_work WHERE state = 'open' AND waits = 0
      AND (lease_until IS NULL OR lease_until < $1)
      LIMIT GREATEST(1, (SELECT n FROM a)) * $4) o), (SELECT n FROM a)
  WHERE NOT EXISTS (SELECT 1 FROM s)
$$;

-- A QUESTION AFTER ITS DEMANDS MOVED: the earliest deadline still standing, the
-- larger wait and that deadline's producer and initiator — or withdrawn, nobody asking.
CREATE OR REPLACE FUNCTION factory_restate(text) RETURNS void LANGUAGE sql AS $$
  UPDATE state_compute_work q SET due_at = l.due, waits = l.waits, producer = l.producer, initiator = l.initiator
    FROM (SELECT min(due_at) AS due, max(waits) AS waits, count(*) AS n, (array_agg(producer ORDER BY due_at, producer))[1] AS producer,
            (array_agg(initiator ORDER BY due_at, producer))[1] AS initiator
          FROM relation_task_consumers WHERE question = $1 AND state = 'live') l
    WHERE q.id = $1 AND l.n > 0;
  UPDATE state_compute_work SET state = 'withdrawn', lease = NULL, lease_until = NULL, leased_to = NULL
    WHERE id = $1 AND state = 'open' AND NOT EXISTS (SELECT 1 FROM relation_task_consumers WHERE question = $1 AND state = 'live');
$$;

-- COVERED (`record_candidate_answers.credited = 2`): on the questions $1 (json), an agreeing answer
-- still unpaid whose owner or network a paid witness of the same answer already
-- is — no witness rule can ever pay it. Marked where the witnesses are paid
-- (worker/tasks.js `credit`); returns how many.
CREATE OR REPLACE FUNCTION factory_cover(text) RETURNS bigint LANGUAGE sql AS $$
  WITH c AS (UPDATE record_candidate_answers a SET credited = 2
      WHERE a.question IN (SELECT jsonb_array_elements_text($1::jsonb)) AND a.credited = 0 AND a.void = 0 AND a.official = 0
        AND EXISTS (SELECT 1 FROM record_candidate_answers w WHERE w.question = a.question AND w.credited = 1 AND w.void = 0 AND w.official = 0
                    AND w.canon = a.canon AND (w.owner = a.owner OR (coalesce(a.net, '') <> '' AND w.net = a.net)))
      RETURNING 1)
  SELECT count(*) FROM c
$$;

-- AN ASK THAT IS OVER KEEPS NO CODE: a demand no longer live and a question no
-- longer open lose a user or bot initiator (the settler's pass, dial
-- `initiator_forget_ms`). The rows erased.
CREATE OR REPLACE FUNCTION factory_forget_codes() RETURNS bigint LANGUAGE sql AS $$
  -- Separate ranges skip the active state through the existing state indexes;
  -- a not-equal filter would walk active rows before reaching a bounded batch.
  WITH d_before AS MATERIALIZED (SELECT producer,ref FROM relation_task_consumers
       WHERE (initiator LIKE 'user:%' OR initiator LIKE 'bot:%') AND state < 'live' FOR UPDATE SKIP LOCKED),
  d_after AS MATERIALIZED (SELECT producer,ref FROM relation_task_consumers
       WHERE (initiator LIKE 'user:%' OR initiator LIKE 'bot:%') AND state > 'live' FOR UPDATE SKIP LOCKED),
  ds AS MATERIALIZED (SELECT * FROM d_before UNION ALL SELECT * FROM d_after LIMIT 1000),
  q_before AS MATERIALIZED (SELECT id FROM state_compute_work
       WHERE (initiator LIKE 'user:%' OR initiator LIKE 'bot:%') AND state < 'open' FOR UPDATE SKIP LOCKED),
  q_after AS MATERIALIZED (SELECT id FROM state_compute_work
       WHERE (initiator LIKE 'user:%' OR initiator LIKE 'bot:%') AND state > 'open' FOR UPDATE SKIP LOCKED),
  qs AS MATERIALIZED (SELECT * FROM q_before UNION ALL SELECT * FROM q_after LIMIT 1000),
  d AS (UPDATE relation_task_consumers x SET initiator = NULL FROM ds
          WHERE x.producer=ds.producer AND x.ref=ds.ref AND x.state <> 'live' RETURNING 1),
  q AS (UPDATE state_compute_work x SET initiator = NULL FROM qs
          WHERE x.id=qs.id AND x.state <> 'open' RETURNING 1)
  SELECT (SELECT count(*) FROM d) + (SELECT count(*) FROM q)
$$;

-- AN OFFICIAL CHECK OF ANOTHER ENGINE IS NONE: the official machines run the
-- served engine $1 alone, so a dispute or a non-deterministic question no
-- answer of $1 is in can neither be settled nor refuse anyone, and a spot of a
-- fact $1 did not form cannot check it. The first is asked of the volunteers
-- again, as a fact of another engine is (worker/tasks.js `demandAll`): open,
-- its answers carried, and withdrawn where no live demand waits
-- (`factory_restate`); the second is the fact it was. Any answer of $1 keeps it
-- the official machines', a void one too: the pick reads the index alone, not
-- every answer of tens of thousands of questions. At most $2 a call;
-- returns how many moved. Definer, as `factory_forget_past`.
CREATE OR REPLACE FUNCTION factory_moot_checks(text, bigint) RETURNS bigint
LANGUAGE plpgsql SECURITY DEFINER AS $$
DECLARE v text[]; n bigint; qid text;
BEGIN
  SELECT coalesce(array_agg(t.id), '{}') INTO v FROM (
    SELECT q.id FROM state_compute_work q WHERE q.state IN ('disputed', 'nondeterministic')
       AND NOT EXISTS (SELECT 1 FROM state_compute_candidate_answers a WHERE a.question = q.id AND a.engine = $1)
     LIMIT $2 FOR UPDATE OF q SKIP LOCKED) t;
  PERFORM compute_open_task(q.id,'engine_recalibration',$1,to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')) FROM state_compute_work q WHERE q.id=ANY(v) ORDER BY q.id;
  FOREACH qid IN ARRAY v LOOP PERFORM factory_restate(qid); END LOOP;
  WITH back AS (UPDATE state_compute_work q SET state = 'fact', lease = NULL, lease_until = NULL, leased_to = NULL
      WHERE q.id IN (SELECT p.id FROM state_compute_work p WHERE p.state = 'spot' AND p.fact_engine IS DISTINCT FROM $1
                      LIMIT $2 FOR UPDATE SKIP LOCKED)
      RETURNING 1)
  SELECT count(*) INTO n FROM back;
  RETURN cardinality(v) + n;
END $$;

-- Delivery and credit must finish before task detail can expire. Recent asks
-- and answers restart its window; refusal markers still prevent unsafe re-asks.
-- Scope pending-credit checks by question before filtering: the global partial
-- credit index can look empty to the planner while retaining many old entries.
CREATE OR REPLACE FUNCTION factory_past_safe(text, text) RETURNS boolean LANGUAGE plpgsql STABLE AS $$
BEGIN
  RETURN NOT EXISTS (SELECT 1 FROM relation_task_consumers d WHERE d.question = $1
                        AND (d.state = 'live' OR d.asked_at >= $2 OR d.served_at >= $2))
     AND NOT EXISTS (SELECT 1 FROM (SELECT at FROM record_candidate_answers WHERE question=$1 OFFSET 0) a WHERE a.at >= $2)
     AND NOT EXISTS (SELECT 1 FROM
          (SELECT credit_work,credited,void,official FROM record_candidate_answers WHERE question=$1 OFFSET 0) a
          WHERE a.credit_work IS NOT NULL AND a.credited = 0 AND a.void = 0 AND a.official = 0)
     AND NOT EXISTS (SELECT 1 FROM record_refusal_withdrawals r WHERE r.question = $1)
     AND NOT EXISTS (SELECT 1 FROM state_pending_verifications s WHERE s.question = $1)
     AND NOT EXISTS (SELECT 1 FROM entity_measurement_reviews r WHERE r.question = $1 AND EXISTS
          (SELECT 1 FROM state_board_measurement_selection s WHERE s.identity = r.identity AND s.ruler = r.ruler AND s.mode = r.mode AND s.review_id = r.id))
     AND NOT EXISTS (SELECT 1 FROM state_compute_work q WHERE q.id = $1 AND (q.lease_until >= $2 OR q.lease_prev_until >= $2))
     AND NOT EXISTS (SELECT 1 FROM (SELECT * FROM record_candidate_answers WHERE question=$1 OFFSET 0) a
          CROSS JOIN (SELECT state,canon,fact_at FROM state_compute_work WHERE id=$1 OFFSET 0) q
          WHERE q.state = 'fact' AND a.credited = 0 AND a.void = 0 AND a.official = 0
            AND a.canon = q.canon AND a.at <= q.fact_at
            AND NOT EXISTS (SELECT 1 FROM (SELECT * FROM record_candidate_answers WHERE question=a.question OFFSET 0) c
                              WHERE c.credited = 1 AND c.void = 0 AND c.official = 0
                              AND (c.owner = a.owner OR (coalesce(a.net, '') <> '' AND c.net = a.net))));
END $$;

-- Unarchived callers cannot remove history. The private archive service calls
-- factory_forget_archived only after the object and manifest pass readback.
CREATE OR REPLACE FUNCTION factory_forget_past(text, text, text, bigint) RETURNS bigint
LANGUAGE plpgsql SECURITY DEFINER AS $$
BEGIN
  RETURN 0;
END $$;

-- Each archive contains every column of the rows its cleanup can remove.
CREATE OR REPLACE FUNCTION factory_task_detail(text) RETURNS jsonb LANGUAGE plpgsql STABLE AS $$
BEGIN
  RETURN (SELECT jsonb_build_object('question',to_jsonb(q),
    'answers',coalesce((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM record_candidate_answers a WHERE a.question=q.id),'[]'::jsonb),
    'demands',coalesce((SELECT jsonb_agg(to_jsonb(d) ORDER BY d.producer,d.ref) FROM relation_task_consumers d WHERE d.question=q.id),'[]'::jsonb),
    'engine_refusals',coalesce((SELECT jsonb_agg(to_jsonb(e) ORDER BY e.device,e.engine) FROM record_engine_refusals e WHERE e.question=q.id),'[]'::jsonb),
    'led_questions',coalesce((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.engine) FROM state_task_answer_heads l WHERE l.question=q.id),'[]'::jsonb))
  FROM state_compute_work q WHERE q.id=$1);
END $$;

CREATE OR REPLACE FUNCTION factory_archive_candidates(text,bigint) RETURNS jsonb LANGUAGE sql STABLE AS $$
  SELECT coalesce(jsonb_agg(factory_task_detail(t.id)),'[]'::jsonb) FROM (
    SELECT q.id FROM state_compute_work q
     WHERE ((q.state='fact' AND q.fact_at<$1) OR
       (q.state='withdrawn' AND q.opened_at<$1 AND coalesce(q.withdrawn_at,q.opened_at)<$1))
       AND factory_past_safe(q.id,$1)
     LIMIT LEAST(GREATEST($2,0),5000)) t
$$;

-- Lock and recheck the exact archived rows with a fresh statement snapshot.
-- A new ask, credit update or changed result leaves the current detail intact.
CREATE OR REPLACE FUNCTION factory_forget_archived(text,jsonb,text,text) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE v text[];
BEGIN
  IF coalesce($3,'') !~ '^r2:wfsim-pg-backup/task-details-v1/[0-9]{8}/[a-f0-9]{32}/manifest[.]json$'
     OR coalesce($4,'') !~ '^[a-f0-9]{64}$' OR coalesce(jsonb_typeof($2),'')<>'array' OR jsonb_array_length($2)>100 THEN
    RAISE EXCEPTION 'invalid verified task archive';
  END IF;
  SELECT coalesce(array_agg(t.id), '{}') INTO v FROM (
    SELECT q.id FROM state_compute_work q JOIN jsonb_array_elements($2) a ON q.id=a#>>'{question,id}'
     WHERE ((q.state = 'fact' AND q.fact_at < $1) OR (q.state = 'withdrawn' AND q.opened_at < $1 AND coalesce(q.withdrawn_at, q.opened_at) < $1))
       AND factory_past_safe(q.id, $1)
     ORDER BY q.id FOR UPDATE OF q SKIP LOCKED) t;
  SELECT coalesce(array_agg(q.id), '{}') INTO v FROM state_compute_work q WHERE q.id = ANY (v)
     AND ((q.state = 'fact' AND q.fact_at < $1) OR (q.state = 'withdrawn' AND q.opened_at < $1 AND coalesce(q.withdrawn_at, q.opened_at) < $1))
     AND factory_past_safe(q.id, $1)
     AND EXISTS (SELECT 1 FROM jsonb_array_elements($2) a WHERE a#>>'{question,id}'=q.id AND a=factory_task_detail(q.id));
  IF cardinality(v) = 0 THEN RETURN '[]'::jsonb; END IF;
  DELETE FROM record_candidate_answers WHERE question = ANY (v);
  DELETE FROM relation_task_consumers WHERE question = ANY (v) AND state <> 'live';
  DELETE FROM record_engine_refusals WHERE question = ANY (v);
  DELETE FROM state_pending_verifications WHERE question = ANY (v);
  DELETE FROM state_task_answer_heads WHERE question = ANY (v);
  DELETE FROM state_compute_work WHERE id = ANY (v) AND state IN ('fact', 'withdrawn');
  RETURN to_jsonb(v);
END $$;
REVOKE ALL ON FUNCTION factory_task_detail(text),factory_archive_candidates(text,bigint),
  factory_forget_archived(text,jsonb,text,text) FROM PUBLIC;

-- The named calibration read shares one query between the API and official jobs.
-- Cursor pages avoid rescanning earlier groups; bodies are optional for sampling.
SELECT set_config('search_path',quote_ident(current_schema())||',pg_catalog,pg_temp',false);
CREATE OR REPLACE FUNCTION factory_calibration_page(text,text,text,bigint,bigint,boolean,boolean)
RETURNS jsonb LANGUAGE plpgsql STABLE SECURITY DEFINER
SET search_path FROM CURRENT SET plan_cache_mode = force_custom_plan AS $$
BEGIN
  -- Each cursor has different selectivity; its bounds must shape the page plan.
  RETURN (
  SELECT coalesce(jsonb_agg(to_jsonb(t) ORDER BY t.id),'[]'::jsonb) FROM (
    SELECT q.id,CASE WHEN $6 THEN q.request ELSE NULL END AS request,
           max(a.work) AS work,max(a.counts) AS counts,
           '['||string_agg(to_json(a.device)::text,',' ORDER BY a.id)||']' AS devices
      FROM state_compute_work q JOIN record_candidate_answers a ON a.question=q.id
     WHERE q.verb='simulate' AND q.id >= $3 AND (q.id > $3 OR $7)
       AND a.question >= $3
       AND a.credited=1 AND a.official=0 AND a.void=0 AND a.at>$1 AND a.at<=$2
     GROUP BY q.id,q.request ORDER BY q.id
     LIMIT LEAST(GREATEST($5,1),2000) OFFSET GREATEST($4,0)
  ) t);
END;
$$;
REVOKE ALL ON FUNCTION factory_calibration_page(text,text,text,bigint,bigint,boolean,boolean) FROM PUBLIC;

-- A BOARD ROW STILL OWED (worker/board_tasks.js): `queue` holds it.
CREATE OR REPLACE FUNCTION board_owed(text, text, text) RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT EXISTS (SELECT 1 FROM state_board_pending_work WHERE build_id = $1 AND ruler = $2 AND mode = $3)
$$;

-- Inventory exposes task identity and lifecycle, never requests or witnesses.
SELECT set_config('search_path',quote_ident(current_schema())||',pg_catalog,pg_temp',false);
CREATE OR REPLACE FUNCTION factory_inventory_rows(text, text[])
RETURNS TABLE(id text, verb text, state text, fact_engine text, kind text, served_answered boolean)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
  WITH other_live AS MATERIALIZED (
    SELECT d.question,min(d.producer) kind FROM relation_task_consumers d
    WHERE d.state='live' AND NOT (d.producer=ANY($2)) GROUP BY d.question)
  SELECT q.id,q.verb,q.state,q.fact_engine,l.kind,
    CASE WHEN q.state IN ('disputed','nondeterministic')
      THEN EXISTS(SELECT 1 FROM record_candidate_answers a WHERE a.question=q.id AND a.engine=$1) ELSE false END
  FROM state_compute_work q LEFT JOIN other_live l ON l.question=q.id
  WHERE q.state<>'withdrawn' AND ((q.state IN ('fact','spot') AND q.fact_engine=$1)
    OR q.state IN ('disputed','nondeterministic') OR l.question IS NOT NULL)
  UNION ALL SELECT r.question,'','refused',r.engine,NULL,false FROM record_refusal_withdrawals r
    WHERE r.engine=$1 AND NOT EXISTS(SELECT 1 FROM state_compute_work q WHERE q.id=r.question AND q.state<>'withdrawn')
$$;
REVOKE ALL ON FUNCTION factory_inventory_rows(text,text[]) FROM PUBLIC;

CREATE OR REPLACE FUNCTION board_review_wanted(text, text) RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT EXISTS (SELECT 1 FROM entity_measurement_reviews r JOIN state_board_measurement_selection s ON s.identity = r.identity AND s.ruler = r.ruler AND s.mode = r.mode
    WHERE r.ref = $1 AND r.question = $2 AND r.state = 'waiting')
$$;

-- A CLIENT THAT ASKS (worker/verify.js `/work`, worker/sync.js): remembered by
-- its id, its yes kept when given ($4, $5), a refusal lifted once over at $3,
-- its day ($2).
CREATE OR REPLACE FUNCTION verifier_seen(text, text, text, bigint, text) RETURNS void LANGUAGE sql AS $$
  INSERT INTO entity_compute_devices (id, seen) VALUES ($1, $2) ON CONFLICT DO NOTHING;
  UPDATE entity_compute_devices SET consent_v = $4, consent_at = $5
    WHERE id = $1 AND $4 IS NOT NULL AND (consent_v IS DISTINCT FROM $4 OR consent_at IS DISTINCT FROM $5);
  UPDATE entity_compute_devices SET banned = 0 WHERE id = $1 AND banned = 1 AND (refused_until IS NULL OR refused_until <= $3);
  UPDATE entity_compute_devices SET seen = $2 WHERE id = $1 AND seen IS DISTINCT FROM $2;
$$;

-- …ON, its engine and free cores ($3, $4) kept, written at $2 at most once
-- since $5 unless they changed (worker/verify.js `askedOf`).
CREATE OR REPLACE FUNCTION verifier_asked(text, text, text, bigint, text) RETURNS void LANGUAGE sql AS $$
  UPDATE entity_compute_devices SET asked_at = $2, asked_engine = $3, asked_lanes = coalesce($4, asked_lanes)
    WHERE id = $1 AND (asked_at IS NULL OR asked_at < $5 OR asked_engine IS DISTINCT FROM $3
      OR ($4 IS NOT NULL AND asked_lanes IS DISTINCT FROM $4))
$$;

-- …AND WHAT IT ASKED FOR, by the hour (worker/verify.js `asksOf`), added to.
CREATE OR REPLACE FUNCTION verifier_asks(text) RETURNS void LANGUAGE sql AS $$
  INSERT INTO record_hourly_work_requests AS h (verifier, hour, asks, idle, idle_lanes, failed, relayed, relay_failed, direct_failed)
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
CREATE OR REPLACE FUNCTION factory_held(text, text, text, text, bigint) RETURNS SETOF state_compute_work LANGUAGE sql STABLE AS $$
  SELECT * FROM state_compute_work WHERE id IN (SELECT value FROM jsonb_array_elements_text($3::jsonb))
    AND ((lease = $1 AND leased_to = $2 AND ($5 = 1 OR lease_until >= $4)) OR (lease_prev = $1 AND leased_to_prev = $2))
$$;

-- …AND THOSE DEVICE $1 ALREADY ANSWERED on engine $2: a send repeated is not a second witness.
CREATE OR REPLACE FUNCTION factory_given(text, text, text) RETURNS SETOF text LANGUAGE sql STABLE AS $$
  SELECT DISTINCT question FROM state_compute_candidate_answers WHERE device = $1 AND engine = $2 AND question IN (SELECT jsonb_array_elements_text($3::jsonb))
$$;

-- ONE ANSWER WRITTEN, unless its device gave one on this engine already, the
-- unique index `answers_once` the last word on a race. Columns in table order.
CREATE OR REPLACE FUNCTION factory_answer(text, text, text, text, text, bigint, text, text, bigint, bigint, text, bigint, text, bigint)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE held bigint;
BEGIN
  SELECT task_id INTO held FROM state_compute_scheduling WHERE id=$1 FOR UPDATE;
  IF held IS NULL OR $14 IS NULL OR held IS DISTINCT FROM $14 THEN
    RAISE EXCEPTION 'compute task changed before candidate reception' USING ERRCODE='40001';
  END IF;
  INSERT INTO record_candidate_answers (question,engine,device,owner,net,official,canon,result,work,compute_ms,counts,lanes,at,task_id)
  SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14
  WHERE $6=1 OR NOT EXISTS(SELECT 1 FROM state_compute_candidate_answers x WHERE x.question=$1 AND x.device=$3 AND x.engine=$2)
  ON CONFLICT (task_id,device,engine) WHERE official=0 DO NOTHING;
END;
$$;

-- EVERY WRITER OF PRODUCER_USE LOCKS ITS ROWS IN PRODUCER ORDER first (this,
-- the pack's charge, board_sync): one statement taking them in its plan's order
-- deadlocked against another taking them in its own.
-- WHAT ANSWERS COST, folded into each producer's mean: `n` answers of `ms` in
-- all move it as `n` folds of share $2 would ($1: [{ p, ms, n }]).
CREATE OR REPLACE FUNCTION factory_learn(text, double precision) RETURNS void LANGUAGE sql AS $$
  SELECT 1 FROM state_producer_compute_usage WHERE producer IN (SELECT p FROM jsonb_to_recordset($1::jsonb) AS x(p text)) ORDER BY producer FOR UPDATE;
  UPDATE state_producer_compute_usage u SET answer_ms = CASE WHEN u.answer_ms = 0 THEN x.ms / x.n
      ELSE u.answer_ms * power(1 - $2, x.n) + x.ms / x.n * (1 - power(1 - $2, x.n)) END
  FROM jsonb_to_recordset($1::jsonb) AS x(p text, ms double precision, n double precision) WHERE u.producer = x.p
$$;

-- QUESTIONS $1 (json) QUEUED FOR THE SETTLER on engine $2 beside peer $3; an
-- answer on a row a settler pass holds clears its claim.
CREATE OR REPLACE FUNCTION factory_queue(text, text, text, bigint, text) RETURNS void LANGUAGE sql AS $$
  INSERT INTO state_pending_verifications (question, engine, peer, joined, queued_at)
  SELECT value, $2, $3, $4, $5 FROM jsonb_array_elements_text($1::jsonb)
  ON CONFLICT (question, engine, peer) DO UPDATE SET claim = NULL, claimed_at = NULL
$$;

-- WHEN A VOLUNTEER LAST ANSWERED and what it spent ($2 ms on $3 tasks), in its hour.
CREATE OR REPLACE FUNCTION verifier_worked(text, bigint, bigint, text) RETURNS void LANGUAGE sql AS $$
  UPDATE entity_compute_devices SET compute_ms = compute_ms + $2, last_at = $4 WHERE id = $1;
  INSERT INTO record_hourly_compute_totals AS h (verifier, hour, tasks, ms) VALUES ($1, substr($4, 1, 13), $3, $2)
    ON CONFLICT (verifier, hour) DO UPDATE SET tasks = h.tasks + excluded.tasks, ms = h.ms + excluded.ms;
$$;

-- PROTOCOL 8'S RENEWAL: every row $1 ([{ lease, question }]) device $2 names
-- held until $3 at least, as its holder or as the holder before. The ids held.
-- Its rows are locked in id order first (worker/tasks.js `inIdOrder`): inside
-- `board_sync` it holds them while it takes others.
CREATE OR REPLACE FUNCTION factory_renew(text, text, text) RETURNS SETOF text LANGUAGE plpgsql AS $$
DECLARE x text;
BEGIN
  PERFORM 1 FROM state_compute_work q, jsonb_to_recordset($1::jsonb) AS r(lease text, question text)
    WHERE q.id = r.question AND ((q.lease = r.lease AND q.leased_to = $2) OR (q.lease_prev = r.lease AND q.leased_to_prev = $2))
    ORDER BY q.id FOR UPDATE OF q;
  FOR x IN UPDATE state_compute_work q SET lease_until = GREATEST(q.lease_until, $3) FROM jsonb_to_recordset($1::jsonb) AS r(lease text, question text)
      WHERE q.id = r.question AND q.lease = r.lease AND q.leased_to = $2 RETURNING q.id LOOP
    RETURN NEXT x;
  END LOOP;
  FOR x IN UPDATE state_compute_work q SET lease_prev_until = GREATEST(q.lease_prev_until, $3) FROM jsonb_to_recordset($1::jsonb) AS r(lease text, question text)
      WHERE q.id = r.question AND q.lease_prev = r.lease AND q.leased_to_prev = $2 RETURNING q.id LOOP
    RETURN NEXT x;
  END LOOP;
END $$;

-- …AND EVERY LIVE LEASE OF DEVICE $1 AT $2 ON A ROW IT NAMES NOWHERE ($3) goes back.
CREATE OR REPLACE FUNCTION factory_release_unnamed(text, text, text) RETURNS void LANGUAGE sql AS $$
  UPDATE state_compute_work SET lease = NULL, lease_until = NULL, leased_to = NULL
  WHERE id IN (SELECT id FROM state_compute_work WHERE leased_to = $1 AND lease_until > $2
    AND id NOT IN (SELECT value FROM jsonb_array_elements_text($3::jsonb)) ORDER BY id FOR UPDATE)
$$;

-- ROWS A CLIENT'S ENGINE REFUSED ($1: [{ question, step }]) by device $2 of
-- owner $3 on engine $4: kept and given back; on the served engine $5, one
-- refused by $6 owners is withdrawn and written down at $7. The ids withdrawn.
CREATE OR REPLACE FUNCTION factory_refuse(text, text, text, text, text, bigint, text) RETURNS SETOF text LANGUAGE plpgsql AS $$
DECLARE x record;
BEGIN
  INSERT INTO record_engine_refusals (question, device, owner, engine, step, at)
    SELECT r.question, $2, $3, $4, r.step, $7 FROM jsonb_to_recordset($1::jsonb) AS r(question text, step text) ON CONFLICT DO NOTHING;
  UPDATE state_compute_work SET lease = NULL, lease_until = NULL, leased_to = NULL
    WHERE id IN (SELECT id FROM state_compute_work WHERE id IN (SELECT r.question FROM jsonb_to_recordset($1::jsonb) AS r(question text))
      AND leased_to = $2 ORDER BY id FOR UPDATE);
  IF $4 IS DISTINCT FROM $5 THEN RETURN; END IF;
  FOR x IN SELECT e.question, count(DISTINCT e.owner) AS owners FROM record_engine_refusals e
      WHERE e.question IN (SELECT r.question FROM jsonb_to_recordset($1::jsonb) AS r(question text)) AND e.engine = $5
      GROUP BY e.question HAVING count(DISTINCT e.owner) >= $6 ORDER BY e.question LOOP
    UPDATE relation_task_consumers SET state = 'withdrawn' WHERE question = x.question AND state = 'live';
    INSERT INTO record_refusal_withdrawals (question, engine, owners, at) VALUES (x.question, $5, x.owners, $7) ON CONFLICT (question) DO NOTHING;
    PERFORM factory_restate(x.question);
    RETURN NEXT x.question;
  END LOOP;
END $$;

-- WHERE EACH OF $2 (json) STANDS for device $1: its answers counted and credited.
CREATE OR REPLACE FUNCTION factory_states(text, text)
RETURNS TABLE (id text, state text, fact_at text, credited bigint, mine bigint) LANGUAGE sql STABLE AS $$
  SELECT q.id, q.state, q.fact_at,
    (SELECT max(a.credited) FROM record_candidate_answers a WHERE a.question = q.id AND a.device = $1 AND a.void = 0),
    (SELECT count(*) FROM record_candidate_answers a WHERE a.question = q.id AND a.device = $1 AND a.void = 0)
  FROM state_compute_work q WHERE q.id IN (SELECT value FROM jsonb_array_elements_text($2::jsonb)) ORDER BY q.id
$$;

-- WHAT DEVICE $1 EARNED THAT NO ACCOUNT HOLDS, as worker/contribution.js
-- `devicePointsOf` reads it: from hour $2 ('' all of it; NULL while an account
-- holds it), since days $3 and $4, and since hour $5 when given; work, not points.
CREATE OR REPLACE FUNCTION device_points(text, text, text, text, text) RETURNS jsonb LANGUAGE sql STABLE AS $$
  SELECT jsonb_build_object('held', $2 IS NULL,
    'work', CASE WHEN $2 IS NULL OR v.id IS NULL THEN 0 WHEN $2 = '' THEN v.work
      ELSE COALESCE((SELECT sum(h.work) FROM record_hourly_compute_totals h WHERE h.verifier = $1 AND h.hour >= $2), 0) END,
    'recent', CASE WHEN $2 IS NULL OR v.id IS NULL THEN 0
      ELSE COALESCE((SELECT sum(h.work) FROM record_hourly_compute_totals h WHERE h.verifier = $1 AND h.hour >= $2 AND h.hour >= $3), 0) END,
    'week', CASE WHEN $2 IS NULL OR v.id IS NULL THEN 0
      ELSE COALESCE((SELECT sum(h.work) FROM record_hourly_compute_totals h WHERE h.verifier = $1 AND h.hour >= $2 AND h.hour >= $4), 0) END,
    'today', CASE WHEN $5 IS NULL THEN NULL ELSE (SELECT jsonb_build_object('tasks', COALESCE(sum(h.tasks), 0),
      'ms', COALESCE(sum(h.ms), 0), 'work', COALESCE(sum(h.work), 0)) FROM record_hourly_compute_totals h WHERE h.verifier = $1 AND h.hour >= $5) END)
  FROM (SELECT 1) one LEFT JOIN entity_compute_devices v ON v.id = $1 AND v.banned = 0
$$;

-- PROTOCOL 8'S SYNC, ONE CALL AND ONE TRANSACTION (worker/sync.js, which builds
-- `p` and reads `r`; docs/BOARD.md §"The sync in the database"). The order and
-- every gate are worker/sync.js's JS path, step for step, so the two answer
-- alike (scripts/check_sync_proc.mjs). Whatever needs JS raises WS001 and
-- nothing is written: an answer the settler may not take, or a pack demand of
-- a producer whose wanting is not `board_owed`. `outcome` says which reply:
-- idle, busy, banned, early, full, alone (a waited-on head: worker/sync.js
-- leases it), or pack. `use` is what it owes producer_use (`board_sync`).
CREATE OR REPLACE PROCEDURE board_sync_steps(p jsonb, INOUT r jsonb) LANGUAGE plpgsql AS $$
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
  v_next text;
  v_got jsonb;
  v_ds jsonb;
  v_unwanted jsonb := '[]'::jsonb;
  v_wanted jsonb;
  v_gone jsonb;
  v_x record;
  v_learn jsonb := '[]'::jsonb;
  v_have jsonb := '[]'::jsonb;
  v_charges jsonb := '[]'::jsonb;
  v_round bigint;
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
    v_rows := COALESCE((SELECT jsonb_agg(jsonb_build_object('a', x.a, 'id', q.id, 'verb', q.verb, 'producer', q.producer,'task_id',q.task_id) ORDER BY x.k)
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
          (v_x.i #>> '{a,lanes}')::bigint, v_at,(v_x.i ->> 'task_id')::bigint);
      END LOOP;
      v_learn := v_learn || COALESCE((SELECT jsonb_build_array(y.x)
        FROM (SELECT jsonb_agg(jsonb_build_object('p', l.producer, 'ms', l.ms, 'n', l.n) ORDER BY l.producer) AS x
          FROM (SELECT i ->> 'producer' AS producer, sum((i #>> '{a,compute_ms}')::bigint) AS ms, count(*) AS n
            FROM jsonb_array_elements(v_rows) i WHERE (i #>> '{a,compute_ms}')::bigint > 0 GROUP BY 1) l) y
        WHERE y.x IS NOT NULL), '[]'::jsonb);
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
    'points', v_points, 'queued', v_queued, 'use', jsonb_build_object('learn', v_learn, 'charge', '[]'::jsonb));

  -- THE GATES, in worker/sync.js's order.
  SELECT banned, refused_until, last_at, next_at INTO v_me FROM entity_compute_devices WHERE id = v_dev;
  IF v_stale THEN
    UPDATE entity_compute_devices SET next_at = p ->> 'idle_at' WHERE id = v_dev;
    r := r || jsonb_build_object('outcome', 'stale');
    RETURN;
  END IF;
  IF v_me.banned <> 0 THEN
    r := r || jsonb_build_object('outcome', 'banned', 'until', v_me.refused_until);
    RETURN;
  END IF;
  IF NOT (p ->> 'consented')::boolean THEN
    UPDATE entity_compute_devices SET next_at = p ->> 'idle_at' WHERE id = v_dev;
    r := r || jsonb_build_object('outcome', 'idle');
    RETURN;
  END IF;
  IF v_me.next_at IS NOT NULL
      AND v_at < to_char((v_me.next_at::timestamptz - interval '2 seconds') AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') THEN
    r := r || jsonb_build_object('outcome', 'early', 'next_at', v_me.next_at);
    RETURN;
  END IF;
  SELECT count(*) AS questions, count(DISTINCT lease) AS packs INTO v_holds FROM state_compute_work WHERE leased_to = v_dev AND lease_until > v_at;
  IF (p ->> 'want')::bigint <= 0 OR v_holds.packs >= (p ->> 'packs_held')::bigint THEN
    r := r || jsonb_build_object('outcome', 'full');
    RETURN;
  END IF;
  SELECT * INTO v_head FROM factory_head(v_at, 0, v_lanes, v_engine, v_siblings, v_owner, v_net, NULL, v_ceiling, v_grace_s, p ->> 'official_states');
  v_has_head := FOUND;
  v_packable := v_has_head AND v_head.waits = 0 AND (p -> 'packable') ? v_head.verb;
  IF NOT v_has_head AND NOT v_grace THEN
    UPDATE entity_compute_devices SET next_at = p ->> 'idle_at' WHERE id = v_dev;
    r := r || jsonb_build_object('outcome', 'idle');
    RETURN;
  END IF;
  -- A WAITED-ON HEAD goes alone to a computer holding nothing; one holding rows
  -- is handed a pack of the rest (worker/sync.js says why).
  IF NOT v_grace AND NOT v_packable AND v_holds.questions = 0 THEN
    r := r || jsonb_build_object('outcome', 'alone');
    RETURN;
  END IF;
  -- A busy witness chooses the fairest packable head, not a global fallback.
  IF NOT v_grace AND NOT v_packable THEN
    SELECT * INTO v_head FROM factory_head(v_at, 0, v_lanes, v_engine, v_siblings, v_owner, v_net, NULL,
      v_ceiling, v_grace_s, p ->> 'official_states', (p -> 'packable')::text);
    v_packable := FOUND;
    IF NOT v_packable THEN
      UPDATE entity_compute_devices SET next_at = p ->> 'idle_at' WHERE id = v_dev;
      r := r || jsonb_build_object('outcome', 'idle');
      RETURN;
    END IF;
  END IF;
  SELECT * INTO v_fair FROM factory_open(v_at, NULL, p #>> '{fair,asking_since}', (p #>> '{fair,per}')::bigint);
  IF (p #>> '{fair,cap}')::bigint <> 0 AND v_fair.asking > (p #>> '{fair,cap}')::bigint
      AND NOT (v_me.last_at IS NOT NULL AND v_me.last_at > p #>> '{fair,cap_since}') THEN
    UPDATE entity_compute_devices SET next_at = p ->> 'idle_at' WHERE id = v_dev;
    r := r || jsonb_build_object('outcome', 'busy');
    RETURN;
  END IF;
  -- WHILE WORK IS SHORT an account past its fair share (the dice are JS's) is handed nothing.
  v_short := v_fair.open / GREATEST(1, v_fair.asking) < (p #>> '{fair,short_rows}')::bigint;
  IF v_short AND (p #>> '{fair,throttle}')::boolean THEN
    UPDATE entity_compute_devices SET next_at = p ->> 'idle_at' WHERE id = v_dev;
    r := r || jsonb_build_object('outcome', 'idle');
    RETURN;
  END IF;
  v_size := GREATEST(1, LEAST((p #>> '{pack,max}')::bigint, (p #>> '{pack,by_pace}')::bigint, v_fair.open / GREATEST(1, v_fair.asking)));
  -- A PICK THAT WAS DEAD IS PICKED AGAIN, under the same lease, as
  -- worker/tasks.js `nextPack` does (`PICK_ROUNDS`): until the pack is full, a
  -- pick finds nothing dead, or PICK_ROUNDS picks. A row of the pick no longer owed
  -- is withdrawn where it is found and left out, its question restated once
  -- all are (`sift`). Each pick is charged as its own (`board_sync`).
  FOR v_round IN 1..8 LOOP
    EXIT WHEN jsonb_array_length(v_have) >= v_size;
    v_got := COALESCE((SELECT jsonb_agg(to_jsonb(t)) FROM factory_take_pack(v_at, v_lanes, v_engine, v_siblings, v_owner, v_net, NULL, v_ceiling,
      v_grace_s, (p -> 'packable')::text, p #>> '{grace,exclude}', p #>> '{grace,join}', CASE WHEN v_packable THEN v_head.producer END,
      v_size - jsonb_array_length(v_have), v_lease, CASE WHEN v_short THEN p #>> '{pack,short_until}' ELSE p #>> '{pack,long_until}' END, v_dev,
      (p ->> 'packs_held')::bigint) t), '[]'::jsonb);
    EXIT WHEN jsonb_array_length(v_got) = 0;
    v_charges := v_charges || jsonb_build_array((SELECT jsonb_agg(jsonb_build_object('p', c.p, 'n', c.n) ORDER BY c.p)
      FROM (SELECT t ->> 'producer' AS p, count(*) AS n FROM jsonb_array_elements(v_got) t GROUP BY 1) c));
    v_ds := COALESCE((SELECT jsonb_agg(to_jsonb(d) ORDER BY d.k) FROM (
        SELECT d.question, d.producer, d.ref, d.payload, row_number() OVER (ORDER BY d.question, d.producer, d.ref) AS k
        FROM relation_task_consumers d JOIN state_compute_work q ON q.id = d.question WHERE q.lease = v_lease AND q.leased_to = v_dev AND d.state = 'live'
          AND q.id IN (SELECT t ->> 'id' FROM jsonb_array_elements(v_got) t)) d), '[]'::jsonb);
    IF EXISTS (SELECT 1 FROM jsonb_array_elements(v_ds) e WHERE NOT (p -> 'owed') ? (e ->> 'producer')) THEN
      RAISE EXCEPTION 'board_sync: a pack demand of a producer not owed in sql' USING ERRCODE = 'WS001';
    END IF;
    v_unwanted := COALESCE((SELECT jsonb_agg(jsonb_build_array(e ->> 'producer', e ->> 'ref', e ->> 'question')) FROM jsonb_array_elements(v_ds) e
      WHERE NOT CASE WHEN e ->> 'producer' = 'board_review' THEN board_review_wanted(e ->> 'ref', e ->> 'question')
        ELSE board_owed((e ->> 'payload')::jsonb ->> 'identity', (e ->> 'payload')::jsonb ->> 'ruler', (e ->> 'payload')::jsonb ->> 'mode') END),
      '[]'::jsonb);
    IF jsonb_array_length(v_unwanted) > 0 THEN
      UPDATE relation_task_consumers SET state = 'withdrawn' WHERE (producer, ref) IN (SELECT d.producer, d.ref FROM relation_task_consumers d
        JOIN jsonb_array_elements(v_unwanted) u ON d.producer = u ->> 0 AND d.ref = u ->> 1 ORDER BY d.producer, d.ref FOR UPDATE OF d);
      PERFORM factory_restate(g.id) FROM (SELECT DISTINCT u ->> 2 AS id FROM jsonb_array_elements(v_unwanted) u ORDER BY 1) g;
    END IF;
    v_wanted := COALESCE((SELECT jsonb_agg(e ORDER BY (e ->> 'k')::bigint) FROM jsonb_array_elements(v_ds) e
      WHERE NOT v_unwanted @> jsonb_build_array(jsonb_build_array(e ->> 'producer', e ->> 'ref', e ->> 'question'))), '[]'::jsonb);
    v_gone := COALESCE((SELECT jsonb_agg(t ->> 'id') FROM jsonb_array_elements(v_got) t
      WHERE NOT EXISTS (SELECT 1 FROM jsonb_array_elements(v_wanted) e WHERE e ->> 'question' = t ->> 'id')), '[]'::jsonb);
    IF jsonb_array_length(v_gone) > 0 THEN
      UPDATE state_compute_work SET lease = NULL, lease_until = NULL, leased_to = NULL
        WHERE id IN (SELECT id FROM state_compute_work WHERE id IN (SELECT jsonb_array_elements_text(v_gone)) AND lease = v_lease ORDER BY id FOR UPDATE);
      -- A ROW LEFT WITH NO LIVE DEMAND is withdrawn, not only given back: still
      -- open, it heads every pack's pick again and every sync is handed nothing.
      PERFORM factory_restate(g.id) FROM (SELECT DISTINCT jsonb_array_elements_text(v_gone) AS id ORDER BY 1) g;
    END IF;
    v_have := v_have || COALESCE((SELECT jsonb_agg(jsonb_build_object('question', t ->> 'id', 'verb', t ->> 'verb', 'request', t ->> 'request',
        'due_at', t ->> 'due_at', 'demands', (SELECT COALESCE(jsonb_agg(jsonb_build_object('producer', e ->> 'producer', 'ref', e ->> 'ref',
          'payload', e ->> 'payload') ORDER BY (e ->> 'k')::bigint), '[]'::jsonb) FROM jsonb_array_elements(v_wanted) e WHERE e ->> 'question' = t ->> 'id')))
      FROM jsonb_array_elements(v_got) t WHERE NOT v_gone ? (t ->> 'id')), '[]'::jsonb);
    EXIT WHEN jsonb_array_length(v_gone) = 0;
  END LOOP;
  r := jsonb_set(r, '{use,charge}', v_charges);
  v_got := COALESCE((SELECT jsonb_agg(t ORDER BY t ->> 'due_at' COLLATE "C", t ->> 'question' COLLATE "C") FROM jsonb_array_elements(v_have) t), '[]'::jsonb);
  IF jsonb_array_length(v_got) = 0 THEN
    UPDATE entity_compute_devices SET next_at = p ->> 'idle_at' WHERE id = v_dev;
    r := r || jsonb_build_object('outcome', 'idle');
    RETURN;
  END IF;
  IF p #> '{pack,refill}' IS NULL THEN
    v_next := p ->> 'floor_at';
  ELSE
    v_next := to_char((v_at::timestamptz + make_interval(secs => round(
      LEAST((p #>> '{pack,refill,floor}')::numeric, GREATEST(0,
        jsonb_array_length(v_got) * (p #>> '{pack,refill,pace}')::numeric - (p #>> '{pack,refill,low}')::numeric))
        * (p #>> '{pack,refill,scale}')::numeric / 100) / 1000)) AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
  END IF;
  UPDATE entity_compute_devices SET next_at = v_next WHERE id = v_dev;
  r := r || jsonb_build_object('outcome', 'pack', 'short', v_short, 'lease', v_lease, 'questions', v_got);
END $$;

-- …AND WHAT IT OWES PRODUCER_USE, WRITTEN LAST. Every sync writes those rows, so
-- a sync holding them from its answers through the pick of its pack made one
-- slow pick every other sync's lock timeout. They are locked whole, in producer
-- order, as two syncs taking them in opposite orders deadlocked.
CREATE OR REPLACE PROCEDURE board_sync(p jsonb, INOUT r jsonb) LANGUAGE plpgsql AS $$
DECLARE
  v_use jsonb;
  v_x record;
BEGIN
  CALL board_sync_steps(p, r);
  v_use := r -> 'use';
  r := r - 'use';
  IF jsonb_array_length(v_use -> 'learn') + jsonb_array_length(v_use -> 'charge') = 0 THEN RETURN; END IF;
  PERFORM 1 FROM state_producer_compute_usage ORDER BY producer FOR UPDATE;
  FOR v_x IN SELECT l FROM jsonb_array_elements(v_use -> 'learn') WITH ORDINALITY x(l, k) ORDER BY k LOOP
    PERFORM factory_learn(v_x.l::text, (p ->> 'share')::double precision);
  END LOOP;
  FOR v_x IN SELECT c FROM jsonb_array_elements(v_use -> 'charge') WITH ORDINALITY x(c, k) ORDER BY k LOOP
    PERFORM factory_charge(v_x.c::text);
  END LOOP;
END $$;
