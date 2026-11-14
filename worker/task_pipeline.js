// SPDX-License-Identifier: AGPL-3.0-or-later
// Stage controls and observations; durable work stays in each stage's own outbox.
import { iso } from "./instant.js";

export const TASK_PIPELINE_STAGES = Object.freeze({
  verification: { input: "state_pending_verifications", output: "accepted result, delivery_at, credit_work" },
  delivery: { input: "relation_task_consumers.delivery_at", output: "consumer writes and served acknowledgement" },
  contribution: { input: "record_candidate_answers.credit_work", output: "idempotent credit and hourly totals" },
});

const stageOf = name => {
  if (!Object.hasOwn(TASK_PIPELINE_STAGES,name)) throw Error("unknown task pipeline stage");
  return TASK_PIPELINE_STAGES[name];
};

export function createTaskPipeline(handlers) {
  for (const name of Object.keys(TASK_PIPELINE_STAGES))
    if (typeof handlers[name] !== "function") throw Error("missing task pipeline handler: "+name);

  async function control(env,name,{paused,by},now=Date.now()) {
    stageOf(name);
    if (typeof paused !== "boolean" || typeof by !== "string" || !by.trim()) throw Error("invalid pipeline control");
    await env.LIBRARY.prepare(`INSERT INTO state_task_pipeline_controls(name,paused,changed_at,changed_by)
      VALUES(?1::text,?2::boolean,?3::text,?4::text) ON CONFLICT(name) DO UPDATE SET
      paused=excluded.paused,changed_at=excluded.changed_at,changed_by=excluded.changed_by`)
      .bind(name,paused,iso(now),by).run();
  }

  async function run(env,name,now=Date.now(),options={}) {
    stageOf(name);
    const db=env.LIBRARY, runtime=options.runtime || "native";
    const state=await db.prepare("SELECT paused FROM state_task_pipeline_controls WHERE name=?").bind(name).first();
    if(state?.paused) return {stage:name,paused:true,processed:0};
    const start=performance.now();
    let processed=0,error=null;
    try { processed=await handlers[name](env,now,options); }
    catch(failure) {error=failure;}
    if(processed || error) {
      await db.prepare(`INSERT INTO state_task_pipeline_progress(name,at,processed,failures,elapsed_ms,runtime,error)
        VALUES(?1::text,?2::text,?3::bigint,?4::bigint,?5::double precision,?6::text,?7::text)
        ON CONFLICT(name) DO UPDATE SET at=excluded.at,processed=state_task_pipeline_progress.processed+excluded.processed,
        failures=state_task_pipeline_progress.failures+excluded.failures,elapsed_ms=excluded.elapsed_ms,runtime=excluded.runtime,error=excluded.error`)
        .bind(name,iso(now),processed,error?1:0,performance.now()-start,runtime,error?String(error.message || error).slice(0,1000):null).run();
    }
    if(error) throw error;
    return {stage:name,paused:false,processed};
  }

  async function status(env,now=Date.now(),limit=1000) {
    if(!Number.isSafeInteger(limit) || limit<1 || limit>10000)throw Error("invalid pipeline observation bound");
    const db=env.LIBRARY;
    const [controls,progress]=await db.batch([
      db.prepare("SELECT * FROM state_task_pipeline_controls"),db.prepare("SELECT * FROM state_task_pipeline_progress"),
    ]);
    const sources={
      verification:"SELECT queued_at AS at FROM state_pending_verifications ORDER BY queued_at LIMIT ?1::bigint",
      delivery:"SELECT delivery_at AS at FROM relation_task_consumers WHERE state='live' AND delivery_at IS NOT NULL ORDER BY delivery_at LIMIT ?1::bigint",
      contribution:"SELECT credit_at AS at FROM record_candidate_answers WHERE credit_work IS NOT NULL AND credited=0 AND void=0 AND official=0 ORDER BY credit_at,id LIMIT ?1::bigint",
    };
    const rows=await db.batch(Object.values(sources).map(sql=>db.prepare(`SELECT count(*) AS n,min(at) AS oldest FROM (${sql}) pending`).bind(limit+1)));
    return Object.keys(TASK_PIPELINE_STAGES).map((name,i)=>({name,...TASK_PIPELINE_STAGES[name],
      paused:controls.results.find(row=>row.name===name)?.paused || false,
      control:controls.results.find(row=>row.name===name) || null,
      progress:progress.results.find(row=>row.name===name) || null,
      pending:Math.min(limit,Number(rows[i].results[0].n)),at_least:Number(rows[i].results[0].n)>limit,
      oldest:rows[i].results[0].oldest,observed_at:iso(now)}));
  }

  return {run,control,status};
}
