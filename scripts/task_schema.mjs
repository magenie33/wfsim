// SPDX-License-Identifier: AGPL-3.0-or-later
// Print the additive task-delivery upgrade from its authoritative schema.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { computationSchemaSql } from "./computation_schema.mjs";

export function taskSchemaSql() {
  const schema = readFileSync(new URL("../worker/pg/schema.sql", import.meta.url), "utf8");
  const library = readFileSync(new URL("../worker/pg/library.sql", import.meta.url), "utf8");
  const sync = readFileSync(new URL("../worker/pg/sync.sql", import.meta.url), "utf8");
  const incremental = readFileSync(new URL("../worker/pg/inventory.sql", import.meta.url), "utf8");
  const columns = ["relation_task_consumers delivery_at", "record_candidate_answers credit_work", "record_candidate_answers credit_at"].map(pair => {
    const [table, column] = pair.split(" ");
    return schema.match(new RegExp(`ALTER TABLE ${table} ADD COLUMN IF NOT EXISTS ${column}[^;]+;`))?.[0];
  });
  const indexes = ["demands_delivery", "answers_credit_pending"].map(name =>
    schema.match(new RegExp(`CREATE INDEX IF NOT EXISTS ${name}\\b[^;]+;`))?.[0]);
  const identity = [
    library.match(/CREATE OR REPLACE FUNCTION protect_retired_board_score[^]*?END \$\$;/)?.[0],
    ...[...library.matchAll(/ALTER TABLE entity_measurement_reviews ADD COLUMN IF NOT EXISTS (?:task_id|result_engine)[^;]+;/g)].map(match => match[0]),
    schema.match(/CREATE TABLE IF NOT EXISTS relation_compute_task_sources[^]*?CREATE INDEX IF NOT EXISTS compute_task_sources_ref[^;]+;/)?.[0],
    ...["compute_open_task", "compute_admit_tasks", "compute_admit_user_request", "compute_select_report", "compute_source_exists", "compute_remove_untraced", "compute_rekey_active"].map(name => schema.match(new RegExp(`CREATE OR REPLACE FUNCTION ${name}\\([^]*?END \\$\\$;`))?.[0]),
    schema.match(/CREATE OR REPLACE FUNCTION compute_task_has_source[^]*?\$\$;/)?.[0],
    schema.match(/ALTER TABLE entity_user_calculation_requests ADD COLUMN IF NOT EXISTS result_engine[^;]+;/)?.[0],
    schema.match(/CREATE TABLE IF NOT EXISTS record_compute_queue_removals[^]*?\n\);/)?.[0],
    schema.match(/CREATE TABLE IF NOT EXISTS record_compute_identity_changes[^]*?\n\);/)?.[0],
    "REVOKE ALL ON FUNCTION compute_rekey_active(text,bigint,text,text,text,text) FROM PUBLIC;",
    "REVOKE ALL ON FUNCTION compute_remove_untraced(text,bigint,text) FROM PUBLIC;",
    sync.match(/CREATE OR REPLACE FUNCTION factory_task_detail[^]*?END \$\$;/)?.[0],
  ];
  if (identity.some(sql => !sql)) throw new Error("incomplete compute identity schema");
  const guard = sync.match(/CREATE OR REPLACE FUNCTION factory_delivery_guard\([^]*?END \$\$;/)?.[0];
  const retention = sync.match(/CREATE OR REPLACE FUNCTION factory_past_safe\([^]*?\$\$;/)?.[0];
  const inventory = sync.match(/SELECT set_config\('search_path'[^\n]+\nCREATE OR REPLACE FUNCTION factory_inventory_rows\([^]*?\$\$;\nREVOKE[^;]+;/)?.[0];
  if (columns.some(x => !x) || indexes.some(x => !x) || !guard || !retention || !inventory) throw new Error("incomplete task-delivery schema");
  return `SET ROLE wfsim;\nSET lock_timeout = '5s';\n${columns.join("\n")}\n${indexes.map(sql => sql.replace("CREATE INDEX ", "CREATE INDEX CONCURRENTLY ")).join("\n")}\n${guard}\n${retention}\n${inventory}\n${incremental}\n${identity.join("\n")}\nDO $grant$ BEGIN
    IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname='wfsim_factoryd') THEN
      GRANT SELECT,INSERT ON relation_compute_task_sources TO wfsim_factoryd;
    END IF;
    IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname='wfsim_pipeline') THEN
      REVOKE ALL ON state_compute_inventory_invalidations,state_compute_inventory_generation,record_inventory_captures FROM wfsim_pipeline;
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
