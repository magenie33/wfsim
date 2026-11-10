// Arrange valid task/result fixtures through the same storage boundaries as producers.
export async function seedComputeWork(db,row) {
  const at=row.opened_at || "2026-01-01T00:00:00.000Z", engine=row.fact_engine || "fixture";
  const spec={id:row.id,verb:row.verb || "simulate",request:typeof row.request==="string" ? row.request : JSON.stringify(row.request || {}),
    due_at:row.due_at || at,lanes:row.lanes || null,waits:row.waits || 0,producer:row.producer || "",initiator:row.initiator || null};
  await db.run("SELECT compute_register_tasks(?1::text,?2::text,?3::text)",JSON.stringify([spec]),engine,at);
  let task=await db.get("SELECT task_id FROM state_compute_work WHERE id=?",row.id);
  if (["fact","spot"].includes(row.state) || row.canon!=null || row.fact_at!=null) {
    await db.run("SELECT compute_accept_result(?1::text,?2::bigint,'open',?3::text,?4::text,?5::text,?6::text,?7::bigint,?8::text)",
      row.id,task.task_id,["fact","spot"].includes(row.state) ? row.state : "fact",row.canon || "{}",engine,row.fact_at || at,
      row.odd || 0,typeof row.witnesses==="string" ? row.witnesses : JSON.stringify(row.witnesses || []));
    if (row.state==="open") {
      await db.run("SELECT compute_open_task(?1::text,'fixture_recheck',?2::text,?3::text)",row.id,engine,at);
      task=await db.get("SELECT task_id FROM state_compute_work WHERE id=?",row.id);
    }
  }
  const columns=new Set(["state","due_at","waits","opened_at","lanes","lease","lease_until","leased_to","leased_at",
    "lease_prev","leased_to_prev","lease_prev_until","needs","odd","producer","initiator","withdrawn_at"]);
  const entries=Object.entries(row).filter(([key])=>columns.has(key));
  if (entries.length) await db.run(`UPDATE state_compute_work SET ${entries.map(([key],i)=>`${key}=?${i+1}`).join(",")}
    WHERE id=?${entries.length+1}`, ...entries.map(([,value])=>value),row.id);
  return task.task_id;
}

export async function replaceComputeResult(db,id,fields={}) {
  const old=await db.get("SELECT * FROM state_compute_work WHERE id=?",id);
  if (!old) throw new Error("missing computation fixture: "+id);
  const row={...old,...fields};
  const {task}=await db.get("SELECT compute_open_task(?1::text,'fixture_correction',?2::text,?3::text,true) AS task",
    id,row.fact_engine,row.fact_at);
  await db.run("SELECT compute_accept_result(?1::text,?2::bigint,'open',?3::text,?4::text,?5::text,?6::text,?7::bigint,?8::text)",
    id,task,row.state,row.canon,row.fact_engine,row.fact_at,row.odd,row.witnesses);
}

export async function seedComputeWorkMany(db,rows) {
  for(let from=0;from<rows.length;from+=512) {
    const page=rows.slice(from,from+512),at=page[0].opened_at,engine="fixture";
    const specs=page.map(row=>({id:row.id,verb:row.verb || "simulate",request:typeof row.request==="string" ? row.request : JSON.stringify(row.request || {}),
      due_at:row.due_at,lanes:null,waits:0,producer:"",initiator:null}));
    await db.run("SELECT compute_register_tasks(?1::text,?2::text,?3::text)",JSON.stringify(specs),engine,at);
    await db.run(`SELECT compute_accept_result(s.id,s.task_id,'open',x.state,'{}',?2::text,?3::text,0,'[]')
      FROM jsonb_to_recordset((?1::text)::jsonb) x(id text,state text) JOIN state_compute_scheduling s ON s.id=x.id
      WHERE x.state IN('fact','spot')`,JSON.stringify(page),engine,at);
    await db.run(`UPDATE state_compute_scheduling s SET state=x.state,opened_at=x.opened_at
      FROM jsonb_to_recordset((?1::text)::jsonb) x(id text,state text,opened_at text) WHERE s.id=x.id`,JSON.stringify(page));
  }
}
