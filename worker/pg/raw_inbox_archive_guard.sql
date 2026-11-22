CREATE OR REPLACE FUNCTION raw_archive_consume_guarded_slice(uuid,bigint,text,integer,integer,text[],text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE known text[]; receipt jsonb;
BEGIN
  IF cardinality($6)>64 THEN RAISE EXCEPTION 'raw archive scope exceeds its row bound' USING ERRCODE='22023'; END IF;
  PERFORM compute_order_cut_lock();
  SELECT ARRAY(SELECT s.intent FROM relation_build_intake_sources s WHERE s.source_id=ANY($6)) INTO known;
  receipt:=raw_archive_consume_slice($1,$2,$3,$4,$5,$6,$7);
  IF NOT coalesce((receipt->>'replay')::boolean,false) AND EXISTS(
    SELECT FROM relation_build_intake_sources s
    JOIN state_build_admission_work w ON w.kind='intake' AND w.key=s.intent
    JOIN LATERAL (SELECT item->'input' AS input FROM jsonb_array_elements($3::jsonb->'rows') WITH ORDINALITY r(item,n)
      WHERE r.n>$4 AND r.n<=$5 AND item#>>'{input,id}'=s.source_id) source ON true
    WHERE s.source_id=ANY($6) AND NOT s.intent=ANY(known)
      AND (w.family OR compute_order_pending_cancelled(w.zone,9223372036854775807,s.source_id,source.input->>'at'))
  ) THEN
    RAISE EXCEPTION 'raw archive would add paused or cancelled intake' USING ERRCODE='WSA01';
  END IF;
  RETURN receipt;
END $$;
REVOKE ALL ON FUNCTION raw_archive_consume_guarded_slice(uuid,bigint,text,integer,integer,text[],text) FROM PUBLIC;
