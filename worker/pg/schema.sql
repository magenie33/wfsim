-- THE FACT FACTORY (database `wfsim`; docs/BOARD.md §"The fact factory"): the
-- volunteers, the work they are credited, and the questions they answer. The
-- library's other tables are in library.sql, in the same database.
--
-- Instants are ISO-ms TEXT (docs/NAMING.md §9) and every text column is
-- COLLATE "C", so they compare byte by byte whatever the database's locale.
-- Every whole number is BIGINT: work sums past 2^31. Applied by the role that
-- owns the tables:
--
--   psql -d wfsim -f worker/pg/schema.sql

-- A VERIFYING CLIENT, by the random id its browser made for itself — joined to
-- no submission, and to an account only where its owner claimed it (`devices`,
-- in `wfsim_accounts`). Kept so a client caught once can be refused and
-- everything it agreed to re-checked. `compute_ms` is the time its fights took
-- on its machine, summed; `work` the work of every fact it measured, summed
-- (docs/BOARD.md §"Contribution").
CREATE TABLE IF NOT EXISTS entity_compute_devices (
  id            TEXT COLLATE "C" PRIMARY KEY,
  agreed        BIGINT NOT NULL DEFAULT 0,
  banned        BIGINT NOT NULL DEFAULT 0,
  seen          TEXT COLLATE "C" NOT NULL,
  compute_ms    BIGINT NOT NULL DEFAULT 0,
  work          BIGINT NOT NULL DEFAULT 0,
  last_at       TEXT COLLATE "C",
  -- THE READER'S YES: the statement version they agreed to, and when.
  consent_v     BIGINT,
  consent_at    TEXT COLLATE "C",
  -- A REFUSAL IS A COOL-DOWN: how many this client has had, and when the last
  -- one ends (scripts/live_orders.mjs `ban`).
  refusals      BIGINT NOT NULL DEFAULT 0,
  refused_until TEXT COLLATE "C",
  -- …and the facts it has been part of since its last refusal, which forgive
  -- one each `FACTS_PER_REFUSAL_FORGIVEN` (worker/tasks.js).
  clean         BIGINT NOT NULL DEFAULT 0,
  -- WHEN IT LAST ASKED FOR WORK, on which engine and with how many cores free,
  -- to the minute — what tells an idle page from a closed one (verify.js `askedOf`).
  asked_at      TEXT COLLATE "C",
  asked_engine  TEXT COLLATE "C",
  asked_lanes   BIGINT,
  -- THE SOONEST A PROTOCOL 8 SYNC IS HANDED WORK AGAIN (worker/sync.js): a sync
  -- before it is served its results and renewals and nothing new.
  next_at       TEXT COLLATE "C"
);
CREATE INDEX IF NOT EXISTS verifiers_last ON entity_compute_devices (last_at);
CREATE INDEX IF NOT EXISTS verifiers_asked ON entity_compute_devices (asked_at);
ALTER TABLE entity_compute_devices ADD COLUMN IF NOT EXISTS signatures BIGINT NOT NULL DEFAULT 0;

-- THE OFFICIAL POOL: work that was credited and then left its holder — a
-- deleted account's (`deleted_account`), or taken back from a refused client
-- (`refused`). Points are compute and stay counted; the pool holds no name.
CREATE TABLE IF NOT EXISTS record_unowned_credit_transfers (
  at     TEXT COLLATE "C" NOT NULL,
  source TEXT COLLATE "C" NOT NULL,
  work   BIGINT NOT NULL
);

-- THE SAME WORK BY THE HOUR IT WAS CREDITED, for the rankings' recent periods
-- (docs/BOARD.md §"Contribution"). A refused client's hours count for nothing,
-- as its total does: the ranking reads them through `entity_compute_devices.banned`.
CREATE TABLE IF NOT EXISTS record_hourly_compute_totals (
  verifier TEXT COLLATE "C" NOT NULL,
  -- THE UTC HOUR, `YYYY-MM-DDTHH`: a reader's own "today" is whole hours of it
  -- in their time zone, and the rankings' days are its prefix.
  hour     TEXT COLLATE "C" NOT NULL,
  work     BIGINT NOT NULL DEFAULT 0,
  -- …AND WHAT THE BROWSER SENT IN THAT HOUR: results, and what their fights took.
  tasks    BIGINT NOT NULL DEFAULT 0,
  ms       BIGINT NOT NULL DEFAULT 0,
  PRIMARY KEY (verifier, hour)
);
CREATE INDEX IF NOT EXISTS verifier_hours_by_hour ON record_hourly_compute_totals (hour);
ALTER TABLE record_hourly_compute_totals ADD COLUMN IF NOT EXISTS signatures BIGINT NOT NULL DEFAULT 0;

-- WHAT A DEVICE ASKED FOR, by the UTC hour (docs/BOARD.md §"Outages"): `asks`
-- the server saw; `idle` the asks that brought nothing and `idle_lanes` the
-- cores they left waiting; `failed` those that never reached the server. The
-- last three are the page's own count, sent with its next ask that lands.
CREATE TABLE IF NOT EXISTS record_hourly_work_requests (
  verifier   TEXT COLLATE "C" NOT NULL,
  hour       TEXT COLLATE "C" NOT NULL,
  asks       BIGINT NOT NULL DEFAULT 0,
  idle       BIGINT NOT NULL DEFAULT 0,
  idle_lanes BIGINT NOT NULL DEFAULT 0,
  failed     BIGINT NOT NULL DEFAULT 0,
  -- …AND BY ROAD (docs/DISTRIBUTION.md §"The relay"): the asks the relay
  -- carried, and the page's asks each road dropped before the other was tried.
  relayed      BIGINT NOT NULL DEFAULT 0,
  relay_failed BIGINT NOT NULL DEFAULT 0,
  direct_failed BIGINT NOT NULL DEFAULT 0,
  PRIMARY KEY (verifier, hour)
);
CREATE INDEX IF NOT EXISTS verifier_asks_by_hour ON record_hourly_work_requests (hour);

-- WHEN THE FACTORY WAS PAUSED OR ITS WATCHDOG FOUND SOMETHING WRONG, from the
-- first sight to the last (docs/BOARD.md §"Outages"): `pause`, or `alert:<cause>`.
CREATE TABLE IF NOT EXISTS record_factory_incidents (
  kind       TEXT COLLATE "C" NOT NULL,
  started_at TEXT COLLATE "C" NOT NULL,
  last_at    TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY (kind, started_at)
);

-- POINTS GIVEN BY HAND, an outage's compensation among them: each is ALSO in
-- `verifier_hours`, written in the same transaction, and this says which part
-- of an hour it was and why (`reason`, an id the ledger names).
CREATE TABLE IF NOT EXISTS record_compute_credit_adjustments (
  id       BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  verifier TEXT COLLATE "C" NOT NULL,
  hour     TEXT COLLATE "C" NOT NULL,
  work     BIGINT NOT NULL,
  reason   TEXT COLLATE "C" NOT NULL,
  at       TEXT COLLATE "C" NOT NULL
);
CREATE INDEX IF NOT EXISTS adjustments_by_verifier ON record_compute_credit_adjustments (verifier, hour);

-- Computations name frozen inputs; tasks name execution rounds; results survive both queues and task cleanup.
CREATE TABLE IF NOT EXISTS entity_compute_specs (
  id TEXT COLLATE "C" PRIMARY KEY,
  verb TEXT COLLATE "C" NOT NULL,
  request TEXT COLLATE "C" NOT NULL,
  UNIQUE (id, verb)
);
CREATE TABLE IF NOT EXISTS entity_compute_tasks (
  id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  computation TEXT COLLATE "C" NOT NULL REFERENCES entity_compute_specs(id),
  generation BIGINT NOT NULL,
  created_at TEXT COLLATE "C" NOT NULL,
  reason TEXT COLLATE "C" NOT NULL,
  origin TEXT COLLATE "C" NOT NULL DEFAULT 'native',
  requested_engine TEXT COLLATE "C",
  UNIQUE (computation, generation),
  UNIQUE (id, computation)
);
-- Each admission preserves its source independently of mutable delivery state.
CREATE TABLE IF NOT EXISTS relation_compute_task_sources (
  task_id BIGINT NOT NULL REFERENCES entity_compute_tasks(id) ON DELETE CASCADE,
  producer TEXT COLLATE "C" NOT NULL CHECK (length(btrim(producer)) > 0),
  ref TEXT COLLATE "C" NOT NULL CHECK (length(btrim(ref)) > 0),
  trigger_id TEXT COLLATE "C" NOT NULL CHECK (length(btrim(trigger_id)) > 0),
  asked_at TEXT COLLATE "C" NOT NULL,
  payload TEXT COLLATE "C" NOT NULL CHECK (jsonb_typeof(payload::jsonb) = 'object'),
  PRIMARY KEY (task_id, producer, ref, trigger_id)
);
CREATE INDEX IF NOT EXISTS compute_task_sources_ref ON relation_compute_task_sources(producer,ref,task_id);
ALTER TABLE relation_compute_task_sources ADD COLUMN IF NOT EXISTS requirements JSONB NOT NULL DEFAULT '{}';


