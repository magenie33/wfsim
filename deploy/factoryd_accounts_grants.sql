-- THE FAIR TABLE reads device ownership and account age, never account secrets.
GRANT CONNECT ON DATABASE wfsim_accounts TO wfsim_factoryd;
GRANT SELECT (verifier, account) ON devices TO wfsim_factoryd;
GRANT SELECT (id, created_at) ON accounts TO wfsim_factoryd;
