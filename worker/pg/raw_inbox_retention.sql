-- Only a matching independent owner or canonical intake receipt can retire a raw row.
CREATE OR REPLACE FUNCTION build_raw_inbox_disposition(row_input record_build_submissions) RETURNS text
LANGUAGE sql STABLE SET search_path FROM CURRENT AS $$
  SELECT CASE
    WHEN EXISTS(SELECT 1 FROM state_build_family_inbox f
      WHERE f.id=row_input.id AND f.input=to_jsonb(row_input)) THEN 'family_copy'
    WHEN row_input.family_source IS NULL AND EXISTS(
      SELECT 1 FROM entity_board_requests r JOIN state_board_requests s USING(id)
      WHERE r.id=row_input.id AND s.phase IN('pending','staged','admitted')
        AND EXISTS(SELECT 1 FROM state_board_request_control WHERE id=1 AND enabled)
        AND r.input->>'id'=row_input.id AND r.input->>'at'=row_input.at
        AND r.input->>'initiator' IS NOT DISTINCT FROM row_input.initiator
        AND r.input->>'observation' IS NOT DISTINCT FROM row_input.observation
        AND CASE jsonb_typeof(r.input->'record') WHEN 'string' THEN r.input->>'record'=row_input.record
          WHEN 'object' THEN CASE WHEN pg_input_is_valid(row_input.record,'jsonb')
            THEN r.input->'record'=row_input.record::jsonb ELSE false END ELSE false END
    ) THEN 'request_copy'
    WHEN EXISTS(SELECT 1 FROM relation_build_intake_sources src
      JOIN entity_build_intake_intents i ON i.id=src.intent
      JOIN entity_weapon_builds b ON b.id=i.build_id
      JOIN entity_build_intake_plans p ON p.id=i.plan_id
      WHERE src.source_id=row_input.id AND (
        EXISTS(SELECT 1 FROM state_build_admission_work w WHERE w.kind='intake' AND w.key=i.id)
        OR EXISTS(SELECT 1 FROM record_build_workflow_triggers w WHERE w.id=i.id)
      )) THEN 'canonical_handoff'
    WHEN row_input.observation IS NOT NULL OR row_input.id LIKE 'review:%' THEN 'review_input'
    WHEN row_input.id LIKE 'workflow:%' THEN 'workflow_input'
    WHEN row_input.id LIKE 'family:%' THEN 'legacy_winner'
    WHEN row_input.family_source IS NOT NULL THEN 'derived_input'
    ELSE 'unconfirmed_input' END
$$;

CREATE OR REPLACE FUNCTION build_raw_inbox_retire_page(text,integer,boolean) RETURNS jsonb
LANGUAGE plpgsql SET search_path FROM CURRENT AS $$
DECLARE scanned bigint; last_id text; tally jsonb; ids text[]; locked text[]; removed bigint:=0;
  disposable constant text[]:=ARRAY['family_copy','request_copy','canonical_handoff'];
BEGIN
  IF $1 IS NULL OR $2 IS NULL OR $2 NOT BETWEEN 1 AND 1000 OR $3 IS NULL THEN
    RAISE EXCEPTION 'invalid raw retention page'; END IF;
  WITH page AS MATERIALIZED (
    SELECT b AS input FROM record_build_submissions b WHERE b.id>$1 ORDER BY b.id LIMIT $2
  ), classified AS MATERIALIZED (
    SELECT (input).id AS id,build_raw_inbox_disposition(input) AS kind,pg_column_size(input) AS bytes FROM page
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
    DELETE FROM record_build_submissions b WHERE b.id=ANY(locked)
      AND build_raw_inbox_disposition(b)=ANY(disposable);
    GET DIAGNOSTICS removed=ROW_COUNT;
  END IF;
  RETURN jsonb_build_object('scanned',scanned,'next',last_id,'groups',tally,'removed',removed);
END $$;
REVOKE ALL ON FUNCTION build_raw_inbox_disposition(record_build_submissions),build_raw_inbox_retire_page(text,integer,boolean) FROM PUBLIC;
