-- Pending board rows own business obligations; these rows track unfinished registration only.
CREATE SEQUENCE IF NOT EXISTS board_admission_revision;
CREATE TABLE IF NOT EXISTS state_board_task_admissions (
  build_id TEXT COLLATE "C" NOT NULL,
  ruler TEXT COLLATE "C" NOT NULL,
  mode TEXT COLLATE "C" NOT NULL,
  revision BIGINT NOT NULL DEFAULT nextval('board_admission_revision'),
  retry_at TEXT COLLATE "C" NOT NULL,
  claim TEXT COLLATE "C",
  claim_until TEXT COLLATE "C",
  error TEXT COLLATE "C" CHECK(error IN('retry','missing_source','refresh_not_due')),
  PRIMARY KEY(build_id,ruler,mode)
);
CREATE INDEX IF NOT EXISTS board_task_admissions_ready ON state_board_task_admissions(retry_at,revision);
CREATE INDEX IF NOT EXISTS board_task_admissions_deferred ON state_board_task_admissions(retry_at,revision) WHERE error IS NOT NULL;
CREATE TABLE IF NOT EXISTS state_board_admission_recovery (
  id BIGINT PRIMARY KEY CHECK(id=1),
  build_id TEXT COLLATE "C" NOT NULL DEFAULT '',
  ruler TEXT COLLATE "C" NOT NULL DEFAULT '',
  mode TEXT COLLATE "C" NOT NULL DEFAULT '',
  scanned BIGINT NOT NULL DEFAULT 0,
  complete BOOLEAN NOT NULL DEFAULT false,
  updated_at TEXT COLLATE "C" NOT NULL
);

CREATE OR REPLACE FUNCTION board_admission_changed() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE at text:=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
BEGIN
  IF TG_OP='DELETE' THEN
    DELETE FROM state_board_task_admissions WHERE build_id=OLD.build_id AND ruler=OLD.ruler AND mode=OLD.mode;
    RETURN OLD;
  END IF;
  IF TG_OP='UPDATE' AND (OLD.batch,OLD.build_id,OLD.ruler,OLD.mode,OLD.initiator)
      IS NOT DISTINCT FROM (NEW.batch,NEW.build_id,NEW.ruler,NEW.mode,NEW.initiator) THEN RETURN NEW; END IF;
  IF TG_OP='UPDATE' AND (OLD.build_id,OLD.ruler,OLD.mode) IS DISTINCT FROM (NEW.build_id,NEW.ruler,NEW.mode) THEN
    DELETE FROM state_board_task_admissions WHERE build_id=OLD.build_id AND ruler=OLD.ruler AND mode=OLD.mode;
  END IF;
  INSERT INTO state_board_task_admissions(build_id,ruler,mode,retry_at) VALUES(NEW.build_id,NEW.ruler,NEW.mode,at)
    ON CONFLICT(build_id,ruler,mode) DO UPDATE SET revision=nextval('board_admission_revision'),retry_at=excluded.retry_at,
      claim=NULL,claim_until=NULL,error=NULL;
  RETURN NEW;
END $$;
DROP TRIGGER IF EXISTS board_admission_changed ON state_board_pending_work;
CREATE TRIGGER board_admission_changed AFTER INSERT OR UPDATE OR DELETE ON state_board_pending_work
  FOR EACH ROW EXECUTE FUNCTION board_admission_changed();

