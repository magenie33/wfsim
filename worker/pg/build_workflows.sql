-- A captured expansion is a durable entity; only its progress is mutable.
CREATE TABLE IF NOT EXISTS entity_build_workflow_scopes (
  id TEXT COLLATE "C" PRIMARY KEY,
  definition JSONB NOT NULL
);
CREATE TABLE IF NOT EXISTS entity_build_workflows (
  id TEXT COLLATE "C" PRIMARY KEY,
  build_id TEXT COLLATE "C" NOT NULL REFERENCES entity_weapon_builds(id),
  scope_key TEXT COLLATE "C" NOT NULL,
  kind TEXT COLLATE "C" NOT NULL CHECK(kind IN('intake','correction','system')),
  scope_id TEXT COLLATE "C" NOT NULL REFERENCES entity_build_workflow_scopes(id),
  source JSONB NOT NULL,
  created_at TEXT COLLATE "C" NOT NULL
);
CREATE TABLE IF NOT EXISTS entity_build_workflow_expansions (
  id TEXT COLLATE "C" PRIMARY KEY,
  workflow TEXT COLLATE "C" NOT NULL REFERENCES entity_build_workflows(id),
  parent_node TEXT COLLATE "C",
  plan JSONB NOT NULL,
  created_at TEXT COLLATE "C" NOT NULL,
  UNIQUE(workflow,parent_node)
);
CREATE TABLE IF NOT EXISTS state_build_workflows (
  id TEXT COLLATE "C" PRIMARY KEY REFERENCES entity_build_workflows(id),
  build_id TEXT COLLATE "C" NOT NULL REFERENCES entity_weapon_builds(id),
  scope_key TEXT COLLATE "C" NOT NULL,
  state TEXT COLLATE "C" NOT NULL CHECK(state IN('active','succeeded','failed','cancelled')),
  zone BIGINT NOT NULL CHECK(zone BETWEEN 0 AND 99),
  due_at TEXT COLLATE "C" NOT NULL,
  initiator TEXT COLLATE "C",
  completed_at TEXT COLLATE "C"
);
CREATE UNIQUE INDEX IF NOT EXISTS build_workflow_active ON state_build_workflows(scope_key) WHERE state='active';
CREATE TABLE IF NOT EXISTS state_build_workflow_nodes (
  workflow TEXT COLLATE "C" NOT NULL REFERENCES entity_build_workflows(id),
  id TEXT COLLATE "C" NOT NULL,
  expansion TEXT COLLATE "C" NOT NULL REFERENCES entity_build_workflow_expansions(id),
  definition JSONB NOT NULL,
  state TEXT COLLATE "C" NOT NULL CHECK(state IN('pending','queued','awaiting_intake','succeeded','failed','skipped')),
  position BIGINT GENERATED ALWAYS AS IDENTITY,
  computation TEXT COLLATE "C",
  task_id BIGINT REFERENCES entity_compute_tasks(id) ON DELETE SET NULL,
  result_id BIGINT REFERENCES entity_compute_results(id) ON DELETE SET NULL,
  delivered_at TEXT COLLATE "C",
  attempts BIGINT NOT NULL DEFAULT 0,
  retry_at TEXT COLLATE "C" NOT NULL,
  error TEXT COLLATE "C",
  PRIMARY KEY(workflow,id)
);
CREATE INDEX IF NOT EXISTS build_workflow_nodes_pending ON state_build_workflow_nodes(state,retry_at,workflow,id)
  WHERE state IN('pending','queued','awaiting_intake');
CREATE INDEX IF NOT EXISTS build_workflow_nodes_computation ON state_build_workflow_nodes(computation,task_id,workflow);
CREATE TABLE IF NOT EXISTS record_build_workflow_triggers (
  id TEXT COLLATE "C" PRIMARY KEY,
  workflow TEXT COLLATE "C" NOT NULL REFERENCES entity_build_workflows(id),
  source JSONB NOT NULL,
  at TEXT COLLATE "C" NOT NULL
);
CREATE INDEX IF NOT EXISTS build_workflow_triggers_workflow ON record_build_workflow_triggers(workflow,id);
ALTER TABLE record_build_submissions ADD COLUMN IF NOT EXISTS observation TEXT COLLATE "C";
ALTER TABLE entity_measurement_reviews ADD COLUMN IF NOT EXISTS workflow TEXT COLLATE "C";

CREATE TABLE IF NOT EXISTS state_build_family_inbox (
  id TEXT COLLATE "C" PRIMARY KEY,
  retry_at TEXT COLLATE "C" NOT NULL,
  lease UUID,
  lease_until TEXT COLLATE "C",
  error TEXT,
  input JSONB NOT NULL
);
-- Backfill only existing family jobs by primary key before detaching their input.
ALTER TABLE state_build_family_inbox ADD COLUMN IF NOT EXISTS input JSONB;
UPDATE state_build_family_inbox f SET input=(SELECT to_jsonb(b) FROM record_build_submissions b WHERE b.id=f.id)
  WHERE f.input IS NULL;
ALTER TABLE state_build_family_inbox ALTER COLUMN input SET NOT NULL;
ALTER TABLE state_build_family_inbox DROP CONSTRAINT IF EXISTS state_build_family_inbox_id_fkey;
CREATE INDEX IF NOT EXISTS build_family_inbox_ready ON state_build_family_inbox(retry_at,id);

CREATE OR REPLACE FUNCTION build_workflow_system_key(text,text) RETURNS text LANGUAGE sql IMMUTABLE AS $$
  SELECT 'system:'||md5(jsonb_build_array($1,$2)::text)
$$;
CREATE OR REPLACE FUNCTION build_workflow_recovery_key(text) RETURNS text LANGUAGE sql IMMUTABLE AS $$
  SELECT 'recover-empty:'||$1
$$;
-- Scope enrichment must preserve the primary-key path used by bounded readers.
CREATE TABLE IF NOT EXISTS state_build_inbox_positions (
  id TEXT COLLATE "C" PRIMARY KEY,
  position BIGINT NOT NULL DEFAULT nextval('compute_queue_position')
);
ALTER TABLE state_build_inbox_positions ADD COLUMN IF NOT EXISTS generation BIGINT;
ALTER TABLE state_build_inbox_positions ADD COLUMN IF NOT EXISTS payload_generation BIGINT;
CREATE INDEX IF NOT EXISTS build_inbox_positions_generation ON state_build_inbox_positions(generation,id)
  WHERE generation IS NOT NULL;
CREATE TABLE IF NOT EXISTS record_build_inbox_payloads (
  generation BIGINT NOT NULL,
  id TEXT COLLATE "C" NOT NULL,
  at TEXT COLLATE "C",
  record TEXT COLLATE "C",
  initiator TEXT COLLATE "C",
  family_source TEXT COLLATE "C",
  observation TEXT COLLATE "C",
  input_hash BYTEA NOT NULL CHECK(octet_length(input_hash)=32),
  state TEXT NOT NULL CHECK(state IN('pending','accepted','refused')),
  PRIMARY KEY(generation,id),
  CHECK(state<>'pending' OR (at IS NOT NULL AND record IS NOT NULL))
) PARTITION BY LIST(generation);
CREATE OR REPLACE VIEW state_build_inbox_inputs AS
  SELECT b.id,b.at,b.record,b.initiator,b.family_source,b.observation,
    (SELECT s.definition::text FROM entity_build_workflows w JOIN entity_build_workflow_scopes s ON s.id=w.scope_id
      WHERE split_part(b.family_source,'|',1)='workflow' AND w.id=split_part(b.family_source,'|',2)) AS workflow_scope
    FROM (
      SELECT id,at,record,initiator,family_source,observation FROM record_build_submissions
      UNION ALL
      SELECT p.id,b.at,b.record,b.initiator,b.family_source,b.observation FROM state_build_inbox_positions p
        CROSS JOIN LATERAL (SELECT x.at,x.record,x.initiator,x.family_source,x.observation
          FROM record_build_inbox_payloads x WHERE x.generation=p.payload_generation AND x.id=p.id
            AND x.state='pending' OFFSET 0) b
        WHERE p.generation IS NOT NULL
    ) b
  UNION ALL SELECT id,at,record,NULL,NULL,NULL,NULL FROM record_equipment_submissions;

CREATE OR REPLACE VIEW state_build_inbox_priorities AS
  SELECT 'review:'||r.id AS id,factory_default_zone('board_review') AS zone,r.created_at AS position
    FROM entity_measurement_reviews r WHERE r.state='waiting' AND r.workflow IS NULL AND r.task_id IS NULL
  UNION ALL
  SELECT 'workflow:'||n.workflow||':'||n.id,s.zone,w.created_at
    FROM state_build_workflow_nodes n JOIN state_build_workflows s ON s.id=n.workflow
      JOIN entity_build_workflows w ON w.id=s.id
    WHERE n.state='awaiting_intake' AND s.state='active' AND s.zone<factory_default_zone('new_build');

ALTER TABLE state_build_workflow_nodes ADD COLUMN IF NOT EXISTS queue_position BIGINT;
-- Pending captured positions and registered rounds use the same durable cut.
CREATE OR REPLACE FUNCTION compute_order_workflow_node_cancelled(text,text) RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
  SELECT EXISTS(SELECT 1 FROM state_build_workflow_nodes n JOIN entity_build_workflows w ON w.id=n.workflow
    JOIN state_build_workflows f ON f.id=w.id LEFT JOIN state_compute_scheduling q ON q.id=n.computation
    WHERE n.workflow=$1 AND n.id=$2 AND n.state IN('pending','queued') AND n.result_id IS NULL AND n.delivered_at IS NULL
      AND CASE WHEN q.id IS NOT NULL AND n.task_id IS NOT NULL THEN
        q.task_id=n.task_id AND compute_order_cancelled(q.id,q.task_id,coalesce(q.zone,factory_default_zone(q.producer)),coalesce(q.zone_position,q.task_id),q.state) IS NOT NULL
      ELSE compute_order_pending_cancelled(f.zone,coalesce(n.queue_position,n.position),coalesce(n.definition->>'computation',n.id),w.created_at) END)
