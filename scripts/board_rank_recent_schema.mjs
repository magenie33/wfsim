// SPDX-License-Identifier: AGPL-3.0-or-later
// The storage owner enables measured-result wakeups at a fresh transaction boundary.
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';

const recent=readFileSync(new URL('../worker/pg/board_rank_recent.sql',import.meta.url),'utf8');
const capture=recent.slice(recent.indexOf('CREATE OR REPLACE FUNCTION board_rank_capture_recent_measurement'),
 recent.indexOf('CREATE TRIGGER board_rank_recent_measurement'));
const discovery=recent.slice(recent.indexOf('CREATE OR REPLACE FUNCTION board_rank_discover_scope'));
const zones=readFileSync(new URL('../worker/pg/board_rank_zones.sql',import.meta.url),'utf8');

export function boardRankRecentDDL(role){
 if(!/^[a-z_][a-z0-9_]{0,62}$/.test(role))throw Error('invalid ranking reader role');
 return recent+zones+'GRANT SELECT ON state_board_rank_new_scopes TO '+role+';\n'+
  'GRANT SELECT,DELETE,UPDATE(queued_at) ON state_board_rank_recent_inputs TO '+role+';\n'+
  'GRANT EXECUTE ON FUNCTION board_rank_discover_scope(uuid,text,text) TO '+role+';\n';
}
export function boardRankRecentUpgrade({schema='public',readerRole='wfsim_board_read',zoneOnly=false,tablespace='wfsim_data'}={}){
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
${zoneOnly ? zones+capture+discovery+'GRANT SELECT ON state_board_rank_new_scopes TO '+readerRole+';\n' : boardRankRecentDDL(readerRole)}
COMMIT;
`;
}
if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url)
 process.stdout.write(boardRankRecentUpgrade({zoneOnly:process.argv.includes('--zones')}));