-- The historical pass advances independently of new writes, which the trigger already retains.
CREATE OR REPLACE FUNCTION board_admission_seed(bigint,text,bigint) RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE cursor state_board_admission_recovery%ROWTYPE; item record; n bigint:=0; occupied bigint; budget bigint;
BEGIN
  IF $1 IS NULL OR $1<1 OR $1>20000 OR $2 IS NULL OR $3 IS NULL OR $3<1 OR $3>1000000 THEN RAISE EXCEPTION 'invalid board recovery batch'; END IF;
  INSERT INTO state_board_admission_recovery(id,updated_at) VALUES(1,$2) ON CONFLICT DO NOTHING;
  SELECT * INTO STRICT cursor FROM state_board_admission_recovery WHERE id=1 FOR UPDATE;
  IF cursor.complete THEN RETURN to_jsonb(cursor); END IF;
  SELECT count(*) INTO occupied FROM (SELECT 1 FROM state_compute_effective_open LIMIT $3) active;
  budget:=least($1,greatest(0,$3-occupied));
  IF budget=0 THEN RETURN to_jsonb(cursor)||jsonb_build_object('window_full',true); END IF;
  FOR item IN SELECT q.build_id,q.ruler,q.mode FROM state_board_effective_pending_work q
      WHERE (q.build_id,q.ruler,q.mode)>(cursor.build_id,cursor.ruler,cursor.mode)
      ORDER BY q.build_id,q.ruler,q.mode LIMIT budget LOOP
    n:=n+1;cursor.build_id:=item.build_id;cursor.ruler:=item.ruler;cursor.mode:=item.mode;
    IF NOT EXISTS(SELECT 1 FROM relation_task_consumers d JOIN state_compute_scheduling q ON q.id=d.question
        WHERE d.ref=item.build_id||'|'||item.ruler||'|'||item.mode
          AND d.producer IN('new_build','survey_build','refresh','sweep') AND d.state='live' AND q.state NOT IN('fact','spot')) THEN
      INSERT INTO state_board_task_admissions(build_id,ruler,mode,retry_at) VALUES(item.build_id,item.ruler,item.mode,$2)
        ON CONFLICT DO NOTHING;
    END IF;
  END LOOP;
  UPDATE state_board_admission_recovery SET build_id=cursor.build_id,ruler=cursor.ruler,mode=cursor.mode,
    scanned=scanned+n,complete=n<budget,updated_at=$2 WHERE id=1 RETURNING * INTO cursor;
  RETURN to_jsonb(cursor);
END $$;

CREATE OR REPLACE FUNCTION board_admission_claim(bigint,text,text) RETURNS TABLE(
  build_id text,ruler text,mode text,revision bigint,claim text,batch text,initiator text,record text,built_at text,measured_at text)
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  IF $1 IS NULL OR $1<1 OR $1>250 OR $2 IS NULL OR $3 IS NULL OR $3<=$2 THEN RAISE EXCEPTION 'invalid board admission claim'; END IF;
  RETURN QUERY WITH candidates AS (
    SELECT a.build_id,a.ruler,a.mode FROM state_board_task_admissions a
      WHERE EXISTS(SELECT 1 FROM state_board_effective_pending_work p WHERE (p.build_id,p.ruler,p.mode)=(a.build_id,a.ruler,a.mode)) AND a.retry_at<=$2 AND (a.claim_until IS NULL OR a.claim_until<$2)
      ORDER BY a.retry_at,a.revision LIMIT $1 FOR UPDATE SKIP LOCKED
  ), claimed AS (
    UPDATE state_board_task_admissions a SET claim=gen_random_uuid()::text,claim_until=$3
      FROM candidates c WHERE (a.build_id,a.ruler,a.mode)=(c.build_id,c.ruler,c.mode)
      RETURNING a.*
  ) SELECT a.build_id,a.ruler,a.mode,a.revision,a.claim,q.batch,q.initiator,b.record,b.at,s.finished_at
    FROM claimed a LEFT JOIN state_board_effective_pending_work q ON (q.build_id,q.ruler,q.mode)=(a.build_id,a.ruler,a.mode)
    LEFT JOIN entity_compute_batches batch ON batch.id=q.batch
    LEFT JOIN entity_weapon_builds b ON b.id=q.build_id AND batch.id IS NOT NULL
    LEFT JOIN state_board_measurement_selection s ON (s.identity,s.ruler,s.mode)=(a.build_id,a.ruler,a.mode)
    ORDER BY a.retry_at,a.revision;
END $$;

-- Run after compute_admit_tasks in its transaction, preserving task -> business row -> registration lock order.
CREATE OR REPLACE FUNCTION board_admission_finish(text) RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE rows jsonb:=$1::jsonb; item record; held state_board_pending_work%ROWTYPE; admission state_board_task_admissions%ROWTYPE;
BEGIN
  IF jsonb_typeof(rows) IS DISTINCT FROM 'array' OR jsonb_array_length(rows)>250 THEN RAISE EXCEPTION 'invalid board admission receipt'; END IF;
  FOR item IN SELECT * FROM jsonb_to_recordset(rows) AS x(build_id text,ruler text,mode text,revision bigint,claim text,batch text,initiator text)
      ORDER BY build_id,ruler,mode LOOP
    SELECT * INTO held FROM state_board_pending_work q WHERE (q.build_id,q.ruler,q.mode)=(item.build_id,item.ruler,item.mode) FOR UPDATE;
    IF NOT FOUND OR (held.batch,held.initiator) IS DISTINCT FROM (item.batch,item.initiator) THEN
      RAISE EXCEPTION 'board source changed before admission' USING ERRCODE='40001'; END IF;
    SELECT * INTO admission FROM state_board_task_admissions a WHERE (a.build_id,a.ruler,a.mode)=(item.build_id,item.ruler,item.mode) FOR UPDATE;
    IF NOT FOUND OR (admission.revision,admission.claim) IS DISTINCT FROM (item.revision,item.claim) OR item.claim IS NULL THEN
      RAISE EXCEPTION 'board admission claim changed' USING ERRCODE='40001'; END IF;
    DELETE FROM state_board_task_admissions a WHERE (a.build_id,a.ruler,a.mode)=(item.build_id,item.ruler,item.mode);
  END LOOP;
