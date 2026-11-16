// SPDX-License-Identifier: AGPL-3.0-or-later
// The first install adds isolated ranking state without seeding historical tasks.
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';

export function boardReadSchema(httpRole) {
 if(typeof httpRole!=='string'||!/^[a-z][a-z0-9_]{0,62}$/.test(httpRole))throw Error('an existing HTTP database role is required');
 const read=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');
 return `BEGIN;
SET LOCAL lock_timeout='1s';
SET LOCAL statement_timeout='30s';
SET LOCAL default_tablespace='wfsim_data';
DO $$BEGIN
 IF to_regclass('state_board_input_mailbox') IS NOT NULL OR to_regclass('state_board_read_partitions') IS NOT NULL THEN
  RAISE EXCEPTION 'online ranking schema exists; an explicit upgrade is required';
 END IF;
END $$;
${['board_mailbox','board_capture','board_pages'].map(name=>read('worker/pg/'+name+'.sql')).join('\n')}
${read('deploy/board_read_grants.sql')}
GRANT EXECUTE ON FUNCTION board_read_catalog(text,text,text,bigint),board_read_page(text,text,text,bigint,bigint) TO "${httpRole}";
COMMIT;
`;
}
if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href){
 const index=process.argv.indexOf('--http-role');
 process.stdout.write(boardReadSchema(index<0?null:process.argv[index+1]));
}