CREATE TABLE IF NOT EXISTS entity_compute_results (
  id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  computation TEXT COLLATE "C" NOT NULL REFERENCES entity_compute_specs(id),
  task_id BIGINT NOT NULL,
  canon TEXT COLLATE "C" NOT NULL,
  engine TEXT COLLATE "C" NOT NULL,
  accepted_at TEXT COLLATE "C" NOT NULL,
  proof_archive TEXT COLLATE "C",
  proof_digest TEXT COLLATE "C",
  FOREIGN KEY (task_id, computation) REFERENCES entity_compute_tasks(id, computation),
  UNIQUE (id, computation, engine, accepted_at)
);
CREATE INDEX IF NOT EXISTS compute_results_by_task ON entity_compute_results(task_id, id);
CREATE INDEX IF NOT EXISTS compute_results_by_computation ON entity_compute_results(computation, id);
CREATE TABLE IF NOT EXISTS record_compute_result_proofs (
  result_id BIGINT PRIMARY KEY REFERENCES entity_compute_results(id) ON DELETE CASCADE,
  witnesses TEXT COLLATE "C" NOT NULL
);
CREATE OR REPLACE FUNCTION compute_immutable_content() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_TABLE_NAME IN ('entity_compute_specs','record_compute_result_proofs') THEN
    IF NEW IS DISTINCT FROM OLD THEN RAISE EXCEPTION 'frozen computation input is immutable'; END IF;
  ELSIF TG_TABLE_NAME='entity_compute_results' THEN
    IF (NEW.id,NEW.computation,NEW.task_id,NEW.canon,NEW.engine,NEW.accepted_at)
      IS DISTINCT FROM (OLD.id,OLD.computation,OLD.task_id,OLD.canon,OLD.engine,OLD.accepted_at) THEN
      RAISE EXCEPTION 'accepted result content is immutable';
    END IF;
  END IF;
  RETURN NEW;
END $$;
DROP TRIGGER IF EXISTS compute_immutable_input ON entity_compute_specs;
CREATE TRIGGER compute_immutable_input BEFORE UPDATE ON entity_compute_specs
  FOR EACH ROW EXECUTE FUNCTION compute_immutable_content();
DROP TRIGGER IF EXISTS compute_immutable_result ON entity_compute_results;
CREATE TRIGGER compute_immutable_result BEFORE UPDATE ON entity_compute_results
  FOR EACH ROW EXECUTE FUNCTION compute_immutable_content();
DROP TRIGGER IF EXISTS compute_immutable_proof ON record_compute_result_proofs;
CREATE TRIGGER compute_immutable_proof BEFORE UPDATE ON record_compute_result_proofs
  FOR EACH ROW EXECUTE FUNCTION compute_immutable_content();
CREATE TABLE IF NOT EXISTS record_compute_result_selections (
  id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  computation TEXT COLLATE "C" NOT NULL,
  task_id BIGINT NOT NULL,
  -- Historical keys survive an output's explicitly expired access policy.
  result_id BIGINT,
  previous_result_id BIGINT,
  reason TEXT COLLATE "C" NOT NULL,
  at TEXT COLLATE "C" NOT NULL
);
CREATE INDEX IF NOT EXISTS compute_selections_by_computation ON record_compute_result_selections(computation, id);
CREATE OR REPLACE FUNCTION factory_default_zone(text) RETURNS bigint
LANGUAGE sql IMMUTABLE AS $$
  SELECT CASE WHEN $1 IN ('user_calculation','chat','survey') THEN 0
    WHEN $1='board_review' THEN 1 ELSE 3 END::bigint
$$;
CREATE SEQUENCE IF NOT EXISTS compute_queue_position;
SELECT setval('compute_queue_position',GREATEST((SELECT coalesce(max(id),0) FROM entity_compute_tasks),
  (SELECT last_value FROM compute_queue_position)),true);
CREATE TABLE IF NOT EXISTS state_compute_scheduling (
  id TEXT COLLATE "C" PRIMARY KEY,
  -- This dispatch key is a constrained copy of the immutable specification's verb.
  verb TEXT COLLATE "C" NOT NULL,
  task_id BIGINT NOT NULL,
  state TEXT COLLATE "C" NOT NULL,
  due_at TEXT COLLATE "C" NOT NULL,
  waits BIGINT NOT NULL DEFAULT 0,
  opened_at TEXT COLLATE "C" NOT NULL,
  lanes TEXT COLLATE "C",
  lease TEXT COLLATE "C",
  lease_until TEXT COLLATE "C",
  leased_to TEXT COLLATE "C",
  leased_at TEXT COLLATE "C",
  lease_prev TEXT COLLATE "C",
  leased_to_prev TEXT COLLATE "C",
  lease_prev_until TEXT COLLATE "C",
  needs BIGINT,
  odd BIGINT NOT NULL DEFAULT 0,
  producer TEXT COLLATE "C" NOT NULL DEFAULT '',
  initiator TEXT COLLATE "C",
  withdrawn_at TEXT COLLATE "C",
  result_id BIGINT,
  fact_engine TEXT COLLATE "C",
  fact_at TEXT COLLATE "C",
  FOREIGN KEY (id, verb) REFERENCES entity_compute_specs(id, verb),
  FOREIGN KEY (task_id, id) REFERENCES entity_compute_tasks(id, computation),
  -- Selection metadata cannot disagree with the referenced authoritative result.
  FOREIGN KEY (result_id, id, fact_engine, fact_at)
    REFERENCES entity_compute_results(id, computation, engine, accepted_at),
  CHECK ((result_id IS NULL AND fact_engine IS NULL AND fact_at IS NULL)
      OR (result_id IS NOT NULL AND fact_engine IS NOT NULL AND fact_at IS NOT NULL))
);
ALTER TABLE state_compute_scheduling ADD COLUMN IF NOT EXISTS zone BIGINT;
ALTER TABLE state_compute_scheduling ADD COLUMN IF NOT EXISTS zone_position BIGINT;
ALTER TABLE state_compute_scheduling ADD COLUMN IF NOT EXISTS required_capability TEXT COLLATE "C";
ALTER TABLE state_compute_scheduling ADD COLUMN IF NOT EXISTS required_engine TEXT COLLATE "C";
CREATE INDEX IF NOT EXISTS compute_scheduling_zone ON state_compute_scheduling
  (state,coalesce(zone,factory_default_zone(producer)),coalesce(zone_position,task_id),id);
CREATE INDEX IF NOT EXISTS compute_scheduling_order ON state_compute_scheduling(state, waits DESC, due_at, id);
CREATE INDEX IF NOT EXISTS compute_scheduling_holder ON state_compute_scheduling(leased_to);
CREATE INDEX IF NOT EXISTS compute_scheduling_lease ON state_compute_scheduling(lease);
CREATE INDEX IF NOT EXISTS compute_scheduling_fact_at ON state_compute_scheduling(fact_at, verb);
CREATE INDEX IF NOT EXISTS compute_scheduling_open_verb ON state_compute_scheduling(verb) WHERE state='open';
CREATE INDEX IF NOT EXISTS compute_scheduling_odd_at ON state_compute_scheduling(fact_at) WHERE odd=1;
CREATE INDEX IF NOT EXISTS compute_scheduling_dispatched ON state_compute_scheduling(id)
  WHERE state='open' AND verb='simulate' AND (leased_at IS NOT NULL OR lease IS NOT NULL);
CREATE INDEX IF NOT EXISTS compute_scheduling_withdrawn ON state_compute_scheduling(opened_at) WHERE state='withdrawn';
ALTER TABLE state_compute_scheduling SET (autovacuum_analyze_scale_factor=0, autovacuum_analyze_threshold=50000);

-- One base relation keeps ordinary lease updates and row locks on scheduling alone.
CREATE OR REPLACE VIEW state_compute_work AS
  SELECT s.id,s.verb,(SELECT d.request FROM entity_compute_specs d WHERE d.id=s.id) AS request,
    s.state,s.due_at,s.waits,s.opened_at,s.lanes,s.lease,s.lease_until,s.leased_to,s.leased_at,
    s.lease_prev,s.leased_to_prev,s.lease_prev_until,s.needs,
    (SELECT r.canon FROM entity_compute_results r WHERE r.id=s.result_id) AS canon,
    s.fact_engine,s.fact_at,s.odd,s.withdrawn_at,
    COALESCE((SELECT p.witnesses FROM record_compute_result_proofs p WHERE p.result_id=s.result_id),'[]') AS witnesses,
    s.producer,s.initiator,s.task_id,s.result_id,
    coalesce(s.zone,factory_default_zone(s.producer)) AS zone,coalesce(s.zone_position,s.task_id) AS zone_position,s.required_capability,s.required_engine
  FROM state_compute_scheduling s;

