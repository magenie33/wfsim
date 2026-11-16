// SPDX-License-Identifier: AGPL-3.0-or-later
// The canonical storage owner installs a fresh requested-source boundary transactionally.
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';

export function boardRequestedSchema({httpRole='wfsim',runnerRole='wfsim_board_requested'}={}) {
 for(const role of [httpRole,runnerRole])if(!/^[a-z_][a-z0-9_]{0,62}$/.test(role))throw Error('invalid requested board role');
 const body=readFileSync(new URL('../worker/pg/board_requests.sql',import.meta.url),'utf8');
 return `BEGIN;
SET LOCAL lock_timeout='1s';
SET LOCAL statement_timeout='30s';
SET LOCAL default_tablespace='wfsim_data';
DO $$ BEGIN
 IF to_regclass('entity_board_requests') IS NOT NULL OR to_regclass('state_board_requests') IS NOT NULL THEN
  RAISE EXCEPTION 'requested board schema exists; an explicit upgrade is required'; END IF;
 IF NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname='${runnerRole}') THEN
  RAISE EXCEPTION 'provision the requested board runner role before installation'; END IF;
END $$;
${body}
GRANT EXECUTE ON FUNCTION board_request_submit(text,text,text,text) TO ${httpRole};
GRANT SELECT ON entity_board_requests,state_board_requests TO ${httpRole};
GRANT EXECUTE ON FUNCTION board_request_claim(uuid),board_request_stage(text,uuid,text),board_request_admit(text,uuid),board_request_retry(text,uuid,text,boolean) TO ${runnerRole};
COMMIT;
`;
}
if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url)process.stdout.write(boardRequestedSchema());
