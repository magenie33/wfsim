-- Install online before activating a cut; both builds read their existing heap.
CREATE INDEX CONCURRENTLY IF NOT EXISTS compute_order_execution_evidence ON state_compute_scheduling(id)
  WHERE lease IS NOT NULL OR leased_at IS NOT NULL OR leased_to IS NOT NULL OR lease_until IS NOT NULL
    OR lease_prev IS NOT NULL OR leased_to_prev IS NOT NULL OR lease_prev_until IS NOT NULL
    OR result_id IS NOT NULL OR fact_at IS NOT NULL OR fact_engine IS NOT NULL;
CREATE INDEX CONCURRENTLY IF NOT EXISTS compute_order_prior_rounds ON entity_compute_tasks(computation)
  WHERE generation<>1;
