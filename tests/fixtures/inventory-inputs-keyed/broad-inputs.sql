CREATE OR REPLACE FUNCTION inventory_inputs(text,text,text) RETURNS jsonb
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held state_compute_inventory_generation%ROWTYPE; changes jsonb; build_keys text[]; question_keys text[];
  build_rows jsonb; fact_rows jsonb; queue_rows jsonb; task_rows jsonb;
BEGIN
  held:=inventory_read_guard($1,$2);
  changes:=inventory_change_keys($3);
  SELECT array_agg(DISTINCT x->>'key') FILTER(WHERE x->>'scope'='build'),
    array_agg(DISTINCT x->>'key') FILTER(WHERE x->>'scope'='question') INTO build_keys,question_keys FROM jsonb_array_elements(changes) x;
  SELECT COALESCE(jsonb_agg(CASE WHEN b.id IS NULL THEN jsonb_build_object('id',k.id,'deleted',true)
    ELSE jsonb_build_object('id',b.id,'at',b.at,'record',b.record::jsonb) END ORDER BY k.id),'[]'::jsonb)
    INTO build_rows FROM unnest(build_keys) k(id) LEFT JOIN entity_weapon_builds b ON b.id=k.id;
  SELECT COALESCE(jsonb_agg(to_jsonb(f) ORDER BY f.identity,f.ruler,f.mode),'[]'::jsonb)
    INTO fact_rows FROM board_facts f WHERE f.identity=ANY(build_keys);
  SELECT COALESCE(jsonb_agg(jsonb_build_object('batch',q.batch,'build_id',q.build_id,'ruler',q.ruler,'mode',q.mode)
    ORDER BY q.batch,q.build_id,q.ruler,q.mode),'[]'::jsonb) INTO queue_rows FROM state_board_effective_pending_work q
    WHERE q.build_id=ANY(build_keys) AND EXISTS(SELECT 1 FROM entity_compute_batches b WHERE b.id=q.batch);
  SELECT COALESCE(jsonb_agg(CASE
    WHEN q.id IS NOT NULL AND q.state<>'withdrawn' AND ((q.state IN ('fact','spot') AND q.fact_engine=$1)
      OR q.state IN ('disputed','nondeterministic') OR live.kind IS NOT NULL)
      THEN jsonb_build_object('id',q.id,'verb',q.verb,'state',q.state,'fact_engine',q.fact_engine,'kind',live.kind,
        'served_answered',CASE WHEN q.state IN ('disputed','nondeterministic')
          THEN EXISTS(SELECT 1 FROM record_candidate_answers a WHERE a.question=q.id AND a.engine=$1) ELSE false END)
    WHEN refused.question IS NOT NULL AND (q.id IS NULL OR q.state='withdrawn')
      THEN jsonb_build_object('id',k.id,'verb','','state','refused','fact_engine',refused.engine,'kind',NULL,'served_answered',false)
    ELSE jsonb_build_object('id',k.id,'deleted',true) END ORDER BY k.id),'[]'::jsonb) INTO task_rows
    FROM unnest(question_keys) k(id) LEFT JOIN state_compute_work q ON q.id=k.id
    LEFT JOIN LATERAL (SELECT min(d.producer) kind FROM relation_task_consumers d WHERE d.question=k.id AND d.state='live'
      AND NOT(d.producer=ANY(ARRAY(SELECT jsonb_array_elements_text($2::jsonb))))) live ON true
    LEFT JOIN record_refusal_withdrawals refused ON refused.question=k.id AND refused.engine=$1;
  RETURN jsonb_build_object('v',1,'generation',held.generation::text,'engine',held.engine,'contract',held.contract,
    'snapshot',pg_current_snapshot()::text,'started_at',to_char(transaction_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    'changes',changes,'builds',build_rows,'facts',fact_rows,'queue',queue_rows,'tasks',task_rows);
END $$;
