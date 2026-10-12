// SPDX-License-Identifier: AGPL-3.0-or-later
// ONE STATEMENT TO THE DATABASES, for the shell scripts (scripts/db.sh): a D1
// query-API body `{ sql, params }` on stdin, D1's answer on stdout — so a
// script reads `.result[0].results` whichever database answered. Exit 1 on a
// refusal, its errors in the answer. scripts/db.mjs says which database.
//
//   echo '{"sql":"SELECT count(*) AS n FROM builds"}' | node scripts/pgq.mjs [library|accounts]
import { answer, close } from "./db.mjs";

let text = "";
for await (const chunk of process.stdin) text += chunk;
const body = JSON.parse(text);
const j = await answer(body.sql, body.params || [], process.argv[2] || "library");
await close();
process.stdout.write(JSON.stringify(j));
process.exitCode = j.success ? 0 : 1;
