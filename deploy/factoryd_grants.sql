-- THE SETTLER'S ROLE, on an existing library and accounts database.
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wfsim_factoryd') THEN
    CREATE ROLE wfsim_factoryd LOGIN;
  END IF;
END $$;
GRANT CONNECT ON DATABASE wfsim TO wfsim_factoryd;
GRANT SELECT ON record_candidate_answers, record_engine_refusals, state_factory_parameters, state_compute_work, state_board_pending_work,
  record_refusal_withdrawals, state_pending_verifications, record_hourly_compute_totals, entity_compute_devices TO wfsim_factoryd;
GRANT SELECT, INSERT, UPDATE, DELETE ON state_factory_findings, state_factory_snapshot, state_compute_fair_shares TO wfsim_factoryd;
GRANT SELECT ON state_task_pipeline_controls TO wfsim_factoryd;
GRANT SELECT,INSERT,UPDATE ON state_task_pipeline_progress TO wfsim_factoryd;
GRANT SELECT (id) ON entity_compute_batches TO wfsim_factoryd;
GRANT INSERT ON entity_compute_batches TO wfsim_factoryd;
GRANT SELECT ON entity_weapon_builds TO wfsim_factoryd;
GRANT SELECT,INSERT ON state_producer_compute_usage TO wfsim_factoryd;
GRANT SELECT, UPDATE ON record_candidate_answers, relation_task_consumers, state_compute_work, entity_compute_devices TO wfsim_factoryd;
GRANT INSERT ON relation_task_consumers TO wfsim_factoryd;
GRANT SELECT, INSERT, UPDATE ON state_board_measurement_selection, record_hourly_compute_totals TO wfsim_factoryd;
GRANT SELECT, UPDATE, DELETE ON state_pending_verifications TO wfsim_factoryd;
GRANT DELETE ON state_board_pending_work TO wfsim_factoryd;
GRANT SELECT (channel, request, at, done_at, weapon, ruler, agreed_at, started_at) ON entity_appraisal_requests TO wfsim_factoryd;
GRANT SELECT, UPDATE ON entity_user_calculation_requests TO wfsim_factoryd;
GRANT SELECT, INSERT ON entity_calculation_reports TO wfsim_factoryd;
GRANT SELECT, INSERT, UPDATE, DELETE ON state_public_computation_summaries, relation_computation_witnesses, relation_measurement_witnesses TO wfsim_factoryd;

GRANT SELECT,INSERT,UPDATE ON entity_measurement_reviews TO wfsim_factoryd;
GRANT SELECT ON board_facts TO wfsim_factoryd;
GRANT SELECT, UPDATE ON entity_family_search_requests TO wfsim_factoryd;
GRANT SELECT, INSERT ON record_build_submissions TO wfsim_factoryd;

GRANT SELECT,INSERT,DELETE ON entity_compute_specs,entity_compute_tasks,entity_compute_results TO wfsim_factoryd;
GRANT UPDATE(id) ON entity_compute_specs TO wfsim_factoryd;
GRANT UPDATE (proof_archive,proof_digest) ON entity_compute_results TO wfsim_factoryd;
GRANT SELECT,INSERT ON record_compute_result_selections TO wfsim_factoryd;
GRANT SELECT,INSERT,DELETE ON record_compute_result_proofs TO wfsim_factoryd;
GRANT SELECT,INSERT,UPDATE,DELETE ON state_compute_scheduling,relation_task_answer_evidence TO wfsim_factoryd;
GRANT SELECT,INSERT ON relation_compute_task_sources TO wfsim_factoryd;
GRANT SELECT ON state_compute_candidate_answers TO wfsim_factoryd;
GRANT SELECT,INSERT,DELETE ON entity_calculation_reports,relation_calculation_report_access TO wfsim_factoryd;
GRANT SELECT,INSERT,UPDATE,DELETE ON state_calculation_report_selection TO wfsim_factoryd;
GRANT SELECT ON state_current_calculation_reports TO wfsim_factoryd;
GRANT USAGE,SELECT ON SEQUENCE entity_compute_tasks_id_seq,entity_compute_results_id_seq,
  record_compute_result_selections_id_seq,entity_calculation_reports_id_seq,compute_queue_position TO wfsim_factoryd;
GRANT SELECT ON entity_build_workflow_scopes,entity_build_workflows,entity_build_workflow_expansions,record_build_workflow_triggers TO wfsim_factoryd;
GRANT SELECT,UPDATE ON state_build_workflows,state_build_workflow_nodes TO wfsim_factoryd;
GRANT EXECUTE ON FUNCTION build_workflow_repair(text,text,text,text) TO wfsim_factoryd;
GRANT EXECUTE ON FUNCTION board_admission_seed(bigint,text,bigint),board_admission_claim(bigint,text,text),board_admission_finish(text),
  board_admission_retry(text,text,text),board_admission_status(text),factory_assert_official_claim(text,bigint,text,text) TO wfsim_factoryd;
