\set ON_ERROR_STOP on
SET ROLE wfsim;
SET search_path=public,pg_catalog,pg_temp;
SET lock_timeout=:'lock_wait';
SET statement_timeout='15min';
SET maintenance_work_mem='64MB';
SET max_parallel_maintenance_workers=0;
SELECT task_cutoff AS verified_cutoff
FROM public.compute_order_cut() WHERE task_cutoff=:'cutoff'::bigint
\gset
SELECT format($ddl$
  CREATE INDEX CONCURRENTLY compute_scheduling_stock_epoch_due
  ON public.state_compute_scheduling (waits DESC,due_at,id)
  TABLESPACE %I WHERE state='open' AND task_id>%s AND waits IN(0,1)$ddl$,
  :'tablespace', :'verified_cutoff'::bigint)
\gexec
SELECT i.indisready,i.indisvalid,pg_relation_size(i.indexrelid) AS bytes,pg_get_indexdef(i.indexrelid) AS definition
FROM pg_index i WHERE i.indexrelid='public.compute_scheduling_stock_epoch_due'::regclass
  AND i.indrelid='public.state_compute_scheduling'::regclass;
