CREATE OR REPLACE FUNCTION compute_admit_tasks(text,text,text,text) RETURNS void
LANGUAGE plpgsql AS $$
DECLARE s jsonb; d jsonb; prepared jsonb:='[]'; q record; source record; opened text[]:='{}'; previous_question text; affected text[]:='{}';
BEGIN
  s := $1::jsonb; d := $2::jsonb;
  IF jsonb_typeof(s)<>'array' OR jsonb_typeof(d)<>'array' OR jsonb_array_length(s)>250
    OR jsonb_array_length(d)>250 OR $4 IS NULL THEN RAISE EXCEPTION 'invalid compute admission'; END IF;
  IF EXISTS(SELECT 1 FROM jsonb_array_elements(d) x WHERE
      coalesce(length(btrim(x->>'producer')),0)=0 OR coalesce(length(btrim(x->>'ref')),0)=0
      OR coalesce(length(btrim(x->>'trigger_id')),0)=0 OR coalesce(jsonb_typeof((x->>'payload')::jsonb),'null')<>'object'
      OR NOT EXISTS(SELECT 1 FROM jsonb_array_elements(s) y WHERE y->>'id'=x->>'question'))
    OR EXISTS(SELECT 1 FROM jsonb_array_elements(s) x WHERE NOT EXISTS(
      SELECT 1 FROM jsonb_array_elements(d) y WHERE y->>'question'=x->>'id'))
    THEN RAISE EXCEPTION 'a compute task requires a source'; END IF;
  FOR source IN SELECT value FROM jsonb_array_elements(s) ORDER BY coalesce((value->>'ordinal')::bigint,0) LOOP
    prepared:=prepared||jsonb_build_array(source.value||jsonb_build_object('position',nextval('compute_queue_position')));
  END LOOP;
  PERFORM compute_register_tasks(prepared::text,$3,$4);
  FOR q IN SELECT DISTINCT ON(x.id) x.id,x.reason,x.invalidate FROM jsonb_to_recordset(s) AS x(id text,force boolean,invalidate boolean,reason text)
      JOIN state_compute_scheduling a ON a.id=x.id WHERE
      (x.force AND a.state IN ('fact','spot'))
      OR (x.invalidate AND a.state IN ('open','disputed','nondeterministic','fact','spot')) ORDER BY x.id LOOP
    IF NOT EXISTS(SELECT 1 FROM jsonb_array_elements(d) x JOIN state_compute_scheduling a ON a.id=q.id
      JOIN entity_compute_tasks t ON t.computation=a.id
      JOIN relation_compute_task_sources p ON p.task_id=t.id AND p.producer=x->>'producer'
        AND p.ref=x->>'ref' AND p.trigger_id=x->>'trigger_id' WHERE x->>'question'=q.id) THEN
      PERFORM compute_open_task(q.id,CASE WHEN q.invalidate THEN 'input_invalidated' WHEN q.reason='retention_expired' THEN q.reason ELSE 'requested_recalculation' END,$3,$4,true);
      opened:=array_append(opened,q.id);
    END IF;
  END LOOP;
  UPDATE state_compute_scheduling target SET required_engine=nullif($3,'')
    FROM jsonb_to_recordset(s) x(id text,force boolean,invalidate boolean)
    WHERE target.id=x.id AND (x.force OR x.invalidate) AND nullif($3,'') IS NOT NULL
      AND EXISTS(SELECT 1 FROM jsonb_array_elements(d) incoming WHERE incoming->>'question'=target.id
        AND NOT EXISTS(SELECT 1 FROM relation_compute_task_sources previous JOIN entity_compute_tasks t ON t.id=previous.task_id
          WHERE t.computation=target.id AND previous.producer=incoming->>'producer' AND previous.ref=incoming->>'ref'
            AND previous.trigger_id=incoming->>'trigger_id'));
  FOR source IN SELECT * FROM jsonb_to_recordset(d) AS x(producer text,ref text,question text,
      trigger_id text,ordinal bigint,asked_at text,due_at text,waits bigint,payload text,initiator text,zone bigint,force boolean,invalidate boolean)
      ORDER BY producer,ref,ordinal,trigger_id LOOP
    SELECT * INTO STRICT q FROM state_compute_scheduling WHERE id=source.question;
    SELECT question INTO previous_question FROM relation_task_consumers
      WHERE producer=source.producer AND ref=source.ref FOR UPDATE;
    IF previous_question IS NOT NULL THEN affected:=array_append(affected,previous_question); END IF;
    IF EXISTS(SELECT 1 FROM relation_compute_task_sources p WHERE p.task_id=q.task_id AND p.producer=source.producer
      AND p.ref=source.ref AND p.trigger_id=source.trigger_id AND
        (p.payload::jsonb IS DISTINCT FROM source.payload::jsonb)) THEN
      RAISE EXCEPTION 'compute source identity conflict' USING ERRCODE='23505';
    END IF;
    INSERT INTO relation_compute_task_sources(task_id,producer,ref,trigger_id,asked_at,payload,requirements)
      SELECT q.task_id,source.producer,source.ref,source.trigger_id,coalesce(source.asked_at,$4),source.payload,
        jsonb_build_object('force',coalesce(source.force,x.force,false),'invalidate',coalesce(source.invalidate,x.invalidate,false),'engine',$3,'zone',source.zone)
      FROM jsonb_to_recordset(s) x(id text,force boolean,invalidate boolean) WHERE x.id=source.question
      ON CONFLICT DO NOTHING;
    INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at,waits,payload,initiator,delivery_at,zone)
      VALUES(source.producer,source.ref,source.question,'live',coalesce(source.asked_at,$4),source.due_at,source.waits,source.payload,source.initiator,
        CASE WHEN q.state IN ('fact','spot') THEN $4 END,coalesce(source.zone,factory_default_zone(source.producer)))
      ON CONFLICT(producer,ref) DO UPDATE SET question=excluded.question,state='live',asked_at=excluded.asked_at,
        due_at=excluded.due_at,waits=excluded.waits,payload=excluded.payload,served_at=NULL,
        initiator=excluded.initiator,delivery_at=excluded.delivery_at,
        zone=CASE WHEN relation_task_consumers.question=excluded.question AND relation_task_consumers.state='live'
          THEN least(relation_task_consumers.zone,excluded.zone) ELSE excluded.zone END;
  END LOOP;
  PERFORM factory_restate(id) FROM (SELECT DISTINCT unnest(affected) id
    UNION SELECT value->>'id' FROM jsonb_array_elements(s)) changed ORDER BY id;
  INSERT INTO state_producer_compute_usage(producer,used)
    SELECT DISTINCT value->>'producer',coalesce((SELECT min(used) FROM state_producer_compute_usage),0)
      FROM jsonb_array_elements(d) ON CONFLICT DO NOTHING;
END $$;