END $$;

CREATE OR REPLACE FUNCTION board_admission_retry(text,text,text) RETURNS bigint LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE changed bigint;
BEGIN
  IF jsonb_typeof($1::jsonb) IS DISTINCT FROM 'array' OR jsonb_array_length($1::jsonb)>250 OR $2 IS NULL
      OR $3 NOT IN('retry','missing_source','refresh_not_due') THEN RAISE EXCEPTION 'invalid board admission retry'; END IF;
  UPDATE state_board_task_admissions a SET claim=NULL,claim_until=NULL,retry_at=coalesce(x.retry_at,$2),error=coalesce(x.error,$3)
    FROM jsonb_to_recordset($1::jsonb) AS x(build_id text,ruler text,mode text,revision bigint,claim text,retry_at text,error text)
    WHERE (a.build_id,a.ruler,a.mode,a.revision,a.claim)=(x.build_id,x.ruler,x.mode,x.revision,x.claim);
  GET DIAGNOSTICS changed=ROW_COUNT;RETURN changed;
END $$;

CREATE OR REPLACE FUNCTION board_admission_status(text) RETURNS jsonb LANGUAGE sql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
  SELECT jsonb_build_object('pending',EXISTS(SELECT 1 FROM state_board_task_admissions a WHERE retry_at<=$1 AND EXISTS(SELECT 1 FROM state_board_effective_pending_work p WHERE (p.build_id,p.ruler,p.mode)=(a.build_id,a.ruler,a.mode))),
    'recovery',coalesce((SELECT to_jsonb(r) FROM state_board_admission_recovery r WHERE id=1),jsonb_build_object('complete',false,'scanned',0)),
    'deferred',EXISTS(SELECT 1 FROM state_board_task_admissions WHERE error IS NOT NULL))
$$;

-- Reception and its claim check share the scheduling row lock and transaction.
CREATE OR REPLACE FUNCTION factory_assert_official_claim(text,bigint,text,text) RETURNS void LANGUAGE plpgsql AS $$
DECLARE held state_compute_scheduling%ROWTYPE;
BEGIN
  SELECT * INTO held FROM state_compute_scheduling WHERE id=$1 FOR UPDATE;
  IF NOT FOUND OR $2 IS NULL OR $3 IS NULL OR $4 IS NULL
      OR (held.task_id,held.lease,held.leased_to) IS DISTINCT FROM ($2,$3,$4) THEN
    RAISE EXCEPTION 'official claim changed before reception' USING ERRCODE='40001';
  END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS build_workflow_expansions_workflow_id ON entity_build_workflow_expansions(workflow,id);
DO $workflow_expansion_fk$ BEGIN
  IF NOT EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='state_build_workflow_nodes'::regclass
      AND conname='state_build_workflow_nodes_workflow_expansion_fkey') THEN
    ALTER TABLE state_build_workflow_nodes
      ADD CONSTRAINT state_build_workflow_nodes_workflow_expansion_fkey
      FOREIGN KEY(workflow,expansion) REFERENCES entity_build_workflow_expansions(workflow,id) NOT VALID;
    ALTER TABLE state_build_workflow_nodes
      VALIDATE CONSTRAINT state_build_workflow_nodes_workflow_expansion_fkey;
  END IF;
  ALTER TABLE state_build_workflow_nodes DROP CONSTRAINT IF EXISTS state_build_workflow_nodes_expansion_fkey;
END $workflow_expansion_fk$;
CREATE INDEX IF NOT EXISTS build_admission_expansion_id ON state_build_admission_work(expansion_id);
CREATE INDEX IF NOT EXISTS build_workflow_nodes_result_id ON state_build_workflow_nodes(result_id,workflow);
CREATE INDEX IF NOT EXISTS build_workflow_nodes_task_id ON state_build_workflow_nodes(task_id,workflow);
CREATE INDEX IF NOT EXISTS board_reviews_task_id ON entity_measurement_reviews(task_id);
CREATE INDEX IF NOT EXISTS answers_by_task_id ON record_candidate_answers(task_id);
CREATE INDEX IF NOT EXISTS order_cancellations_task_id ON record_compute_order_cancellations(task_id);
CREATE INDEX IF NOT EXISTS build_workflows_completed_at ON state_build_workflows(completed_at,id)
  WHERE state IN('succeeded','failed','cancelled');
