// SPDX-License-Identifier: AGPL-3.0-or-later
// ONE STATEMENT TO THE DATABASES, for the shell scripts (scripts/db.sh): a body
// `{ sql, params }` on stdin, the answer scripts/db.mjs gives on stdout, which a
// script reads as `.result[0].results`. Exit 1 on a refusal, its errors in the
// answer.
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
