CREATE OR REPLACE VIEW state_build_inbox_inputs AS
  SELECT b.id,b.at,b.record,b.initiator,b.family_source,b.observation,
    (SELECT s.definition::text FROM entity_build_workflows w JOIN entity_build_workflow_scopes s ON s.id=w.scope_id
      WHERE split_part(b.family_source,'|',1)='workflow' AND w.id=split_part(b.family_source,'|',2)) AS workflow_scope
    FROM record_build_submissions b
  UNION ALL SELECT id,at,record,NULL,NULL,NULL,NULL FROM record_equipment_submissions;
