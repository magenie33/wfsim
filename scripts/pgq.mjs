// SPDX-License-Identifier: AGPL-3.0-or-later
// ONE STATEMENT TO THE DATABASES, for the shell scripts (scripts/db.sh): a body
// `{ sql, params }` on stdin, the answer scripts/db.mjs gives on stdout, which a
// script reads as `.result[0].results`. Exit 1 on a refusal, its errors in the
// answer.
//
//   echo '{"sql":"SELECT count(*) AS n FROM builds"}' | node scripts/pgq.mjs [library|accounts]
//
// `--lines`: one body a line in, one answer a line out, over one connection
// until stdin ends — what `db.sh` keeps open so a paged read pays for one
// process and one connect, not one a page.
import { createInterface } from "node:readline";
import { answer, close } from "./db.mjs";

const lines = process.argv[2] === "--lines";
const database = process.argv[lines ? 3 : 2] || "library";
if (lines) {
  for await (const line of createInterface({ input: process.stdin, crlfDelay: Infinity })) {
    if (!line.trim()) continue;
    let j;
    try { const body = JSON.parse(line); j = await answer(body.sql, body.params || [], database); }
    catch (e) { j = { success: false, errors: [{ code: "", message: String(e && e.message || e) }] }; }
    process.stdout.write(JSON.stringify(j) + "\n");
  }
  await close();
} else {
  let text = "";
  for await (const chunk of process.stdin) text += chunk;
  const body = JSON.parse(text);
  const j = await answer(body.sql, body.params || [], database);
  await close();
  process.stdout.write(JSON.stringify(j));
  process.exitCode = j.success ? 0 : 1;
}