CREATE OR REPLACE FUNCTION compute_open_task(text,text,text,text,boolean DEFAULT false) RETURNS bigint
LANGUAGE plpgsql AS $$
DECLARE held state_compute_scheduling%ROWTYPE; next_task bigint; next_generation bigint;
BEGIN
  SELECT * INTO held FROM state_compute_scheduling WHERE id=$1 FOR UPDATE;
  IF NOT FOUND THEN RETURN NULL; END IF;
  IF held.state='open' AND NOT $5 THEN RETURN held.task_id; END IF;
  SELECT COALESCE(max(generation),0)+1 INTO next_generation FROM entity_compute_tasks WHERE computation=$1;
  INSERT INTO entity_compute_tasks(computation,generation,created_at,reason,requested_engine)
    VALUES ($1,next_generation,$4,$2,$3) RETURNING id INTO next_task;
  UPDATE state_compute_scheduling SET task_id=next_task,state='open',opened_at=$4,
    zone_position=CASE WHEN compute_round_occupied(held.id,held.task_id) THEN coalesce(held.zone_position,held.task_id) ELSE nextval('compute_queue_position') END,
    lease=NULL,lease_until=NULL,leased_to=NULL,leased_at=NULL,
    lease_prev=NULL,leased_to_prev=NULL,lease_prev_until=NULL,needs=NULL,odd=0,withdrawn_at=NULL,required_engine=NULL,
    result_id=CASE WHEN $5 THEN NULL ELSE result_id END,
    fact_engine=CASE WHEN $5 THEN NULL ELSE fact_engine END,
    fact_at=CASE WHEN $5 THEN NULL ELSE fact_at END WHERE id=$1;
  INSERT INTO relation_compute_task_sources(task_id,producer,ref,trigger_id,asked_at,payload,requirements)
    SELECT next_task,producer,ref,trigger_id,asked_at,payload,requirements FROM relation_compute_task_sources WHERE task_id=held.task_id;
  IF $2 IN('official_correction','refused_evidence','input_invalidated') THEN
    INSERT INTO relation_compute_task_sources(task_id,producer,ref,trigger_id,asked_at,payload)
      VALUES(next_task,'system',$2||':'||$1,$2||':'||held.task_id::text,$4,
        jsonb_build_object('reason',$2,'previous_task',held.task_id,'requested_engine',$3)::text);
  END IF;
  IF $5 AND held.result_id IS NOT NULL THEN
    INSERT INTO record_compute_result_selections(computation,task_id,previous_result_id,reason,at)
      VALUES ($1,next_task,held.result_id,$2,$4);
  END IF;
  UPDATE state_build_workflow_nodes node SET task_id=next_task
    WHERE node.computation=$1 AND node.task_id=held.task_id AND node.state='queued'
      AND EXISTS(SELECT 1 FROM state_build_workflows f WHERE f.id=node.workflow AND f.state='active');
  RETURN next_task;
END $$;

CREATE OR REPLACE FUNCTION compute_register_tasks(text,text,text) RETURNS void LANGUAGE plpgsql AS $$
DECLARE x record; held state_compute_scheduling%ROWTYPE; next_task bigint; spec entity_compute_specs%ROWTYPE;
BEGIN
  FOR x IN SELECT * FROM jsonb_to_recordset($1::jsonb) AS x(id text,verb text,request text,due_at text,
    lanes text,waits bigint,producer text,initiator text,zone bigint,position bigint,capability text) ORDER BY id LOOP
    INSERT INTO entity_compute_specs(id,verb,request) VALUES(x.id,x.verb,x.request) ON CONFLICT(id) DO NOTHING;
    SELECT * INTO spec FROM entity_compute_specs WHERE id=x.id FOR UPDATE;
    IF spec.verb IS DISTINCT FROM x.verb OR spec.request::jsonb IS DISTINCT FROM x.request::jsonb THEN
      RAISE EXCEPTION 'computation identity has different frozen input';
    END IF;
    SELECT * INTO held FROM state_compute_scheduling WHERE id=x.id FOR UPDATE;
    IF NOT FOUND THEN
      INSERT INTO entity_compute_tasks(computation,generation,created_at,reason,requested_engine)
        SELECT x.id,COALESCE(max(generation),0)+1,$3,'request',$2 FROM entity_compute_tasks WHERE computation=x.id
        RETURNING id INTO next_task;
      INSERT INTO state_compute_scheduling(id,verb,task_id,state,due_at,opened_at,lanes,waits,producer,initiator,zone,zone_position,required_capability)
        VALUES(x.id,x.verb,next_task,'open',x.due_at,$3,x.lanes,x.waits,x.producer,x.initiator,
          coalesce(x.zone,factory_default_zone(x.producer)),coalesce(x.position,nextval('compute_queue_position')),x.capability);
    ELSE
      IF held.state='withdrawn' AND NOT EXISTS(SELECT 1 FROM record_refusal_withdrawals WHERE question=x.id AND engine=$2) THEN
        PERFORM compute_open_task(x.id,'renewed_request',$2,$3);
        held.due_at:=x.due_at;
        held.waits:=x.waits;
        held.producer:=x.producer;
        held.initiator:=x.initiator;
      END IF;
      UPDATE state_compute_scheduling SET due_at=LEAST(held.due_at,x.due_at),
        required_capability=coalesce(required_capability,x.capability),
        zone=CASE WHEN held.state='withdrawn' THEN coalesce(x.zone,factory_default_zone(x.producer)) ELSE
          least(coalesce(held.zone,factory_default_zone(held.producer)),coalesce(x.zone,factory_default_zone(x.producer))) END,
        zone_position=CASE WHEN coalesce(x.zone,factory_default_zone(x.producer))<coalesce(held.zone,factory_default_zone(held.producer))
          THEN coalesce(x.position,nextval('compute_queue_position')) ELSE zone_position END,
        producer=CASE WHEN x.due_at<held.due_at THEN x.producer ELSE held.producer END,
        initiator=CASE WHEN x.due_at<held.due_at THEN x.initiator ELSE held.initiator END,
        waits=GREATEST(held.waits,x.waits),lanes=COALESCE(held.lanes,x.lanes) WHERE id=x.id;
    END IF;
  END LOOP;
END $$;

