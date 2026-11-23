// SPDX-License-Identifier: AGPL-3.0-or-later
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const identifier = /^[a-z_][a-z0-9_]*$/;
export const COMPUTE_ORDER_IMPORT_ROWS = 1024;
export function computeOrderImportSchema(tablespace, schema = 'public') {
  if (!identifier.test(tablespace || '') || !identifier.test(schema)) throw Error('choose an existing library schema and tablespace');
  const read = name => readFileSync(new URL(`../worker/pg/${name}.sql`, import.meta.url), 'utf8');
  const name = 'board_coverage_admit(batch jsonb, epoch text, engine text)';
  const coverage = read('board_coverage').split(`CREATE OR REPLACE FUNCTION ${name}`, 2)[1]?.split('END $$;', 1)[0];
  const width = 'BETWEEN 1 AND 32';
  if (!coverage?.startsWith(' RETURNS void') || coverage.split(width).length !== 3)
    throw Error('the canonical coverage batch contract changed');
  return `BEGIN;
SET LOCAL ROLE wfsim;
SET LOCAL search_path="${schema}",pg_catalog,pg_temp;
SET LOCAL lock_timeout='5s';
SET LOCAL default_tablespace='${tablespace}';
${read('compute_order_import')}
CREATE OR REPLACE FUNCTION ${name.replace('board_coverage_admit', 'board_coverage_admit_import')}${coverage.replaceAll(width, `BETWEEN 1 AND ${COMPUTE_ORDER_IMPORT_ROWS}`)}END $$;
REVOKE ALL ON FUNCTION board_coverage_admit_import(jsonb,text,text) FROM PUBLIC;
COMMIT;
`;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [option, ...extra] = process.argv.slice(2);
  const tablespace = option?.match(/^--tablespace=([a-z_][a-z0-9_]*)$/)?.[1];
  if (!tablespace || extra.length) throw Error('usage: compute_order_import_schema.mjs --tablespace=NAME');
  process.stdout.write(computeOrderImportSchema(tablespace));
}
