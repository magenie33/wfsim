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
ALTER TABLE record_build_submissions ADD COLUMN IF NOT EXISTS observation TEXT COLLATE "C";
ALTER TABLE entity_measurement_reviews ADD COLUMN IF NOT EXISTS workflow TEXT COLLATE "C";
CREATE OR REPLACE FUNCTION build_workflow_system_key(text,text) RETURNS text LANGUAGE sql IMMUTABLE AS $$
  SELECT 'system:'||md5(jsonb_build_array($1,$2)::text)
$$;
CREATE OR REPLACE VIEW state_build_inbox_inputs AS
  SELECT b.id,b.at,b.record,b.initiator,b.family_source,b.observation,s.definition::text AS workflow_scope
    FROM record_build_submissions b LEFT JOIN entity_build_workflows w
      ON split_part(b.family_source,'|',1)='workflow' AND w.id=split_part(b.family_source,'|',2)
      LEFT JOIN entity_build_workflow_scopes s ON s.id=w.scope_id
  UNION ALL SELECT id,at,record,NULL,NULL,NULL,NULL FROM record_equipment_submissions;

CREATE OR REPLACE FUNCTION compute_round_occupied(text,bigint) RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT EXISTS(SELECT 1 FROM relation_task_consumers WHERE question=$1 AND state='live')
    OR EXISTS(SELECT 1 FROM state_build_workflow_nodes n JOIN state_build_workflows f ON f.id=n.workflow
      WHERE n.computation=$1 AND n.task_id=$2 AND f.state='active')
$$;
CREATE OR REPLACE FUNCTION build_workflow_wanted(text,text) RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT EXISTS(SELECT 1 FROM state_build_workflow_nodes n JOIN state_build_workflows f ON f.id=n.workflow
    WHERE n.workflow=split_part($1,'|',1) AND n.id=split_part($1,'|',2) AND n.computation=$2 AND n.state='queued' AND f.state='active')
$$;
CREATE OR REPLACE FUNCTION build_workflow_delivery_guard(text,text,text,bigint) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  PERFORM 1 FROM state_build_workflow_nodes n JOIN state_build_workflows f ON f.id=n.workflow
    WHERE n.workflow=$1 AND n.id=$2 AND n.computation=$3 AND n.task_id=$4 AND n.state='queued' AND f.state='active' FOR UPDATE OF n;
  IF NOT FOUND THEN RAISE EXCEPTION 'workflow node changed before delivery' USING ERRCODE='WS002'; END IF;
END $$;

CREATE OR REPLACE FUNCTION build_workflow_finish(text,text) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  PERFORM 1 FROM state_build_workflows WHERE id=$1 AND state='active' FOR UPDATE;
  IF NOT FOUND THEN RETURN; END IF;
  IF NOT EXISTS(SELECT 1 FROM state_build_workflow_nodes WHERE workflow=$1 AND state IN('pending','queued','awaiting_intake')) THEN
    UPDATE state_build_workflows SET state=CASE WHEN EXISTS(
      SELECT 1 FROM state_build_workflow_nodes WHERE workflow=$1 AND state='failed') THEN 'failed' ELSE 'succeeded' END,
      completed_at=$2,initiator=NULL WHERE id=$1;
    UPDATE entity_measurement_reviews SET state='superseded',completed_at=$2
      WHERE workflow=$1 AND state='waiting' AND task_id IS NULL;
  END IF;
END $$;

CREATE OR REPLACE FUNCTION build_workflow_expansion(text,text,jsonb,text) RETURNS void LANGUAGE plpgsql AS $$
DECLARE expansion_id text; b jsonb; held text; n record; f record; request jsonb; patch jsonb;
  specs jsonb:='[]';demands jsonb:='[]';payload jsonb;ordinal bigint:=0;verb text;question text;v_review_id text;v_review_ref text;
