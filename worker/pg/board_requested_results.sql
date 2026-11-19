-- An admitted player request wakes one canonical ranking identity after verification.
CREATE TABLE state_board_requested_results (
  request TEXT COLLATE "C" PRIMARY KEY REFERENCES entity_board_requests(id) ON DELETE CASCADE,
  available_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX board_requested_results_ready ON state_board_requested_results(available_at,request);
REVOKE ALL ON state_board_requested_results FROM PUBLIC;

CREATE FUNCTION board_requested_result_capture() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
  INSERT INTO state_board_requested_results(request) VALUES(NEW.id) ON CONFLICT DO NOTHING;
  RETURN NEW;
END $$;
REVOKE ALL ON FUNCTION board_requested_result_capture() FROM PUBLIC;
CREATE TRIGGER board_requested_result_admitted AFTER UPDATE OF phase ON state_board_requests
  FOR EACH ROW WHEN (NEW.phase='admitted' AND OLD.phase IS DISTINCT FROM NEW.phase)
  EXECUTE FUNCTION board_requested_result_capture();

CREATE FUNCTION board_requested_result_take(uuid,text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE chosen text; item record; delay interval; target jsonb;
BEGIN
  PERFORM board_rank_source_lock($1::text);
  IF NOT EXISTS(SELECT FROM state_board_request_control WHERE id=1 AND enabled) THEN RETURN NULL; END IF;
  SELECT request INTO chosen FROM state_board_requested_results WHERE available_at<=statement_timestamp()
    ORDER BY available_at,request LIMIT 1 FOR UPDATE SKIP LOCKED;
  IF NOT FOUND THEN RETURN NULL; END IF;
  SELECT s.phase,s.workflow,w.state AS work_state,r.kind,r.ruler,r.mode,i.build_id,b.weapon,
    f.identity AS measured,f.review_id,review.state AS review_state INTO STRICT item
    FROM entity_board_requests r JOIN state_board_requests s USING(id)
    LEFT JOIN entity_build_intake_intents i ON i.id=s.intent LEFT JOIN entity_weapon_builds b ON b.id=i.build_id
    LEFT JOIN state_build_workflows w ON w.id=s.workflow
    LEFT JOIN board_facts f ON f.identity=i.build_id AND f.ruler=r.ruler AND f.mode=r.mode AND f.retired_at IS NULL
    LEFT JOIN entity_measurement_reviews review ON r.kind='correction' AND review.id=substr(r.id,8)
    WHERE r.id=chosen;
  IF item.phase='admitted' AND (item.work_state='active' OR item.review_state='waiting') THEN
    delay:=interval '15 seconds';
  ELSIF item.phase='admitted' AND (item.workflow IS NULL OR item.work_state='succeeded')
    AND item.measured IS NOT NULL AND (item.kind='missing' OR (item.review_state='succeeded' AND 'review:'||item.review_id=chosen)) THEN
    IF EXISTS(SELECT FROM state_board_rank_partitions p JOIN state_board_read_partitions v USING(weapon,ruler)
      WHERE p.weapon=item.weapon AND p.ruler=item.ruler AND p.complete AND p.source=$1::text AND p.native=$2
        AND v.enabled AND v.canonical_sources AND v.revision IS NOT NULL) THEN
      target:=jsonb_build_object('weapon',item.weapon,'ruler',item.ruler,'identity',item.build_id);
    ELSE
      delay:=interval '5 minutes';
    END IF;
  END IF;
  IF delay IS NULL THEN DELETE FROM state_board_requested_results WHERE request=chosen;
  ELSE UPDATE state_board_requested_results SET available_at=clock_timestamp()+delay WHERE request=chosen; END IF;
  RETURN target;
END $$;
REVOKE ALL ON FUNCTION board_requested_result_take(uuid,text) FROM PUBLIC;

CREATE FUNCTION board_requested_result_recover(text[]) RETURNS integer
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE inserted integer;
BEGIN
  IF coalesce(cardinality($1),0) NOT BETWEEN 1 AND 64 THEN RAISE EXCEPTION 'supply 1 to 64 request receipts'; END IF;
  INSERT INTO state_board_requested_results(request)
    SELECT s.id FROM unnest($1) AS wanted(id) JOIN state_board_requests s USING(id)
    WHERE s.phase='admitted' ON CONFLICT DO NOTHING;
  GET DIAGNOSTICS inserted=ROW_COUNT;
  RETURN inserted;
END $$;
REVOKE ALL ON FUNCTION board_requested_result_recover(text[]) FROM PUBLIC;
