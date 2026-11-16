-- A fresh explicit click may replace an unadmitted legacy review, without consuming its old inbox.
CREATE FUNCTION board_requested_review_fresh(text,text,text,text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE prior entity_measurement_reviews%ROWTYPE; selected state_board_measurement_selection%ROWTYPE;
  body jsonb:=$4::jsonb; at text; existing text; held text; observation jsonb;
BEGIN
  IF NOT EXISTS(SELECT 1 FROM state_board_request_control WHERE id=1 AND enabled) THEN RETURN NULL;END IF;
  SELECT * INTO prior FROM entity_measurement_reviews WHERE id=$1 FOR UPDATE;
  IF NOT FOUND OR EXISTS(SELECT 1 FROM entity_board_requests WHERE id='review:'||$1) THEN RETURN NULL;END IF;
  IF prior.state<>'waiting' THEN
    SELECT id INTO existing FROM entity_measurement_reviews WHERE ref=prior.ref AND engine=body->>'engine' AND state='waiting';
    RETURN CASE WHEN existing IS NULL THEN NULL ELSE jsonb_build_object('ok',true,'accepted',false,'id',existing,'next_ms',10000) END;
  END IF;
  IF prior.task_id IS NOT NULL AND EXISTS(SELECT 1 FROM state_compute_work WHERE task_id=prior.task_id AND state<>'withdrawn') THEN RETURN NULL;END IF;
  IF prior.identity IS DISTINCT FROM body->>'identity' OR prior.ruler IS DISTINCT FROM body->>'ruler'
    OR prior.mode IS DISTINCT FROM body->>'mode' THEN RAISE EXCEPTION 'legacy review does not match the requested row';END IF;
  SELECT * INTO selected FROM state_board_measurement_selection WHERE identity=prior.identity AND ruler=prior.ruler AND mode=prior.mode FOR UPDATE;
  IF NOT FOUND OR selected.score IS DISTINCT FROM (body->>'baseline_score')::double precision OR selected.metric IS DISTINCT FROM body->>'metric' THEN
    RAISE EXCEPTION 'review baseline changed' USING ERRCODE='40001';END IF;
  SELECT record INTO STRICT held FROM entity_weapon_builds WHERE id=prior.identity;
  IF held::jsonb IS DISTINCT FROM prior.record::jsonb THEN RAISE EXCEPTION 'legacy review inputs changed';END IF;
  at:=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
  UPDATE entity_measurement_reviews SET state='superseded',completed_at=at WHERE id=prior.id;
  INSERT INTO entity_measurement_reviews(ref,id,round_at,identity,ruler,mode,engine,question,record,state,
    baseline_score,baseline_metric,baseline_cost_seconds,baseline_review_id,baseline_by,baseline_started_at,baseline_finished_at,created_at)
    VALUES(prior.ref,$2,at,prior.identity,prior.ruler,prior.mode,
      body->>'engine',$3,held,'waiting',selected.score,selected.metric,selected.cost_seconds,selected.review_id,
      selected.measured_by,selected.started_at,selected.finished_at,at);
  observation:=jsonb_build_object('score',body->'observed_score','metric',body->>'metric','engine',body->>'engine',
    'baseline_score',selected.score,'baseline_finished_at',selected.finished_at,'request',body->'request',
    'ruler',prior.ruler,'mode',prior.mode,'review',$2);
  INSERT INTO record_build_submissions(id,at,record,initiator,observation)
    VALUES('review:'||$2,at,(held::jsonb||jsonb_build_object('benchmark',prior.ruler,'mode',prior.mode))::text,'anonymous',observation::text);
  RETURN jsonb_build_object('ok',true,'accepted',true,'id',$2,'next_ms',10000);
END $$;
REVOKE ALL ON FUNCTION board_requested_review_fresh(text,text,text,text) FROM PUBLIC;
