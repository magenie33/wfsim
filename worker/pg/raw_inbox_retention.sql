-- A historical copy needs an exact independent owner before retirement.
CREATE OR REPLACE FUNCTION build_raw_inbox_bytes(row_input record_build_submissions) RETURNS bigint
LANGUAGE sql IMMUTABLE SET search_path FROM CURRENT AS $$
  SELECT octet_length(row_input.id)::bigint+octet_length(row_input.at)+octet_length(row_input.record)
    +coalesce(octet_length(row_input.initiator),0)+coalesce(octet_length(row_input.family_source),0)
    +coalesce(octet_length(row_input.observation),0)
$$;

CREATE OR REPLACE FUNCTION build_raw_inbox_json(value text) RETURNS jsonb
LANGUAGE plpgsql IMMUTABLE STRICT SET search_path FROM CURRENT AS $$
BEGIN
  IF pg_input_is_valid(value,'jsonb') THEN RETURN value::jsonb; END IF;
  RETURN NULL;
END $$;

CREATE OR REPLACE FUNCTION build_raw_inbox_request_matches(row_input record_build_submissions,owner_input jsonb) RETURNS boolean
LANGUAGE sql IMMUTABLE SET search_path FROM CURRENT AS $$
  SELECT row_input.family_source IS NULL
    AND owner_input->>'id'=row_input.id AND owner_input->>'at'=row_input.at
    AND owner_input->>'initiator' IS NOT DISTINCT FROM row_input.initiator
    AND owner_input->>'observation' IS NOT DISTINCT FROM row_input.observation
    AND CASE jsonb_typeof(owner_input->'record') WHEN 'string' THEN owner_input->>'record'=row_input.record
      WHEN 'object' THEN owner_input->'record'=build_raw_inbox_json(row_input.record) ELSE false END
$$;

CREATE OR REPLACE FUNCTION build_raw_inbox_disposition(row_input record_build_submissions) RETURNS text
LANGUAGE sql STABLE SET search_path FROM CURRENT AS $$
  SELECT CASE
    WHEN EXISTS(SELECT 1 FROM state_build_family_inbox f
      WHERE f.id=row_input.id AND f.input=to_jsonb(row_input)) THEN 'family_copy'
    WHEN EXISTS(
      SELECT 1 FROM entity_board_requests r JOIN state_board_requests s USING(id)
      WHERE r.id=row_input.id AND s.phase IN('pending','staged','admitted')
        AND EXISTS(SELECT 1 FROM state_board_request_control WHERE id=1 AND enabled)
        AND build_raw_inbox_request_matches(row_input,r.input)
    ) THEN 'request_copy'
    WHEN EXISTS(SELECT 1 FROM relation_build_intake_sources src
      JOIN entity_build_intake_intents i ON i.id=src.intent
      JOIN entity_weapon_builds b ON b.id=i.build_id
      JOIN entity_build_intake_plans p ON p.id=i.plan_id
      WHERE src.source_id=row_input.id AND (
        EXISTS(SELECT 1 FROM state_build_admission_work w WHERE w.kind='intake' AND w.key=i.id)
        OR EXISTS(SELECT 1 FROM record_build_workflow_triggers w WHERE w.id=i.id)
      )) THEN 'canonical_unproven'
    WHEN row_input.observation IS NOT NULL OR row_input.id LIKE 'review:%' THEN 'review_input'
    WHEN row_input.id LIKE 'workflow:%' THEN 'workflow_input'
    WHEN row_input.id LIKE 'family:%' THEN 'legacy_winner'
    WHEN row_input.family_source IS NOT NULL THEN 'derived_input'
    ELSE 'unconfirmed_input' END
$$;

CREATE OR REPLACE FUNCTION build_raw_inbox_retire_page(text,integer,boolean) RETURNS jsonb
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE scanned bigint; last_id text; tally jsonb; ids text[]; locked text[]; removed bigint:=0;
  disposable constant text[]:=ARRAY['family_copy','request_copy'];
  row_max constant bigint:=64*1024; page_max constant bigint:=8*1024*1024;
BEGIN
  IF $1 IS NULL OR $2 IS NULL OR $2 NOT BETWEEN 1 AND 1000 OR $3 IS NULL THEN
    RAISE EXCEPTION 'invalid raw retention page'; END IF;
  -- Logical text sizes count compressed input at its full size before payload comparison.
  WITH keys AS MATERIALIZED (
    SELECT b.id,build_raw_inbox_bytes(b) AS bytes FROM record_build_submissions b WHERE b.id>$1 ORDER BY b.id LIMIT $2
  ), measured AS (
    SELECT id,bytes,sum(CASE WHEN bytes<=row_max THEN bytes ELSE 0 END) OVER(ORDER BY id) AS through_bytes FROM keys
  ), page AS MATERIALIZED (
    SELECT id,bytes FROM measured WHERE through_bytes<=page_max
  ), classified AS MATERIALIZED (
    SELECT p.id,CASE WHEN p.bytes>row_max THEN 'oversize_input' ELSE
      (SELECT build_raw_inbox_disposition(b) FROM record_build_submissions b WHERE b.id=p.id) END AS kind,p.bytes
    FROM page p
  ), grouped AS (
    SELECT kind,count(*) AS rows,sum(bytes) AS bytes FROM classified GROUP BY kind
  ) SELECT count(*),max(id),
      coalesce((SELECT jsonb_object_agg(kind,jsonb_build_object('rows',rows,'bytes',bytes)) FROM grouped),'{}'::jsonb),
      ARRAY(SELECT id FROM classified WHERE kind=ANY(disposable) ORDER BY id LIMIT 128)
    INTO scanned,last_id,tally,ids FROM classified;
  IF $3 AND cardinality(ids)>0 THEN
    SELECT ARRAY(SELECT b.id FROM record_build_submissions b WHERE b.id=ANY(ids)
      ORDER BY b.id FOR UPDATE OF b SKIP LOCKED) INTO locked;
    -- A new statement checks the same source after acquiring the locks.
    WITH rechecked AS MATERIALIZED (
      SELECT k.id FROM unnest(locked) k(id)
      CROSS JOIN LATERAL (SELECT b AS input FROM record_build_submissions b WHERE b.id=k.id OFFSET 0) found
      WHERE CASE WHEN build_raw_inbox_bytes(found.input)<=row_max
        THEN build_raw_inbox_disposition(found.input)=ANY(disposable) ELSE false END
    ) DELETE FROM record_build_submissions b USING rechecked r WHERE b.id=r.id;
    GET DIAGNOSTICS removed=ROW_COUNT;
  END IF;
  RETURN jsonb_build_object('scanned',scanned,'next',last_id,'groups',tally,'removed',removed);
END $$;
REVOKE ALL ON FUNCTION build_raw_inbox_bytes(record_build_submissions),build_raw_inbox_json(text),
  build_raw_inbox_request_matches(record_build_submissions,jsonb),
  build_raw_inbox_disposition(record_build_submissions),build_raw_inbox_retire_page(text,integer,boolean) FROM PUBLIC;
