-- Explicit board requests reuse canonical intake and the common computation queue.
CREATE TABLE entity_board_requests (
  id TEXT COLLATE "C" PRIMARY KEY,
  kind TEXT NOT NULL CHECK(kind IN('missing','correction')),
  zone BIGINT GENERATED ALWAYS AS(CASE kind WHEN 'missing' THEN 0 ELSE 1 END) STORED,
  input JSONB NOT NULL CHECK(jsonb_typeof(input)='object'),
  ruler TEXT COLLATE "C" NOT NULL,
  mode TEXT COLLATE "C" NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE state_board_request_control (
  id INTEGER PRIMARY KEY CHECK(id=1),
  source UUID NOT NULL DEFAULT gen_random_uuid(),
  enabled BOOLEAN NOT NULL DEFAULT false
);
INSERT INTO state_board_request_control(id) VALUES(1);
CREATE TABLE state_board_requests (
  id TEXT COLLATE "C" PRIMARY KEY REFERENCES entity_board_requests(id),
  position BIGINT NOT NULL DEFAULT nextval('compute_queue_position'),
  phase TEXT NOT NULL DEFAULT 'pending' CHECK(phase IN('pending','staged','admitted','rejected','cancelled')),
  intent TEXT COLLATE "C",
  workflow TEXT COLLATE "C",
  lease UUID,
  lease_until TIMESTAMPTZ,
  retry_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  attempts BIGINT NOT NULL DEFAULT 0,
  error TEXT
);
CREATE INDEX board_requests_ready ON state_board_requests(position,id) WHERE phase IN('pending','staged');
CREATE TRIGGER board_request_retained BEFORE UPDATE OR DELETE ON entity_board_requests
  FOR EACH ROW EXECUTE FUNCTION build_intake_immutable();

CREATE FUNCTION board_request_submit(text,text,text,text) RETURNS text
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE record jsonb:=$3::jsonb; raw jsonb;
BEGIN
  IF NOT EXISTS(SELECT 1 FROM state_board_request_control WHERE id=1 AND enabled) THEN
    RAISE EXCEPTION 'requested board intake is paused' USING ERRCODE='55000'; END IF;
  IF jsonb_typeof(record) IS DISTINCT FROM 'object' OR octet_length($3)>4096
    OR coalesce(record->>'benchmark','')!~'^[a-z0-9_]{1,128}$'
    OR coalesce(record->>'mode','base')!~'^[a-z0-9_]{1,128}$' THEN
    RAISE EXCEPTION 'requested board input requires a ruler and mode'; END IF;
  raw:=jsonb_build_object('id',$1,'at',$2,'record',record,'initiator',$4);
  INSERT INTO record_build_submissions(id,at,record,initiator) VALUES($1,$2,$3,$4);
  INSERT INTO entity_board_requests(id,kind,input,ruler,mode)
    VALUES($1,'missing',raw,record->>'benchmark',coalesce(record->>'mode','base'));
  INSERT INTO state_board_requests(id) VALUES($1);
  RETURN $1;
END $$;

CREATE FUNCTION board_request_capture_review() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE observation jsonb; review entity_measurement_reviews%ROWTYPE;
BEGIN
  IF NEW.observation IS NULL OR NEW.family_source IS NOT NULL THEN RETURN NEW; END IF;
  observation:=NEW.observation::jsonb;
  SELECT * INTO review FROM entity_measurement_reviews WHERE id=observation->>'review' AND state='waiting';
  IF NOT FOUND OR NEW.id<>'review:'||review.id THEN RETURN NEW; END IF;
  INSERT INTO entity_board_requests(id,kind,input,ruler,mode)
    VALUES(NEW.id,'correction',to_jsonb(NEW),review.ruler,review.mode);
  INSERT INTO state_board_requests(id) VALUES(NEW.id);
  RETURN NEW;
END $$;
CREATE TRIGGER board_requested_review AFTER INSERT ON record_build_submissions
  FOR EACH ROW EXECUTE FUNCTION board_request_capture_review();

CREATE FUNCTION board_request_claim(uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held state_board_requests%ROWTYPE;
BEGIN
  IF NOT EXISTS(SELECT 1 FROM state_board_request_control WHERE id=1 AND source=$1 AND enabled) THEN RETURN NULL; END IF;
  SELECT * INTO held FROM state_board_requests WHERE phase IN('pending','staged') AND retry_at<=clock_timestamp()
    AND (lease IS NULL OR lease_until<=clock_timestamp()) ORDER BY position,id LIMIT 1 FOR UPDATE SKIP LOCKED;
  IF NOT FOUND THEN RETURN NULL; END IF;
  UPDATE state_board_requests SET lease=gen_random_uuid(),lease_until=clock_timestamp()+interval '2minutes',attempts=attempts+1
    WHERE id=held.id RETURNING * INTO held;
  RETURN to_jsonb(held)||(SELECT to_jsonb(r) FROM entity_board_requests r WHERE r.id=held.id);
END $$;

CREATE FUNCTION board_request_stage(text,uuid,text) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE request entity_board_requests%ROWTYPE; input jsonb:=$3::jsonb; plan jsonb:=input->'flow';
BEGIN
  PERFORM 1 FROM state_board_request_control WHERE id=1 AND enabled FOR SHARE;
  IF NOT FOUND THEN RAISE EXCEPTION 'requested board intake is paused' USING ERRCODE='55000'; END IF;
  PERFORM 1 FROM state_board_requests WHERE id=$1 AND phase='pending' AND lease=$2 AND lease_until>clock_timestamp() FOR UPDATE;
  IF NOT FOUND THEN RAISE EXCEPTION 'requested input lease changed' USING ERRCODE='40001'; END IF;
  SELECT * INTO STRICT request FROM entity_board_requests WHERE id=$1;
  IF input->'from' IS DISTINCT FROM jsonb_build_array($1) OR input->>'derived' IS DISTINCT FROM 'false'
    OR input->>'system_source' IS NOT NULL OR input->>'recovery_of' IS NOT NULL
    OR jsonb_array_length(plan->'nodes') IS DISTINCT FROM 1
    OR plan#>>'{nodes,0,kind}' IS DISTINCT FROM 'measure'
    OR plan#>>'{nodes,0,identity}' IS DISTINCT FROM input->>'id'
    OR plan#>>'{nodes,0,ruler}' IS DISTINCT FROM request.ruler
    OR plan#>>'{nodes,0,mode}' IS DISTINCT FROM request.mode
    OR jsonb_array_length(plan#>'{scope,boards}') IS DISTINCT FROM 1
    OR plan#>>'{scope,boards,0,id}' IS DISTINCT FROM request.ruler
    OR plan#>'{scope,modes}' IS DISTINCT FROM jsonb_build_object(input#>>'{record,weapon}',jsonb_build_array(request.mode))
    OR (request.kind='correction' AND (input#>>'{observation,valid}' IS DISTINCT FROM 'true'
      OR input#>>'{observation,review}' IS DISTINCT FROM (request.input->>'observation')::jsonb->>'review'))
    OR (request.kind='missing' AND input->'observation' IS DISTINCT FROM 'null'::jsonb) THEN
    RAISE EXCEPTION 'requested input changed its captured scope'; END IF;
  input:=input||jsonb_build_object('zone',request.zone);
  PERFORM build_intake_batch(jsonb_build_array(input)::text);
  UPDATE state_board_requests SET phase='staged',intent=build_intake_key(input),lease=NULL,lease_until=NULL,error=NULL WHERE id=$1;
  PERFORM build_spend_submissions(jsonb_build_array($1)::text);
END $$;

CREATE FUNCTION board_request_admit(text,uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held state_board_requests%ROWTYPE; at text; work record; flow text; pending boolean; requested_zone bigint;
BEGIN
  PERFORM 1 FROM state_board_request_control WHERE id=1 AND enabled FOR SHARE;
  IF NOT FOUND THEN RAISE EXCEPTION 'requested board intake is paused' USING ERRCODE='55000'; END IF;
  SELECT * INTO held FROM state_board_requests WHERE id=$1 AND phase='staged' AND lease=$2 AND lease_until>clock_timestamp() FOR UPDATE;
  IF NOT FOUND THEN RAISE EXCEPTION 'requested admission lease changed' USING ERRCODE='40001'; END IF;
  at:=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
  IF EXISTS(SELECT 1 FROM state_build_admission_work WHERE kind='intake' AND key=held.intent) THEN
    IF NOT EXISTS(SELECT 1 FROM state_build_effective_admission_work WHERE kind='intake' AND key=held.intent) THEN
      UPDATE state_board_requests SET phase='cancelled',lease=NULL,lease_until=NULL WHERE id=$1;
      RETURN jsonb_build_object('cancelled',true); END IF;
    UPDATE state_build_admission_work SET lease=$2::text,lease_until=to_char((clock_timestamp()+interval '2minutes') AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
      WHERE kind='intake' AND key=held.intent AND (lease IS NULL OR lease_until<=at);
    IF NOT FOUND THEN RAISE EXCEPTION 'requested intake is already leased' USING ERRCODE='40001'; END IF;
    PERFORM build_admission_run('intake',held.intent,$2::text,1,at);
  END IF;
  SELECT workflow INTO flow FROM record_build_workflow_triggers WHERE id=held.intent;
  IF flow IS NOT NULL AND ((SELECT count(*) FROM(SELECT 1 FROM state_build_workflow_nodes WHERE workflow=flow LIMIT 2) bounded)<>1
    OR NOT EXISTS(SELECT 1 FROM state_build_workflow_nodes n JOIN entity_build_intake_intents i ON i.id=held.intent
      JOIN entity_build_intake_plans p ON p.id=i.plan_id WHERE n.workflow=flow AND n.definition->>'computation'=p.definition#>>'{nodes,0,computation}')) THEN
    RAISE EXCEPTION 'requested workflow includes unrelated computations';END IF;
  SELECT zone INTO STRICT requested_zone FROM entity_board_requests WHERE id=$1;
  UPDATE state_build_workflows SET zone=least(zone,requested_zone) WHERE id=flow;
  UPDATE relation_task_consumers SET zone=least(coalesce(zone,3),requested_zone)
    WHERE producer='build_workflow' AND ref>=flow AND ref<flow||'}' AND split_part(ref,'|',1)=flow AND state='live';
  PERFORM factory_restate(n.computation) FROM state_build_workflow_nodes n WHERE n.workflow=flow AND n.state='queued' ORDER BY n.computation;
  FOR work IN SELECT w.kind,w.key FROM entity_build_workflow_expansions e JOIN state_build_effective_admission_work w
    ON w.kind='expansion' AND w.key=e.id WHERE e.workflow=flow ORDER BY w.zone,w.position,w.key LIMIT 2 LOOP
    UPDATE state_build_admission_work SET lease=$2::text,lease_until=to_char((clock_timestamp()+interval '2minutes') AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
      WHERE kind=work.kind AND key=work.key AND (lease IS NULL OR lease_until<=at);
    IF NOT FOUND THEN RAISE EXCEPTION 'requested expansion is already leased' USING ERRCODE='40001'; END IF;
    PERFORM build_admission_run(work.kind,work.key,$2::text,1,at);
  END LOOP;
  SELECT EXISTS(SELECT 1 FROM entity_build_workflow_expansions e JOIN state_build_effective_admission_work w
    ON w.kind='expansion' AND w.key=e.id WHERE e.workflow=flow) INTO pending;
  UPDATE state_board_requests SET phase=CASE WHEN pending THEN 'staged' ELSE 'admitted' END,
    workflow=flow,lease=NULL,lease_until=NULL,error=NULL WHERE id=$1;
  RETURN jsonb_build_object('workflow',flow,'pending',pending);
END $$;

CREATE FUNCTION board_request_retry(text,uuid,text,boolean) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  UPDATE state_board_requests SET phase=CASE WHEN $4 THEN 'rejected' ELSE phase END,
    lease=NULL,lease_until=NULL,error=left($3,1000),retry_at=clock_timestamp()+interval '30seconds'
    WHERE id=$1 AND lease=$2 AND lease_until>clock_timestamp();
  IF NOT FOUND THEN RETURN false; END IF;
  IF $4 THEN
    UPDATE entity_measurement_reviews r SET state='failed',completed_at=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
      FROM entity_board_requests request WHERE request.id=$1 AND request.kind='correction'
        AND r.id=(request.input->>'observation')::jsonb->>'review' AND r.state='waiting' AND r.task_id IS NULL AND r.workflow IS NULL;
  END IF;
  RETURN true;
END $$;
REVOKE ALL ON FUNCTION board_request_submit(text,text,text,text),board_request_capture_review(),board_request_claim(uuid),
  board_request_stage(text,uuid,text),board_request_admit(text,uuid),board_request_retry(text,uuid,text,boolean) FROM PUBLIC;
