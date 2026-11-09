// SPDX-License-Identifier: AGPL-3.0-or-later
// Print the additive task-delivery upgrade from its authoritative schema.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { computationSchemaSql } from "./computation_schema.mjs";

export function taskSchemaSql() {
  const schema = readFileSync(new URL("../worker/pg/schema.sql", import.meta.url), "utf8");
  const sync = readFileSync(new URL("../worker/pg/sync.sql", import.meta.url), "utf8");
  const incremental = readFileSync(new URL("../worker/pg/inventory.sql", import.meta.url), "utf8");
  const columns = ["demands delivery_at", "answers credit_work", "answers credit_at"].map(pair => {
    const [table, column] = pair.split(" ");
    return schema.match(new RegExp(`ALTER TABLE ${table} ADD COLUMN IF NOT EXISTS ${column}[^;]+;`))?.[0];
  });
  const indexes = ["demands_delivery", "answers_credit_pending"].map(name =>
    schema.match(new RegExp(`CREATE INDEX IF NOT EXISTS ${name}\\b[^;]+;`))?.[0]);
  const guard = sync.match(/CREATE OR REPLACE FUNCTION factory_delivery_guard\([^]*?END \$\$;/)?.[0];
  const retention = sync.match(/CREATE OR REPLACE FUNCTION factory_past_safe\([^]*?\$\$;/)?.[0];
  const inventory = sync.match(/SELECT set_config\('search_path'[^\n]+\nCREATE OR REPLACE FUNCTION factory_inventory_rows\([^]*?\$\$;\nREVOKE[^;]+;/)?.[0];
  if (columns.some(x => !x) || indexes.some(x => !x) || !guard || !retention || !inventory) throw new Error("incomplete task-delivery schema");
  return `SET ROLE wfsim;\nSET lock_timeout = '5s';\n${columns.join("\n")}\n${indexes.map(sql => sql.replace("CREATE INDEX ", "CREATE INDEX CONCURRENTLY ")).join("\n")}\n${guard}\n${retention}\n${inventory}\n${incremental}\nDO $grant$ BEGIN
    IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname='wfsim_pipeline') THEN
      REVOKE ALL ON inventory_dirty,inventory_state,inventory_epochs FROM wfsim_pipeline;
      REVOKE ALL ON SEQUENCE inventory_revision,inventory_generation,inventory_epoch_token FROM wfsim_pipeline;
      GRANT EXECUTE ON FUNCTION factory_inventory_rows(text,text[]) TO wfsim_pipeline;
      GRANT EXECUTE ON FUNCTION inventory_status(),inventory_begin(text,text,text),inventory_changes(bigint),
        inventory_inputs(text,text,text),inventory_cohort_rows(text,text,text),inventory_ack(text,text),inventory_publish(text),
        inventory_epoch_begin(text),inventory_epoch_changes(text),inventory_epoch_open(text),inventory_epoch_seal(text,bigint,text),inventory_epoch_status(text),
        inventory_publish_cut(text,text) TO wfsim_pipeline;
    END IF;
  END $grant$;\n${computationSchemaSql()}`;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url)
  process.stdout.write(taskSchemaSql());
