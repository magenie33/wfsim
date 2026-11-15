// SPDX-License-Identifier: AGPL-3.0-or-later
// Install functions separately from the additive index; retained node indexes are prerequisites.
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {orderCutSchemaSql,orderCutBusinessSql} from './task_schema.mjs';

const workflowSql=()=>readFileSync(new URL('../worker/pg/build_workflows.sql',import.meta.url),'utf8');
const triggerIndex=()=>{
  const statement=workflowSql().match(/^CREATE INDEX IF NOT EXISTS build_workflow_triggers_workflow ON record_build_workflow_triggers\(workflow,id\);$/m)?.[0];
  if(!statement)throw Error('missing authoritative workflow trigger index');return statement;
};
export function buildAdmissionIndexSql({concurrently=true,tablespace=null}={}) {
  if(typeof concurrently!=='boolean' || tablespace!==null&&!/^[a-z_][a-z0-9_]*$/.test(tablespace))throw Error('invalid additive index options');
  return triggerIndex().replace('CREATE INDEX ',concurrently?'CREATE INDEX CONCURRENTLY ':'CREATE INDEX ').slice(0,-1)
    +(tablespace?` TABLESPACE "${tablespace}"`:'')+';\n';
}

export function buildAdmissionSchemaSql({role='wfsim',tablespace=null}={}) {
  for(const value of [role,tablespace])if(value!==null&&!/^[a-z_][a-z0-9_]*$/.test(value))throw Error('invalid schema installation identifier');
  const workflows=workflowSql();
  const schema=readFileSync(new URL('../worker/pg/schema.sql',import.meta.url),'utf8');
  const admission=schema.match(/CREATE OR REPLACE FUNCTION compute_admit_tasks[^]*?END \$\$;/)?.[0];
  if(!admission?.includes("coalesce((source.value->>'position')::bigint"))throw Error('missing captured queue position support');
  const indexes=[...workflows.matchAll(/CREATE (?:UNIQUE )?INDEX IF NOT EXISTS ([a-z_]+) ON ([a-z_]+)/g)]
    .filter(match=>!['state_build_admission_work','state_build_inbox_positions','record_build_workflow_triggers'].includes(match[2])).map(match=>match[1]);
  if(indexes.length!==3)throw Error('workflow index prerequisite set changed');
  const names=indexes.map(name=>`('${name}')`).join(',');
  return `BEGIN;
SET LOCAL lock_timeout='1s';SET LOCAL statement_timeout='15s';
${role?`SET LOCAL ROLE "${role}";`:''}
${tablespace?`SET LOCAL default_tablespace="${tablespace}";`:''}
DO $prerequisite$ BEGIN
 IF to_regclass('state_task_pipeline_controls') IS NULL OR to_regclass('state_build_workflow_nodes') IS NULL
  OR to_regprocedure('build_intake_batch(text)') IS NULL THEN RAISE EXCEPTION 'base task and workflow schema required';END IF;
 IF EXISTS(SELECT 1 FROM(VALUES ${names}) wanted(name) LEFT JOIN pg_class c ON c.relname=wanted.name
  AND c.relnamespace=(SELECT oid FROM pg_namespace WHERE nspname=current_schema()) LEFT JOIN pg_index i ON i.indexrelid=c.oid
  WHERE c.oid IS NULL OR NOT i.indisvalid) THEN RAISE EXCEPTION 'existing workflow indexes must already be valid';END IF;
 IF NOT EXISTS(SELECT 1 FROM pg_class c JOIN pg_index i ON i.indexrelid=c.oid
  WHERE c.relname='build_workflow_triggers_workflow' AND c.relnamespace=(SELECT oid FROM pg_namespace WHERE nspname=current_schema())
   AND i.indrelid='record_build_workflow_triggers'::regclass AND i.indisvalid AND i.indisready AND NOT i.indisunique
   AND c.relam=(SELECT oid FROM pg_am WHERE amname='btree')
   AND i.indnatts=2 AND i.indnkeyatts=2 AND i.indpred IS NULL AND i.indexprs IS NULL
   AND pg_get_indexdef(i.indexrelid,1,true)='workflow' AND pg_get_indexdef(i.indexrelid,2,true)='id') THEN
  RAISE EXCEPTION 'additive workflow trigger index must already be valid; install --indexes output first';END IF;
END $prerequisite$;
${orderCutSchemaSql()}
${orderCutBusinessSql()}
${workflows.replace(triggerIndex(),'')}
${admission}
DO $grant$ BEGIN
 IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname='wfsim_factoryd') THEN
  GRANT SELECT ON state_build_admission_work,state_build_effective_admission_work,state_compute_effective_open,state_compute_effective_work,state_board_effective_pending_work TO wfsim_factoryd;
  GRANT EXECUTE ON FUNCTION build_admission_claim(text,text),build_admission_run(text,text,text,bigint,text),
   build_admission_retry(text,text,text,text,text) TO wfsim_factoryd;
 END IF;
 IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname='wfsim_pipeline') THEN
  GRANT SELECT ON state_build_inbox_positions TO wfsim_pipeline;
  GRANT EXECUTE ON FUNCTION build_intake_batch(text),build_spend_submissions(text) TO wfsim_pipeline;
 END IF;
END $grant$;
COMMIT;
`;
}
if(process.argv[1]&&pathToFileURL(resolve(process.argv[1])).href===import.meta.url) {
  const args=process.argv.slice(2);let tablespace=null;const indexes=args[0]==='--indexes';if(indexes)args.shift();
  if(args.length) {
    if(args.length!==2||args[0]!=='--tablespace')throw Error('usage: build_admission_schema.mjs [--indexes] [--tablespace name]');
    tablespace=args[1];
  }
  process.stdout.write(indexes?buildAdmissionIndexSql({tablespace}):buildAdmissionSchemaSql({tablespace}));
}
