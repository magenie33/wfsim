// SPDX-License-Identifier: AGPL-3.0-or-later
// The first install adds isolated ranking state without seeding historical tasks.
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';

const read=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8');
const catalogGrant=httpRole=>`GRANT EXECUTE ON FUNCTION board_read_catalog(text,text,text,bigint),
  board_read_catalog(text,text,text,bigint,text) TO "${httpRole}";`;
function validateRole(httpRole) {
 if(typeof httpRole!=='string'||!/^[a-z][a-z0-9_]{0,62}$/.test(httpRole))throw Error('an existing HTTP database role is required');
}

export function boardReadSchema(httpRole) {
 validateRole(httpRole);
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
${catalogGrant(httpRole)}
GRANT EXECUTE ON FUNCTION board_read_page(text,text,text,bigint,bigint) TO "${httpRole}";
COMMIT;
`;
}

export function boardCatalogUpgrade(httpRole) {
 validateRole(httpRole);
 const source=read('worker/pg/board_pages.sql');
 const start=source.indexOf('CREATE OR REPLACE FUNCTION board_read_catalog(');
 const end=source.indexOf('CREATE OR REPLACE FUNCTION board_read_page(');
 if(start<0||end<=start)throw Error('board catalog definitions are missing');
 return `BEGIN;
SET LOCAL lock_timeout='1s';
SET LOCAL statement_timeout='30s';
SELECT set_config('search_path',quote_ident(current_schema())||',pg_catalog,pg_temp',true);
DO $$BEGIN
 IF (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid=to_regprocedure('board_read_catalog(text,text,text,bigint)'))
  IS DISTINCT FROM current_user THEN RAISE EXCEPTION 'board catalog owner required'; END IF;
END $$;
${source.slice(start,end)}
REVOKE ALL ON FUNCTION board_read_catalog(text,text,text,bigint,text) FROM PUBLIC;
${catalogGrant(httpRole)}
COMMIT;
`;
}
if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href){
 const index=process.argv.indexOf('--http-role');
 const install=process.argv.includes('--catalog-upgrade')?boardCatalogUpgrade:boardReadSchema;
 process.stdout.write(install(index<0?null:process.argv[index+1]));
}
