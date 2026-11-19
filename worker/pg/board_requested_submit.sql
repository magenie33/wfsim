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
  INSERT INTO record_build_submissions(id,at,record,initiator) VALUES($1,$2,$3,$4);
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
