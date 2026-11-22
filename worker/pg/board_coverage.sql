-- The coverage writer stores dispositions for its source keys and uses the ordinary intake and compute ledgers.

CREATE OR REPLACE FUNCTION board_coverage_fingerprint(jsonb,jsonb,jsonb,jsonb,text,text) RETURNS text
LANGUAGE sql IMMUTABLE STRICT AS $$
  SELECT encode(sha256(convert_to(jsonb_build_array($1,$2,$3,$4,$5,$6)::text,'UTF8')),'hex')
$$;

CREATE TABLE IF NOT EXISTS record_board_coverage_receipts (
  source text COLLATE "C" PRIMARY KEY,
  fingerprint text NOT NULL CHECK (fingerprint ~ '^[0-9a-f]{64}$'),
  disposition jsonb NOT NULL CHECK (disposition->>'source' IS NOT DISTINCT FROM source)
);
DROP TRIGGER IF EXISTS board_coverage_receipt_retained ON record_board_coverage_receipts;
CREATE TRIGGER board_coverage_receipt_retained BEFORE UPDATE OR DELETE ON record_board_coverage_receipts
  FOR EACH ROW EXECUTE FUNCTION build_intake_immutable();

CREATE OR REPLACE FUNCTION board_coverage_lease(work_kind text, work_key text, width bigint, admitted_at text)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE
  token text := gen_random_uuid()::text;
  reply jsonb;
