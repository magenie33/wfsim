-- A score replacement clears its previous witnesses through an invoker trigger.
DO $grant$ BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wfsim_pipeline') THEN
    GRANT SELECT (identity, ruler, mode), DELETE ON relation_measurement_witnesses TO wfsim_pipeline;
  END IF;
END $grant$;
