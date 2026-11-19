// SPDX-License-Identifier: AGPL-3.0-or-later
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';

export function boardPublicHeadSchema({schema='public',tablespace='wfsim_data'}={}) {
  for (const name of [schema,tablespace]) {
    if (!/^[a-z_][a-z0-9_]{0,62}$/.test(name)) throw Error('invalid publication installation name');
  }
  const ddl=name=>readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8');
  return `BEGIN;
SET LOCAL lock_timeout='1s';
SET LOCAL statement_timeout='30s';
SET LOCAL default_tablespace='${tablespace}';
SELECT pg_catalog.set_config('search_path','${schema},pg_catalog,pg_temp',true);
DO $$ BEGIN
 IF (SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid='state_board_read_partitions'::regclass) IS DISTINCT FROM current_user
  OR to_regclass('record_board_rank_snapshot_refs') IS NULL THEN
  RAISE EXCEPTION 'existing indexed publication and its storage owner required';
 END IF;
END $$;
${ddl('board_pages')}
${ddl('board_rank_pages')}
COMMIT;
`;
}
if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href)process.stdout.write(boardPublicHeadSchema());
