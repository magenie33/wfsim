-- THE SETTLER'S ROLE, on an existing library and accounts database.
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wfsim_factoryd') THEN
    CREATE ROLE wfsim_factoryd LOGIN;
  END IF;
END $$;
GRANT CONNECT ON DATABASE wfsim TO wfsim_factoryd;
GRANT SELECT ON answers, engine_refusals, factory_dials, questions, queue,
  refusal_withdrawals, settle_queue, verifier_hours, verifiers TO wfsim_factoryd;
GRANT SELECT, INSERT, UPDATE, DELETE ON factory_findings, factory_snapshot, fair_shares TO wfsim_factoryd;
GRANT SELECT (id) ON batches TO wfsim_factoryd;
GRANT SELECT, UPDATE ON answers, demands, questions, verifiers TO wfsim_factoryd;
GRANT SELECT, INSERT, UPDATE ON scores, verifier_hours TO wfsim_factoryd;
GRANT SELECT, UPDATE, DELETE ON settle_queue TO wfsim_factoryd;
GRANT DELETE ON queue TO wfsim_factoryd;
GRANT SELECT (channel, request, at, done_at, weapon, ruler, agreed_at, started_at) ON appraisals TO wfsim_factoryd;
GRANT SELECT, UPDATE ON user_calculations TO wfsim_factoryd;
GRANT SELECT, INSERT ON calculation_reports TO wfsim_factoryd;
