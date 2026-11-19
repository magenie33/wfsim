// SPDX-License-Identifier: AGPL-3.0-or-later
// Install the request handoff independently on the canonical owner's existing database.
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';

export function boardRequestedResultDDL(readerRole) {
 if(typeof readerRole!=='string'||!/^[a-z_][a-z0-9_]{0,62}$/.test(readerRole))throw Error('invalid ranking reader role');
 return readFileSync(new URL('../worker/pg/board_requested_results.sql',import.meta.url),'utf8')+
  'GRANT EXECUTE ON FUNCTION board_requested_result_take(uuid,text) TO '+readerRole+';\n';
}
export function boardRequestedResultUpgrade({schema='public',readerRole='wfsim_board_read',tablespace='wfsim_data'}={}) {
 for(const name of [schema,tablespace])if(!/^[a-z_][a-z0-9_]{0,62}$/.test(name))throw Error('invalid installation name');
 return `BEGIN;
SET LOCAL lock_timeout='1s';
SET LOCAL statement_timeout='10s';
SET LOCAL default_tablespace='${tablespace}';
SELECT pg_catalog.set_config('search_path','${schema},pg_catalog,pg_temp',true);
DO $$ BEGIN
 IF (SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid='entity_weapon_builds'::regclass) IS DISTINCT FROM current_user THEN
  RAISE EXCEPTION 'requested ranking handoff requires the canonical storage owner'; END IF;
END $$;
${boardRequestedResultDDL(readerRole)}
COMMIT;
`;
}
if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url)process.stdout.write(boardRequestedResultUpgrade());
