// SPDX-License-Identifier: AGPL-3.0-or-later
// Print the bounded board-registration upgrade without rebuilding task identities.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export function boardAdmissionSchemaSql() {
  const sql = readFileSync(new URL('../worker/pg/board_admission.sql', import.meta.url),'utf8');
  return `BEGIN;\nSET LOCAL ROLE wfsim;\nSET LOCAL lock_timeout='5s';\n${sql}\nCOMMIT;\n`;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  process.stdout.write(boardAdmissionSchemaSql());
}
