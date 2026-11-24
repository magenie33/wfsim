// SPDX-License-Identifier: AGPL-3.0-or-later
// Print only the additive RCS pack contract and its sync procedure.
import { readFileSync } from "node:fs";

const schema = readFileSync(new URL("../worker/pg/schema.sql", import.meta.url), "utf8");
const sync = readFileSync(new URL("../worker/pg/sync.sql", import.meta.url), "utf8");
const statements = [
  schema.match(/CREATE TABLE IF NOT EXISTS state_compute_rcs_prices[^]*?compute_rcs_prices_computation[^;]+;/)?.[0],
  ...["factory_take_pack_rcs", "factory_lease_pack_rcs"].map(name =>
    sync.match(new RegExp(`CREATE OR REPLACE FUNCTION ${name}\\([^]*?\\$\\$;`))?.[0]),
  sync.match(/CREATE OR REPLACE PROCEDURE board_sync_steps\([^]*?\$\$;/)?.[0],
];
if (statements.some(statement => !statement)) throw new Error("incomplete RCS dispatch contract");
console.log("BEGIN;\nSET LOCAL ROLE wfsim;\nSET LOCAL lock_timeout='2s';\nSET LOCAL statement_timeout='20s';");
console.log(statements.join("\n"));
console.log("COMMIT;");
