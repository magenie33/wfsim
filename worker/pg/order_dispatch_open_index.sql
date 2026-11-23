-- Install online before switching a live post-cut selector to open scheduling rows.
CREATE INDEX CONCURRENTLY IF NOT EXISTS compute_scheduling_open_task ON state_compute_scheduling(task_id)
  WHERE state='open';
