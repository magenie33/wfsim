\set ON_ERROR_STOP on
SET ROLE wfsim;
SET search_path=public,pg_catalog,pg_temp;
SET lock_timeout=:'lock_wait';
SET statement_timeout='15min';
SET maintenance_work_mem='64MB';
SET max_parallel_maintenance_workers=0;
SELECT format($ddl$
  CREATE INDEX CONCURRENTLY compute_scheduling_stock_exception
  ON public.state_compute_scheduling (task_id,id) TABLESPACE %I
  WHERE state='open' AND waits=0 AND (required_engine IS NOT NULL OR lanes IS NOT NULL OR
    (required_capability IS NOT NULL AND required_capability<>'fixed-board-input-v1') OR
    verb NOT IN ('simulate','optimize'))$ddl$, :'tablespace')
\gexec
SELECT i.indisready,i.indisvalid,pg_relation_size(i.indexrelid) AS bytes,pg_get_indexdef(i.indexrelid) AS definition
FROM pg_index i WHERE i.indexrelid='public.compute_scheduling_stock_exception'::regclass
  AND i.indrelid='public.state_compute_scheduling'::regclass;