CREATE OR REPLACE FUNCTION compute_admit_tasks(text,text,text,text) RETURNS void
LANGUAGE plpgsql AS $$
DECLARE s jsonb; d jsonb; prepared jsonb:='[]'; q record; source record; opened text[]:='{}'; previous_question text; affected text[]:='{}';
BEGIN
  s := $1::jsonb; d := $2::jsonb;
  IF jsonb_typeof(s)<>'array' OR jsonb_typeof(d)<>'array' OR jsonb_array_length(s)>250
    OR jsonb_array_length(d)>250 OR $4 IS NULL THEN RAISE EXCEPTION 'invalid compute admission'; END IF;
  IF EXISTS(SELECT 1 FROM jsonb_array_elements(d) x WHERE
      coalesce(length(btrim(x->>'producer')),0)=0 OR coalesce(length(btrim(x->>'ref')),0)=0
      OR coalesce(length(btrim(x->>'trigger_id')),0)=0 OR coalesce(jsonb_typeof((x->>'payload')::jsonb),'null')<>'object'
      OR NOT EXISTS(SELECT 1 FROM jsonb_array_elements(s) y WHERE y->>'id'=x->>'question'))
    OR EXISTS(SELECT 1 FROM jsonb_array_elements(s) x WHERE NOT EXISTS(
      SELECT 1 FROM jsonb_array_elements(d) y WHERE y->>'question'=x->>'id'))
    THEN RAISE EXCEPTION 'a compute task requires a source'; END IF;
  FOR source IN SELECT value FROM jsonb_array_elements(s) ORDER BY coalesce((value->>'ordinal')::bigint,0) LOOP
    prepared:=prepared||jsonb_build_array(source.value||jsonb_build_object('position',nextval('compute_queue_position')));
  END LOOP;
  PERFORM compute_register_tasks(prepared::text,$3,$4);
  FOR q IN SELECT DISTINCT ON(x.id) x.id,x.reason,x.invalidate FROM jsonb_to_recordset(s) AS x(id text,force boolean,invalidate boolean,reason text)
      JOIN state_compute_scheduling a ON a.id=x.id WHERE
      (x.force AND a.state IN ('fact','spot'))
      OR (x.invalidate AND a.state IN ('open','disputed','nondeterministic','fact','spot')) ORDER BY x.id LOOP
    IF NOT EXISTS(SELECT 1 FROM jsonb_array_elements(d) x JOIN state_compute_scheduling a ON a.id=q.id
      JOIN entity_compute_tasks t ON t.computation=a.id
      JOIN relation_compute_task_sources p ON p.task_id=t.id AND p.producer=x->>'producer'
        AND p.ref=x->>'ref' AND p.trigger_id=x->>'trigger_id' WHERE x->>'question'=q.id) THEN
      PERFORM compute_open_task(q.id,CASE WHEN q.invalidate THEN 'input_invalidated' WHEN q.reason='retention_expired' THEN q.reason ELSE 'requested_recalculation' END,$3,$4,true);
      opened:=array_append(opened,q.id);
    END IF;
  END LOOP;
  UPDATE state_compute_scheduling target SET required_engine=nullif($3,'')
    FROM jsonb_to_recordset(s) x(id text,force boolean,invalidate boolean)
    WHERE target.id=x.id AND (x.force OR x.invalidate) AND nullif($3,'') IS NOT NULL
      AND EXISTS(SELECT 1 FROM jsonb_array_elements(d) incoming WHERE incoming->>'question'=target.id
        AND NOT EXISTS(SELECT 1 FROM relation_compute_task_sources previous JOIN entity_compute_tasks t ON t.id=previous.task_id
          WHERE t.computation=target.id AND previous.producer=incoming->>'producer' AND previous.ref=incoming->>'ref'
            AND previous.trigger_id=incoming->>'trigger_id'));
  FOR source IN SELECT * FROM jsonb_to_recordset(d) AS x(producer text,ref text,question text,
      trigger_id text,ordinal bigint,asked_at text,due_at text,waits bigint,payload text,initiator text,zone bigint,force boolean,invalidate boolean)
      ORDER BY producer,ref,ordinal,trigger_id LOOP
    SELECT * INTO STRICT q FROM state_compute_scheduling WHERE id=source.question;
    SELECT question INTO previous_question FROM relation_task_consumers
      WHERE producer=source.producer AND ref=source.ref FOR UPDATE;
    IF previous_question IS NOT NULL THEN affected:=array_append(affected,previous_question); END IF;
    IF EXISTS(SELECT 1 FROM relation_compute_task_sources p WHERE p.task_id=q.task_id AND p.producer=source.producer
      AND p.ref=source.ref AND p.trigger_id=source.trigger_id AND
        (p.payload::jsonb IS DISTINCT FROM source.payload::jsonb)) THEN
      RAISE EXCEPTION 'compute source identity conflict' USING ERRCODE='23505';
    END IF;
    INSERT INTO relation_compute_task_sources(task_id,producer,ref,trigger_id,asked_at,payload,requirements)
      SELECT q.task_id,source.producer,source.ref,source.trigger_id,coalesce(source.asked_at,$4),source.payload,
        jsonb_build_object('force',coalesce(source.force,x.force,false),'invalidate',coalesce(source.invalidate,x.invalidate,false),'engine',$3,'zone',source.zone)
      FROM jsonb_to_recordset(s) x(id text,force boolean,invalidate boolean) WHERE x.id=source.question
      ON CONFLICT DO NOTHING;
    INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at,waits,payload,initiator,delivery_at,zone)
      VALUES(source.producer,source.ref,source.question,'live',coalesce(source.asked_at,$4),source.due_at,source.waits,source.payload,source.initiator,
        CASE WHEN q.state IN ('fact','spot') THEN $4 END,coalesce(source.zone,factory_default_zone(source.producer)))
      ON CONFLICT(producer,ref) DO UPDATE SET question=excluded.question,state='live',asked_at=excluded.asked_at,
        due_at=excluded.due_at,waits=excluded.waits,payload=excluded.payload,served_at=NULL,
        initiator=excluded.initiator,delivery_at=excluded.delivery_at,
        zone=CASE WHEN relation_task_consumers.question=excluded.question AND relation_task_consumers.state='live'
          THEN least(relation_task_consumers.zone,excluded.zone) ELSE excluded.zone END;
  END LOOP;
  PERFORM factory_restate(id) FROM (SELECT DISTINCT unnest(affected) id
    UNION SELECT value->>'id' FROM jsonb_array_elements(s)) changed ORDER BY id;
  INSERT INTO state_producer_compute_usage(producer,used)
    SELECT DISTINCT value->>'producer',coalesce((SELECT min(used) FROM state_producer_compute_usage),0)
      FROM jsonb_array_elements(d) ON CONFLICT DO NOTHING;
END $$;

CREATE OR REPLACE FUNCTION compute_accept_result(text,bigint,text,text,text,text,text,bigint,text) RETURNS bigint
LANGUAGE plpgsql AS $$
DECLARE held state_compute_scheduling%ROWTYPE; result_key bigint;
BEGIN
  SELECT * INTO held FROM state_compute_scheduling WHERE id=$1 FOR UPDATE;
  IF NOT FOUND OR $2 IS NULL OR held.task_id<>$2 OR held.state<>$3
    OR (held.required_engine IS NOT NULL AND held.required_engine<>$6) THEN
    RAISE EXCEPTION 'compute task changed before result acceptance' USING ERRCODE='40001';
  END IF;
  IF $4 NOT IN ('fact','spot') OR $5 IS NULL OR $6 IS NULL OR $7 IS NULL
     OR jsonb_typeof($9::jsonb)<>'array' THEN RAISE EXCEPTION 'invalid accepted result'; END IF;
  INSERT INTO entity_compute_results(computation,task_id,canon,engine,accepted_at)
    VALUES($1,$2,$5,$6,$7) RETURNING id INTO result_key;
  INSERT INTO record_compute_result_proofs(result_id,witnesses) VALUES(result_key,$9);
  UPDATE state_compute_scheduling SET result_id=result_key,fact_engine=$6,fact_at=$7,state=$4,odd=$8 WHERE id=$1;
  INSERT INTO record_compute_result_selections(computation,task_id,result_id,previous_result_id,reason,at)
    VALUES($1,$2,result_key,held.result_id,'accepted',$7);
  IF held.verb IN ('simulate_report','optimize_report') THEN PERFORM compute_select_report($1,$6,$7); END IF;
  RETURN result_key;
END $$;

CREATE OR REPLACE FUNCTION compute_withdrawn_at() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.state='withdrawn' THEN
    IF TG_OP='INSERT' OR OLD.state IS DISTINCT FROM NEW.state THEN
      NEW.withdrawn_at:=COALESCE(NEW.withdrawn_at,to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'));
    END IF;
  ELSE NEW.withdrawn_at:=NULL;
  END IF;
  IF TG_OP='UPDATE' AND NEW.state='open' AND OLD.state<>'open' AND NEW.task_id=OLD.task_id THEN
    RAISE EXCEPTION 'reopening requires a new compute task';
  END IF;
  RETURN NEW;
END $$;
DROP TRIGGER IF EXISTS compute_withdrawn_at ON state_compute_scheduling;
CREATE TRIGGER compute_withdrawn_at BEFORE INSERT OR UPDATE ON state_compute_scheduling
  FOR EACH ROW EXECUTE FUNCTION compute_withdrawn_at();

CREATE TABLE IF NOT EXISTS relation_task_consumers (
  producer  TEXT COLLATE "C" NOT NULL,
  ref       TEXT COLLATE "C" NOT NULL,
  question  TEXT COLLATE "C" NOT NULL,
  -- live | served | withdrawn
  state     TEXT COLLATE "C" NOT NULL,
  asked_at  TEXT COLLATE "C" NOT NULL,
  due_at    TEXT COLLATE "C" NOT NULL,
  waits     BIGINT NOT NULL DEFAULT 0,
  -- What the producer's consumer needs back, which the factory never reads.
  payload   TEXT COLLATE "C" NOT NULL DEFAULT '{}',
  served_at TEXT COLLATE "C",
  PRIMARY KEY (producer, ref)
);
CREATE INDEX IF NOT EXISTS demands_by_question ON relation_task_consumers (question, state);
CREATE INDEX IF NOT EXISTS demands_producer_question ON relation_task_consumers (producer, question);
-- A ROW'S DEMAND BY ITS REF ALONE, whichever producer (worker/verify.js `mine`).
CREATE INDEX IF NOT EXISTS demands_by_ref ON relation_task_consumers (ref);
CREATE INDEX IF NOT EXISTS demands_live ON relation_task_consumers (state, producer);
-- THE QUESTIONS SOMEBODY STILL WAITS ON, a tenth of every demand: a redelivery
-- starts here, not from every settled question (worker/tasks.js `redeliver`).
CREATE INDEX IF NOT EXISTS demands_live_question ON relation_task_consumers (question) WHERE state = 'live';
ALTER TABLE relation_task_consumers ADD COLUMN IF NOT EXISTS delivery_at TEXT COLLATE "C";
ALTER TABLE relation_task_consumers ADD COLUMN IF NOT EXISTS zone BIGINT;
DO $queue_zone$ BEGIN
  IF NOT EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='state_compute_scheduling'::regclass AND conname='compute_zone_bounds') THEN
    ALTER TABLE state_compute_scheduling ADD CONSTRAINT compute_zone_bounds CHECK(zone BETWEEN 0 AND 99) NOT VALID;
  END IF;
  IF NOT EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='relation_task_consumers'::regclass AND conname='consumer_zone_bounds') THEN
    ALTER TABLE relation_task_consumers ADD CONSTRAINT consumer_zone_bounds CHECK(zone BETWEEN 0 AND 99) NOT VALID;
  END IF;
END $queue_zone$;
CREATE INDEX IF NOT EXISTS demands_delivery ON relation_task_consumers (delivery_at, question) WHERE state = 'live' AND delivery_at IS NOT NULL;
-- THE LIVE DEMANDS PAST DUE, a thousand of a million and a half (worker/tasks.js
-- `audit`): the planner prices `due_at` over every state, where served rows are
-- all past due, so without its own index it scans every live row.
CREATE INDEX IF NOT EXISTS demands_live_due ON relation_task_consumers (due_at) WHERE state = 'live';

