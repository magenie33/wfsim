// SPDX-License-Identifier: AGPL-3.0-or-later
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';

export function boardRankCatalogDDL(role){
 if(!/^[a-z_][a-z0-9_]{0,62}$/.test(role))throw Error('invalid ranking reader role');
 return readFileSync(new URL('../worker/pg/board_rank_catalog.sql',import.meta.url),'utf8')+
  'GRANT SELECT ON state_board_rank_catalog_inputs TO '+role+';\n'+
  'GRANT EXECUTE ON FUNCTION board_rank_catalog_activate(text,text,text,text) TO '+role+';\n';
}
if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url)
 process.stdout.write("BEGIN;\nSET LOCAL lock_timeout='1s';\nSET LOCAL default_tablespace='wfsim_data';\nSET LOCAL search_path=public,pg_catalog,pg_temp;\n"+
  boardRankCatalogDDL('wfsim_board_read')+'COMMIT;\n');
