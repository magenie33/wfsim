-- THE FAIR TABLE reads device ownership and account age, never account secrets.
GRANT CONNECT ON DATABASE wfsim_accounts TO wfsim_factoryd;
GRANT SELECT (verifier, account) ON relation_account_compute_devices TO wfsim_factoryd;
GRANT SELECT (id, created_at) ON entity_accounts TO wfsim_factoryd;
