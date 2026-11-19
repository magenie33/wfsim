// SPDX-License-Identifier: AGPL-3.0-or-later
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
export function prioritySchema(schema='public'){
 if(!/^[a-z_][a-z0-9_]{0,62}$/.test(schema))throw Error('invalid schema');
 const ddl=readFileSync(new URL('../worker/pg/board_requested_partitions.sql',import.meta.url),'utf8');
 const start=ddl.indexOf('CREATE OR REPLACE FUNCTION board_requested_partition_selected('),end=ddl.indexOf('CREATE FUNCTION board_requested_partition_activate(',start);
 const recent=readFileSync(new URL('../worker/pg/board_rank_recent.sql',import.meta.url),'utf8');
 const capture=recent.indexOf('CREATE OR REPLACE FUNCTION board_rank_capture_recent_measurement()'),stop=recent.indexOf('CREATE TRIGGER board_rank_recent_measurement',capture);
 if(start<0||end<0||capture<0||stop<0)throw Error('missing requested qualification function');
 return `BEGIN;
SET LOCAL lock_timeout='1s';
SELECT pg_catalog.set_config('search_path','${schema},pg_catalog,pg_temp',true);
DO $$ BEGIN
 IF (SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid='state_board_public_heads'::regclass) IS DISTINCT FROM current_user THEN
  RAISE EXCEPTION 'priority installation requires the publication owner';
 END IF;
END $$;
${ddl.slice(start,end)}
ALTER TABLE state_board_rank_new_scopes ADD COLUMN IF NOT EXISTS last_measured_at text COLLATE "C";
CREATE INDEX IF NOT EXISTS board_rank_new_scopes_latest ON state_board_rank_new_scopes(coalesce(last_measured_at,queued_at),weapon,ruler);
${recent.slice(capture,stop)}
CREATE INDEX IF NOT EXISTS board_requested_unactivated_position ON state_board_requested_partitions(position) WHERE activated_at IS NULL;
COMMIT;
`;
}
if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href)process.stdout.write(prioritySchema());
