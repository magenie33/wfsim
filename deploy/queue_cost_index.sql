-- Rare verbs seek their most recent completed rounds independently of simulations.
CREATE INDEX CONCURRENTLY IF NOT EXISTS queue_cost_rare_facts ON state_compute_scheduling(verb,fact_at DESC)
  WHERE verb<>'simulate' AND fact_at IS NOT NULL AND state IN('fact','spot');
