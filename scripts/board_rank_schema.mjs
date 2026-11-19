// SPDX-License-Identifier: AGPL-3.0-or-later
// The canonical owner installs derived storage without changing producer identities or controls.
import {boardRankCompactDDL} from './board_rank_compact_schema.mjs';
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
export function boardRankSchema({schema='public',readerRole='wfsim_board_read',httpRole='wfsim',tablespace='wfsim_data'}={}) {
 for(const value of [schema,readerRole,httpRole,tablespace])if(!/^[a-z_][a-z0-9_]{0,62}$/.test(value))throw Error('invalid ranking installation name');
 const tables=['state_board_rank_partitions','state_board_rank_sources','state_board_rank_route_inputs',
  'state_board_rank_rows','state_board_rank_winners','state_board_rank_dirty','state_board_rank_baseline_obligations','state_board_rank_publication_ready',
  'state_board_rank_generations','state_board_rank_snapshot_rows','record_board_rank_snapshot_refs'];
 return `BEGIN;
SET LOCAL lock_timeout='1s';
SET LOCAL statement_timeout='30s';
SET LOCAL default_tablespace='${tablespace}';
SELECT pg_catalog.set_config('search_path','${schema},pg_catalog,pg_temp',true);
DO $$ BEGIN
 IF (SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid='entity_weapon_builds'::regclass) IS DISTINCT FROM current_user THEN
  RAISE EXCEPTION 'ranking installation requires the canonical storage owner';END IF;
 IF to_regclass('state_board_rank_partitions') IS NOT NULL THEN RAISE EXCEPTION 'ranking projection exists; explicit upgrade required';END IF;
 IF NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname='${readerRole}') THEN RAISE EXCEPTION 'provision ranking reader first';END IF;
END $$;
${readFileSync(new URL('../worker/pg/board_rank.sql',import.meta.url),'utf8')}
${boardRankCompactDDL(readerRole)}
${readFileSync(new URL('../worker/pg/board_rank_pages.sql',import.meta.url),'utf8')}
GRANT SELECT,INSERT,UPDATE,DELETE ON ${tables.join(',')} TO ${readerRole};
GRANT SELECT(scope,key,version,removed) ON state_board_input_mailbox TO ${readerRole};
GRANT USAGE ON SEQUENCE board_rank_generation TO ${readerRole};
GRANT EXECUTE ON FUNCTION board_rank_source_lock(text),board_input_timestamp(timestamptz) TO ${readerRole};
GRANT EXECUTE ON FUNCTION board_read_page(text,text,text,bigint,bigint) TO ${httpRole};
COMMIT;
`;
}
if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url)process.stdout.write(boardRankSchema());

export function boardRankRollbackSchema({schema='public',httpRole='wfsim'}={}) {
 for(const value of [schema,httpRole])if(!/^[a-z_][a-z0-9_]{0,62}$/.test(value))throw Error('invalid ranking rollback name');
 const pages=readFileSync(new URL('../worker/pg/board_pages.sql',import.meta.url),'utf8');
 const original=pages.slice(pages.indexOf('CREATE OR REPLACE FUNCTION board_read_page('));
 return `BEGIN;
SET LOCAL lock_timeout='1s';SET LOCAL statement_timeout='30s';
SELECT pg_catalog.set_config('search_path','${schema},pg_catalog,pg_temp',true);
DO $$ BEGIN
 IF (SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid='entity_weapon_builds'::regclass) IS DISTINCT FROM current_user THEN
  RAISE EXCEPTION 'ranking rollback requires the canonical storage owner';END IF;
 IF EXISTS(SELECT 1 FROM record_board_rank_snapshot_refs) THEN RAISE EXCEPTION 'ranking revisions still retained; preserve compatible page reader';END IF;
 IF EXISTS(SELECT 1 FROM state_board_rank_partitions) OR EXISTS(SELECT 1 FROM state_board_rank_route_inputs WHERE scope<>'partition') THEN
  RAISE EXCEPTION 'ranking inputs already transferred; preserve recovery boundary';END IF;
END $$;
${original}
GRANT EXECUTE ON FUNCTION board_read_page(text,text,text,bigint,bigint) TO ${httpRole};
DROP FUNCTION board_rank_expand_inputs(integer);
DROP VIEW state_board_rank_dependencies;
DROP TABLE state_board_rank_input_sets,state_board_rank_descriptions;
DROP TABLE record_board_rank_snapshot_refs,state_board_rank_snapshot_rows,state_board_rank_generations,
 state_board_rank_publication_ready,state_board_rank_baseline_obligations,state_board_rank_dirty,
 state_board_rank_winners,state_board_rank_rows,state_board_rank_legacy_dependencies,state_board_rank_sources,
 state_board_rank_route_inputs,state_board_rank_partitions;
DROP SEQUENCE board_rank_generation;
DROP FUNCTION board_rank_source_lock(text);
COMMIT;
`;
}