CREATE TABLE IF NOT EXISTS record_candidate_answers (
  id         BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  question   TEXT COLLATE "C" NOT NULL,
  task_id    BIGINT REFERENCES entity_compute_tasks(id),
  origin     TEXT COLLATE "C" NOT NULL DEFAULT 'native',
  engine     TEXT COLLATE "C" NOT NULL,
  -- THE WITNESS: its device id, its owner then (the device itself when unclaimed),
  -- the salted hash of its network (verify.js `netOf`), and whether it is official.
  device     TEXT COLLATE "C" NOT NULL,
  owner      TEXT COLLATE "C" NOT NULL,
  net        TEXT COLLATE "C" NOT NULL DEFAULT '',
  official   BIGINT NOT NULL DEFAULT 0,
  -- The verb's canonical answer, which agreement compares byte for byte, and the
  -- whole result the consumers read (a build, a score).
  canon      TEXT COLLATE "C" NOT NULL,
  result     TEXT COLLATE "C" NOT NULL,
  work       BIGINT NOT NULL,
  compute_ms BIGINT,
  at         TEXT COLLATE "C" NOT NULL,
  -- 1 when it is one of a fact's witnesses, credited; 2 COVERED, an agreeing
  -- answer whose owner or network a witness already is, never to be paid
  -- (worker/tasks.js `repay`); `void` when its witness was refused, after
  -- which it counts for nothing.
  credited   BIGINT NOT NULL DEFAULT 0,
  void       BIGINT NOT NULL DEFAULT 0,
  -- WHAT IT COST, to fit what a point is worth and never credited: the engine's
  -- work counts (`WORK_COUNTERS`, and a search's own), and the cores it ran on.
  counts     TEXT COLLATE "C",
  lanes      BIGINT
);
ALTER TABLE record_candidate_answers ADD COLUMN IF NOT EXISTS credit_work BIGINT;
ALTER TABLE record_candidate_answers ADD COLUMN IF NOT EXISTS credit_at TEXT COLLATE "C";
CREATE INDEX IF NOT EXISTS answers_credit_pending ON record_candidate_answers (credit_at, id)
  WHERE credit_work IS NOT NULL AND credited = 0 AND void = 0 AND official = 0;
CREATE INDEX IF NOT EXISTS answers_by_question ON record_candidate_answers (question, engine);
CREATE INDEX IF NOT EXISTS answers_by_device ON record_candidate_answers (device, at);
-- A DEVICE'S ANSWERS THAT EARNED NOTHING, the ledger's few among its many
-- (worker/contribution.js `ledgerOf`).
CREATE INDEX IF NOT EXISTS answers_unpaid_by_device ON record_candidate_answers (device, at) WHERE credited = 0 AND official = 0;
-- ONE VOLUNTEER ANSWER per question, device and engine: a send repeated or raced
-- is one witness, never two rows (worker/tasks.js `recordMany`). Official
-- witnesses may answer a question again, so they are left out.
CREATE UNIQUE INDEX IF NOT EXISTS answers_once ON record_candidate_answers (task_id, device, engine) WHERE official = 0;
-- WHEN AN ENGINE'S UNIT OF WORK BEGAN, one seek (worker/fair.js `tableOf`).
CREATE INDEX IF NOT EXISTS answers_by_engine ON record_candidate_answers (engine, at);
-- THE ANSWERS OF THE LAST HOURS, whatever engine, and their questions without
-- a visit to the table (worker/tasks.js `audit`): `answers_by_engine` holds `at`
-- second, so a time range alone walked all of it, past the role's timeout.
CREATE INDEX IF NOT EXISTS answers_by_at ON record_candidate_answers (at, question);

CREATE TABLE IF NOT EXISTS relation_task_answer_evidence (
  task_id BIGINT NOT NULL REFERENCES entity_compute_tasks(id),
  answer_id BIGINT NOT NULL REFERENCES record_candidate_answers(id) ON DELETE CASCADE,
  role TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY (task_id,answer_id)
);
CREATE INDEX IF NOT EXISTS task_answer_evidence_by_answer ON relation_task_answer_evidence(answer_id);
CREATE OR REPLACE VIEW state_compute_candidate_answers AS
  SELECT a.* FROM record_candidate_answers a JOIN relation_task_answer_evidence e ON e.answer_id=a.id
    JOIN state_compute_scheduling s ON s.task_id=e.task_id AND s.id=a.question;
CREATE OR REPLACE FUNCTION compute_answer_task() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP='INSERT' AND NEW.task_id IS NULL THEN
    SELECT task_id INTO NEW.task_id FROM state_compute_scheduling WHERE id=NEW.question;
  END IF;
  RETURN NEW;
END $$;
DROP TRIGGER IF EXISTS compute_answer_task ON record_candidate_answers;
CREATE TRIGGER compute_answer_task BEFORE INSERT ON record_candidate_answers
  FOR EACH ROW EXECUTE FUNCTION compute_answer_task();
CREATE OR REPLACE FUNCTION compute_answer_evidence() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.task_id IS NOT NULL THEN
    INSERT INTO relation_task_answer_evidence(task_id,answer_id,role) VALUES(NEW.task_id,NEW.id,'submission') ON CONFLICT DO NOTHING;
  END IF;
  RETURN NULL;
END $$;
DROP TRIGGER IF EXISTS compute_answer_evidence ON record_candidate_answers;
CREATE TRIGGER compute_answer_evidence AFTER INSERT ON record_candidate_answers
  FOR EACH ROW EXECUTE FUNCTION compute_answer_evidence();
CREATE OR REPLACE FUNCTION compute_carried_evidence() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.reason='engine_recalibration' THEN
    INSERT INTO relation_task_answer_evidence(task_id,answer_id,role)
      SELECT NEW.id,a.id,'previous_engine' FROM state_compute_scheduling s
        JOIN relation_task_answer_evidence e ON e.task_id=s.task_id
        JOIN record_candidate_answers a ON a.id=e.answer_id
      WHERE s.id=NEW.computation AND a.engine IS DISTINCT FROM NEW.requested_engine AND a.void=0 ON CONFLICT DO NOTHING;
  END IF;
  RETURN NULL;
END $$;
DROP TRIGGER IF EXISTS compute_carried_evidence ON entity_compute_tasks;
CREATE TRIGGER compute_carried_evidence AFTER INSERT ON entity_compute_tasks
  FOR EACH ROW EXECUTE FUNCTION compute_carried_evidence();
-- AN OPEN QUESTION A VOLUNTEER HAS ANSWERED, on any engine: one more agreeing
-- answer makes it a fact, so a pack hands these out first (sync.sql
-- `factory_take`). Kept by the triggers in sync.sql, a row while its question
-- is open, so the pick reads a few thousand rows, never every answer.
CREATE TABLE IF NOT EXISTS state_task_answer_heads (
  question TEXT COLLATE "C" NOT NULL,
  engine   TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY (question, engine)
);
DROP INDEX IF EXISTS led_questions_by_engine;

-- WHAT THE WATCHDOG SAID, a round at most hourly, one row a finding under one
-- `at` (worker/factory.js `watchdog`): read here by whoever fixes it, never
-- sent to anyone. Kept thirty days.
CREATE TABLE IF NOT EXISTS record_monitor_findings (
  at    TEXT COLLATE "C" NOT NULL,
  cause TEXT COLLATE "C" NOT NULL,
  said  TEXT NOT NULL,
  PRIMARY KEY (cause, at)
);

-- THE FACTORY'S DIALS (worker/dials.js): a value the owner set, overriding the
-- code's default within the code's bounds, and every change ever made to one.
CREATE TABLE IF NOT EXISTS state_factory_parameters (
  name   TEXT COLLATE "C" PRIMARY KEY,
  value  DOUBLE PRECISION NOT NULL,
  set_by TEXT COLLATE "C" NOT NULL,
  set_at TEXT COLLATE "C" NOT NULL
);
CREATE TABLE IF NOT EXISTS record_factory_parameter_changes (
  id           BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  name         TEXT COLLATE "C" NOT NULL,
  value_before DOUBLE PRECISION,
  value_after  DOUBLE PRECISION NOT NULL,
  set_by       TEXT COLLATE "C" NOT NULL,
  set_at       TEXT COLLATE "C" NOT NULL
);
CREATE INDEX IF NOT EXISTS factory_dial_log_by_name ON record_factory_parameter_changes (name, set_at);

-- THE SETTLER'S TABLES (worker/settler.js, scripts/factoryd.mjs; docs/BOARD.md
-- §"The settler"). The daemon beside the database writes them; while it runs in
-- shadow nothing the worker decides reads them.
-- A QUESTION WITH NEW ANSWERS, waiting for the settler (dial `settle_async`).
-- `claim` is the pass settling it; an answer landing meanwhile clears it, so the
-- pass that read before that answer does not take the row away unsettled.
CREATE TABLE IF NOT EXISTS state_pending_verifications (
  question   TEXT COLLATE "C" NOT NULL,
  engine     TEXT COLLATE "C" NOT NULL,
  peer       TEXT COLLATE "C" NOT NULL DEFAULT '',
  joined     BIGINT NOT NULL DEFAULT 0,
  queued_at  TEXT COLLATE "C" NOT NULL,
  claim      TEXT COLLATE "C",
  claimed_at TEXT COLLATE "C",
  PRIMARY KEY (question, engine, peer)
);
ALTER TABLE state_pending_verifications ADD COLUMN IF NOT EXISTS claim TEXT COLLATE "C";
ALTER TABLE state_pending_verifications ADD COLUMN IF NOT EXISTS claimed_at TEXT COLLATE "C";
CREATE INDEX IF NOT EXISTS settle_queue_by_age ON state_pending_verifications (queued_at);
-- THE FACTORY AT ONE MOMENT, one row: what is open and who is on, and the
-- daemon's own heartbeat and the commit it runs.
CREATE TABLE IF NOT EXISTS state_factory_snapshot (
  id                 BIGINT PRIMARY KEY CHECK (id = 1),
  at                 TEXT COLLATE "C" NOT NULL,
  engine             TEXT COLLATE "C",
  release            TEXT COLLATE "C",
  open               BIGINT NOT NULL,
  urgent             BIGINT NOT NULL,
  asking             BIGINT NOT NULL,
  computing          BIGINT NOT NULL,
  queue_rows         BIGINT NOT NULL,
  settle_queue_rows  BIGINT NOT NULL,
  daemon_at          TEXT COLLATE "C" NOT NULL,
  daemon_commit      TEXT COLLATE "C" NOT NULL
);
-- THE FAIR-SHARE TABLE (worker/fair.js `tableOf`) as the daemon worked it out,
-- per engine and account, and when.
CREATE TABLE IF NOT EXISTS state_compute_fair_shares (
  engine  TEXT COLLATE "C" NOT NULL,
  account TEXT COLLATE "C" NOT NULL,
  target  DOUBLE PRECISION NOT NULL,
  share   DOUBLE PRECISION NOT NULL,
  ratio   DOUBLE PRECISION NOT NULL,
  at      TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY (engine, account)
);
-- WHAT THE SETTLER FOUND, one row per kind, its body JSON: the shadow's
-- comparisons, and later the findings the watchdog tells the owner.
CREATE TABLE IF NOT EXISTS state_factory_findings (
  kind TEXT COLLATE "C" PRIMARY KEY,
  body TEXT COLLATE "C" NOT NULL,
  at   TEXT COLLATE "C" NOT NULL
);
-- PROTOCOL 8 (worker/sync.js). A database laid before it gains the column here.
ALTER TABLE entity_compute_devices ADD COLUMN IF NOT EXISTS next_at TEXT COLLATE "C";
-- A QUESTION A CLIENT'S ENGINE REFUSED (`/api/board/order` answered ok: false),
-- by device and engine: not handed to that device again, and withdrawn once
-- `refusals_to_withdraw` owners refused it on the served engine.
CREATE TABLE IF NOT EXISTS record_engine_refusals (
  question TEXT COLLATE "C" NOT NULL,
  device   TEXT COLLATE "C" NOT NULL,
  owner    TEXT COLLATE "C" NOT NULL,
  engine   TEXT COLLATE "C" NOT NULL,
  step     TEXT COLLATE "C" NOT NULL,
  at       TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY (question, device, engine)
);
CREATE INDEX IF NOT EXISTS engine_refusals_by_engine ON record_engine_refusals (question, engine);
-- …AND A QUESTION THEY WITHDREW, which the watchdog tells the owner.
CREATE TABLE IF NOT EXISTS record_refusal_withdrawals (
  question TEXT COLLATE "C" PRIMARY KEY,
  engine   TEXT COLLATE "C" NOT NULL,
  owners   BIGINT NOT NULL,
  at       TEXT COLLATE "C" NOT NULL
);
-- FAIR SHARE BETWEEN PRODUCERS (worker/tasks.js §"Order", docs/BOARD.md
-- §"Fair share between producers"): a question's PRODUCER is its earliest-due
-- live demand's, kept where `due_at` is; each producer's head is one probe.
ALTER TABLE state_compute_scheduling ADD COLUMN IF NOT EXISTS producer TEXT COLLATE "C" NOT NULL DEFAULT '';
-- WHO ASKED (docs/BOARD.md §"Who asked"): 'system', 'anonymous', 'user:<code>' or
-- 'bot:<code>', NULL where it was not recorded. A question's is its earliest-due
-- live demand's, as its producer is; a code is erased once the ask is over.
ALTER TABLE state_compute_scheduling ADD COLUMN IF NOT EXISTS initiator TEXT COLLATE "C";
ALTER TABLE relation_task_consumers ADD COLUMN IF NOT EXISTS initiator TEXT COLLATE "C";
-- HOW FAR A PASS OVER TIME HAS READ, by name (worker/tasks.js `redeliver`).
CREATE TABLE IF NOT EXISTS state_factory_progress (
  name    TEXT COLLATE "C" PRIMARY KEY,
  through TEXT COLLATE "C" NOT NULL
);

