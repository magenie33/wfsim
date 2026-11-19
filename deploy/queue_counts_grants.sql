GRANT SELECT ON state_queue_zone_changes TO wfsim_factoryd;
GRANT SELECT,INSERT ON state_queue_zone_seed TO wfsim_factoryd;
GRANT EXECUTE ON FUNCTION queue_count_fold() TO wfsim_factoryd;
GRANT UPDATE(workload) ON state_queue_zone_seed TO wfsim_factoryd;
GRANT SELECT,INSERT,DELETE ON state_queue_clearance_days TO wfsim_factoryd;
