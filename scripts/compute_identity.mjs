// Normalize retained requests before the fenced identity cutover; accepted outputs remain immutable.
import { canon, questionId } from "../worker/task_contract.js";
import { computationInput, computationReferences } from "../worker/computation_input.js";

export async function identityPage(db, normalize, { after = "", limit = 100, engine, duplicatesOnly=false, at=new Date().toISOString() } = {}) {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 250 || !engine) throw new Error("invalid identity migration page");
  const rows = (await db.prepare(`SELECT q.* FROM (SELECT DISTINCT d.question FROM relation_task_consumers d
    WHERE d.state='live' AND d.producer IN ('user_calculation','family_build','chat','survey','board_review') AND d.question>?1::text
      AND EXISTS(SELECT 1 FROM state_compute_scheduling s WHERE s.id=d.question AND s.state='open' OFFSET 0)
    ORDER BY d.question LIMIT ?2::bigint) scoped JOIN state_compute_work q ON q.id=scoped.question ORDER BY q.id`)
    .bind(after, limit).all()).results;
  const entries = [];
  for (const row of rows) {
    if(duplicatesOnly && (row.lease_until>=at || row.lease_prev_until>=at || await db.prepare(`SELECT 1 FROM state_pending_verifications WHERE question=?
      UNION ALL SELECT 1 FROM state_build_workflow_nodes n JOIN state_build_workflows f ON f.id=n.workflow WHERE n.computation=? AND f.state='active' LIMIT 1`)
      .bind(row.id,row.id).first())) {
      entries.push({action:'deferred',id:row.id,task_id:row.task_id,reason:'running_or_finishing'});continue;
    }
    const snapshot = await db.prepare(`SELECT coalesce(jsonb_agg(to_jsonb(d) ORDER BY d.producer,d.ref),'[]'::jsonb)::text AS body
      FROM relation_task_consumers d WHERE d.question=? AND d.state='live'`).bind(row.id).first();
    const consumers = (await db.prepare(`SELECT d.*,compute_source_exists(d.producer,d.ref,d.payload,d.question) AS traced
      FROM relation_task_consumers d WHERE d.question=? AND d.state='live' ORDER BY d.producer,d.ref`).bind(row.id).all()).results;
    const traced = consumers.filter(demand => demand.traced);
    if (!traced.length || (duplicatesOnly && traced.length!==consumers.length)) {
      entries.push({ action: duplicatesOnly ? 'deferred' : "remove_untraced", id: row.id, task_id: row.task_id,reason:'unproven_source' }); continue;
    }
    const input = JSON.parse(row.request);
    if(duplicatesOnly && row.verb==='simulate' && !input.weapon) {
      entries.push({action:'deferred',id:row.id,task_id:row.task_id,reason:'legacy_board_input_not_frozen'});continue;
    }
    let normalized = { request: input, aliases: {} };
    try {
      if (row.verb === "simulate_report" || row.verb === "optimize_report")
        normalized = await normalize(row.verb === "simulate_report" ? "simulation" : "optimization", input);
      else if (row.verb === "optimize") {
        normalized = await normalize("optimization", input.request, input.context);
        normalized.request = { request: normalized.request };
      } else if (row.verb !== "simulate") throw new Error("unrecognized retained computation verb");
    } catch(error) {
      if(!duplicatesOnly || !error.inputRefused)throw error;
      entries.push({action:'deferred',id:row.id,task_id:row.task_id,reason:'normalization_refused'});continue;
    }
    const request = computationInput(row.verb, normalized.request), id = await questionId(row.verb, request);
    if (id === row.id) { entries.push({ action: "unchanged", id, task_id: row.task_id }); continue; }
    const demands = [];
    for (const [ordinal, demand] of traced.entries()) {
      let payload;
      try { payload = JSON.parse(demand.payload); } catch (_) { payload = {}; }
      if (!payload || typeof payload !== "object" || Array.isArray(payload)) payload = {};
      let context = null;
      if (demand.producer === "user_calculation" && typeof payload.board_upload !== "boolean")
        payload.board_upload = input.__board_upload === true;
      if (demand.producer === "chat" || demand.producer === "survey") {
        const business = await db.prepare("SELECT request FROM entity_appraisal_requests WHERE code=?").bind(demand.ref).first();
        context = computationReferences(JSON.parse(business.request).context, normalized.aliases);
        payload.context = context;
      } else if (demand.producer === "family_build") {
        const business = await db.prepare("SELECT context FROM entity_family_search_requests WHERE id=?").bind(demand.ref).first();
        context = computationReferences(JSON.parse(business.context), normalized.aliases);
      }
      demands.push({ producer: demand.producer, ref: demand.ref, question: id, trigger_id: payload.review || payload.batch || demand.ref,
        ordinal, asked_at: demand.asked_at, due_at: demand.due_at, waits: Number(demand.waits),
        zone:demand.zone ?? row.zone,payload: JSON.stringify(payload), initiator: demand.initiator, context: context ? JSON.stringify(context) : null });
    }
    const head = [...demands].sort((a,b) => a.due_at.localeCompare(b.due_at))[0];
    const spec = { id, verb: row.verb, request: canon(request), due_at: head.due_at, waits: Math.max(...demands.map(d => d.waits)),
      producer: head.producer, initiator: head.initiator, lanes: row.lanes, requested_engine: engine,
      zone:Math.min(...demands.map(d=>d.zone)),capability:row.required_capability,
      force: demands.some(d => d.producer === "board_review") };
    entries.push({ action: "rekey", id: row.id, task_id: row.task_id, spec, demands, consumers: snapshot.body });
  }
  if(duplicatesOnly)for(const entry of entries.filter(e=>e.action==='rekey')) {
    const target=await db.prepare('SELECT verb,request,state FROM state_compute_work WHERE id=?').bind(entry.spec.id).first();
    const peers=entries.filter(other=>other.action==='rekey' && other.spec.id===entry.spec.id);
    if(target && (target.verb!==entry.spec.verb || canon(JSON.parse(target.request))!==entry.spec.request))throw Error('duplicate target input conflict');
    if(!target && peers.length<2) {entry.action='deferred';entry.reason='no_proven_duplicate';}
    else entry.proof={target:entry.spec.id,equivalent:peers.map(e=>e.id),existing:!!target};
  }
  return { schema: 1, engine, duplicates_only:duplicatesOnly,entries, after: rows.at(-1)?.id || after, page_full: rows.length === limit };
}

export async function applyIdentityEntry(db, entry, at = new Date().toISOString()) {
  if (entry.action === "unchanged" || entry.action==='deferred') return { unchanged: entry.action==='unchanged',deferred:entry.action==='deferred' };
  if (entry.action === "remove_untraced") return db.prepare("SELECT compute_remove_untraced(?1::text,?2::bigint,?3::text) AS removed")
    .bind(entry.id, entry.task_id, at).first();
  if (entry.action !== "rekey") throw new Error("invalid identity migration action");
  return db.prepare("SELECT compute_rekey_active(?1::text,?2::bigint,?3::text,?4::text,?5::text,?6::text) AS task_id")
    .bind(entry.id, entry.task_id, JSON.stringify([entry.spec]), JSON.stringify(entry.demands), entry.consumers, at).first();
}
