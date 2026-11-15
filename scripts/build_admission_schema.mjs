// SPDX-License-Identifier: AGPL-3.0-or-later
// Install the stage from its authoritative SQL without building historical indexes or rewriting old nodes.
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';

export function buildAdmissionSchemaSql({role='wfsim',tablespace=null}={}) {
  for(const value of [role,tablespace])if(value!==null&&!/^[a-z_][a-z0-9_]*$/.test(value))throw Error('invalid schema installation identifier');
  const workflows=readFileSync(new URL('../worker/pg/build_workflows.sql',import.meta.url),'utf8');
  const schema=readFileSync(new URL('../worker/pg/schema.sql',import.meta.url),'utf8');
  const admission=schema.match(/CREATE OR REPLACE FUNCTION compute_admit_tasks[^]*?END \$\$;/)?.[0];
  if(!admission?.includes("coalesce((source.value->>'position')::bigint"))throw Error('missing captured queue position support');
  const indexes=[...workflows.matchAll(/CREATE (?:UNIQUE )?INDEX IF NOT EXISTS ([a-z_]+) ON ([a-z_]+)/g)]
    .filter(match=>!['state_build_admission_work','state_build_inbox_positions'].includes(match[2])).map(match=>match[1]);
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
END $prerequisite$;
${workflows}
${admission}
DO $grant$ BEGIN
 IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname='wfsim_factoryd') THEN
  GRANT SELECT ON state_build_admission_work TO wfsim_factoryd;
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
  const args=process.argv.slice(2);let tablespace=null;
  if(args.length) {
    if(args.length!==2||args[0]!=='--tablespace')throw Error('usage: build_admission_schema.mjs [--tablespace name]');
    tablespace=args[1];
  }
  process.stdout.write(buildAdmissionSchemaSql({tablespace}));
}