$$;
CREATE OR REPLACE FUNCTION compute_order_workflow_cancelled(text) RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
  SELECT EXISTS(SELECT 1 FROM state_build_workflows f JOIN entity_build_workflows w ON w.id=f.id
    WHERE f.id=$1 AND f.state='active' AND EXISTS(SELECT 1 FROM compute_order_cut() c WHERE w.created_at<=c.cut_at)
      AND EXISTS(SELECT 1 FROM state_build_workflow_nodes n WHERE n.workflow=f.id AND compute_order_workflow_node_cancelled(n.workflow,n.id))
      AND NOT EXISTS(SELECT 1 FROM state_build_workflow_nodes n WHERE n.workflow=f.id AND
        (n.state='awaiting_intake' OR n.state IN('pending','queued') AND NOT compute_order_workflow_node_cancelled(n.workflow,n.id))))
$$;

CREATE OR REPLACE FUNCTION compute_round_occupied(text,bigint) RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT EXISTS(SELECT 1 FROM relation_task_consumers WHERE question=$1 AND state='live')
    OR EXISTS(SELECT 1 FROM state_build_workflow_nodes n JOIN state_build_workflows f ON f.id=n.workflow
      WHERE n.computation=$1 AND n.task_id=$2 AND f.state='active')
$$;
CREATE OR REPLACE FUNCTION build_workflow_wanted(text,text) RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT EXISTS(SELECT 1 FROM state_build_workflow_nodes n JOIN state_build_workflows f ON f.id=n.workflow
    WHERE n.workflow=split_part($1,'|',1) AND n.id=split_part($1,'|',2) AND n.computation=$2 AND n.state='queued' AND f.state='active' AND NOT compute_order_workflow_node_cancelled(n.workflow,n.id))
$$;
CREATE OR REPLACE FUNCTION build_workflow_delivery_guard(text,text,text,bigint) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  PERFORM 1 FROM state_build_workflow_nodes n JOIN state_build_workflows f ON f.id=n.workflow
    WHERE n.workflow=$1 AND n.id=$2 AND n.computation=$3 AND n.task_id=$4 AND n.state='queued' AND f.state='active' AND NOT compute_order_workflow_node_cancelled(n.workflow,n.id) FOR UPDATE OF n;
  IF NOT FOUND THEN RAISE EXCEPTION 'workflow node changed before delivery' USING ERRCODE='WS002'; END IF;
END $$;

CREATE OR REPLACE FUNCTION build_workflow_finish(text,text) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  PERFORM 1 FROM state_build_workflows WHERE id=$1 AND state='active' FOR UPDATE;
  IF NOT FOUND THEN RETURN; END IF;
  IF compute_order_workflow_cancelled($1) THEN
    UPDATE state_build_workflows SET state='cancelled',completed_at=$2,initiator=NULL WHERE id=$1; RETURN;
  END IF;
  IF NOT EXISTS(SELECT 1 FROM state_build_workflow_nodes WHERE workflow=$1 AND state IN('pending','queued','awaiting_intake')) THEN
    UPDATE state_build_workflows SET state=CASE WHEN EXISTS(
      SELECT 1 FROM state_build_workflow_nodes WHERE workflow=$1 AND state='failed') THEN 'failed' ELSE 'succeeded' END,
      completed_at=$2,initiator=NULL WHERE id=$1;
    UPDATE entity_measurement_reviews SET state='superseded',completed_at=$2
      WHERE workflow=$1 AND state='waiting' AND task_id IS NULL;
  END IF;
END $$;

ALTER TABLE state_build_workflow_nodes ADD COLUMN IF NOT EXISTS queue_position BIGINT;
CREATE OR REPLACE FUNCTION build_workflow_capture(text,text,jsonb,text,jsonb) RETURNS void LANGUAGE plpgsql AS $$
DECLARE expansion_id text; b jsonb; held text; f record;
BEGIN
  PERFORM compute_order_cut_lock();
  IF jsonb_typeof($3->'scope') IS DISTINCT FROM 'object' THEN RAISE EXCEPTION 'invalid captured workflow scope'; END IF;
  IF jsonb_typeof($3->'nodes') IS DISTINCT FROM 'array' OR jsonb_typeof($3->'builds') IS DISTINCT FROM 'array' THEN
    RAISE EXCEPTION 'invalid workflow expansion'; END IF;
  expansion_id:=$1||':'||coalesce($2,'root');
  IF EXISTS(SELECT 1 FROM entity_build_workflow_expansions WHERE id=expansion_id) THEN RETURN; END IF;
  IF compute_order_workflow_cancelled($1) THEN RETURN; END IF;
  IF $5 IS NOT NULL AND (jsonb_typeof($5)<>'array' OR jsonb_array_length($5)<>jsonb_array_length($3->'nodes')) THEN
    RAISE EXCEPTION 'captured queue positions differ from node count'; END IF;
  FOR b IN SELECT value FROM jsonb_array_elements($3->'builds') ORDER BY value->>'id' LOOP
    INSERT INTO entity_weapon_builds(id,at,record) VALUES(b->>'id',left($4,10),(b->'record')::text) ON CONFLICT DO NOTHING;
    SELECT record INTO held FROM entity_weapon_builds WHERE id=b->>'id';
    IF held::jsonb IS DISTINCT FROM b->'record' THEN RAISE EXCEPTION 'build identity has different canonical inputs'; END IF;
  END LOOP;
  INSERT INTO entity_build_workflow_expansions(id,workflow,parent_node,plan,created_at) VALUES(expansion_id,$1,$2,$3-'scope'-'builds',$4);
  INSERT INTO state_build_workflow_nodes(workflow,id,expansion,definition,state,retry_at,queue_position)
    SELECT $1,coalesce($2||'.','')||(ordinality-1)::text,expansion_id,value,'pending',$4,coalesce(($5->>((ordinality-1)::int))::bigint,nextval('compute_queue_position'))
      FROM jsonb_array_elements($3->'nodes') WITH ORDINALITY;
  SELECT w.kind,w.build_id,w.source,w.created_at,s.zone,s.due_at,s.initiator INTO STRICT f FROM entity_build_workflows w JOIN state_build_workflows s ON s.id=w.id WHERE w.id=$1;
  IF $2 IS NULL AND f.kind='correction' AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements($3->'nodes') item
      WHERE item->>'kind'='measure' AND item->>'identity'=f.build_id
        AND item->>'ruler'=f.source#>>'{observation,ruler}' AND item->>'mode'=f.source#>>'{observation,mode}') THEN
    RAISE EXCEPTION 'correction expansion is missing the observed measurement';
  END IF;
  INSERT INTO state_build_admission_work(kind,key,zone,lane,position,created_at,retry_at,family)
    SELECT 'expansion',expansion_id,f.zone,CASE WHEN f.source->>'origin'='requested' THEN 'requested' ELSE 'bulk' END,
      min(queue_position),$4,$4,coalesce(f.source->>'family'='true',false)
      FROM state_build_workflow_nodes WHERE workflow=$1 AND expansion=expansion_id HAVING count(*)>0
    ON CONFLICT(kind,key) DO NOTHING;
END $$;

CREATE OR REPLACE FUNCTION build_workflow_expansion(text,text,jsonb,text) RETURNS void LANGUAGE plpgsql AS $$
BEGIN PERFORM build_workflow_capture($1,$2,$3,$4,NULL); END $$;


