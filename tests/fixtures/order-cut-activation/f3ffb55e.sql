CREATE OR REPLACE FUNCTION compute_order_cut_activate(text,bigint,text) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE n bigint; digest text;captured bigint;captured_digest text;
BEGIN
  PERFORM pg_advisory_xact_lock(hashtextextended('compute-order-cut:'||current_schema(),0));
  IF NOT EXISTS(SELECT 1 FROM entity_compute_order_cuts WHERE id=$1)
    OR EXISTS(SELECT 1 FROM record_compute_order_cut_activations) THEN RAISE EXCEPTION 'order cut activation changed'; END IF;
  IF to_regprocedure('factory_candidates(text,bigint,bigint,text,text,text,text,text,bigint,bigint,text,text,bigint,text,text,text,boolean)') IS NULL
    OR position('compute_order_cut_lock' IN pg_get_functiondef('compute_open_task(text,text,text,text,boolean)'::regprocedure))=0
    OR position('compute_order_cut_lock' IN pg_get_functiondef('compute_register_tasks(text,text,text)'::regprocedure))=0
    OR position('compute_order_cancelled' IN pg_get_functiondef('compute_reissue_task(text,bigint,text,text,text)'::regprocedure))=0
    OR position('state_board_effective_pending_work' IN pg_get_functiondef('inventory_inputs(text,text,text)'::regprocedure))=0 THEN
    RAISE EXCEPTION 'complete order cut runtime upgrade required'; END IF;
  SELECT count(*),encode(sha256(convert_to(coalesce(string_agg(question,E'\n' ORDER BY question),''),'UTF8')),'hex')
    INTO n,digest FROM relation_compute_order_cut_protection WHERE cut_id=$1 AND kind='manifest';
  IF n IS DISTINCT FROM $2 OR digest IS DISTINCT FROM $3 THEN RAISE EXCEPTION 'order protection manifest changed'; END IF;
  PERFORM compute_order_cut_capture($1);
  SELECT count(*),encode(sha256(convert_to(coalesce(string_agg(question,E'\n' ORDER BY question),''),'UTF8')),'hex')
    INTO captured,captured_digest FROM relation_compute_order_cut_protection WHERE cut_id=$1;
  UPDATE state_factory_snapshot SET at='' WHERE id=1;
  INSERT INTO record_compute_order_cut_activations VALUES($1,
    to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),n,digest,captured,captured_digest);
END $$;
