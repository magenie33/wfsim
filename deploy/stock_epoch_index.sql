\set ON_ERROR_STOP on
SET ROLE wfsim;
SET search_path=public,pg_catalog,pg_temp;
SET lock_timeout=:'lock_wait';
SET statement_timeout='15min';
SET maintenance_work_mem='64MB';
SET max_parallel_maintenance_workers=0;
SELECT format($ddl$
  CREATE INDEX CONCURRENTLY compute_scheduling_stock_epoch_zone
  ON public.state_compute_scheduling
    ((coalesce(zone,factory_default_zone(producer))::numeric),coalesce(zone_position,task_id),id)
  TABLESPACE %I WHERE state='open' AND task_id>%s$ddl$, :'tablespace', task_cutoff)
FROM public.compute_order_cut() WHERE task_cutoff=:'cutoff'::bigint
\gexec
SELECT i.indisready,i.indisvalid,pg_relation_size(i.indexrelid) AS bytes,pg_get_indexdef(i.indexrelid) AS definition
FROM pg_index i WHERE i.indexrelid='public.compute_scheduling_stock_epoch_zone'::regclass
  AND i.indrelid='public.state_compute_scheduling'::regclass;
