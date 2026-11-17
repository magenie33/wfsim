// SPDX-License-Identifier: AGPL-3.0-or-later
// Render reviewed installation units; this command never connects to a database.
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
export function statisticsSchemaSql(action,{tablespace}={}){
 const schema=readFileSync(new URL('../worker/pg/statistics_projection.sql',import.meta.url),'utf8');
 if(['schema','index'].includes(action)&&!/^[a-z_][a-z0-9_]*$/.test(tablespace??''))throw Error('choose an existing, capacity-qualified tablespace explicitly');
 if(action==='schema')return "BEGIN;\nSET LOCAL default_tablespace='"+tablespace+"';\nSET LOCAL lock_timeout='5s';\n"+schema+'\nCOMMIT;\n';
 if(action==='index')return "SET default_tablespace='"+tablespace+"';\nSET lock_timeout='5s';\n"+schema.match(/^-- (CREATE INDEX CONCURRENTLY[^\n]+);$/m)[1]+";\nRESET default_tablespace;\n";
 if(action==='grants')return readFileSync(new URL('../deploy/statistics_grants.sql',import.meta.url),'utf8');
 if(action==='recover')return `BEGIN;
 SET LOCAL lock_timeout='5s';
 SELECT pg_advisory_xact_lock(hashtextextended('statistics-projector:'||current_schema(),0));
 DO $recover$ DECLARE name text; BEGIN
  IF NOT coalesce((SELECT paused FROM state_task_pipeline_controls c WHERE c.name='statistics'),false) THEN RAISE EXCEPTION 'pause statistics before recovery'; END IF;
  FOREACH name IN ARRAY statistics_capture_relations() LOOP EXECUTE format('LOCK TABLE %I IN SHARE MODE NOWAIT',name); END LOOP;
  PERFORM statistics_invalidate();
  DELETE FROM state_statistics_head;
  DELETE FROM state_statistics_invalidations;
  UPDATE state_statistics_generation SET blocked=NULL WHERE id=1;
 END $recover$;
 COMMIT;
`;
 throw Error('usage: statistics_schema.mjs schema|index --tablespace=NAME | grants|recover');
}
if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url){
 const [action,option,...extra]=process.argv.slice(2),tablespace=option?.match(/^--tablespace=([a-z_][a-z0-9_]*)$/)?.[1];
 if(extra.length||option&&!tablespace||!['schema','index'].includes(action)&&option)throw Error('usage: statistics_schema.mjs schema|index --tablespace=NAME | grants|recover');
 process.stdout.write(statisticsSchemaSql(action,{tablespace}));
}
