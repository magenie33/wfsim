// Print the computation upgrade from its authoritative schemas; apply before worker publication.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

export function computationSchemaSql(kind = "library") {
  const read = file => readFileSync(new URL("../worker/pg/" + file + ".sql", import.meta.url), "utf8");
  const find = (source, pattern) => {
    const found = source.match(pattern)?.[0];
    if (!found) throw new Error("Incomplete computation schema: " + pattern);
    return found;
  };
  if (kind === "accounts") {
    const source = read("accounts");
    return find(source, /ALTER TABLE accounts ADD COLUMN IF NOT EXISTS profile_id[^;]+;/) + "\n"
      + find(source, /CREATE UNIQUE INDEX IF NOT EXISTS accounts_profile_id[^;]+;/) + "\n";
  }
  if (kind !== "library") throw new Error("Choose library or accounts");
  const schema = read("schema"), library = read("library"), statements = [];
  for (const [table, column] of [["questions", "witnesses"], ["verifiers", "signatures"], ["verifier_hours", "signatures"]]) {
    statements.push(find(schema, new RegExp(`ALTER TABLE ${table} ADD COLUMN IF NOT EXISTS ${column}[^;]+;`)));
  }
  for (const table of ["computation_records", "computation_witnesses", "score_witnesses"]) {
    statements.push(find(table === "score_witnesses" ? library : schema, new RegExp(`CREATE TABLE IF NOT EXISTS ${table} \\([^]*?\\n\\);`)));
  }
  statements.push(...[...(schema + library).matchAll(/CREATE INDEX IF NOT EXISTS (?:computation_records|computation_witnesses|score_witnesses)_[^;]+;/g)].map(match => match[0]));
  statements.push(find(library, /CREATE OR REPLACE FUNCTION score_witnesses_replaced\([^]*?EXECUTE FUNCTION score_witnesses_replaced\(\);/));
  statements.push(find(schema, /UPDATE user_calculations SET expires_at=[^;]+;/));
  statements.push(readFileSync(new URL("../deploy/computation_grants.sql", import.meta.url), "utf8"));
  return statements.join("\n") + "\n";
}
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) process.stdout.write(computationSchemaSql(process.argv[2]));