BEGIN
  IF jsonb_typeof($3->'nodes') IS DISTINCT FROM 'array' OR jsonb_typeof($3->'builds') IS DISTINCT FROM 'array' THEN
    RAISE EXCEPTION 'invalid workflow expansion'; END IF;
  expansion_id:=$1||':'||coalesce($2,'root');
  IF EXISTS(SELECT 1 FROM entity_build_workflow_expansions WHERE id=expansion_id) THEN RETURN; END IF;
  FOR b IN SELECT value FROM jsonb_array_elements($3->'builds') ORDER BY value->>'id' LOOP
    INSERT INTO entity_weapon_builds(id,at,record) VALUES(b->>'id',left($4,10),(b->'record')::text) ON CONFLICT DO NOTHING;
    SELECT record INTO held FROM entity_weapon_builds WHERE id=b->>'id';
    IF held::jsonb IS DISTINCT FROM b->'record' THEN RAISE EXCEPTION 'build identity has different canonical inputs'; END IF;
  END LOOP;
  INSERT INTO entity_build_workflow_expansions(id,workflow,parent_node,plan,created_at) VALUES(expansion_id,$1,$2,$3-'scope'-'builds',$4);
  INSERT INTO state_build_workflow_nodes(workflow,id,expansion,definition,state,retry_at)
    SELECT $1,coalesce($2||'.','')||(ordinality-1)::text,expansion_id,value,'pending',$4
      FROM jsonb_array_elements($3->'nodes') WITH ORDINALITY;
  SELECT w.kind,w.build_id,w.source,s.zone,s.due_at,s.initiator INTO STRICT f FROM entity_build_workflows w JOIN state_build_workflows s ON s.id=w.id WHERE w.id=$1;
  FOR n IN SELECT * FROM state_build_workflow_nodes WHERE expansion=expansion_id ORDER BY position LOOP
    IF n.definition->>'kind'='measure' THEN
      patch:=$3->'patches'->(n.definition->>'patch')::int;
      request:=(($3->'templates'->(n.definition->>'template')::int)||(patch->'set'))-
        ARRAY(SELECT jsonb_array_elements_text(patch->'remove'));
      verb:='simulate';
      SELECT record INTO STRICT held FROM entity_weapon_builds WHERE id=n.definition->>'identity';
      payload:=jsonb_build_object('identity',n.definition->>'identity','record',held::jsonb,
        'ruler',n.definition->>'ruler','mode',n.definition->>'mode','metric',request->>'metric');
      IF f.kind IN('correction','system') AND EXISTS(SELECT 1 FROM state_board_measurement_selection s
        WHERE s.identity=n.definition->>'identity' AND s.ruler=n.definition->>'ruler' AND s.mode=n.definition->>'mode') THEN
        IF n.definition->>'identity'=f.build_id AND n.definition->>'ruler'=f.source#>>'{observation,ruler}'
          AND n.definition->>'mode'=f.source#>>'{observation,mode}' THEN
          v_review_id:=f.source#>>'{observation,review}';
          SELECT ref INTO v_review_ref FROM entity_measurement_reviews WHERE id=v_review_id;
        ELSE
          v_review_id:=gen_random_uuid()::text;v_review_ref:=$1||'|'||n.id;
          INSERT INTO entity_measurement_reviews(ref,id,round_at,identity,ruler,mode,engine,question,record,state,
            baseline_score,baseline_metric,baseline_cost_seconds,baseline_review_id,baseline_by,baseline_started_at,baseline_finished_at,created_at,workflow)
            SELECT v_review_ref,v_review_id,$4,s.identity,s.ruler,s.mode,coalesce(f.source#>>'{observation,engine}',''),
              n.definition->>'computation',held,'waiting',s.score,s.metric,s.cost_seconds,s.review_id,s.measured_by,s.started_at,s.finished_at,$4,$1
              FROM state_board_measurement_selection s WHERE s.identity=n.definition->>'identity' AND s.ruler=n.definition->>'ruler' AND s.mode=n.definition->>'mode';
        END IF;
        payload:=payload||jsonb_build_object('review',v_review_id,'review_ref',v_review_ref);
        UPDATE state_build_workflow_nodes SET definition=definition||jsonb_build_object('review',v_review_id)
          WHERE workflow=$1 AND id=n.id;
        UPDATE entity_measurement_reviews SET question=n.definition->>'computation',workflow=$1 WHERE id=v_review_id AND task_id IS NULL;
      END IF;
    ELSIF n.definition->>'kind'='optimize' THEN
      request:=n.definition->'request';verb:='optimize_report';
      payload:=jsonb_build_object('context',n.definition->'context','weapon',n.definition->>'weapon','kind','optimization');
    ELSE
      RAISE EXCEPTION 'unsupported workflow node kind';
    END IF;
    question:=n.definition->>'computation';
    IF question IS NULL OR question!~'^[0-9a-f]{32}$' THEN RAISE EXCEPTION 'workflow node requires a canonical computation identity'; END IF;
    payload:=payload||jsonb_build_object('workflow',$1,'node',n.id,'node_kind',n.definition->>'kind','purpose',f.kind);
    specs:=specs||jsonb_build_array(jsonb_build_object('id',question,'verb',verb,'request',request::text,'ordinal',ordinal,
      'producer','build_workflow','due_at',f.due_at,'waits',0,'zone',f.zone,'initiator',f.initiator,
      'capability',CASE WHEN verb='simulate' THEN 'fixed-board-input-v1' ELSE 'calculation-reports-v1' END,
      'invalidate',f.source#>>'{system,reason}'='retracted measurement proof',
      'force',f.kind IN('correction','system')));
    demands:=demands||jsonb_build_array(jsonb_build_object('producer','build_workflow','ref',$1||'|'||n.id,'question',question,
      'trigger_id',$1,'ordinal',ordinal,'due_at',f.due_at,'waits',0,'payload',payload::text,'zone',f.zone,'initiator',f.initiator));
    ordinal:=ordinal+1;
    IF jsonb_array_length(specs)=250 THEN
      PERFORM compute_admit_tasks(specs::text,demands::text,coalesce(f.source#>>'{observation,engine}',''),$4);specs:='[]';demands:='[]';
    END IF;
  END LOOP;
  IF jsonb_array_length(specs)>0 THEN PERFORM compute_admit_tasks(specs::text,demands::text,coalesce(f.source#>>'{observation,engine}',''),$4); END IF;
  UPDATE state_build_workflow_nodes node SET computation=q.id,task_id=q.task_id,state='queued'
    FROM state_compute_scheduling q WHERE node.expansion=expansion_id AND node.state='pending' AND q.id=node.definition->>'computation';
  UPDATE entity_measurement_reviews r SET task_id=node.task_id FROM state_build_workflow_nodes node
    WHERE node.expansion=expansion_id AND r.id=node.definition->>'review' AND r.task_id IS NULL;
END $$;

-- First registration and its captured intent commit together. Repeated input
-- does not manufacture a missing-coverage request.
CREATE OR REPLACE FUNCTION build_intake_batch(text) RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE input jsonb; plan jsonb; observation jsonb; inserted text; held text; flow_id text;
  at text:=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
  reason text; priority bigint; origin jsonb; parent text; node_key text; captured_key text; trigger_key text;
BEGIN
  IF jsonb_typeof($1::jsonb) IS DISTINCT FROM 'array' OR jsonb_array_length($1::jsonb)>1000 THEN
    RAISE EXCEPTION 'invalid intake batch'; END IF;
  FOR input IN SELECT value FROM jsonb_array_elements($1::jsonb) ORDER BY value->>'id' LOOP
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
    trigger_key:=coalesce(observation->>'review',CASE WHEN input#>>'{system_source,id}' IS NOT NULL
      THEN build_workflow_system_key(input#>>'{system_source,id}',input->>'id') END);
    IF EXISTS(SELECT 1 FROM record_build_workflow_triggers WHERE id=trigger_key) THEN CONTINUE; END IF;
    parent:=input#>>'{flow_source,workflow}';node_key:=input#>>'{flow_source,node}';
    IF parent IS NOT NULL THEN
      PERFORM 1 FROM state_build_workflow_nodes n JOIN state_build_workflows f ON f.id=n.workflow
        WHERE n.workflow=parent AND n.id=node_key AND n.state='awaiting_intake' AND f.state='active' FOR UPDATE OF n;
      IF FOUND THEN
        PERFORM build_workflow_expansion(parent,node_key,plan,at);
        UPDATE state_build_workflow_nodes SET state='succeeded' WHERE workflow=parent AND id=node_key;
        PERFORM build_workflow_finish(parent,at);
      END IF;
      CONTINUE;
    END IF;
    IF coalesce((input->>'derived')::boolean,false) THEN CONTINUE; END IF;
    reason:=NULL;
    IF jsonb_typeof(input->'system_source')='object' THEN reason:='system';priority:=coalesce((input->>'zone')::bigint,0);
    ELSIF inserted IS NOT NULL THEN reason:='intake';priority:=3;
    ELSIF observation->>'valid'='true' AND EXISTS(SELECT 1 FROM state_board_measurement_selection s
      JOIN entity_measurement_reviews r ON r.id=observation->>'review'
      WHERE s.identity=input->>'id' AND s.ruler=observation->>'ruler' AND s.mode=observation->>'mode'
        AND r.identity=s.identity AND r.ruler=s.ruler AND r.mode=s.mode AND r.state='waiting'
        AND s.finished_at=observation->>'baseline_finished_at' AND s.score=(observation->>'baseline_score')::double precision
        AND s.metric=observation->>'metric' AND abs(s.score-(observation->>'score')::double precision)>
          1e-12*greatest(abs(s.score),abs((observation->>'score')::double precision))) THEN
      reason:='correction';priority:=1;
    END IF;
    IF reason IS NULL THEN
      IF observation->>'review' IS NOT NULL THEN UPDATE entity_measurement_reviews SET state='superseded',completed_at=at
        WHERE id=observation->>'review' AND state='waiting'; END IF;
      CONTINUE;
    END IF;
    IF plan IS NULL OR plan->>'schema' IS DISTINCT FROM '1' THEN RAISE EXCEPTION 'first intake requires a captured workflow'; END IF;
    captured_key:=md5((input->>'id')||E'\n'||plan::text);
    SELECT id INTO flow_id FROM state_build_workflows WHERE scope_key=captured_key AND state='active' FOR UPDATE;
    origin:=jsonb_build_object('kind',reason,'inbox',coalesce(input->'from','[]'::jsonb),'observation',observation,
      'system',input->'system_source','deadlines',input->'deadlines','origin',CASE WHEN reason='system' OR input->>'initiator'='system' THEN 'system'
        WHEN input->>'initiator'='anonymous' OR input->>'initiator' LIKE 'user:%' OR input->>'initiator' LIKE 'bot:%' THEN 'requested' ELSE 'unknown' END);
    IF flow_id IS NULL THEN
      flow_id:=gen_random_uuid()::text;
      INSERT INTO entity_build_workflow_scopes(id,definition) VALUES(md5((plan->'scope')::text),plan->'scope') ON CONFLICT DO NOTHING;
      INSERT INTO entity_build_workflows(id,build_id,scope_key,kind,scope_id,source,created_at)
        VALUES(flow_id,input->>'id',captured_key,reason,md5((plan->'scope')::text),origin,at);
      INSERT INTO state_build_workflows(id,build_id,scope_key,state,zone,due_at,initiator) VALUES(flow_id,input->>'id',captured_key,'active',priority,
        coalesce(input#>>ARRAY['deadlines',reason],at),coalesce(input->>'initiator',CASE WHEN reason='system' THEN 'system' END));
      PERFORM build_workflow_expansion(flow_id,NULL,plan,at);
    ELSE
      UPDATE state_build_workflows SET zone=least(zone,priority),due_at=least(due_at,coalesce(input#>>ARRAY['deadlines',reason],at)) WHERE id=flow_id;
      UPDATE relation_task_consumers d SET zone=least(coalesce(d.zone,3),priority),due_at=least(d.due_at,coalesce(input#>>ARRAY['deadlines',reason],at))
        WHERE d.producer='build_workflow' AND split_part(d.ref,'|',1)=flow_id AND d.state='live';
      PERFORM factory_restate(node.computation) FROM state_build_workflow_nodes node WHERE node.workflow=flow_id AND node.state='queued' ORDER BY node.computation;
    END IF;
    INSERT INTO record_build_workflow_triggers(id,workflow,source,at)
      VALUES(coalesce(trigger_key,'intake:'||(input->>'id')),flow_id,origin,at) ON CONFLICT DO NOTHING;
    IF observation->>'review' IS NOT NULL THEN UPDATE entity_measurement_reviews SET workflow=flow_id
      WHERE id=observation->>'review' AND state='waiting'; END IF;
    PERFORM build_workflow_finish(flow_id,at);
  END LOOP;
END $$;

CREATE OR REPLACE FUNCTION build_spend_submissions(text) RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE input record; at text:=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
BEGIN
  FOR input IN SELECT b.* FROM record_build_submissions b JOIN jsonb_array_elements_text($1::jsonb) x ON b.id=x.value ORDER BY b.id FOR UPDATE OF b LOOP
    UPDATE entity_measurement_reviews SET state='failed',completed_at=at
      WHERE id=input.observation::jsonb->>'review' AND state='waiting' AND workflow IS NULL AND task_id IS NULL;
    IF split_part(input.family_source,'|',1)='workflow' THEN
      UPDATE state_build_workflow_nodes SET state='failed',error='derived build was refused by intake'
        WHERE workflow=split_part(input.family_source,'|',2) AND id=split_part(input.family_source,'|',3) AND state='awaiting_intake';
      PERFORM build_workflow_finish(split_part(input.family_source,'|',2),at);
    END IF;
  END LOOP;
  DELETE FROM record_build_submissions WHERE id IN(SELECT jsonb_array_elements_text($1::jsonb));
END $$;

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
  PERFORM build_intake_batch(jsonb_build_array(jsonb_build_object('id',s.identity,'at',$4,
    'record',(SELECT record::jsonb FROM entity_weapon_builds WHERE id=s.identity),'flow',plan,
    'initiator','system','zone',1,'system_source',jsonb_build_object('id',source_id,'reason','retracted measurement proof','computation',$3)))::text);
  SELECT workflow INTO next_flow FROM record_build_workflow_triggers WHERE id=build_workflow_system_key(source_id,s.identity);
  UPDATE entity_measurement_reviews r SET invalidated_at=$4,invalidation_reason='retracted measurement proof'
    WHERE r.workflow=next_flow AND r.state='waiting';
  UPDATE state_board_measurement_selection current SET review_id=r.id FROM entity_measurement_reviews r
    WHERE r.workflow=next_flow AND r.state='waiting' AND current.identity=r.identity AND current.ruler=r.ruler AND current.mode=r.mode
      AND current.finished_at=s.finished_at;
END $$;
REVOKE ALL ON FUNCTION build_intake_batch(text),build_spend_submissions(text),build_workflow_repair(text,text,text,text) FROM PUBLIC;

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
