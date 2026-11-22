// SPDX-License-Identifier: AGPL-3.0-or-later
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

export function boardCoverageSchemaSql(tablespace) {
  if (!/^[a-z_][a-z0-9_]*$/.test(tablespace ?? '')) {
    throw Error('choose an existing, capacity-qualified tablespace explicitly');
  }
  const read = name => readFileSync(new URL('../worker/pg/' + name + '.sql', import.meta.url), 'utf8');
  const workflow = read('build_workflows').match(/CREATE OR REPLACE FUNCTION build_workflow_admit_nodes\(text,bigint,text\)[\s\S]*?\nEND \$\$;/);
  if (!workflow) throw Error('missing workflow admission function');
  return `BEGIN;
SET LOCAL ROLE wfsim;
SET LOCAL lock_timeout='5s';
SET LOCAL default_tablespace='${tablespace}';
${workflow[0]}
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
