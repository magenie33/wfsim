// SPDX-License-Identifier: AGPL-3.0-or-later
// The canonical storage owner installs a fresh requested-source boundary transactionally.
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';

export function boardRequestedSchema({httpRole='wfsim',runnerRole='wfsim_board_requested',readerRole='wfsim_board_read'}={}) {
 for(const role of [httpRole,runnerRole,readerRole])if(!/^[a-z_][a-z0-9_]{0,62}$/.test(role))throw Error('invalid requested board role');
 const body=readFileSync(new URL('../worker/pg/board_requests.sql',import.meta.url),'utf8');
 return `BEGIN;
SET LOCAL lock_timeout='1s';
SET LOCAL statement_timeout='30s';
SET LOCAL default_tablespace='wfsim_data';
SELECT pg_catalog.set_config('search_path',pg_catalog.quote_ident(pg_catalog.current_schema())||',pg_catalog,pg_temp',true);
DO $$ BEGIN
 IF (SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid='entity_weapon_builds'::regclass) IS DISTINCT FROM current_user THEN
  RAISE EXCEPTION 'requested board installation requires the canonical storage owner'; END IF;
 IF to_regclass('entity_board_requests') IS NOT NULL OR to_regclass('state_board_requests') IS NOT NULL THEN
  RAISE EXCEPTION 'requested board schema exists; an explicit upgrade is required'; END IF;
 IF NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname='${runnerRole}') THEN
  RAISE EXCEPTION 'provision the requested board runner role before installation'; END IF;
END $$;
${body}
${readFileSync(new URL('../worker/pg/board_requested_partitions.sql',import.meta.url),'utf8')}
${readFileSync(new URL('../worker/pg/board_requested_reviews.sql',import.meta.url),'utf8')}
GRANT EXECUTE ON FUNCTION board_request_submit(text,text,text,text) TO ${httpRole};
GRANT EXECUTE ON FUNCTION board_requested_review_fresh(text,text,text,text) TO ${httpRole};
GRANT SELECT ON entity_board_requests,state_board_requests TO ${httpRole};
GRANT EXECUTE ON FUNCTION board_request_claim(uuid),board_request_stage(text,uuid,text),board_request_admit(text,uuid),board_request_retry(text,uuid,text,boolean) TO ${runnerRole};
GRANT EXECUTE ON FUNCTION board_requested_partition_enqueue(text,uuid),board_requested_partition_selected(text,text,boolean),board_requested_partition_activate(text,text,text,uuid) TO ${readerRole};
COMMIT;
`;
}
if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url)process.stdout.write(boardRequestedSchema());
