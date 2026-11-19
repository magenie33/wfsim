// SPDX-License-Identifier: AGPL-3.0-or-later
// The storage owner enables measured-result wakeups at a fresh transaction boundary.
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';

export function boardRankRecentDDL(role){
 if(!/^[a-z_][a-z0-9_]{0,62}$/.test(role))throw Error('invalid ranking reader role');
 return readFileSync(new URL('../worker/pg/board_rank_recent.sql',import.meta.url),'utf8')+
  'GRANT SELECT,DELETE,UPDATE(queued_at) ON state_board_rank_recent_inputs TO '+role+';\n'+
  'GRANT EXECUTE ON FUNCTION board_rank_discover_scope(uuid,text,text) TO '+role+';\n';
}
export function boardRankRecentUpgrade({schema='public',readerRole='wfsim_board_read',tablespace='wfsim_data'}={}){
 for(const name of [schema,tablespace])if(!/^[a-z_][a-z0-9_]{0,62}$/.test(name))throw Error('invalid ranking installation name');
 return `BEGIN;
SET LOCAL lock_timeout='1s';
SET LOCAL statement_timeout='10s';
SET LOCAL default_tablespace='${tablespace}';
SELECT pg_catalog.set_config('search_path','${schema},pg_catalog,pg_temp',true);
DO $$ BEGIN
 IF (SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid='entity_weapon_builds'::regclass) IS DISTINCT FROM current_user THEN
  RAISE EXCEPTION 'ranking wakeup installation requires the canonical storage owner'; END IF;
END $$;
${boardRankRecentDDL(readerRole)}
COMMIT;
`;
}
if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url)process.stdout.write(boardRankRecentUpgrade());