-- MERIT SPENT ON A ROW (docs/BOARD.md §"Merit priority"): the demand it moved,
-- the code that paid, the price held, and the deadline it had (`natural_due`)
-- and was given (`boosted_due`). `state` held → charged | refunded, and a
-- settled hold keeps no code.
CREATE TABLE IF NOT EXISTS state_compute_priority_holds (
  id          BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  producer    TEXT COLLATE "C" NOT NULL,
  ref         TEXT COLLATE "C" NOT NULL,
  code        TEXT COLLATE "C" NOT NULL,
  price       BIGINT NOT NULL,
  natural_due TEXT COLLATE "C" NOT NULL,
  boosted_due TEXT COLLATE "C" NOT NULL,
  at          TEXT COLLATE "C" NOT NULL,
  state       TEXT COLLATE "C" NOT NULL DEFAULT 'held'
);
CREATE INDEX IF NOT EXISTS merit_holds_held ON state_compute_priority_holds (state, at) WHERE state = 'held';
-- …THE CODES STILL KEPT, found without a scan by the pass that erases them.
CREATE INDEX IF NOT EXISTS questions_coded ON state_compute_scheduling (state) WHERE initiator LIKE 'user:%' OR initiator LIKE 'bot:%';
CREATE INDEX IF NOT EXISTS demands_coded ON relation_task_consumers (state) WHERE initiator LIKE 'user:%' OR initiator LIKE 'bot:%';
CREATE INDEX IF NOT EXISTS questions_by_producer ON state_compute_scheduling (state, producer, waits DESC, due_at, id);
-- …AND WHAT EACH HAS USED: compute charged at hand-out (`used`, ms over
-- `weight`), at `answer_ms`, the running mean of its answers' fights.
CREATE TABLE IF NOT EXISTS state_producer_compute_usage (
  producer  TEXT COLLATE "C" PRIMARY KEY,
  used      DOUBLE PRECISION NOT NULL DEFAULT 0,
  answer_ms DOUBLE PRECISION NOT NULL DEFAULT 0,
  weight    DOUBLE PRECISION NOT NULL DEFAULT 1
);

-- A browser secret owns a request; reports are shared immutable factory facts.
CREATE TABLE IF NOT EXISTS entity_user_calculation_requests (
  id TEXT COLLATE "C" PRIMARY KEY,
  task TEXT COLLATE "C" NOT NULL,
  owner TEXT COLLATE "C" NOT NULL,
  kind TEXT COLLATE "C" NOT NULL,
  engine TEXT COLLATE "C" NOT NULL,
  question TEXT COLLATE "C" NOT NULL,
  state TEXT COLLATE "C" NOT NULL,
  created_at TEXT COLLATE "C" NOT NULL,
  completed_at TEXT COLLATE "C",
  expires_at TEXT COLLATE "C" NOT NULL
);
ALTER TABLE entity_user_calculation_requests ADD COLUMN IF NOT EXISTS result_engine TEXT COLLATE "C";
CREATE INDEX IF NOT EXISTS user_calculations_owner ON entity_user_calculation_requests (owner, state);
CREATE INDEX IF NOT EXISTS user_calculations_expiry ON entity_user_calculation_requests (expires_at);
CREATE OR REPLACE FUNCTION compute_admit_user_request(text,text,text,text,text,text,text,text) RETURNS void LANGUAGE plpgsql AS $$
DECLARE stored entity_user_calculation_requests%ROWTYPE;
BEGIN
  PERFORM pg_advisory_xact_lock(hashtextextended('user-calculation:'||$1,0));
  IF EXISTS(SELECT 1 FROM state_request_cancellation_tombstones WHERE id=$1 AND owner=$3) THEN
    RAISE EXCEPTION 'user calculation already cancelled';
  END IF;
  INSERT INTO entity_user_calculation_requests(id,task,owner,kind,engine,question,state,created_at,expires_at)
    VALUES($1,$2,$3,$4,$5,$6,'waiting',$7,$8) ON CONFLICT(id) DO NOTHING;
  SELECT * INTO stored FROM entity_user_calculation_requests WHERE id=$1 FOR UPDATE;
  IF stored.task<>$2 OR stored.owner<>$3 OR stored.question<>$6 THEN
    RAISE EXCEPTION 'user calculation identity conflict' USING ERRCODE='23505';
  END IF;
