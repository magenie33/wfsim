CREATE OR REPLACE FUNCTION board_request_submit(text,text,text,text,text) RETURNS text
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE record jsonb:=$3::jsonb; raw jsonb;
BEGIN
  IF NOT EXISTS(SELECT 1 FROM state_board_request_control WHERE id=1 AND enabled) THEN
    RAISE EXCEPTION 'requested board intake is paused' USING ERRCODE='55000'; END IF;
  IF jsonb_typeof(record) IS DISTINCT FROM 'object' OR octet_length($3)>4096
    OR coalesce($5,'')!~'^[a-z0-9_]{1,128}$'
    OR coalesce(record->>'mode','base')!~'^[a-z0-9_]{1,128}$' THEN
    RAISE EXCEPTION 'requested board input requires a ruler and mode'; END IF;
  raw:=jsonb_build_object('id',$1,'at',$2,'record',record,'initiator',$4);
  INSERT INTO entity_board_requests(id,kind,input,ruler,mode)
    VALUES($1,'missing',raw,$5,coalesce(record->>'mode','base'));
  INSERT INTO state_board_requests(id) VALUES($1);
  RETURN $1;
END $$;

CREATE OR REPLACE FUNCTION board_request_submit(text,text,text,text) RETURNS text
LANGUAGE sql SECURITY DEFINER SET search_path FROM CURRENT AS $$
  SELECT board_request_submit($1,$2,$3,$4,$3::jsonb->>'benchmark')
$$;
REVOKE ALL ON FUNCTION board_request_submit(text,text,text,text,text),board_request_submit(text,text,text,text) FROM PUBLIC;

-- An enabled request owns its observation even when an older writer uses the raw table.
CREATE OR REPLACE FUNCTION board_request_capture_review() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE observation jsonb; review entity_measurement_reviews%ROWTYPE;
BEGIN
  IF NEW.observation IS NULL OR NEW.family_source IS NOT NULL THEN RETURN NEW; END IF;
  observation:=NEW.observation::jsonb;
  SELECT * INTO review FROM entity_measurement_reviews WHERE id=observation->>'review' AND state='waiting';
  IF NOT FOUND OR NEW.id<>'review:'||review.id THEN RETURN NEW; END IF;
  INSERT INTO entity_board_requests(id,kind,input,ruler,mode)
    VALUES(NEW.id,'correction',to_jsonb(NEW),review.ruler,review.mode) ON CONFLICT(id) DO NOTHING;
  IF NOT FOUND AND NOT EXISTS(SELECT 1 FROM entity_board_requests r WHERE r.id=NEW.id
    AND r.kind='correction' AND r.ruler=review.ruler AND r.mode=review.mode AND r.input-'at'=to_jsonb(NEW)-'at') THEN
    RAISE EXCEPTION 'review source already owns a different input' USING ERRCODE='23505';
  END IF;
  INSERT INTO state_board_requests(id) VALUES(NEW.id) ON CONFLICT(id) DO NOTHING;
  IF EXISTS(SELECT 1 FROM state_board_request_control WHERE id=1 AND enabled) THEN RETURN NULL; END IF;
  RETURN NEW;
END $$;
DROP TRIGGER IF EXISTS board_requested_review ON record_build_submissions;
CREATE TRIGGER board_requested_review BEFORE INSERT ON record_build_submissions
  FOR EACH ROW WHEN(NEW.observation IS NOT NULL) EXECUTE FUNCTION board_request_capture_review();
REVOKE ALL ON FUNCTION board_request_capture_review() FROM PUBLIC;
