// SPDX-License-Identifier: AGPL-3.0-or-later
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

export function buildInboxSchemaSql({tablespace,schema='public',writerRole='wfsim',pipelineRole='wfsim_pipeline'}={}) {
  for(const value of [tablespace,schema,writerRole,pipelineRole])
    if(!/^[a-z_][a-z0-9_]{0,62}$/.test(value||''))throw Error('choose the existing tablespace, schema and roles');
  const workflow=readFileSync(new URL('../worker/pg/build_workflows.sql',import.meta.url),'utf8');
  const start=workflow.indexOf('CREATE TABLE IF NOT EXISTS state_build_inbox_positions (');
  const end=workflow.indexOf('CREATE OR REPLACE VIEW state_build_inbox_priorities AS',start);
  const ack=workflow.match(/CREATE OR REPLACE FUNCTION build_ack_submissions\(text\)[\s\S]*?\nEND \$\$;/)?.[0];
  const assertion=workflow.match(/CREATE OR REPLACE FUNCTION build_inbox_assert_ack\(text\)[\s\S]*?\nEND \$\$;/)?.[0];
  if(start<0||end<start||!ack||!assertion)throw Error('missing authoritative inbox definitions');
  const module=readFileSync(new URL('../worker/pg/build_inbox_storage.sql',import.meta.url),'utf8');
  return `BEGIN;
SET LOCAL lock_timeout='500ms';SET LOCAL statement_timeout='15s';
SET LOCAL default_tablespace='${tablespace}';
SET LOCAL search_path="${schema}",pg_catalog,pg_temp;
DO $owner$ BEGIN
 IF (SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid='entity_weapon_builds'::regclass) IS DISTINCT FROM current_user THEN
  RAISE EXCEPTION 'install inbox storage under the existing canonical owner'; END IF;
END $owner$;
DO $install$
DECLARE routine regprocedure:=to_regprocedure('"${schema}".build_ack_submissions(text)');
 before jsonb; after jsonb; configuration text[]; existing_path text;
BEGIN
 SELECT to_jsonb(p)-'prosrc',p.proconfig INTO STRICT before,configuration FROM pg_catalog.pg_proc p WHERE p.oid=routine AND p.prosecdef;
 IF cardinality(configuration) IS DISTINCT FROM 1 OR left(configuration[1],12) IS DISTINCT FROM 'search_path=' THEN
   RAISE EXCEPTION 'unsupported inbox acknowledgement configuration'; END IF;
 existing_path:=substring(configuration[1] from 13);
 IF regexp_replace(existing_path,'[[:space:]]','','g') IS DISTINCT FROM quote_ident('${schema}')||',pg_catalog,pg_temp' THEN
   RAISE EXCEPTION 'unsupported inbox acknowledgement search path'; END IF;
 PERFORM pg_catalog.set_config('search_path',existing_path,true);
 EXECUTE $ack$${ack}$ack$;
 SELECT to_jsonb(p)-'prosrc' INTO STRICT after FROM pg_catalog.pg_proc p WHERE p.oid=routine;
 IF before IS DISTINCT FROM after THEN RAISE EXCEPTION 'inbox acknowledgement attributes changed'; END IF;
END $install$;
${workflow.slice(start,end)}
${assertion}
REVOKE ALL ON FUNCTION build_inbox_assert_ack(text) FROM PUBLIC;
ALTER TABLE record_build_inbox_payloads SET TABLESPACE "${tablespace}";
${module}
GRANT EXECUTE ON FUNCTION build_inbox_submit(text,text,text,text,text,text,bigint),build_inbox_submit_family(text,text,text,text,text) TO "${writerRole}";
GRANT EXECUTE ON FUNCTION build_inbox_family_owned(text) TO "${writerRole}";
DO $writers$ BEGIN
 IF EXISTS(SELECT FROM pg_roles WHERE rolname='wfsim_factoryd') THEN
  GRANT EXECUTE ON FUNCTION build_inbox_submit(text,text,text,text,text,text,bigint),build_inbox_submit_family(text,text,text,text,text),build_inbox_family_owned(text) TO wfsim_factoryd;
 END IF;
END $writers$;
GRANT EXECUTE ON FUNCTION build_inbox_library_count() TO "${writerRole}";
GRANT EXECUTE ON FUNCTION build_ack_submissions(text),build_inbox_assert_ack(text) TO "${pipelineRole}";
COMMIT;
`;
}

if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href) {
  const [tablespace,schema,writerRole,pipelineRole,...extra]=process.argv.slice(2);
  if(extra.length)throw Error('usage: build_inbox_schema.mjs TABLESPACE [SCHEMA WRITER_ROLE PIPELINE_ROLE]');
  process.stdout.write(buildInboxSchemaSql({tablespace,schema,writerRole,pipelineRole}));
}