END $$;
CREATE OR REPLACE FUNCTION compute_cancel_user_request(text,text,text,text) RETURNS boolean LANGUAGE plpgsql AS $$
DECLARE changed bigint;
BEGIN
  PERFORM pg_advisory_xact_lock(hashtextextended('user-calculation:'||$1,0));
  INSERT INTO state_request_cancellation_tombstones(id,owner,expires_at) VALUES($1,$2,$4) ON CONFLICT DO NOTHING;
  UPDATE entity_user_calculation_requests SET state='cancelled',completed_at=$3 WHERE id=$1 AND owner=$2 AND state='waiting';
  GET DIAGNOSTICS changed=ROW_COUNT;
  RETURN changed>0;
END $$;
UPDATE entity_user_calculation_requests SET expires_at=to_char(((CASE WHEN state='succeeded' THEN COALESCE(completed_at,created_at)
  ELSE created_at END)::timestamptz + interval '14 days') AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
  WHERE kind IN ('optimization','appraisal') AND expires_at>to_char(((CASE WHEN state='succeeded' THEN COALESCE(completed_at,created_at)
    ELSE created_at END)::timestamptz + interval '14 days') AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
CREATE TABLE IF NOT EXISTS entity_calculation_reports (
  id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
  result_id BIGINT UNIQUE REFERENCES entity_compute_results(id) ON DELETE CASCADE,
  question TEXT COLLATE "C" NOT NULL,
  engine TEXT COLLATE "C" NOT NULL,
  at TEXT COLLATE "C" NOT NULL,
  origin TEXT COLLATE "C" NOT NULL DEFAULT 'accepted',
  body TEXT COLLATE "C",
  UNIQUE (id,question,engine,at),
  FOREIGN KEY (result_id,question,engine,at) REFERENCES entity_compute_results(id,computation,engine,accepted_at),
  CHECK ((origin='accepted' AND result_id IS NOT NULL AND body IS NULL)
    OR (origin='imported' AND result_id IS NULL AND body IS NOT NULL))
);
CREATE TABLE IF NOT EXISTS state_calculation_report_selection (
  question TEXT COLLATE "C" NOT NULL,
  engine TEXT COLLATE "C" NOT NULL,
  report_id BIGINT NOT NULL,
  at TEXT COLLATE "C" NOT NULL,
  retained_from TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY (question,engine),
  FOREIGN KEY (report_id,question,engine,at) REFERENCES entity_calculation_reports(id,question,engine,at) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS relation_calculation_report_access (
  id TEXT COLLATE "C" PRIMARY KEY,
  owner TEXT COLLATE "C" NOT NULL,
  question TEXT COLLATE "C" NOT NULL,
  engine TEXT COLLATE "C" NOT NULL,
  task TEXT COLLATE "C" NOT NULL,
  kind TEXT COLLATE "C" NOT NULL,
  created_at TEXT COLLATE "C" NOT NULL,
  completed_at TEXT COLLATE "C" NOT NULL,
  expires_at TEXT COLLATE "C" NOT NULL
);
CREATE INDEX IF NOT EXISTS report_access_by_result ON relation_calculation_report_access(question,engine,expires_at);
CREATE INDEX IF NOT EXISTS report_access_by_owner ON relation_calculation_report_access(owner,id);
CREATE INDEX IF NOT EXISTS report_access_expiry ON relation_calculation_report_access(expires_at);
CREATE OR REPLACE VIEW state_current_calculation_reports AS
  SELECT h.question,h.engine,CASE WHEN p.origin='accepted' THEN (r.canon::jsonb->'report')::text ELSE p.body END AS result,
    h.at,h.report_id,h.retained_from,p.origin
    FROM state_calculation_report_selection h JOIN entity_calculation_reports p ON p.id=h.report_id
      LEFT JOIN entity_compute_results r ON r.id=p.result_id;
CREATE OR REPLACE FUNCTION compute_select_report(text,text,text) RETURNS void LANGUAGE plpgsql AS $$
DECLARE selected entity_compute_results%ROWTYPE; report_key bigint; renewed boolean;
BEGIN
  SELECT r.* INTO selected FROM state_compute_scheduling s JOIN entity_compute_results r ON r.id=s.result_id
    WHERE s.id=$1 AND r.engine=$2 AND r.accepted_at=$3 FOR UPDATE OF s;
  IF NOT FOUND OR jsonb_typeof(selected.canon::jsonb->'report') NOT IN ('object','array') THEN
    RAISE EXCEPTION 'report must reference the currently accepted output';
  END IF;
  SELECT reason='retention_expired' INTO renewed FROM entity_compute_tasks WHERE id=selected.task_id;
  INSERT INTO entity_calculation_reports(result_id,question,engine,at) VALUES(selected.id,$1,$2,$3) ON CONFLICT(result_id) DO NOTHING;
  SELECT id INTO report_key FROM entity_calculation_reports WHERE result_id=selected.id;
  INSERT INTO state_calculation_report_selection(question,engine,report_id,at,retained_from) VALUES($1,$2,report_key,$3,$3)
    ON CONFLICT(question,engine) DO UPDATE SET report_id=excluded.report_id,at=excluded.at,
      retained_from=CASE WHEN renewed AND state_calculation_report_selection.report_id<>excluded.report_id
        THEN excluded.retained_from ELSE state_calculation_report_selection.retained_from END;
END $$;

CREATE TABLE IF NOT EXISTS state_request_cancellation_tombstones (
  id TEXT COLLATE "C" NOT NULL,
  owner TEXT COLLATE "C" NOT NULL,
  expires_at TEXT COLLATE "C" NOT NULL,
  PRIMARY KEY (id, owner)
);

-- Public provenance contains equipment ids and witnesses, never a private request or report.
CREATE TABLE IF NOT EXISTS state_public_computation_summaries (
  question TEXT COLLATE "C" PRIMARY KEY,
  kind TEXT COLLATE "C" NOT NULL,
  engine TEXT COLLATE "C" NOT NULL,
  weapon TEXT COLLATE "C",
  completed_at TEXT COLLATE "C" NOT NULL,
  expires_at TEXT COLLATE "C" NOT NULL
);
CREATE INDEX IF NOT EXISTS computation_records_expiry ON state_public_computation_summaries (expires_at);
CREATE TABLE IF NOT EXISTS relation_computation_witnesses (
  question TEXT COLLATE "C" NOT NULL REFERENCES state_public_computation_summaries(question) ON DELETE CASCADE,
  owner TEXT COLLATE "C" NOT NULL,
  device TEXT COLLATE "C" NOT NULL,
  official BIGINT NOT NULL,
  completed_at TEXT COLLATE "C" NOT NULL,
  credited_at TEXT COLLATE "C" NOT NULL,
  position BIGINT NOT NULL,
  PRIMARY KEY (question, device)
);
CREATE INDEX IF NOT EXISTS computation_witnesses_owner ON relation_computation_witnesses (owner, question);
CREATE INDEX IF NOT EXISTS computation_witnesses_device ON relation_computation_witnesses (device, question);

-- Source checks follow durable business requests and plans, independent of initiator labels.
CREATE OR REPLACE FUNCTION compute_source_exists(text,text,text,text) RETURNS boolean LANGUAGE plpgsql STABLE AS $$
DECLARE p jsonb;
BEGIN
  IF $1='build_workflow' THEN
    RETURN EXISTS(SELECT 1 FROM state_build_workflow_nodes n JOIN entity_build_workflows f ON f.id=n.workflow
      WHERE n.workflow=split_part($2,'|',1) AND n.id=split_part($2,'|',2) AND n.definition->>'computation'=$4);
  END IF;
  IF $1='user_calculation' THEN RETURN EXISTS(SELECT 1 FROM entity_user_calculation_requests t WHERE t.id=$2 AND t.question=$4);
  ELSIF $1='family_build' THEN RETURN EXISTS(SELECT 1 FROM entity_family_search_requests t WHERE t.id=$2);
  ELSIF $1 IN ('chat','survey') THEN RETURN EXISTS(SELECT 1 FROM entity_appraisal_requests a WHERE a.code=$2 AND a.request IS NOT NULL);
  END IF;
  BEGIN p:=$3::jsonb; EXCEPTION WHEN invalid_text_representation THEN RETURN false; END;
  IF $1='board_review' THEN RETURN EXISTS(SELECT 1 FROM entity_measurement_reviews r WHERE r.id=p->>'review' AND r.ref=$2 AND r.question=$4);
  ELSIF $1 IN ('new_build','survey_build','refresh','sweep') THEN
    RETURN $2=(p->>'identity')||'|'||(p->>'ruler')||'|'||(p->>'mode')
      AND EXISTS(SELECT 1 FROM entity_compute_batches b WHERE b.id=p->>'batch')
      AND EXISTS(SELECT 1 FROM entity_weapon_builds b WHERE b.id=p->>'identity');
  END IF;
  RETURN false;
END $$;
CREATE OR REPLACE FUNCTION compute_task_has_source(text) RETURNS boolean LANGUAGE plpgsql STABLE AS $$
BEGIN
  RETURN EXISTS(SELECT 1 FROM relation_task_consumers d WHERE d.question=$1 AND d.state='live'
    AND compute_source_exists(d.producer,d.ref,d.payload,$1))
    OR EXISTS(SELECT 1 FROM state_build_workflow_nodes n JOIN state_build_workflows f ON f.id=n.workflow WHERE n.computation=$1 AND f.state='active');
END $$;
CREATE TABLE IF NOT EXISTS record_compute_queue_removals (
  task_id BIGINT PRIMARY KEY,
  computation TEXT COLLATE "C" NOT NULL,
  removed_at TEXT COLLATE "C" NOT NULL,
  reason TEXT COLLATE "C" NOT NULL CHECK(reason='untraceable_source'),
  snapshot TEXT COLLATE "C" NOT NULL CHECK(jsonb_typeof(snapshot::jsonb)='object')
);
CREATE OR REPLACE FUNCTION compute_remove_untraced(text,bigint,text) RETURNS boolean LANGUAGE plpgsql AS $$
DECLARE q state_compute_scheduling%ROWTYPE; detail jsonb;
BEGIN
  SELECT * INTO q FROM state_compute_scheduling WHERE id=$1 FOR UPDATE;
  IF NOT FOUND OR q.state<>'open' OR q.task_id<>$2 THEN RETURN false; END IF;
  IF compute_task_has_source(q.id) THEN RETURN false; END IF;
  detail:=jsonb_build_object('scheduling',to_jsonb(q)-'initiator'-'leased_to'-'leased_to_prev','consumers',coalesce((SELECT jsonb_agg(to_jsonb(d)-'initiator' ORDER BY d.producer,d.ref)
    FROM relation_task_consumers d WHERE d.question=q.id),'[]'::jsonb));
  INSERT INTO record_compute_queue_removals(task_id,computation,removed_at,reason,snapshot)
    VALUES(q.task_id,q.id,$3,'untraceable_source',detail::text);
  UPDATE relation_task_consumers SET state='withdrawn',delivery_at=NULL WHERE question=q.id AND state='live';
  UPDATE state_compute_scheduling SET state='withdrawn',withdrawn_at=$3,lease=NULL,lease_until=NULL,leased_to=NULL,
    leased_at=NULL,lease_prev=NULL,leased_to_prev=NULL,lease_prev_until=NULL WHERE id=q.id;
  RETURN true;
END $$;
REVOKE ALL ON FUNCTION compute_remove_untraced(text,bigint,text) FROM PUBLIC;

CREATE TABLE IF NOT EXISTS record_compute_identity_changes (
  task_id BIGINT PRIMARY KEY,
  previous_computation TEXT COLLATE "C" NOT NULL,
  computation TEXT COLLATE "C" NOT NULL,
  replacement_task_id BIGINT NOT NULL,
  changed_at TEXT COLLATE "C" NOT NULL,
  snapshot TEXT COLLATE "C" NOT NULL CHECK(jsonb_typeof(snapshot::jsonb)='object')
);
-- The operator supplies normalized inputs; application writers are fenced during this cutover.
CREATE OR REPLACE FUNCTION compute_rekey_active(text,bigint,text,text,text,text) RETURNS bigint LANGUAGE plpgsql AS $$
DECLARE q state_compute_scheduling%ROWTYPE; spec jsonb; demand record; target text; replacement bigint; detail jsonb; prior record;
BEGIN
  spec:=$3::jsonb->0; target:=spec->>'id';
  SELECT * INTO prior FROM record_compute_identity_changes WHERE task_id=$2;
  IF FOUND THEN
    IF prior.previous_computation<>$1 OR prior.computation<>target THEN RAISE EXCEPTION 'different identity migration'; END IF;
    RETURN prior.replacement_task_id;
  END IF;
  SELECT * INTO q FROM state_compute_scheduling WHERE id=$1 FOR UPDATE;
  IF NOT FOUND OR q.task_id<>$2 OR q.state<>'open' OR target IS NULL OR target=q.id THEN
    RAISE EXCEPTION 'compute task changed before identity migration' USING ERRCODE='40001';
  END IF;
  IF q.lease_until>=$6 OR q.lease_prev_until>=$6 OR EXISTS(SELECT 1 FROM state_pending_verifications WHERE question=q.id)
    OR EXISTS(SELECT 1 FROM state_build_workflow_nodes n JOIN state_build_workflows f ON f.id=n.workflow WHERE n.computation=q.id AND f.state='active') THEN
    RAISE EXCEPTION 'running or finishing computation must retain its identity' USING ERRCODE='40001';
  END IF;
  IF NOT compute_task_has_source(q.id) THEN RAISE EXCEPTION 'untraceable task cannot be migrated'; END IF;
  IF coalesce((SELECT jsonb_agg(to_jsonb(d) ORDER BY d.producer,d.ref) FROM relation_task_consumers d WHERE d.question=q.id AND d.state='live'),'[]'::jsonb)
      IS DISTINCT FROM $5::jsonb THEN RAISE EXCEPTION 'compute sources changed before identity migration' USING ERRCODE='40001'; END IF;
  detail:=jsonb_build_object('scheduling',to_jsonb(q)-'initiator'-'leased_to'-'leased_to_prev','input',(SELECT to_jsonb(s) FROM entity_compute_specs s WHERE s.id=q.id),
    'consumers',(SELECT jsonb_agg(x-'initiator') FROM jsonb_array_elements($5::jsonb) x),'user_requests',coalesce((SELECT jsonb_agg(to_jsonb(t)-'owner') FROM entity_user_calculation_requests t WHERE t.question=q.id),'[]'::jsonb),
    'family_requests',coalesce((SELECT jsonb_agg(to_jsonb(t)) FROM entity_family_search_requests t WHERE EXISTS
      (SELECT 1 FROM relation_task_consumers d WHERE d.question=q.id AND d.producer='family_build' AND d.ref=t.id)),'[]'::jsonb),
    'appraisal_requests',coalesce((SELECT jsonb_agg(to_jsonb(a)) FROM entity_appraisal_requests a WHERE EXISTS
      (SELECT 1 FROM relation_task_consumers d WHERE d.question=q.id AND d.producer IN ('chat','survey') AND d.ref=a.code)),'[]'::jsonb));
  UPDATE state_compute_scheduling SET state='withdrawn',withdrawn_at=$6,lease=NULL,lease_until=NULL,leased_to=NULL,
    leased_at=NULL,lease_prev=NULL,leased_to_prev=NULL,lease_prev_until=NULL WHERE id=q.id;
  UPDATE relation_task_consumers SET state='withdrawn',delivery_at=NULL WHERE question=q.id AND state='live';
  FOR demand IN SELECT * FROM jsonb_to_recordset($4::jsonb) AS x(producer text,ref text,payload text,context text) LOOP
    IF demand.producer='user_calculation' THEN
      UPDATE entity_user_calculation_requests SET question=target WHERE id=demand.ref AND question=q.id AND state='waiting';
    ELSIF demand.producer='family_build' THEN
      UPDATE entity_family_search_requests SET request=spec->>'request',context=coalesce(demand.context,context)
        WHERE id=demand.ref AND state='waiting';
    ELSIF demand.producer IN ('chat','survey') THEN
      UPDATE entity_appraisal_requests SET request=jsonb_build_object('request',(spec->>'request')::jsonb->'request',
        'context',coalesce(demand.context::jsonb,request::jsonb->'context'))::text WHERE code=demand.ref;
    ELSIF demand.producer='board_review' THEN
      UPDATE entity_measurement_reviews SET question=target,task_id=NULL WHERE id=demand.payload::jsonb->>'review' AND question=q.id AND state='waiting';
    END IF;
  END LOOP;
  PERFORM compute_admit_tasks($3,$4,coalesce(spec->>'requested_engine',q.fact_engine),$6);
  SELECT task_id INTO STRICT replacement FROM state_compute_scheduling WHERE id=target;
  INSERT INTO relation_compute_task_sources(task_id,producer,ref,trigger_id,asked_at,payload,requirements)
    SELECT replacement,producer,ref,trigger_id,asked_at,payload,requirements FROM relation_compute_task_sources WHERE task_id=q.task_id
    ON CONFLICT DO NOTHING;
  UPDATE entity_measurement_reviews SET task_id=replacement WHERE question=target AND task_id IS NULL AND state='waiting';
  INSERT INTO record_compute_identity_changes(task_id,previous_computation,computation,replacement_task_id,changed_at,snapshot)
    VALUES(q.task_id,q.id,target,replacement,$6,detail::text);
  RETURN replacement;
END $$;
REVOKE ALL ON FUNCTION compute_rekey_active(text,bigint,text,text,text,text) FROM PUBLIC;
