CREATE OR REPLACE FUNCTION build_ack_submissions(text) RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE proof record; captured record_build_submissions%ROWTYPE; gear record_equipment_submissions%ROWTYPE;
  at text:=to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
BEGIN
  IF jsonb_typeof($1::jsonb) IS DISTINCT FROM 'array' OR jsonb_array_length($1::jsonb)>1000 THEN
    RAISE EXCEPTION 'invalid submission acknowledgement'; END IF;
  FOR proof IN SELECT * FROM jsonb_to_recordset($1::jsonb)
    AS rows(id text,at text,record text,initiator text,family_source text,observation text) ORDER BY id COLLATE "C" LOOP
    IF coalesce(proof.id,'')='' OR proof.at IS NULL OR proof.record IS NULL THEN
      RAISE EXCEPTION 'submission acknowledgement requires captured content'; END IF;
    IF proof.id LIKE 'equipment:%' THEN
      SELECT * INTO gear FROM record_equipment_submissions b WHERE b.id=proof.id FOR UPDATE;
      IF NOT FOUND THEN CONTINUE; END IF;
      IF (gear.at,gear.record) IS DISTINCT FROM (proof.at,proof.record) THEN
        RAISE EXCEPTION 'captured equipment submission changed'; END IF;
      DELETE FROM record_equipment_submissions b WHERE b.id=gear.id;
      CONTINUE;
    END IF;
    SELECT * INTO captured FROM record_build_submissions b WHERE b.id=proof.id FOR UPDATE;
    IF NOT FOUND THEN
      IF to_regprocedure('raw_inbox_rollover_ack(record_build_submissions)') IS NOT NULL THEN
        PERFORM raw_inbox_rollover_ack(ROW(proof.id,proof.at,proof.record,proof.initiator,proof.family_source,
          proof.observation)::record_build_submissions);
      END IF;
      CONTINUE;
    END IF;
    IF (captured.at,captured.record,captured.initiator,captured.family_source,captured.observation) IS DISTINCT FROM
       (proof.at,proof.record,proof.initiator,proof.family_source,proof.observation) THEN
      RAISE EXCEPTION 'captured build submission changed'; END IF;
    IF NOT EXISTS(SELECT 1 FROM relation_build_intake_sources WHERE source_id=captured.id)
      AND NOT EXISTS(SELECT 1 FROM state_build_family_inbox f WHERE f.id=captured.id AND f.input=to_jsonb(captured)) THEN
      UPDATE entity_measurement_reviews SET state='failed',completed_at=at
        WHERE id=captured.observation::jsonb->>'review' AND state='waiting' AND workflow IS NULL AND task_id IS NULL;
      IF split_part(captured.family_source,'|',1)='workflow' THEN
        UPDATE state_build_workflow_nodes SET state='failed',error='derived build was refused by intake'
          WHERE workflow=split_part(captured.family_source,'|',2) AND id=split_part(captured.family_source,'|',3) AND state='awaiting_intake';
        PERFORM build_workflow_finish(split_part(captured.family_source,'|',2),at);
      END IF;
    END IF;
    DELETE FROM record_build_submissions b WHERE b.id=captured.id;
  END LOOP;
END $$;