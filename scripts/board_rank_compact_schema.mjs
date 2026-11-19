// SPDX-License-Identifier: AGPL-3.0-or-later
// Installing compact storage keeps all existing rows and exact key headers readable.
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';

export function boardRankCompactDDL(role){
 if(!/^[a-z_][a-z0-9_]{0,62}$/.test(role))throw Error('invalid ranking reader role');
 return readFileSync(new URL('../worker/pg/board_rank_compact.sql',import.meta.url),'utf8')+`
GRANT SELECT,INSERT,DELETE ON state_board_rank_descriptions TO ${role};
GRANT SELECT,INSERT,UPDATE,DELETE ON state_board_rank_input_sets TO ${role};
GRANT SELECT,DELETE ON state_board_rank_legacy_dependencies TO ${role};
GRANT SELECT ON state_board_rank_dependencies TO ${role};
GRANT USAGE ON SEQUENCE state_board_rank_descriptions_id_seq TO ${role};
`;
}
export function boardRankCompactUpgrade({schema='public',readerRole='wfsim_board_read',tablespace='wfsim_data'}={}){
 for(const name of [schema,tablespace])if(!/^[a-z_][a-z0-9_]{0,62}$/.test(name))throw Error('invalid installation name');
 return `BEGIN;
SET LOCAL lock_timeout='1s';
SET LOCAL statement_timeout='15s';
SET LOCAL default_tablespace='${tablespace}';
SELECT pg_catalog.set_config('search_path','${schema},pg_catalog,pg_temp',true);
DO $$ BEGIN
 IF (SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid='entity_weapon_builds'::regclass) IS DISTINCT FROM current_user THEN
  RAISE EXCEPTION 'compact ranking storage requires canonical owner';END IF;
END $$;
${boardRankCompactDDL(readerRole)}
COMMIT;`;
}

export function boardRankCompactRollback({schema='public',readerRole='wfsim_board_read'}={}){
 for(const value of [schema,readerRole])if(!/^[a-z_][a-z0-9_]{0,62}$/.test(value))throw Error('invalid ranking recovery name');
 return `BEGIN;
SET LOCAL lock_timeout='1s';SET LOCAL statement_timeout='15s';
SELECT pg_catalog.set_config('search_path','${schema},pg_catalog,pg_temp',true);
LOCK TABLE state_board_rank_sources,state_board_rank_input_sets IN ACCESS EXCLUSIVE MODE;
DO $$ BEGIN
 IF (SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid='entity_weapon_builds'::regclass) IS DISTINCT FROM current_user THEN
  RAISE EXCEPTION 'compact ranking recovery requires canonical owner'; END IF;
 IF EXISTS(SELECT 1 FROM state_board_rank_input_sets) THEN RAISE EXCEPTION 'expand compact inputs before rollback';END IF;
END $$;
DROP FUNCTION board_rank_expand_inputs(integer);
DROP VIEW state_board_rank_dependencies;
ALTER TABLE state_board_rank_legacy_dependencies RENAME TO state_board_rank_dependencies;
GRANT SELECT,INSERT,UPDATE,DELETE ON state_board_rank_dependencies TO ${readerRole};
DROP TABLE state_board_rank_input_sets,state_board_rank_descriptions;
COMMIT;`;
}

if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href){
 const [action='upgrade',...extra]=process.argv.slice(2);
 if(extra.length||!['upgrade','rollback'].includes(action))throw Error('use upgrade or rollback');
 process.stdout.write(action==='rollback'?boardRankCompactRollback():boardRankCompactUpgrade());
}