-- Each legal arrival checks its captured current coverage through indexed keys.
-- Existing measurements satisfy intake; explicit correction still requests fresh work.
CREATE OR REPLACE FUNCTION build_intake_matches_observation(jsonb) RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT $1#>>'{observation,valid}'='true' AND EXISTS(SELECT 1 FROM state_board_measurement_selection s
    JOIN entity_measurement_reviews r ON r.id=$1#>>'{observation,review}'
    WHERE s.identity=$1->>'id' AND s.ruler=$1#>>'{observation,ruler}' AND s.mode=$1#>>'{observation,mode}'
      AND r.identity=s.identity AND r.ruler=s.ruler AND r.mode=s.mode AND r.state='waiting'
      AND s.finished_at=$1#>>'{observation,baseline_finished_at}' AND s.score=($1#>>'{observation,baseline_score}')::double precision
      AND s.metric=$1#>>'{observation,metric}' AND abs(s.score-($1#>>'{observation,score}')::double precision)>
        1e-12*greatest(abs(s.score),abs(($1#>>'{observation,score}')::double precision)))
$$;

CREATE OR REPLACE FUNCTION build_intake_register(text) RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE input jsonb; plan jsonb; observation jsonb; inserted text; held text; flow_id text;
  at text:=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
  reason text; priority bigint; origin jsonb; parent text; node_key text; captured_key text; trigger_key text; parent_scope jsonb;
  recovery text; prior record; review record;
BEGIN
  PERFORM compute_order_cut_lock();
  IF jsonb_typeof($1::jsonb) IS DISTINCT FROM 'array' OR jsonb_array_length($1::jsonb)>1000 THEN
    RAISE EXCEPTION 'invalid intake batch'; END IF;
  FOR input IN SELECT value FROM jsonb_array_elements($1::jsonb) ORDER BY value->>'id' LOOP
    at:=coalesce(input->>'admission_at',to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'));
    IF input->'system_source' IS NOT NULL AND input->'system_source'<>'null'::jsonb AND
      (jsonb_typeof(input->'system_source')<>'object' OR coalesce(length(btrim(input#>>'{system_source,id}')),0)=0) THEN
      RAISE EXCEPTION 'system task requires an explicit source identity';
    END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended('build-workflow:'||(input->>'id'),0));
    inserted:=NULL;
    INSERT INTO entity_weapon_builds(id,at,record) VALUES(input->>'id',left(input->>'at',10),(input->'record')::text)
      ON CONFLICT DO NOTHING RETURNING id INTO inserted;
    SELECT record INTO held FROM entity_weapon_builds WHERE id=input->>'id';
    IF held::jsonb IS DISTINCT FROM input->'record' THEN RAISE EXCEPTION 'build identity has different canonical inputs'; END IF;
    plan:=input->'flow'; observation:=input->'observation';
    trigger_key:=build_intake_key(input);
    recovery:=input->>'recovery_of';
    IF recovery IS NOT NULL THEN
      IF NOT pg_has_role(session_user,(SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid='entity_build_workflows'::regclass),'MEMBER') THEN
        RAISE EXCEPTION 'workflow recovery requires the storage owner' USING ERRCODE='42501';
      END IF;
      trigger_key:=build_workflow_recovery_key(recovery);
      IF EXISTS(SELECT 1 FROM record_build_workflow_triggers WHERE id=trigger_key) THEN
        IF NOT EXISTS(SELECT 1 FROM record_build_workflow_triggers t JOIN entity_build_workflows w ON w.id=t.workflow
            WHERE t.id=trigger_key AND w.build_id=input->>'id' AND t.source#>>'{recovery,plan_md5}'=md5(plan::text)
              AND t.source->'observation' IS NOT DISTINCT FROM coalesce(observation,'null'::jsonb)) THEN
          RAISE EXCEPTION 'workflow recovery input changed' USING ERRCODE='40001';
        END IF;
        CONTINUE;
      END IF;
      SELECT w.build_id,w.kind,w.source,s.state,s.zone,s.due_at,s.completed_at,scope.definition AS scope INTO prior
        FROM entity_build_workflows w JOIN state_build_workflows s ON s.id=w.id JOIN entity_build_workflow_scopes scope ON scope.id=w.scope_id
        WHERE w.id=recovery FOR UPDATE OF s;
      IF NOT FOUND OR prior.state<>'succeeded' OR prior.kind NOT IN('intake','correction') OR prior.build_id IS DISTINCT FROM input->>'id'
          OR jsonb_typeof(prior.scope)='object' OR EXISTS(SELECT 1 FROM state_build_workflow_nodes WHERE workflow=recovery)
          OR NOT EXISTS(SELECT 1 FROM entity_build_workflow_expansions e WHERE e.workflow=recovery AND e.parent_node IS NULL AND e.plan->'nodes'='[]'::jsonb)
          OR coalesce((input->>'derived')::boolean,false) OR input->'flow_source' IS NOT NULL
          OR (input->'system_source' IS NOT NULL AND input->'system_source'<>'null'::jsonb) THEN
        RAISE EXCEPTION 'workflow is not an invalid empty completion';
      END IF;
      IF prior.kind='correction' THEN
        IF jsonb_typeof(observation) IS DISTINCT FROM 'object' THEN RAISE EXCEPTION 'workflow recovery requires its original observation'; END IF;
        IF observation-'valid' IS DISTINCT FROM (prior.source->'observation')-'valid' OR observation->>'valid' IS DISTINCT FROM 'true' THEN
          RAISE EXCEPTION 'workflow recovery changed its original observation';
        END IF;
        SELECT * INTO review FROM entity_measurement_reviews WHERE id=observation->>'review' FOR UPDATE;
        IF NOT FOUND OR review.state<>'superseded' OR review.workflow IS DISTINCT FROM recovery OR review.task_id IS NOT NULL
            OR NOT EXISTS(SELECT 1 FROM state_board_measurement_selection s WHERE s.identity=review.identity AND s.ruler=review.ruler AND s.mode=review.mode
              AND s.finished_at=observation->>'baseline_finished_at' AND s.score=(observation->>'baseline_score')::double precision AND s.metric=observation->>'metric') THEN
          RAISE EXCEPTION 'workflow recovery baseline or review changed' USING ERRCODE='40001';
        END IF;
        UPDATE entity_measurement_reviews SET state='waiting',completed_at=NULL WHERE id=review.id;
      END IF;
      input:=input||jsonb_build_object('from',prior.source->'inbox','initiator','system',
        'deadlines',coalesce(prior.source->'deadlines','{}'::jsonb)||jsonb_build_object(prior.kind,prior.due_at));
    END IF;
    IF EXISTS(SELECT 1 FROM record_build_workflow_triggers WHERE id=trigger_key) THEN CONTINUE; END IF;
    IF compute_order_pending_cancelled(coalesce((input->>'zone')::bigint,3),coalesce((input#>>'{admission_positions,0}')::bigint,9223372036854775807),input->>'id',coalesce(input->>'admission_at',input->>'at'))
      AND input#>>'{flow_source,workflow}' IS NULL THEN CONTINUE; END IF;
    parent:=input#>>'{flow_source,workflow}';node_key:=input#>>'{flow_source,node}';
    IF parent IS NOT NULL THEN
      PERFORM 1 FROM state_build_workflow_nodes n JOIN state_build_workflows f ON f.id=n.workflow
        WHERE n.workflow=parent AND n.id=node_key AND n.state='awaiting_intake' AND f.state='active' FOR UPDATE OF n;
      IF FOUND THEN
        SELECT scope.definition INTO STRICT parent_scope FROM entity_build_workflows f JOIN entity_build_workflow_scopes scope ON scope.id=f.scope_id WHERE f.id=parent;
        IF jsonb_typeof(parent_scope) IS DISTINCT FROM 'object' OR plan->'scope' IS DISTINCT FROM parent_scope THEN
          RAISE EXCEPTION 'derived input changed its captured workflow scope';
        END IF;
        PERFORM build_workflow_capture(parent,node_key,plan,at,input->'admission_positions');
        UPDATE state_build_workflow_nodes SET state='succeeded' WHERE workflow=parent AND id=node_key;
        PERFORM build_workflow_finish(parent,at);
      END IF;
      CONTINUE;
    END IF;
    IF coalesce((input->>'derived')::boolean,false) THEN CONTINUE; END IF;
    reason:=NULL;
    IF recovery IS NOT NULL THEN reason:=prior.kind;priority:=prior.zone;
    ELSIF jsonb_typeof(input->'system_source')='object' THEN reason:='system';priority:=coalesce((input->>'zone')::bigint,0);
    ELSIF input->>'family_creation'='true' THEN reason:='intake';priority:=3;
    ELSIF build_intake_matches_observation(input) THEN
      reason:='correction';priority:=1;
    ELSE
      captured_key:=md5((input->>'id')||E'\n'||plan::text);
      IF inserted IS NOT NULL OR coalesce((input->>'admission_new')::boolean,false) OR EXISTS(SELECT 1 FROM jsonb_array_elements(plan->'nodes') n
        WHERE n->>'kind'='measure' AND NOT EXISTS(SELECT 1 FROM state_board_measurement_selection s
          WHERE s.identity=n->>'identity' AND s.ruler=n->>'ruler' AND s.mode=n->>'mode'))
        OR EXISTS(SELECT 1 FROM state_build_workflows WHERE scope_key=captured_key AND state='active')
        OR (EXISTS(SELECT 1 FROM jsonb_array_elements(plan->'nodes') n WHERE n->>'kind'='optimize')
          AND NOT EXISTS(SELECT 1 FROM state_build_workflows WHERE scope_key=captured_key AND state='succeeded')) THEN
        reason:='intake';priority:=3;
      END IF;
    END IF;
    IF reason IS NULL THEN
      IF observation->>'review' IS NOT NULL THEN UPDATE entity_measurement_reviews SET state='superseded',completed_at=at
        WHERE id=observation->>'review' AND state='waiting'; END IF;
      CONTINUE;
    END IF;
    IF plan IS NULL OR plan->>'schema' IS DISTINCT FROM '1' THEN RAISE EXCEPTION 'first intake requires a captured workflow'; END IF;
    captured_key:=md5((input->>'id')||E'\n'||plan::text||CASE WHEN reason='intake' THEN '' ELSE E'\n'||trigger_key END);
    IF input->>'family_creation'='true' THEN captured_key:='family:'||captured_key; END IF;
    UPDATE state_build_workflows SET state='cancelled',completed_at=at WHERE scope_key=captured_key AND state='active' AND compute_order_workflow_cancelled(id);
    SELECT id INTO flow_id FROM state_build_workflows WHERE scope_key=captured_key AND state='active' FOR UPDATE;
    origin:=jsonb_build_object('kind',reason,'inbox',coalesce(input->'from','[]'::jsonb),'observation',observation,
      'system',input->'system_source','deadlines',input->'deadlines','origin',CASE WHEN reason='system' OR input->>'initiator'='system' THEN 'system'
        WHEN input->>'initiator'='anonymous' OR input->>'initiator' LIKE 'user:%' OR input->>'initiator' LIKE 'bot:%' THEN 'requested' ELSE 'unknown' END);
    IF input->>'family_creation'='true' THEN
      origin:=origin||'{"family":true}'::jsonb;
    END IF;
    IF recovery IS NOT NULL THEN
      origin:=origin||jsonb_build_object('recovery',jsonb_build_object('workflow',recovery,'reason','invalid empty captured scope',
        'prior_state',prior.state,'prior_completed_at',prior.completed_at,'plan_md5',md5(plan::text),'original_source',prior.source));
    END IF;
    IF flow_id IS NULL THEN
      flow_id:=gen_random_uuid()::text;
      INSERT INTO entity_build_workflow_scopes(id,definition) VALUES(md5((plan->'scope')::text),plan->'scope') ON CONFLICT DO NOTHING;
      INSERT INTO entity_build_workflows(id,build_id,scope_key,kind,scope_id,source,created_at)
        VALUES(flow_id,input->>'id',captured_key,reason,md5((plan->'scope')::text),origin,at);
      INSERT INTO state_build_workflows(id,build_id,scope_key,state,zone,due_at,initiator) VALUES(flow_id,input->>'id',captured_key,'active',priority,
        coalesce(input#>>ARRAY['deadlines',reason],at),coalesce(input->>'initiator',CASE WHEN reason='system' THEN 'system' END));
      PERFORM build_workflow_capture(flow_id,NULL,plan,at,input->'admission_positions');
    ELSE
      UPDATE state_build_workflows SET zone=least(zone,priority),due_at=least(due_at,coalesce(input#>>ARRAY['deadlines',reason],at)) WHERE id=flow_id;
      -- C collation bounds every original match: the bare workflow or its pipe-prefixed refs.
      UPDATE relation_task_consumers d SET zone=least(coalesce(d.zone,3),priority),due_at=least(d.due_at,coalesce(input#>>ARRAY['deadlines',reason],at))
        WHERE d.producer='build_workflow' AND d.ref>=flow_id AND d.ref<flow_id||'}'
          AND split_part(d.ref,'|',1)=flow_id AND d.state='live';
      PERFORM factory_restate(node.computation) FROM state_build_workflow_nodes node WHERE node.workflow=flow_id AND node.state='queued' ORDER BY node.computation;
    END IF;
    INSERT INTO record_build_workflow_triggers(id,workflow,source,at)
      VALUES(trigger_key,flow_id,origin,at) ON CONFLICT DO NOTHING;
    INSERT INTO relation_compute_task_sources(task_id,producer,ref,trigger_id,asked_at,payload,requirements)
      SELECT n.task_id,'build_workflow',flow_id||'|'||n.id,trigger_key,at,
        jsonb_build_object('workflow',flow_id,'node',n.id,'source',origin)::text,
        jsonb_build_object('force',reason IN('correction','system'),'zone',priority,'engine',NULL)
      FROM state_build_workflow_nodes n WHERE n.workflow=flow_id AND n.task_id IS NOT NULL ON CONFLICT DO NOTHING;
    IF observation->>'review' IS NOT NULL THEN UPDATE entity_measurement_reviews SET workflow=flow_id
      WHERE id=observation->>'review' AND state='waiting'; END IF;
    IF recovery IS NOT NULL THEN UPDATE state_build_workflows SET state='failed' WHERE id=recovery; END IF;
    PERFORM build_workflow_finish(flow_id,at);
  END LOOP;
END $$;

CREATE OR REPLACE FUNCTION build_workflow_recover(text,text) RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE input jsonb:=$2::jsonb; recovered text;
BEGIN
  IF jsonb_typeof(input) IS DISTINCT FROM 'object' THEN RAISE EXCEPTION 'invalid workflow recovery input'; END IF;
  PERFORM build_intake_register(jsonb_build_array(input||jsonb_build_object('recovery_of',$1))::text);
  SELECT workflow INTO STRICT recovered FROM record_build_workflow_triggers WHERE id=build_workflow_recovery_key($1);
  RETURN jsonb_build_object('workflow',recovered,'recovery_of',$1,'trigger',build_workflow_recovery_key($1));
END $$;
REVOKE ALL ON FUNCTION build_workflow_recover(text,text) FROM PUBLIC;

DROP FUNCTION IF EXISTS build_spend_submissions(text);
CREATE OR REPLACE FUNCTION build_ack_submissions(text) RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE proof record; captured record_build_submissions%ROWTYPE; gear record_equipment_submissions%ROWTYPE;
  at text:=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
BEGIN
  IF jsonb_typeof($1::jsonb) IS DISTINCT FROM 'array' OR jsonb_array_length($1::jsonb)>1000 THEN
    RAISE EXCEPTION 'invalid submission acknowledgement'; END IF;
  FOR proof IN SELECT * FROM jsonb_to_recordset($1::jsonb)
    AS rows(id text,at text,record text,initiator text,family_source text,observation text,rejected boolean) ORDER BY id COLLATE "C" LOOP
    IF coalesce(proof.id,'')='' OR proof.at IS NULL OR proof.record IS NULL THEN
      RAISE EXCEPTION 'submission acknowledgement requires captured content'; END IF;
    IF proof.id LIKE 'equipment:%' THEN
      SELECT * INTO gear FROM record_equipment_submissions b WHERE b.id=proof.id FOR UPDATE;
      IF NOT FOUND THEN CONTINUE; END IF;
      IF (gear.at,gear.record) IS DISTINCT FROM (proof.at,proof.record) THEN
        RAISE EXCEPTION 'captured equipment submission changed'; END IF;
      DELETE FROM record_equipment_submissions b WHERE b.id=gear.id;
      CONTINUE;
    END IF;
    IF to_regprocedure('build_inbox_ack(jsonb)') IS NOT NULL THEN
      IF build_inbox_ack(to_jsonb(proof)) THEN CONTINUE; END IF;
    END IF;
    SELECT * INTO captured FROM record_build_submissions b WHERE b.id=proof.id FOR UPDATE;
    IF NOT FOUND THEN
      IF to_regprocedure('raw_inbox_rollover_ack(record_build_submissions)') IS NOT NULL THEN
        PERFORM raw_inbox_rollover_ack(ROW(proof.id,proof.at,proof.record,proof.initiator,proof.family_source,
          proof.observation)::record_build_submissions);
      END IF;
      CONTINUE;
    END IF;
    IF (captured.at,captured.record,captured.initiator,captured.family_source,captured.observation) IS DISTINCT FROM
       (proof.at,proof.record,proof.initiator,proof.family_source,proof.observation) THEN
      RAISE EXCEPTION 'captured build submission changed'; END IF;
    IF NOT EXISTS(SELECT 1 FROM relation_build_intake_sources WHERE source_id=captured.id)
      AND NOT EXISTS(SELECT 1 FROM state_build_family_inbox f WHERE f.id=captured.id AND f.input=to_jsonb(captured)) THEN
      UPDATE entity_measurement_reviews SET state='failed',completed_at=at
        WHERE id=captured.observation::jsonb->>'review' AND state='waiting' AND workflow IS NULL AND task_id IS NULL;
      IF split_part(captured.family_source,'|',1)='workflow' THEN
        UPDATE state_build_workflow_nodes SET state='failed',error='derived build was refused by intake'
          WHERE workflow=split_part(captured.family_source,'|',2) AND id=split_part(captured.family_source,'|',3) AND state='awaiting_intake';
        PERFORM build_workflow_finish(split_part(captured.family_source,'|',2),at);
      END IF;
    END IF;
    DELETE FROM record_build_submissions b WHERE b.id=captured.id;
  END LOOP;
END $$;

CREATE OR REPLACE FUNCTION build_inbox_assert_ack(text) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  IF EXISTS(SELECT FROM state_build_inbox_positions p JOIN jsonb_array_elements($1::jsonb) proof ON p.id=proof->>'id'
    WHERE p.generation IS NOT NULL) THEN
    RAISE EXCEPTION 'the installed acknowledgement did not release the generation input' USING ERRCODE='55000'; END IF;
END $$;
REVOKE ALL ON FUNCTION build_inbox_assert_ack(text) FROM PUBLIC;

-- Retraction creates an explicit sourced repair from the original frozen input.
CREATE OR REPLACE FUNCTION build_workflow_repair(text,text,text,text) RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE node record; s record; plan jsonb; source_id text; next_flow text;
BEGIN
  SELECT n.*,e.plan,scope.definition AS scope INTO node FROM state_build_workflow_nodes n
    JOIN entity_build_workflow_expansions e ON e.id=n.expansion JOIN entity_build_workflows f ON f.id=n.workflow
    JOIN entity_build_workflow_scopes scope ON scope.id=f.scope_id
    WHERE n.workflow=$1 AND n.id=$2 AND n.computation=$3;
  IF NOT FOUND OR node.definition->>'kind'<>'measure' THEN RETURN; END IF;
  SELECT * INTO s FROM state_board_measurement_selection WHERE identity=node.definition->>'identity'
    AND ruler=node.definition->>'ruler' AND mode=node.definition->>'mode' AND finished_at=node.delivered_at FOR UPDATE;
  IF NOT FOUND THEN RETURN; END IF;
  source_id:='retract:'||$1||':'||$2||':'||coalesce(node.result_id::text,'unknown');
  plan:=node.plan||jsonb_build_object('scope',node.scope,'builds','[]'::jsonb,'nodes',jsonb_build_array(node.definition));
  PERFORM build_intake_register(jsonb_build_array(jsonb_build_object('id',s.identity,'at',$4,
    'record',(SELECT record::jsonb FROM entity_weapon_builds WHERE id=s.identity),'flow',plan,
    'initiator','system','zone',1,'system_source',jsonb_build_object('id',source_id,'reason','retracted measurement proof','computation',$3)))::text);
  SELECT workflow INTO next_flow FROM record_build_workflow_triggers WHERE id=build_workflow_system_key(source_id,s.identity);
  PERFORM build_workflow_capture_review(next_flow,pending.id,$4) FROM state_build_workflow_nodes pending WHERE pending.workflow=next_flow;
  UPDATE entity_measurement_reviews r SET invalidated_at=$4,invalidation_reason='retracted measurement proof'
    WHERE r.workflow=next_flow AND r.state='waiting';
  UPDATE state_board_measurement_selection current SET review_id=r.id FROM entity_measurement_reviews r
    WHERE r.workflow=next_flow AND r.state='waiting' AND current.identity=r.identity AND current.ruler=r.ruler AND current.mode=r.mode
      AND current.finished_at=s.finished_at;
END $$;
REVOKE ALL ON FUNCTION build_intake_register(text),build_ack_submissions(text),build_workflow_repair(text,text,text,text) FROM PUBLIC;

CREATE OR REPLACE FUNCTION build_workflow_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW IS DISTINCT FROM OLD THEN RAISE EXCEPTION 'captured workflow input is immutable'; END IF;
  RETURN NEW;
END $$;
DO $workflow_immutable$
DECLARE relation text;
BEGIN
  FOREACH relation IN ARRAY ARRAY['entity_build_workflow_scopes','entity_build_workflows','entity_build_workflow_expansions','record_build_workflow_triggers'] LOOP
    EXECUTE format('DROP TRIGGER IF EXISTS build_workflow_immutable ON %I',relation);
    EXECUTE format('CREATE TRIGGER build_workflow_immutable BEFORE UPDATE ON %I FOR EACH ROW EXECUTE FUNCTION build_workflow_immutable()',relation);
  END LOOP;
END $workflow_immutable$;


-- Canonical input and its captured obligation commit before the source inbox can be acknowledged.
CREATE TABLE IF NOT EXISTS entity_build_intake_plans (
  id TEXT COLLATE "C" PRIMARY KEY,
  definition JSONB NOT NULL
);
CREATE TABLE IF NOT EXISTS entity_build_intake_intents (
  id TEXT COLLATE "C" PRIMARY KEY,
  build_id TEXT COLLATE "C" NOT NULL REFERENCES entity_weapon_builds(id),
  plan_id TEXT COLLATE "C" REFERENCES entity_build_intake_plans(id),
  input JSONB NOT NULL,
  created_at TEXT COLLATE "C" NOT NULL
);
CREATE TABLE IF NOT EXISTS relation_build_intake_sources (
  source_id TEXT COLLATE "C" NOT NULL,
  intent TEXT COLLATE "C" NOT NULL REFERENCES entity_build_intake_intents(id) ON DELETE CASCADE,
  PRIMARY KEY(source_id,intent)
);
CREATE TABLE IF NOT EXISTS state_build_admission_work (
  kind TEXT COLLATE "C" NOT NULL CHECK(kind IN('intake','expansion')),
  key TEXT COLLATE "C" NOT NULL,
  zone BIGINT NOT NULL CHECK(zone BETWEEN 0 AND 99),
  lane TEXT COLLATE "C" NOT NULL CHECK(lane IN('requested','bulk')),
  position BIGINT NOT NULL,
  created_at TEXT COLLATE "C" NOT NULL,
  retry_at TEXT COLLATE "C" NOT NULL,
  lease TEXT COLLATE "C",
  lease_until TEXT COLLATE "C",
  attempts BIGINT NOT NULL DEFAULT 0,
  error TEXT,
  intake_id TEXT COLLATE "C" GENERATED ALWAYS AS(CASE WHEN kind='intake' THEN key END) STORED
    REFERENCES entity_build_intake_intents(id) ON DELETE CASCADE,
  expansion_id TEXT COLLATE "C" GENERATED ALWAYS AS(CASE WHEN kind='expansion' THEN key END) STORED
    REFERENCES entity_build_workflow_expansions(id) ON DELETE CASCADE,
  PRIMARY KEY(kind,key)
);
CREATE INDEX IF NOT EXISTS build_admission_fifo ON state_build_admission_work(kind,lane,zone,position,key);
ALTER TABLE state_build_admission_work ADD COLUMN IF NOT EXISTS family BOOLEAN NOT NULL DEFAULT false;
CREATE INDEX IF NOT EXISTS build_admission_family ON state_build_admission_work(position,key) WHERE family;
CREATE TABLE IF NOT EXISTS state_build_admission_turns (
  kind TEXT COLLATE "C" PRIMARY KEY,
  lane TEXT COLLATE "C" NOT NULL CHECK(lane IN('requested','bulk'))
);
DROP TRIGGER IF EXISTS pipeline_build_admission_ready ON state_build_admission_work;
CREATE TRIGGER pipeline_build_admission_ready AFTER INSERT ON state_build_admission_work
  FOR EACH ROW EXECUTE FUNCTION task_pipeline_notify('admission');

CREATE OR REPLACE FUNCTION build_intake_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'captured intake obligation is immutable'; END $$;
DROP TRIGGER IF EXISTS build_intake_intent_retained ON entity_build_intake_intents;
CREATE TRIGGER build_intake_intent_retained BEFORE UPDATE OR DELETE ON entity_build_intake_intents
  FOR EACH ROW EXECUTE FUNCTION build_intake_immutable();
DROP TRIGGER IF EXISTS build_intake_plan_retained ON entity_build_intake_plans;
CREATE TRIGGER build_intake_plan_retained BEFORE UPDATE OR DELETE ON entity_build_intake_plans
  FOR EACH ROW EXECUTE FUNCTION build_intake_immutable();
DROP TRIGGER IF EXISTS build_intake_source_retained ON relation_build_intake_sources;
CREATE TRIGGER build_intake_source_retained BEFORE UPDATE OR DELETE ON relation_build_intake_sources
  FOR EACH ROW EXECUTE FUNCTION build_intake_immutable();

CREATE OR REPLACE FUNCTION build_intake_key(jsonb) RETURNS text LANGUAGE sql IMMUTABLE AS $$
  SELECT CASE WHEN $1->>'recovery_of' IS NOT NULL THEN build_workflow_recovery_key($1->>'recovery_of')
    WHEN $1->>'family_creation'='true' THEN 'family:'||($1->>'id') ELSE
    coalesce($1#>>'{observation,review}',CASE WHEN $1#>>'{system_source,id}' IS NOT NULL
      THEN build_workflow_system_key($1#>>'{system_source,id}',$1->>'id') END,
      'intake:'||($1->>'id')||':'||md5(jsonb_build_array($1->'from',$1#>'{flow,scope}')::text)) END
$$;

CREATE OR REPLACE FUNCTION build_intake_batch(text) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE input jsonb; identity text; held text; intent text; positions jsonb; priority bigint; lane text;parent_scope jsonb;plan_key text;captured_plan jsonb;inserted text;
  family boolean; family_plan jsonb; root_identity text; root_record jsonb; root_inserted text;
  at text:=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
BEGIN
  PERFORM compute_order_cut_lock();
  IF jsonb_typeof($1::jsonb) IS DISTINCT FROM 'array' OR jsonb_array_length($1::jsonb)>1000 THEN
    RAISE EXCEPTION 'invalid intake batch'; END IF;
  FOR input IN SELECT value FROM jsonb_array_elements($1::jsonb) ORDER BY value->>'id' LOOP
    identity:=input->>'id';
    family:=coalesce(input->>'family_creation'='true',false);
    IF coalesce(length(identity),0)=0 OR jsonb_typeof(input->'record') IS DISTINCT FROM 'object' THEN
      RAISE EXCEPTION 'canonical intake requires a build identity and record'; END IF;
    IF NOT(coalesce((input->>'derived')::boolean,false) AND input#>>'{flow_source,workflow}' IS NULL)
      AND (input#>>'{flow,schema}' IS DISTINCT FROM '1' OR jsonb_typeof(input#>'{flow,scope}') IS DISTINCT FROM 'object'
        OR jsonb_typeof(input#>'{flow,nodes}') IS DISTINCT FROM 'array') THEN
      RAISE EXCEPTION 'canonical intake requires its captured workflow scope'; END IF;
    IF input->>'recovery_of' IS NOT NULL THEN RAISE EXCEPTION 'use the storage-owner recovery operation' USING ERRCODE='42501'; END IF;
    IF EXISTS(SELECT 1 FROM jsonb_array_elements(coalesce(input#>'{flow,nodes}','[]'::jsonb)) n
      WHERE coalesce(n->>'computation','')!~'^[0-9a-f]{32}$') THEN RAISE EXCEPTION 'workflow node requires a canonical computation identity';END IF;
    IF input#>>'{flow_source,workflow}' IS NOT NULL THEN
      SELECT scope.definition,coalesce(w.source->>'family'='true',false) INTO parent_scope,family
        FROM entity_build_workflows w JOIN entity_build_workflow_scopes scope ON scope.id=w.scope_id
        WHERE w.id=input#>>'{flow_source,workflow}';
      IF parent_scope IS NOT NULL AND (jsonb_typeof(parent_scope) IS DISTINCT FROM 'object' OR input#>'{flow,scope}' IS DISTINCT FROM parent_scope)
        THEN RAISE EXCEPTION 'derived input changed its captured workflow scope';END IF;
    END IF;
    IF build_intake_matches_observation(input) AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements(input#>'{flow,nodes}') n
      WHERE n->>'kind'='measure' AND n->>'identity'=identity AND n->>'ruler'=input#>>'{observation,ruler}' AND n->>'mode'=input#>>'{observation,mode}') THEN
      RAISE EXCEPTION 'correction expansion is missing the observed measurement';END IF;
    IF input->'system_source' IS NOT NULL AND input->'system_source'<>'null'::jsonb AND
      (jsonb_typeof(input->'system_source')<>'object' OR coalesce(length(btrim(input#>>'{system_source,id}')),0)=0) THEN
      RAISE EXCEPTION 'system task requires an explicit source identity'; END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended('build-workflow:'||identity,0));
    inserted:=NULL;
    INSERT INTO entity_weapon_builds(id,at,record) VALUES(identity,left(input->>'at',10),(input->'record')::text) ON CONFLICT DO NOTHING RETURNING id INTO inserted;
    SELECT record INTO held FROM entity_weapon_builds WHERE id=identity;
    IF held::jsonb IS DISTINCT FROM input->'record' THEN RAISE EXCEPTION 'build identity has different canonical inputs'; END IF;
    intent:=build_intake_key(input);
    IF EXISTS(SELECT 1 FROM entity_build_intake_intents WHERE id=intent) THEN CONTINUE; END IF;
    plan_key:=NULL;
    IF jsonb_typeof(input->'flow')='object' THEN
      plan_key:=md5((input->'flow')::text);
      INSERT INTO entity_build_intake_plans(id,definition) VALUES(plan_key,input->'flow') ON CONFLICT DO NOTHING;
      SELECT definition INTO captured_plan FROM entity_build_intake_plans WHERE id=plan_key;
      IF captured_plan IS DISTINCT FROM input->'flow' THEN RAISE EXCEPTION 'captured plan identity conflict';END IF;
    END IF;
    SELECT coalesce(jsonb_agg(position ORDER BY ordinal),'[]'::jsonb) INTO positions
      FROM(SELECT ordinality ordinal,nextval('compute_queue_position') position FROM jsonb_array_elements(input#>'{flow,nodes}') WITH ORDINALITY) reserved;
    priority:=CASE WHEN jsonb_typeof(input->'system_source')='object' THEN coalesce((input->>'zone')::bigint,0)
      WHEN input#>>'{observation,valid}'='true' THEN 1 ELSE 3 END;
    lane:=CASE WHEN input->>'initiator'='anonymous' OR input->>'initiator' LIKE 'user:%' OR input->>'initiator' LIKE 'bot:%'
      OR input#>>'{observation,valid}'='true' THEN 'requested' ELSE 'bulk' END;
    INSERT INTO entity_build_intake_intents(id,build_id,plan_id,input,created_at)
      VALUES(intent,identity,plan_key,(input-ARRAY['flow','family_flow','family_root'])||jsonb_build_object(
        'admission_positions',positions,'admission_lane',lane,'admission_at',at,'admission_new',inserted IS NOT NULL),at);
    INSERT INTO relation_build_intake_sources(source_id,intent)
      SELECT value,intent FROM jsonb_array_elements_text(coalesce(input->'from','[]'::jsonb)) ON CONFLICT DO NOTHING;
    INSERT INTO state_build_admission_work(kind,key,zone,lane,position,created_at,retry_at,family)
      VALUES('intake',intent,priority,lane,coalesce((positions->>0)::bigint,nextval('compute_queue_position')),at,at,coalesce(family,false));
    family_plan:=input->'family_flow';
    IF jsonb_typeof(family_plan)='object' AND (jsonb_array_length(family_plan->'nodes')>0 OR jsonb_array_length(family_plan->'builds')>0)
      AND NOT coalesce((input->>'derived')::boolean,false) AND input#>>'{flow_source,workflow}' IS NULL
      AND input->>'observation' IS NULL AND input->>'system_source' IS NULL AND NOT coalesce(family,false) THEN
      root_identity:=coalesce(input#>>'{family_root,id}',identity);
      root_record:=coalesce(input#>'{family_root,record}',input->'record');
      root_inserted:=inserted;
      IF root_identity<>identity THEN
        -- A requested public riven corner can have a different identity from its submitted build.
        IF NOT EXISTS(SELECT 1 FROM jsonb_array_elements(input#>'{flow,builds}') b
          WHERE b->>'id'=root_identity AND b->'record'=root_record AND b#>>'{record,weapon}'=input#>>'{record,weapon}') THEN
          RAISE EXCEPTION 'family root is outside the canonical intake plan'; END IF;
        INSERT INTO entity_weapon_builds(id,at,record) VALUES(root_identity,left(input->>'at',10),root_record::text)
          ON CONFLICT DO NOTHING RETURNING id INTO root_inserted;
        SELECT record INTO held FROM entity_weapon_builds WHERE id=root_identity;
        IF held::jsonb IS DISTINCT FROM root_record THEN RAISE EXCEPTION 'family root has different canonical inputs'; END IF;
      END IF;
      IF root_inserted IS NOT NULL THEN
        PERFORM build_intake_batch(jsonb_build_array(jsonb_build_object('id',root_identity,'record',root_record,
          'at',input->'at','from',input->'from','initiator','system','family_creation',true,
          'flow',family_plan,'deadlines',input->'deadlines'))::text);
      END IF;
    END IF;
  END LOOP;
END $$;
REVOKE ALL ON FUNCTION build_intake_batch(text),build_intake_register(text) FROM PUBLIC;


CREATE OR REPLACE FUNCTION build_workflow_admit_nodes(text,bigint,text) RETURNS bigint
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE expansion_id text:=$1;workflow_id text;plan jsonb;at text:=$3;f record;n record;request jsonb;patch jsonb;
  held text;payload jsonb;verb text;question text;v_review_id text;v_review_ref text;
  specs jsonb:='[]';demands jsonb:='[]';ordinal bigint:=0;processed bigint:=0;touched text[]:='{}';
BEGIN
  PERFORM compute_order_cut_lock();
  IF $2 IS NULL OR $2<1 OR $2>128 THEN RAISE EXCEPTION 'node admission limit must be 1..128';END IF;
  SELECT e.workflow,e.plan INTO STRICT workflow_id,plan FROM entity_build_workflow_expansions e WHERE e.id=expansion_id;
  SELECT w.kind,w.build_id,w.source,w.created_at,s.zone,s.due_at,s.initiator INTO STRICT f FROM entity_build_workflows w JOIN state_build_workflows s ON s.id=w.id WHERE w.id=workflow_id AND s.state='active';
  FOR n IN SELECT * FROM state_build_workflow_nodes WHERE workflow=workflow_id AND expansion=expansion_id AND state='pending' AND NOT compute_order_workflow_node_cancelled(workflow,id) ORDER BY position LIMIT $2 FOR UPDATE SKIP LOCKED LOOP
    touched:=array_append(touched,n.id);processed:=processed+1;
    IF n.definition->>'kind'='measure' THEN
      IF f.kind='intake' AND EXISTS(SELECT 1 FROM state_board_measurement_selection s
        WHERE s.identity=n.definition->>'identity' AND s.ruler=n.definition->>'ruler' AND s.mode=n.definition->>'mode') THEN
        UPDATE state_build_workflow_nodes SET state='skipped',delivered_at=at WHERE workflow=workflow_id AND id=n.id;
        CONTINUE;
      END IF;
      patch:=plan->'patches'->(n.definition->>'patch')::int;
      request:=((plan->'templates'->(n.definition->>'template')::int)||(patch->'set'))-
        ARRAY(SELECT jsonb_array_elements_text(patch->'remove'));
      verb:='simulate';
      SELECT record INTO STRICT held FROM entity_weapon_builds WHERE id=n.definition->>'identity';
      payload:=jsonb_build_object('identity',n.definition->>'identity','record',held::jsonb,
        'ruler',n.definition->>'ruler','mode',n.definition->>'mode','metric',request->>'metric');
      patch:=build_workflow_capture_review(workflow_id,n.id,at);
      IF patch IS NOT NULL THEN payload:=payload||jsonb_build_object('review',patch->>'id','review_ref',patch->>'ref');END IF;
    ELSIF n.definition->>'kind'='optimize' THEN
      request:=n.definition->'request';verb:='optimize_report';
      payload:=jsonb_build_object('context',n.definition->'context','weapon',n.definition->>'weapon','kind','optimization');
    ELSE
      RAISE EXCEPTION 'unsupported workflow node kind';
    END IF;
    question:=n.definition->>'computation';
    IF question IS NULL OR question!~'^[0-9a-f]{32}$' THEN RAISE EXCEPTION 'workflow node requires a canonical computation identity'; END IF;
    payload:=payload||jsonb_build_object('workflow',workflow_id,'node',n.id,'node_kind',n.definition->>'kind','purpose',f.kind);
    specs:=specs||jsonb_build_array(jsonb_build_object('id',question,'verb',verb,'request',request::text,'ordinal',ordinal,
      'position',n.queue_position,'producer','build_workflow','due_at',f.due_at,'waits',0,'zone',f.zone,'initiator',f.initiator,
      'capability',CASE WHEN verb='simulate' THEN 'fixed-board-input-v1' ELSE 'calculation-reports-v1' END,
      'invalidate',f.source#>>'{system,reason}'='retracted measurement proof',
      'force',f.kind IN('correction','system') AND NOT EXISTS(SELECT 1 FROM state_build_workflow_nodes prior
        WHERE prior.workflow=workflow_id AND prior.computation=question AND prior.task_id IS NOT NULL)));
    demands:=demands||jsonb_build_array(jsonb_build_object('producer','build_workflow','ref',workflow_id||'|'||n.id,'question',question,
      'trigger_id',workflow_id,'ordinal',ordinal,'asked_at',f.created_at,'due_at',f.due_at,'waits',0,'payload',payload::text,'zone',f.zone,'initiator',f.initiator));
    ordinal:=ordinal+1;
    -- The observation's engine is provenance, not an execution requirement.
    -- Workflow rounds use the factory's existing executor-compatibility rules.
    IF jsonb_array_length(specs)=250 THEN
      PERFORM compute_admit_tasks(specs::text,demands::text,'',at);specs:='[]';demands:='[]';
    END IF;
  END LOOP;
  IF f.kind='intake' AND f.source->>'origin'='system' AND f.source#>>'{inbox,0}' ~ '^coverage-[0-9a-f]{16}/group/[0-9a-f]{64}$' THEN
    IF EXISTS(SELECT 1 FROM jsonb_array_elements(specs) s GROUP BY s->>'id'
      HAVING count(DISTINCT s-ARRAY['ordinal','position'])>1) THEN
      RAISE EXCEPTION 'grouped aliases do not share a frozen request';
    END IF;
    SELECT coalesce(jsonb_agg(spec ORDER BY ord),'[]'::jsonb) INTO specs
      FROM (SELECT DISTINCT ON (s->>'id') s spec, ord
        FROM jsonb_array_elements(specs) WITH ORDINALITY x(s,ord) ORDER BY s->>'id',ord) shared;
  END IF;
  IF jsonb_array_length(specs)>0 THEN PERFORM compute_admit_tasks(specs::text,demands::text,'',at); END IF;
  UPDATE state_build_workflow_nodes node SET computation=q.id,task_id=q.task_id,state='queued'
    FROM state_compute_work q WHERE node.workflow=workflow_id AND node.expansion=expansion_id AND node.id=ANY(touched) AND node.state='pending' AND q.id=node.definition->>'computation';
  UPDATE entity_measurement_reviews r SET task_id=node.task_id FROM state_build_workflow_nodes node
    WHERE node.workflow=workflow_id AND node.expansion=expansion_id AND node.id=ANY(touched) AND r.id=node.definition->>'review' AND r.task_id IS NULL;
  INSERT INTO relation_compute_task_sources(task_id,producer,ref,trigger_id,asked_at,payload,requirements)
    SELECT node.task_id,'build_workflow',workflow_id||'|'||node.id,t.id,t.at,
      jsonb_build_object('workflow',workflow_id,'node',node.id,'source',t.source)::text,
      jsonb_build_object('force',f.kind IN('correction','system'),'zone',f.zone,'engine',NULL)
    FROM state_build_workflow_nodes node JOIN record_build_workflow_triggers t ON t.workflow=node.workflow
    WHERE node.workflow=workflow_id AND node.expansion=expansion_id AND node.id=ANY(touched) AND node.task_id IS NOT NULL ON CONFLICT DO NOTHING;
  PERFORM build_workflow_finish(workflow_id,at);
  RETURN processed;
END $$;

CREATE OR REPLACE VIEW state_build_effective_admission_work AS
  SELECT w.* FROM state_build_admission_work w WHERE NOT EXISTS(SELECT 1 FROM compute_order_cut())
    OR w.kind='intake' AND NOT compute_order_pending_cancelled(w.zone,w.position,w.key,w.created_at)
    OR w.kind='expansion' AND EXISTS(SELECT 1 FROM entity_build_workflow_expansions e
      JOIN state_build_workflow_nodes n ON n.expansion=e.id AND n.workflow=e.workflow
      WHERE e.id=w.key AND n.state='pending' AND NOT compute_order_workflow_node_cancelled(n.workflow,n.id));

CREATE OR REPLACE FUNCTION build_admission_claim(text,text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held state_build_admission_work%ROWTYPE;wanted text;token text;expiry text;
BEGIN
  PERFORM compute_order_cut_lock();
  IF $1 NOT IN('intake','expansion') OR $2 IS NULL THEN RAISE EXCEPTION 'invalid admission claim';END IF;
  INSERT INTO state_build_admission_turns VALUES($1,'requested') ON CONFLICT DO NOTHING;
  SELECT lane INTO wanted FROM state_build_admission_turns WHERE kind=$1 FOR UPDATE;
  WITH heads AS MATERIALIZED(
    (SELECT key FROM state_build_effective_admission_work WHERE kind=$1 AND lane='requested' AND NOT family AND retry_at<=$2 AND (lease IS NULL OR lease_until<=$2)
      ORDER BY zone,position,key LIMIT 1)
    UNION ALL
    (SELECT key FROM state_build_effective_admission_work WHERE kind=$1 AND lane='bulk' AND NOT family AND retry_at<=$2 AND (lease IS NULL OR lease_until<=$2)
      ORDER BY zone,position,key LIMIT 1))
  SELECT w.* INTO held FROM heads h JOIN state_build_admission_work w ON w.kind=$1 AND w.key=h.key
    ORDER BY w.zone,w.lane<>wanted,w.position,w.key LIMIT 1 FOR UPDATE OF w SKIP LOCKED;
  IF held.key IS NULL THEN RETURN NULL;END IF;
  token:=gen_random_uuid()::text;
  expiry:=to_char(($2::timestamptz+interval '2minutes') AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
  UPDATE state_build_admission_work SET lease=token,lease_until=expiry,attempts=attempts+1 WHERE kind=held.kind AND key=held.key;
  UPDATE state_build_admission_turns SET lane=CASE WHEN held.lane='requested' THEN 'bulk' ELSE 'requested' END WHERE kind=$1;
  RETURN jsonb_build_object('kind',held.kind,'key',held.key,'zone',held.zone,'lane',held.lane,'position',held.position,'lease',token,'lease_until',expiry);
END $$;

CREATE OR REPLACE FUNCTION build_admission_run(text,text,text,bigint,text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held state_build_admission_work%ROWTYPE;captured jsonb;processed bigint:=0;pending boolean:=false;
BEGIN
  PERFORM compute_order_cut_lock();
  IF $4 IS NULL OR $4<1 OR $4>128 THEN RAISE EXCEPTION 'node admission limit must be 1..128';END IF;
  SELECT * INTO held FROM state_build_admission_work WHERE kind=$1 AND key=$2 AND lease=$3 AND lease_until>$5 FOR UPDATE;
  IF NOT FOUND THEN RAISE EXCEPTION 'admission claim changed' USING ERRCODE='40001';END IF;
  IF NOT EXISTS(SELECT 1 FROM state_build_effective_admission_work WHERE kind=held.kind AND key=held.key) THEN
    RETURN jsonb_build_object('kind',held.kind,'key',held.key,'processed',0,'pending',false,'cancelled',true);END IF;
  IF held.kind='intake' THEN
    SELECT i.input||CASE WHEN p.id IS NOT NULL THEN jsonb_build_object('flow',p.definition) ELSE '{}'::jsonb END INTO STRICT captured
      FROM entity_build_intake_intents i LEFT JOIN entity_build_intake_plans p ON p.id=i.plan_id WHERE i.id=held.key;
    PERFORM build_intake_register(jsonb_build_array(captured)::text);processed:=1;
  ELSIF EXISTS(SELECT 1 FROM entity_build_workflow_expansions e JOIN state_build_workflows f ON f.id=e.workflow WHERE e.id=held.key AND f.state='active') THEN
    processed:=build_workflow_admit_nodes(held.key,$4,$5);
    SELECT EXISTS(SELECT 1 FROM entity_build_workflow_expansions e JOIN state_build_workflow_nodes n ON n.workflow=e.workflow
      WHERE e.id=held.key AND n.expansion=e.id AND n.state='pending' AND NOT compute_order_workflow_node_cancelled(n.workflow,n.id)) INTO pending;
  END IF;
  IF pending THEN UPDATE state_build_admission_work SET lease=NULL,lease_until=NULL,error=NULL WHERE kind=held.kind AND key=held.key;
  ELSE DELETE FROM state_build_admission_work WHERE kind=held.kind AND key=held.key;END IF;
  IF processed>0 THEN PERFORM pg_notify('wfsim_task_pipeline','admission');END IF;
  RETURN jsonb_build_object('kind',held.kind,'key',held.key,'processed',processed,'pending',pending);
END $$;

CREATE OR REPLACE FUNCTION build_admission_retry(text,text,text,text,text) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE changed bigint;
BEGIN
  UPDATE state_build_admission_work SET lease=NULL,lease_until=NULL,error=left($4,1000),
    retry_at=to_char(($5::timestamptz+interval '30seconds') AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
    WHERE kind=$1 AND key=$2 AND lease=$3;
  GET DIAGNOSTICS changed=ROW_COUNT;RETURN changed=1;
END $$;
REVOKE ALL ON FUNCTION build_workflow_capture(text,text,jsonb,text,jsonb),build_workflow_admit_nodes(text,bigint,text),
  build_admission_claim(text,text),build_admission_run(text,text,text,bigint,text),build_admission_retry(text,text,text,text,text) FROM PUBLIC;

-- Future arrivals have an indexed FIFO lane; retained legacy inputs keep their existing bounded scan.
CREATE TABLE IF NOT EXISTS state_build_inbox_positions (
  id TEXT COLLATE "C" PRIMARY KEY,
  position BIGINT NOT NULL DEFAULT nextval('compute_queue_position')
);
CREATE INDEX IF NOT EXISTS build_inbox_fifo ON state_build_inbox_positions(position,id);
CREATE OR REPLACE FUNCTION build_inbox_position() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  IF TG_OP='INSERT' THEN INSERT INTO state_build_inbox_positions(id) VALUES(NEW.id) ON CONFLICT DO NOTHING;RETURN NEW;
  ELSE DELETE FROM state_build_inbox_positions WHERE id=OLD.id;RETURN OLD;END IF;
END $$;
DROP TRIGGER IF EXISTS build_inbox_positioned ON record_build_submissions;
CREATE TRIGGER build_inbox_positioned AFTER INSERT OR DELETE ON record_build_submissions FOR EACH ROW EXECUTE FUNCTION build_inbox_position();
DROP TRIGGER IF EXISTS equipment_inbox_positioned ON record_equipment_submissions;
CREATE TRIGGER equipment_inbox_positioned AFTER INSERT OR DELETE ON record_equipment_submissions FOR EACH ROW EXECUTE FUNCTION build_inbox_position();


CREATE OR REPLACE FUNCTION build_workflow_capture_review(text,text,text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE workflow_id text:=$1;at text:=$3;n state_build_workflow_nodes%ROWTYPE;f record;held text;v_review_id text;v_review_ref text;
BEGIN
  SELECT * INTO STRICT n FROM state_build_workflow_nodes WHERE workflow=workflow_id AND id=$2;
  SELECT w.kind,w.build_id,w.source INTO STRICT f FROM entity_build_workflows w WHERE w.id=workflow_id;
  SELECT record INTO STRICT held FROM entity_weapon_builds WHERE id=n.definition->>'identity';
      IF f.kind IN('correction','system') AND EXISTS(SELECT 1 FROM state_board_measurement_selection s
        WHERE s.identity=n.definition->>'identity' AND s.ruler=n.definition->>'ruler' AND s.mode=n.definition->>'mode') THEN
        IF n.definition->>'identity'=f.build_id AND n.definition->>'ruler'=f.source#>>'{observation,ruler}'
          AND n.definition->>'mode'=f.source#>>'{observation,mode}' THEN
          v_review_id:=f.source#>>'{observation,review}';
          SELECT ref INTO v_review_ref FROM entity_measurement_reviews WHERE id=v_review_id;
        ELSE
          v_review_ref:=workflow_id||'|'||n.id;
          SELECT id INTO v_review_id FROM entity_measurement_reviews WHERE ref=v_review_ref AND workflow=workflow_id;
          IF NOT FOUND THEN
          v_review_id:=gen_random_uuid()::text;
          INSERT INTO entity_measurement_reviews(ref,id,round_at,identity,ruler,mode,engine,question,record,state,
            baseline_score,baseline_metric,baseline_cost_seconds,baseline_review_id,baseline_by,baseline_started_at,baseline_finished_at,created_at,workflow)
            SELECT v_review_ref,v_review_id,at,s.identity,s.ruler,s.mode,coalesce(f.source#>>'{observation,engine}',''),
              n.definition->>'computation',held,'waiting',s.score,s.metric,s.cost_seconds,s.review_id,s.measured_by,s.started_at,s.finished_at,at,workflow_id
              FROM state_board_measurement_selection s WHERE s.identity=n.definition->>'identity' AND s.ruler=n.definition->>'ruler' AND s.mode=n.definition->>'mode';
          END IF;
        END IF;
        UPDATE state_build_workflow_nodes SET definition=definition||jsonb_build_object('review',v_review_id)
          WHERE workflow=workflow_id AND id=n.id;
        UPDATE entity_measurement_reviews SET question=n.definition->>'computation',workflow=workflow_id WHERE id=v_review_id AND task_id IS NULL;
        RETURN jsonb_build_object('id',v_review_id,'ref',v_review_ref);
      END IF;
  RETURN NULL;
END $$;
REVOKE ALL ON FUNCTION build_workflow_capture_review(text,text,text) FROM PUBLIC;

INSERT INTO state_task_pipeline_controls(name,paused,changed_at,changed_by) VALUES('family',true,
  to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'schema') ON CONFLICT DO NOTHING;

-- Only continuations of the independently admitted creation workflows use this inbox.
CREATE OR REPLACE FUNCTION build_family_capture_output() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  IF split_part(NEW.family_source,'|',1)='workflow' AND EXISTS(SELECT 1 FROM entity_build_workflows w
    WHERE w.id=split_part(NEW.family_source,'|',2) AND w.source->>'family'='true') THEN
    INSERT INTO state_build_family_inbox(id,retry_at,input) VALUES(NEW.id,NEW.at,to_jsonb(NEW)) ON CONFLICT DO NOTHING;
    IF NOT FOUND AND NOT EXISTS(SELECT 1 FROM state_build_family_inbox f
      WHERE f.id=NEW.id AND f.input-'at'=to_jsonb(NEW)-'at') THEN
      RAISE EXCEPTION 'family source already owns a different input' USING ERRCODE='23505';
    END IF;
    RETURN NULL;
  END IF;
  RETURN NEW;
END $$;
DROP TRIGGER IF EXISTS build_family_output_ready ON record_build_submissions;
CREATE TRIGGER build_family_output_ready BEFORE INSERT ON record_build_submissions
  FOR EACH ROW WHEN(NEW.family_source IS NOT NULL) EXECUTE FUNCTION build_family_capture_output();

CREATE OR REPLACE FUNCTION build_family_owns_inbox(text,jsonb) RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
  SELECT EXISTS(SELECT 1 FROM state_build_family_inbox f WHERE f.id=$1 AND f.input=$2)
$$;
REVOKE ALL ON FUNCTION build_family_owns_inbox(text,jsonb) FROM PUBLIC;

DO $path$
DECLARE routine text; namespace text:=pg_catalog.current_schema();
BEGIN
  FOREACH routine IN ARRAY ARRAY['build_ack_submissions(text)','build_family_owns_inbox(text,jsonb)','build_inbox_assert_ack(text)'] LOOP
    EXECUTE pg_catalog.format('ALTER FUNCTION %I.%s SET search_path=%I,pg_catalog,pg_temp',namespace,routine,namespace);
  END LOOP;
END $path$;

CREATE OR REPLACE FUNCTION build_family_claim(text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held state_build_family_inbox%ROWTYPE;
BEGIN
  PERFORM 1 FROM state_task_pipeline_controls WHERE name='family' AND NOT paused FOR SHARE;
  IF NOT FOUND THEN RETURN NULL; END IF;
  SELECT * INTO held FROM state_build_family_inbox WHERE retry_at<=$1 AND (lease IS NULL OR lease_until<=$1)
    ORDER BY retry_at,id LIMIT 1 FOR UPDATE SKIP LOCKED;
  IF NOT FOUND THEN RETURN NULL; END IF;
  UPDATE state_build_family_inbox SET lease=gen_random_uuid(),
    lease_until=to_char(($1::timestamptz+interval '2minutes') AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
    WHERE id=held.id RETURNING * INTO held;
  RETURN held.input||jsonb_build_object('lease',held.lease,'workflow_scope',
    (SELECT s.definition::text FROM entity_build_workflows w JOIN entity_build_workflow_scopes s ON s.id=w.scope_id
      WHERE w.id=split_part(held.input->>'family_source','|',2)));
END $$;

CREATE OR REPLACE FUNCTION build_family_stage(text,uuid,text,text) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE input jsonb:=$3::jsonb; captured jsonb; workflow_id text; node_id text;
BEGIN
  PERFORM 1 FROM state_task_pipeline_controls WHERE name='family' AND NOT paused FOR SHARE;
  IF NOT FOUND THEN RAISE EXCEPTION 'family intake is paused' USING ERRCODE='55000'; END IF;
  SELECT f.input INTO captured FROM state_build_family_inbox f
    WHERE id=$1 AND lease=$2 AND lease_until>$4 FOR UPDATE;
  IF NOT FOUND THEN RAISE EXCEPTION 'family input lease changed' USING ERRCODE='40001'; END IF;
  workflow_id:=split_part(captured->>'family_source','|',2);
  node_id:=split_part(captured->>'family_source','|',3);
  IF input IS NOT NULL THEN
    IF input->'from' IS DISTINCT FROM jsonb_build_array($1) OR input->>'derived' IS DISTINCT FROM 'true'
      OR input->>'family_flow' IS NOT NULL OR input->>'system_source' IS NOT NULL
      OR input->>'family_creation' IS NOT NULL OR input->>'observation' IS NOT NULL
      OR input#>>'{flow_source,workflow}' IS DISTINCT FROM workflow_id
      OR input#>>'{flow_source,node}' IS DISTINCT FROM node_id
      OR EXISTS(SELECT 1 FROM jsonb_array_elements(input#>'{flow,nodes}') node
        WHERE node->>'kind' IS DISTINCT FROM 'measure') THEN
      RAISE EXCEPTION 'family output changed its captured source'; END IF;
    PERFORM build_intake_batch(jsonb_build_array(input)::text);
  ELSE
    UPDATE state_build_workflow_nodes SET state='failed',error='derived build was refused by intake'
      WHERE workflow=workflow_id AND id=node_id AND state='awaiting_intake';
    PERFORM build_workflow_finish(workflow_id,$4);
  END IF;
  DELETE FROM record_build_submissions b WHERE b.id=$1 AND to_jsonb(b)=captured;
  DELETE FROM state_build_family_inbox WHERE id=$1;
END $$;

CREATE OR REPLACE FUNCTION build_family_retry(text,uuid,text,text) RETURNS void
LANGUAGE sql SECURITY DEFINER SET search_path FROM CURRENT AS $$
  UPDATE state_build_family_inbox SET lease=NULL,lease_until=NULL,error=left($3,1000),
    retry_at=to_char(($4::timestamptz+interval '30 seconds') AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
    WHERE id=$1 AND lease=$2 AND lease_until>$4
$$;

CREATE OR REPLACE FUNCTION build_family_admit(text,bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held state_build_admission_work%ROWTYPE; token text:=gen_random_uuid()::text; failure text;
BEGIN
  PERFORM 1 FROM state_task_pipeline_controls WHERE name='family' AND NOT paused FOR SHARE;
  IF NOT FOUND THEN RETURN NULL; END IF;
  IF $2 IS NULL OR $2<1 OR $2>16 THEN RAISE EXCEPTION 'family admission limit must be 1..16'; END IF;
  PERFORM compute_order_cut_lock();
  SELECT w.* INTO held FROM state_build_admission_work w
    WHERE w.family AND w.retry_at<=$1 AND (w.lease IS NULL OR w.lease_until<=$1)
    ORDER BY w.position,w.key LIMIT 1 FOR UPDATE SKIP LOCKED;
  IF NOT FOUND THEN RETURN NULL; END IF;
  IF NOT EXISTS(SELECT 1 FROM state_build_effective_admission_work WHERE kind=held.kind AND key=held.key) THEN
    DELETE FROM state_build_admission_work WHERE kind=held.kind AND key=held.key;
    RETURN jsonb_build_object('kind',held.kind,'key',held.key,'cancelled',true);
  END IF;
  UPDATE state_build_admission_work SET lease=token,
    lease_until=to_char(($1::timestamptz+interval '2minutes') AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
    WHERE kind=held.kind AND key=held.key;
  BEGIN
    RETURN build_admission_run(held.kind,held.key,token,$2,$1);
  EXCEPTION WHEN OTHERS THEN
    GET STACKED DIAGNOSTICS failure=MESSAGE_TEXT;
    PERFORM build_admission_retry(held.kind,held.key,token,left(failure,1000),$1);
    RETURN jsonb_build_object('kind',held.kind,'key',held.key,'error',failure);
  END;
END $$;
REVOKE ALL ON FUNCTION build_family_capture_output(),build_family_claim(text),build_family_stage(text,uuid,text,text),
  build_family_retry(text,uuid,text,text),build_family_admit(text,bigint) FROM PUBLIC;