BEGIN
  UPDATE state_build_admission_work w
    SET lease=token, lease_until=to_char((admitted_at::timestamptz+interval '1 minute') AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
    WHERE (w.kind,w.key)=(work_kind,work_key) AND w.lease IS NULL AND w.lane='bulk' AND w.zone=3 AND NOT w.family
      AND EXISTS(SELECT 1 FROM state_build_effective_admission_work e WHERE (e.kind,e.key)=(w.kind,w.key));
  IF NOT FOUND THEN RAISE EXCEPTION 'no exact unheld coverage work'; END IF;
  reply:=build_admission_run(work_kind,work_key,token,width,admitted_at);
  IF (reply->>'processed')::bigint IS DISTINCT FROM width OR (reply->>'pending')::boolean IS DISTINCT FROM false
     OR reply->>'cancelled'='true' THEN
    RAISE EXCEPTION 'coverage work did not finish: %',reply;
  END IF;
END $$;

CREATE OR REPLACE FUNCTION board_coverage_admit_group(batch jsonb, epoch text, engine text)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE
  plan jsonb := batch->'plan';
  targets jsonb := batch->'targets';
  captured jsonb := plan->'flow';
  namespace text := batch->>'namespace';
  build text := plan->>'id';
  weapon text := plan#>>'{record,weapon}';
  target_mode text;
  width bigint;
  candidate jsonb;
  receipt_source text;
  target_ruler text;
  target_computation text;
  request jsonb;
  node jsonb;
  board jsonb;
  patch jsonb;
  fingerprint text;
  receipt jsonb;
  outcome text;
  anchor record;
  selected_review record;
  source_plan jsonb;
  intake_sources jsonb;
  parent_source text;
  prior record;
  observed record;
  canonical boolean;
  eligible jsonb := '[]';
  item_no integer := 0;
  intent_key text;
  flow_id text;
  expansion_id text;
  owns_expansion boolean;
  counted bigint[] := '{}';
  started text := to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
BEGIN
  IF jsonb_typeof(targets) IS DISTINCT FROM 'array' OR jsonb_array_length(targets) NOT BETWEEN 1 AND 32
    OR coalesce(batch->>'batch','') !~ '^[0-9a-f]{64}$' OR coalesce(batch->>'manifest','') !~ '^[0-9a-f]{64}$'
    OR namespace IS DISTINCT FROM 'coverage-'||left(batch->>'manifest',16)
    OR coalesce(batch->>'native','') !~ '^[0-9a-f]{64}$' OR engine !~ '^[0-9a-f]{12,64}$' OR batch->>'engine' IS DISTINCT FROM engine
    OR coalesce(build,'') !~ '^[0-9a-f]{32}$' OR weapon IS NULL
    OR plan-ARRAY['id','record','at','from','initiator','derived','deadlines','flow'] <> '{}'::jsonb
    OR plan->>'initiator' IS DISTINCT FROM 'system' OR plan->>'derived' IS DISTINCT FROM 'false'
    OR captured->>'schema' IS DISTINCT FROM '1'
    OR (captured->'scope')-ARRAY['boards','modes'] IS DISTINCT FROM '{}'::jsonb
    OR captured->'builds' IS DISTINCT FROM jsonb_build_array(jsonb_build_object('id',build,'record',plan->'record'))
  THEN RAISE EXCEPTION 'invalid grouped coverage proposal'; END IF;
  width:=jsonb_array_length(targets);
  target_mode:=targets#>>'{0,mode}';
  IF target_mode IS NULL
    OR captured#>'{scope,modes}' IS DISTINCT FROM jsonb_build_object(weapon,jsonb_build_array(target_mode))
    OR jsonb_array_length(captured->'nodes') IS DISTINCT FROM width::integer
    OR jsonb_array_length(captured#>'{scope,boards}') IS DISTINCT FROM width::integer
    OR plan->'from' IS DISTINCT FROM (SELECT jsonb_agg(t->>'source' ORDER BY (t->>'ordinal')::bigint) FROM jsonb_array_elements(targets) t)
    OR (SELECT count(DISTINCT t->>'source') FROM jsonb_array_elements(targets) t) <> width
    OR (SELECT count(DISTINCT (t->>'ordinal')::bigint) FROM jsonb_array_elements(targets) t) <> width
  THEN RAISE EXCEPTION 'grouped coverage scope does not match its targets'; END IF;
  canonical:=EXISTS(SELECT 1 FROM entity_weapon_builds b WHERE b.id=build AND b.record::jsonb=plan->'record' FOR SHARE);
  FOR candidate IN SELECT t FROM jsonb_array_elements(targets) t ORDER BY (t->>'ordinal')::bigint
  LOOP
    receipt_source:=candidate->>'source';
    target_ruler:=candidate->>'ruler';
    target_computation:=candidate->>'computation';
    node:=captured->'nodes'->item_no;
    board:=captured->'scope'->'boards'->item_no;
    item_no:=item_no+1;
    IF receipt_source IS DISTINCT FROM concat_ws('/',namespace,build,target_ruler,target_mode)
      OR candidate->>'identity' IS DISTINCT FROM build OR candidate->>'mode' IS DISTINCT FROM target_mode
      OR candidate->>'weapon' IS DISTINCT FROM weapon OR candidate->>'engine' IS DISTINCT FROM engine
      OR target_ruler IS NULL OR coalesce(target_computation,'') !~ '^[0-9a-f]{32}$' OR (candidate->>'ordinal')::bigint<1
      OR jsonb_array_length(candidate->'witness') IS DISTINCT FROM 2
      OR candidate#>>'{witness,0}' IS NULL OR candidate#>>'{witness,1}' IS NULL
      OR board->>'id' IS DISTINCT FROM target_ruler OR jsonb_typeof(board->'scenario') IS DISTINCT FROM 'object'
      OR node->>'kind' IS DISTINCT FROM 'measure' OR node->>'identity' IS DISTINCT FROM build
      OR node->>'ruler' IS DISTINCT FROM target_ruler OR node->>'mode' IS DISTINCT FROM target_mode
      OR node->>'computation' IS DISTINCT FROM target_computation OR node->>'review' IS NOT NULL
      OR coalesce(node->>'template','') !~ '^[0-9]+$' OR coalesce(node->>'patch','') !~ '^[0-9]+$'
    THEN RAISE EXCEPTION 'grouped node does not match native target'; END IF;
    patch:=captured->'patches'->(node->>'patch')::integer;
    request:=((captured->'templates'->(node->>'template')::integer)||(patch->'set'))-
         ARRAY(SELECT jsonb_array_elements_text(patch->'remove'));
    IF jsonb_typeof(request) IS DISTINCT FROM 'object' OR jsonb_typeof(patch->'remove') IS DISTINCT FROM 'array'
    THEN RAISE EXCEPTION 'grouped node has no frozen request'; END IF;
    fingerprint:=board_coverage_fingerprint(candidate,board,request,plan->'record',batch->>'native',epoch);
    SELECT r.fingerprint,r.disposition INTO prior FROM record_board_coverage_receipts r WHERE r.source=receipt_source;
    IF FOUND THEN
      IF prior.fingerprint IS DISTINCT FROM fingerprint THEN RAISE EXCEPTION 'coverage source changed its frozen input'; END IF;
      CONTINUE;
    END IF;
    IF EXISTS(SELECT 1 FROM relation_build_intake_sources a WHERE a.source_id=receipt_source) THEN
      RAISE EXCEPTION 'coverage source already belongs to the previous writer' USING ERRCODE='40001';
    END IF;

    outcome:=NULL;
    IF NOT canonical THEN outcome:='rejected_identity';
    ELSIF EXISTS(SELECT 1 FROM board_facts f WHERE (f.identity,f.ruler,f.mode)=(build,target_ruler,target_mode)
          AND f.retired_at IS NOT NULL) THEN outcome:='rejected_retired';
    ELSIF EXISTS(SELECT 1 FROM state_board_measurement_selection s WHERE (s.identity,s.ruler,s.mode)=(build,target_ruler,target_mode))
      THEN outcome:='already_measured';
    ELSE
      SELECT s.review_id INTO anchor FROM state_board_measurement_selection s
       WHERE (s.identity,s.ruler,s.mode)=(build,candidate#>>'{witness,0}',candidate#>>'{witness,1}')
       AND s.score>=0 AND s.score<'Infinity'::float8 FOR SHARE;
      IF NOT FOUND THEN
        outcome:='baseline_changed';
      ELSIF anchor.review_id IS NOT NULL THEN
        SELECT r.invalidated_at,r.state INTO STRICT selected_review
          FROM entity_measurement_reviews r WHERE r.id=anchor.review_id FOR SHARE;
        IF selected_review.invalidated_at IS NOT NULL AND selected_review.state IS DISTINCT FROM 'succeeded' THEN
          outcome:='baseline_changed';
        END IF;
      END IF;
      IF outcome IS NULL AND (
          EXISTS(SELECT 1 FROM state_board_pending_work w WHERE (w.build_id,w.ruler,w.mode)=(build,target_ruler,target_mode))
          OR EXISTS(SELECT 1 FROM relation_task_consumers c WHERE
              c.producer IN('sweep','refresh','new_build','survey_build','board_review')
              AND c.ref=concat_ws('|',build,target_ruler,target_mode)))
      THEN outcome:='skipped_prior_work';
      END IF;
    END IF;
    IF outcome IS NULL THEN
      SELECT s.task_id,s.state,s.result_id,s.request::jsonb request,s.verb INTO prior
        FROM state_compute_work s WHERE s.id=target_computation AND s.state IN('open','spot','fact');
      IF FOUND THEN
        IF prior.verb IS DISTINCT FROM 'simulate' OR prior.request IS DISTINCT FROM request
          OR (prior.state IN('spot','fact') AND prior.result_id IS NULL)
        THEN RAISE EXCEPTION 'shared coverage task does not carry the exact frozen request'; END IF;
      ELSIF EXISTS(SELECT 1 FROM entity_compute_specs s WHERE s.id=target_computation)
         OR EXISTS(SELECT 1 FROM state_build_workflow_nodes n WHERE n.computation=target_computation)
      THEN outcome:='skipped_prior_work';
      END IF;
    END IF;
    IF outcome IS NULL THEN
      eligible:=eligible||jsonb_build_array(jsonb_build_object(
        'target',candidate,'board',board,'node',node,'fingerprint',fingerprint,'prior_task',prior.task_id));
    ELSE
      receipt:=jsonb_build_object('source',receipt_source,'computation',target_computation,'outcome',outcome,'batch',batch->>'batch');
      INSERT INTO record_board_coverage_receipts VALUES(receipt_source,fingerprint,receipt);
    END IF;
  END LOOP;

  IF jsonb_array_length(eligible)>0 THEN
    source_plan:=plan||jsonb_build_object(
      'from',(SELECT jsonb_agg(e#>>'{target,source}' ORDER BY ord) FROM jsonb_array_elements(eligible) WITH ORDINALITY x(e,ord)),
      'flow',captured||jsonb_build_object(
        'nodes',(SELECT jsonb_agg(e->'node' ORDER BY ord) FROM jsonb_array_elements(eligible) WITH ORDINALITY x(e,ord)),
        'scope',(captured->'scope')||jsonb_build_object(
          'boards',(SELECT jsonb_agg(e->'board' ORDER BY ord) FROM jsonb_array_elements(eligible) WITH ORDINALITY x(e,ord)))));
    intake_sources:=source_plan->'from';
    IF jsonb_array_length(intake_sources)>1 THEN
      parent_source:=namespace||'/group/'||encode(sha256(convert_to(intake_sources::text,'UTF8')),'hex');
      source_plan:=source_plan||jsonb_build_object('from',jsonb_build_array(parent_source));
    END IF;
    intent_key:=build_intake_key(source_plan);
    PERFORM build_intake_batch(jsonb_build_array(source_plan)::text);
    IF parent_source IS NOT NULL THEN
      INSERT INTO relation_build_intake_sources(source_id,intent)
        SELECT value,intent_key FROM jsonb_array_elements_text(intake_sources) ON CONFLICT DO NOTHING;
    END IF;
    PERFORM board_coverage_lease('intake',intent_key,1,started);
    SELECT t.workflow, f.source=t.source INTO STRICT flow_id,owns_expansion FROM record_build_workflow_triggers t
      JOIN entity_build_workflows f ON f.id=t.workflow
      WHERE t.id=intent_key AND f.kind='intake' AND f.build_id=build AND t.source->>'origin'='system';
    expansion_id:=flow_id||':root';
    IF owns_expansion THEN
      PERFORM board_coverage_lease('expansion',expansion_id,jsonb_array_length(eligible),started);
    ELSIF EXISTS(SELECT 1 FROM state_build_workflow_nodes n
      WHERE n.workflow=flow_id AND n.expansion=expansion_id AND n.state='pending') THEN
      RAISE EXCEPTION 'existing workflow admission still owns this scope' USING ERRCODE='40001';
    END IF;
    item_no:=0;
    FOR candidate IN SELECT e FROM jsonb_array_elements(eligible) e
    LOOP
      receipt_source:=candidate#>>'{target,source}';
      target_computation:=candidate#>>'{target,computation}';
      SELECT n.state,n.task_id,t.generation,t.xmin AS created_in,q.state AS task_state,q.zone INTO STRICT observed
        FROM state_build_workflow_nodes n LEFT JOIN entity_compute_tasks t ON t.id=n.task_id
        LEFT JOIN state_compute_work q ON (q.id,q.task_id)=(n.computation,n.task_id)
        WHERE (n.workflow,n.id)=(flow_id,item_no::text) AND n.definition=candidate->'node';
      IF observed.state='skipped' AND observed.task_id IS NULL AND EXISTS(
        SELECT 1 FROM state_board_measurement_selection s WHERE
          (s.identity,s.ruler,s.mode)=(build,candidate#>>'{target,ruler}',target_mode))
      THEN outcome:='already_measured';
      ELSE
        IF observed.state IS DISTINCT FROM 'queued' OR observed.task_id IS NULL OR observed.task_state IS NULL
          OR NOT EXISTS(SELECT 1 FROM relation_task_consumers c WHERE
              (c.producer,c.ref,c.question,c.state)=('build_workflow',flow_id||'|'||item_no,target_computation,'live'))
          OR NOT EXISTS(SELECT 1 FROM relation_compute_task_sources p WHERE
              (p.task_id,p.producer,p.ref,p.trigger_id)=(observed.task_id,'build_workflow',flow_id||'|'||item_no,intent_key))
          OR NOT EXISTS(SELECT 1 FROM relation_build_intake_sources a WHERE (a.source_id,a.intent)=(receipt_source,intent_key))
        THEN RAISE EXCEPTION 'grouped coverage task lacks its original source, node or consumer'; END IF;
        IF candidate->>'prior_task' IS NOT NULL THEN
          IF observed.task_id IS DISTINCT FROM (candidate->>'prior_task')::bigint
          THEN RAISE EXCEPTION 'grouped coverage reopened existing work'; END IF;
          outcome:='joined_shared_task';
        ELSIF observed.generation IS DISTINCT FROM 1 OR observed.created_in IS DISTINCT FROM pg_current_xact_id()::xid
          OR observed.zone IS DISTINCT FROM 3 OR observed.task_state IS DISTINCT FROM 'open'
        THEN RAISE EXCEPTION 'new grouped coverage task was not inserted here';
        ELSIF observed.task_id=ANY(counted) THEN outcome:='joined_shared_task';
        ELSE
          counted:=array_append(counted,observed.task_id);
          outcome:='enqueued';
        END IF;
      END IF;
      receipt:=jsonb_build_object('source',receipt_source,'computation',target_computation,'outcome',outcome,
        'batch',batch->>'batch','workflow',flow_id,'node',item_no::text,'task_id',observed.task_id);
      INSERT INTO record_board_coverage_receipts VALUES(receipt_source,candidate->>'fingerprint',receipt);
      item_no:=item_no+1;
    END LOOP;
  END IF;

END $$;

REVOKE ALL ON FUNCTION board_coverage_admit_group(jsonb,text,text), board_coverage_lease(text,text,bigint,text) FROM PUBLIC;

CREATE OR REPLACE FUNCTION board_coverage_targets(jsonb) RETURNS SETOF jsonb
LANGUAGE sql IMMUTABLE AS $$
  SELECT t.target
  FROM jsonb_array_elements($1->'groups') WITH ORDINALITY g(group_plan,group_order)
  CROSS JOIN LATERAL jsonb_array_elements(g.group_plan->'targets') WITH ORDINALITY t(target,target_order)
  ORDER BY g.group_order,t.target_order
$$;

CREATE OR REPLACE FUNCTION board_coverage_admit(batch jsonb, epoch text, engine text) RETURNS void
LANGUAGE plpgsql AS $$
DECLARE
  group_plan jsonb;
  build_id text;
  sources bigint;
  first_ordinal bigint;
  last_ordinal bigint;
  distinct_ordinals bigint;
  distinct_sources bigint;
BEGIN
  IF nullif(current_setting('wfsim_coverage.batch',true),'') IS NOT NULL THEN
    RAISE EXCEPTION 'only one coverage batch per transaction';
  END IF;
  PERFORM set_config('wfsim_coverage.batch',coalesce(batch->>'batch','invalid'),true);
  IF jsonb_typeof(batch->'groups') IS DISTINCT FROM 'array'
    OR jsonb_array_length(batch->'groups') NOT BETWEEN 1 AND 32
    OR coalesce(batch->>'batch','') !~ '^[0-9a-f]{64}$'
    OR coalesce(batch->>'manifest','') !~ '^[0-9a-f]{64}$'
    OR batch->>'namespace' IS DISTINCT FROM 'coverage-'||left(batch->>'manifest',16)
    OR coalesce(batch->>'native','') !~ '^[0-9a-f]{64}$'
    OR engine !~ '^[0-9a-f]{12,64}$' OR batch->>'engine' IS DISTINCT FROM engine
  THEN RAISE EXCEPTION 'invalid board coverage batch'; END IF;
  SELECT count(*),min((target->>'ordinal')::bigint),max((target->>'ordinal')::bigint),
    count(DISTINCT target->>'ordinal'),count(DISTINCT target->>'source')
  INTO sources,first_ordinal,last_ordinal,distinct_ordinals,distinct_sources
  FROM board_coverage_targets(batch) target;
  IF sources NOT BETWEEN 1 AND 32 OR distinct_ordinals<>sources OR distinct_sources<>sources
    OR first_ordinal<1 OR last_ordinal-first_ordinal<>sources-1
    OR first_ordinal IS DISTINCT FROM (batch->>'first')::bigint
    OR last_ordinal IS DISTINCT FROM (batch->>'last')::bigint
  THEN RAISE EXCEPTION 'coverage batch exceeds source limit or cursor order'; END IF;
  IF (SELECT array_agg((t.target->>'ordinal')::bigint ORDER BY g.group_order,t.target_order)
    FROM jsonb_array_elements(batch->'groups') WITH ORDINALITY g(group_plan,group_order)
    CROSS JOIN LATERAL jsonb_array_elements(g.group_plan->'targets') WITH ORDINALITY t(target,target_order))
    IS DISTINCT FROM ARRAY(SELECT generate_series(first_ordinal,last_ordinal))
  THEN RAISE EXCEPTION 'coverage batch is out of cursor order'; END IF;

  PERFORM compute_order_cut_lock();
  IF (SELECT paused FROM state_task_pipeline_controls WHERE name='admission' FOR SHARE) IS DISTINCT FROM true
    OR (SELECT value FROM state_factory_parameters WHERE name='board_admission' FOR SHARE) IS DISTINCT FROM 0
    OR coalesce(compute_order_cut_epoch(),'') IS DISTINCT FROM epoch
  THEN RAISE EXCEPTION 'coverage controls or cut changed'; END IF;
  FOR build_id IN SELECT DISTINCT target->>'identity' FROM board_coverage_targets(batch) target ORDER BY 1 LOOP
    PERFORM pg_advisory_xact_lock(hashtextextended('build-workflow:'||build_id,0));
  END LOOP;
  PERFORM 1 FROM entity_compute_specs s
    WHERE s.id IN(SELECT target->>'computation' FROM board_coverage_targets(batch) target) ORDER BY s.id FOR UPDATE;
  PERFORM 1 FROM state_compute_scheduling s
    WHERE s.id IN(SELECT target->>'computation' FROM board_coverage_targets(batch) target) ORDER BY s.id FOR UPDATE;
  FOR group_plan IN SELECT value FROM jsonb_array_elements(batch->'groups') LOOP
    IF group_plan-ARRAY['targets','plan'] IS DISTINCT FROM '{}'::jsonb THEN
      RAISE EXCEPTION 'coverage group overrides its batch identity';
    END IF;
    PERFORM board_coverage_admit_group((batch-'groups')||group_plan,epoch,engine);
  END LOOP;
END $$;

CREATE OR REPLACE FUNCTION board_coverage_readback(batch jsonb, epoch text, engine text) RETURNS jsonb
LANGUAGE sql STABLE AS $$
  WITH cells AS MATERIALIZED (
    SELECT target, g.group_plan->'plan' plan, g.group_plan#>'{plan,flow}' flow, (target_order-1)::integer idx
    FROM jsonb_array_elements(batch->'groups') g(group_plan)
    CROSS JOIN LATERAL jsonb_array_elements(g.group_plan->'targets') WITH ORDINALITY t(target,target_order)
  ), requests AS MATERIALIZED (
    SELECT target, plan->'record' AS build, flow#>'{scope,boards}'->idx AS board, flow->'nodes'->idx AS node, flow
    FROM cells
  ), checked AS MATERIALIZED (
    SELECT t.target, r.disposition, r.source
    FROM requests t
    CROSS JOIN LATERAL (SELECT t.flow->'patches'->(t.node->>'patch')::integer value) p
    CROSS JOIN LATERAL (SELECT ((t.flow->'templates'->(t.node->>'template')::integer)||(p.value->'set'))-
        ARRAY(SELECT jsonb_array_elements_text(p.value->'remove')) request) frozen
    JOIN record_board_coverage_receipts r ON r.source=t.target->>'source'
      AND r.fingerprint=board_coverage_fingerprint(t.target,t.board,frozen.request,t.build,batch->>'native',epoch)
    WHERE r.disposition->>'computation'=t.target->>'computation'
      AND NOT EXISTS(SELECT 1 FROM record_build_submissions raw WHERE raw.id=r.source)
      AND ((r.disposition->>'outcome' IN('already_measured','rejected_identity','rejected_retired','skipped_prior_work','baseline_changed')
           AND r.disposition->>'task_id' IS NULL)
         OR (r.disposition->>'outcome' IN('enqueued','joined_shared_task') AND EXISTS(
           SELECT 1 FROM relation_build_intake_sources a
           JOIN record_build_workflow_triggers tr ON tr.id=a.intent
           JOIN state_build_workflow_nodes n ON n.workflow=tr.workflow
           JOIN relation_compute_task_sources linked ON linked.trigger_id=tr.id
              AND linked.producer='build_workflow' AND linked.ref=n.workflow||'|'||n.id
           JOIN entity_compute_tasks task ON task.id=linked.task_id
           JOIN entity_compute_specs spec ON spec.id=task.computation AND spec.verb='simulate'
           WHERE a.source_id=r.source
             AND n.workflow=r.disposition->>'workflow' AND n.id=r.disposition->>'node'
             AND task.id=(r.disposition->>'task_id')::bigint AND task.computation=t.target->>'computation'
             AND n.definition-ARRAY['template','patch']=t.node-ARRAY['template','patch']
             AND spec.request::jsonb=frozen.request)))
  )
  SELECT CASE WHEN count(*)=(SELECT count(*) FROM cells) AND count(*)=(batch->>'last')::bigint-(batch->>'first')::bigint+1
    AND count(*) BETWEEN 1 AND 32 AND batch->>'engine'=engine AND coalesce(compute_order_cut_epoch(),'')=epoch
    THEN jsonb_build_object('batch',batch->>'batch','engine',engine,'epoch',epoch,
                            'rows',jsonb_agg(disposition ORDER BY (target->>'ordinal')::bigint))
    END
  FROM checked
$$;

REVOKE ALL ON FUNCTION board_coverage_fingerprint(jsonb,jsonb,jsonb,jsonb,text,text),
  board_coverage_targets(jsonb),board_coverage_admit(jsonb,text,text),board_coverage_readback(jsonb,text,text) FROM PUBLIC;
