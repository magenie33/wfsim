// SPDX-License-Identifier: AGPL-3.0-or-later
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const identifier = /^[a-z_][a-z0-9_]*$/;

export function boardCoverageSchemaSql(tablespace, schema = 'public') {
  if (!identifier.test(tablespace ?? '')) throw Error('choose an existing, capacity-qualified tablespace explicitly');
  if (!identifier.test(schema)) throw Error('choose the existing library schema');
  const read = name => readFileSync(new URL('../worker/pg/' + name + '.sql', import.meta.url), 'utf8');
  const signature = 'build_workflow_admit_nodes(text,bigint,text)';
  const workflow = read('build_workflows').match(/CREATE OR REPLACE FUNCTION build_workflow_admit_nodes\(text,bigint,text\)[\s\S]*?\nEND \$\$;/);
  if (!workflow) throw Error('missing workflow admission function');
  const qualified = '"' + schema + '".' + signature;
  const replacement = workflow[0].replace(signature, qualified);
  return `BEGIN;
SET LOCAL ROLE wfsim;
SET LOCAL lock_timeout='5s';
SET LOCAL default_tablespace='${tablespace}';
SET LOCAL search_path=pg_catalog,pg_temp;
DO $coverage_install$
DECLARE
  routine regprocedure := to_regprocedure('${qualified}');
  library_schema text := '${schema}';
  before jsonb;
  after jsonb;
  configuration text[];
  existing_path text;
BEGIN
  SELECT to_jsonb(p)-'prosrc',p.proconfig INTO STRICT before,configuration
    FROM pg_catalog.pg_proc p JOIN pg_catalog.pg_namespace n ON n.oid=p.pronamespace
    WHERE p.oid=routine AND n.nspname=library_schema AND p.prosecdef;
  IF cardinality(configuration) IS DISTINCT FROM 1 OR left(configuration[1],12) IS DISTINCT FROM 'search_path=' THEN
    RAISE EXCEPTION 'unsupported workflow admission function configuration';
  END IF;
  existing_path:=substring(configuration[1] from 13);
  IF regexp_replace(existing_path,'[[:space:]]','','g') IS DISTINCT FROM quote_ident(library_schema)||',pg_catalog,pg_temp' THEN
    RAISE EXCEPTION 'unsupported workflow admission function search path';
  END IF;
  PERFORM pg_catalog.set_config('search_path',existing_path,true);
  EXECUTE $coverage_workflow$${replacement}$coverage_workflow$;
  SELECT to_jsonb(p)-'prosrc' INTO STRICT after FROM pg_catalog.pg_proc p WHERE p.oid=routine;
  IF before IS DISTINCT FROM after THEN
    RAISE EXCEPTION 'workflow admission replacement changed its catalog attributes';
  END IF;
END
$coverage_install$;
${read('board_coverage')}
COMMIT;
`;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [option, ...extra] = process.argv.slice(2);
  const tablespace = option?.match(/^--tablespace=([a-z_][a-z0-9_]*)$/)?.[1];
  if (!tablespace || extra.length) throw Error('usage: board_coverage_schema.mjs --tablespace=NAME');
  process.stdout.write(boardCoverageSchemaSql(tablespace));
}