CREATE INDEX IF NOT EXISTS calculation_reports_question ON entity_calculation_reports(question);
CREATE INDEX IF NOT EXISTS user_calculations_question ON entity_user_calculation_requests(question);
CREATE INDEX IF NOT EXISTS order_cut_protection_question ON relation_compute_order_cut_protection(question);

-- Settled workflow and task intermediates expire after the configured retention window;
-- canonical builds, published scores, witnesses, reports and credit totals remain intact.
CREATE OR REPLACE FUNCTION factory_prune_settled(text,bigint) RETURNS bigint
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE
  cap bigint:=least(greatest(coalesce($2,0),0),1000);
  wfs text[]:='{}';
  qs text[]:='{}';
BEGIN
  IF $1 IS NULL OR cap=0 THEN RETURN 0; END IF;
  PERFORM compute_order_cut_lock();
  SELECT coalesce(array_agg(t.id),'{}') INTO wfs FROM (
    SELECT f.id FROM state_build_workflows f JOIN entity_build_workflows w ON w.id=f.id
     WHERE f.state IN('succeeded','failed','cancelled') AND f.completed_at IS NOT NULL AND f.completed_at<$1
       AND coalesce(w.source->>'family','')<>'true'
       AND NOT EXISTS(SELECT 1 FROM state_build_workflow_nodes n WHERE n.workflow=f.id
         AND (n.state IN('pending','queued','awaiting_intake') OR n.definition->>'kind'='optimize'))
       AND NOT EXISTS(SELECT 1 FROM entity_measurement_reviews r WHERE r.workflow=f.id AND r.state='waiting')
       AND NOT EXISTS(SELECT 1 FROM state_build_admission_work a WHERE a.kind='expansion'
         AND a.key>=f.id||':' AND a.key<f.id||';' AND a.lease_until>=$1)
     ORDER BY f.completed_at,f.id LIMIT cap FOR UPDATE OF f SKIP LOCKED) t;
  IF cardinality(wfs)>0 THEN
    DELETE FROM state_build_admission_work WHERE kind='expansion'
      AND key IN(SELECT id FROM entity_build_workflow_expansions WHERE workflow=ANY(wfs));
    DELETE FROM state_build_workflow_nodes WHERE workflow=ANY(wfs);
    DELETE FROM entity_build_workflow_expansions WHERE workflow=ANY(wfs);
    DELETE FROM record_build_workflow_triggers WHERE workflow=ANY(wfs);
    DELETE FROM state_build_workflows WHERE id=ANY(wfs);
    DELETE FROM entity_build_workflows WHERE id=ANY(wfs);
  END IF;

  WITH candidates AS MATERIALIZED (
    (SELECT q.id FROM state_compute_scheduling q
      WHERE q.state IN('fact','spot') AND q.fact_at IS NOT NULL AND q.fact_at<$1 AND q.state='fact'
        AND factory_past_safe(q.id,$1)
        AND NOT EXISTS(SELECT 1 FROM entity_calculation_reports p WHERE p.question=q.id)
        AND NOT EXISTS(SELECT 1 FROM entity_user_calculation_requests u WHERE u.question=q.id AND (u.state='waiting' OR u.expires_at>=$1))
        AND NOT EXISTS(SELECT 1 FROM entity_measurement_reviews r WHERE r.question=q.id AND r.task_id IS NOT NULL)
        AND NOT EXISTS(SELECT 1 FROM relation_compute_order_cut_protection cp WHERE cp.question=q.id)
        AND NOT EXISTS(SELECT 1 FROM record_compute_order_cancellations oc WHERE oc.question=q.id)
      ORDER BY q.fact_at,q.id LIMIT cap)
    UNION ALL
    (SELECT q.id FROM state_compute_scheduling q
      WHERE q.state='withdrawn' AND q.opened_at<$1 AND coalesce(q.withdrawn_at,q.opened_at)<$1
        AND factory_past_safe(q.id,$1)
        AND NOT EXISTS(SELECT 1 FROM entity_calculation_reports p WHERE p.question=q.id)
        AND NOT EXISTS(SELECT 1 FROM entity_user_calculation_requests u WHERE u.question=q.id AND (u.state='waiting' OR u.expires_at>=$1))
        AND NOT EXISTS(SELECT 1 FROM entity_measurement_reviews r WHERE r.question=q.id AND r.task_id IS NOT NULL)
        AND NOT EXISTS(SELECT 1 FROM relation_compute_order_cut_protection cp WHERE cp.question=q.id)
        AND NOT EXISTS(SELECT 1 FROM record_compute_order_cancellations oc WHERE oc.question=q.id)
      ORDER BY q.opened_at,q.id LIMIT cap)
  ), locked AS (
    SELECT q.id FROM state_compute_scheduling q
     WHERE q.id IN(SELECT id FROM candidates LIMIT cap)
       AND ((q.state='fact' AND q.fact_at<$1)
         OR (q.state='withdrawn' AND q.opened_at<$1 AND coalesce(q.withdrawn_at,q.opened_at)<$1))
       AND factory_past_safe(q.id,$1)
       AND NOT EXISTS(SELECT 1 FROM entity_calculation_reports p WHERE p.question=q.id)
       AND NOT EXISTS(SELECT 1 FROM entity_user_calculation_requests u WHERE u.question=q.id AND (u.state='waiting' OR u.expires_at>=$1))
       AND NOT EXISTS(SELECT 1 FROM entity_measurement_reviews r WHERE r.question=q.id AND r.task_id IS NOT NULL)
       AND NOT EXISTS(SELECT 1 FROM relation_compute_order_cut_protection cp WHERE cp.question=q.id)
       AND NOT EXISTS(SELECT 1 FROM record_compute_order_cancellations oc WHERE oc.question=q.id)
     ORDER BY q.id FOR UPDATE OF q SKIP LOCKED
  ) SELECT coalesce(array_agg(id),'{}') INTO qs FROM locked;

  IF cardinality(qs)>0 THEN
    DELETE FROM relation_task_answer_evidence WHERE task_id IN(SELECT id FROM entity_compute_tasks WHERE computation=ANY(qs));
    DELETE FROM record_candidate_answers WHERE question=ANY(qs);
    DELETE FROM record_candidate_answers WHERE task_id IN(SELECT id FROM entity_compute_tasks WHERE computation=ANY(qs));
    DELETE FROM relation_task_consumers WHERE question=ANY(qs) AND state<>'live';
    DELETE FROM record_engine_refusals WHERE question=ANY(qs);
    DELETE FROM state_pending_verifications WHERE question=ANY(qs);
    DELETE FROM state_task_answer_heads WHERE question=ANY(qs);
    DELETE FROM state_compute_order_stock_holds WHERE task_id IN(SELECT id FROM entity_compute_tasks WHERE computation=ANY(qs));
    DELETE FROM record_compute_result_selections WHERE computation=ANY(qs);
    DELETE FROM state_compute_scheduling WHERE id=ANY(qs) AND state IN('fact','withdrawn');
    DELETE FROM record_compute_result_proofs WHERE result_id IN(SELECT id FROM entity_compute_results WHERE computation=ANY(qs));
    DELETE FROM entity_compute_results WHERE computation=ANY(qs);
    DELETE FROM relation_compute_task_sources WHERE task_id IN(SELECT id FROM entity_compute_tasks WHERE computation=ANY(qs));
    DELETE FROM entity_compute_tasks WHERE computation=ANY(qs);
    DELETE FROM entity_compute_specs WHERE id=ANY(qs);
  END IF;
  RETURN cardinality(wfs)+cardinality(qs);
END $$;

REVOKE ALL ON FUNCTION board_admission_changed(),board_admission_seed(bigint,text,bigint),board_admission_claim(bigint,text,text),
  board_admission_finish(text),board_admission_retry(text,text,text),board_admission_status(text),
  factory_assert_official_claim(text,bigint,text,text),factory_prune_settled(text,bigint) FROM PUBLIC;
DO $board_admission_grants$ BEGIN
  IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname='wfsim_factoryd') THEN
    GRANT INSERT ON relation_task_consumers TO wfsim_factoryd;
    GRANT EXECUTE ON FUNCTION board_admission_seed(bigint,text,bigint),board_admission_claim(bigint,text,text),board_admission_finish(text),
      board_admission_retry(text,text,text),board_admission_status(text),factory_assert_official_claim(text,bigint,text,text),
      factory_prune_settled(text,bigint) TO wfsim_factoryd;
  END IF;
END $board_admission_grants$;
