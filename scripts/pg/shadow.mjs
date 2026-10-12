// THE SHADOW CHECK: any node check that runs the worker on `node:sqlite` runs
// it on Postgres too, statement by statement, with no change to the check.
//
//   node --import ./scripts/pg/shadow.mjs scripts/check_accounts.mjs
//
// `node:sqlite` is swapped for a database that answers every statement from
// SQLite as before and repeats it, in the same order, on an in-memory Postgres
// (PGlite) loaded with worker/pg/*.sql — in the Postgres text the worker states
// beside it (`pick`) where one is given. Each statement the WORKER issued is
// compared: its rows (as a set where the query sets no order), its count of
// changed rows, and whether it failed. A check's own setup and assertions are
// repeated so the two databases hold the same rows, and are never judged. A
// difference fails the run, after the check's own verdict.
import { register } from "node:module";

// THE POSTGRES TEXT OF WHAT `pick` CHOSE SQLite's for. A whole statement carries
// it; a piece of one is wrapped in numbered SQL comments, which SQLite ignores
// and which survive being pasted into the statement around it.
const pieces = new Map(), numbers = new Map();
globalThis.__WFSIM_PG_SHADOW__ = (lite, pg) => {
  const key = `${lite}\u0000${pg}`;
  if (!numbers.has(key)) { numbers.set(key, numbers.size); pieces.set(numbers.size - 1, pg); }
  const n = numbers.get(key);
  return /^\s*(SELECT|INSERT|UPDATE|DELETE|WITH)\b/i.test(lite) ? Object.assign(new String(lite), { pg }) : `/*<${n}*/${lite}/*${n}>*/`;
};
globalThis.__wfsimPgText = (sql) => {
  let s = String((sql && sql.pg) ?? sql), was;
  do { was = s; s = s.replace(/\/\*<(\d+)\*\/[\s\S]*?\/\*\1>\*\//g, (_, n) => pieces.get(Number(n))); } while (s !== was);
  return s;
};
register(new URL("./shadow_hooks.mjs", import.meta.url));

const realExit = process.exit.bind(process);
let reported = false;
async function finish(code) {
  if (reported) return realExit(code);
  reported = true;
  const verdict = globalThis.__wfsimShadow ? await globalThis.__wfsimShadow() : 0;
  realExit(code || verdict ? Math.max(code || 0, verdict) : 0);
}
process.exit = (code) => { finish(code ?? process.exitCode ?? 0); };
process.on("beforeExit", () => { if (!reported) finish(process.exitCode ?? 0); });
