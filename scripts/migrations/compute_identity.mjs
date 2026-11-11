// Identity changes run on the database host while the recorded application-role fence is active.
import { readFileSync, writeFileSync } from "node:fs";
import postgres from "../pg/node_modules/postgres/src/index.js";
import { pgDb } from "../../worker/db.js";
import { identityPage, applyIdentityEntry } from "../compute_identity.mjs";
import { requireAdministrator, requireDatabaseFence } from "./operation.mjs";

const [command, file, ...args] = process.argv.slice(2);
if (!["plan", "apply"].includes(command) || !file) throw new Error("usage: compute_identity.mjs plan|apply <private-plan.json> [--after=id] [--engine=id] [--normalizer=url]");
const options = Object.fromEntries(args.map(arg => { const match = /^--(after|engine|normalizer)=(.*)$/.exec(arg); if (!match) throw new Error("invalid migration option"); return match.slice(1); }));
const url = process.env.WFSIM_COMPUTE_MIGRATION_URL;
const parsed = new URL(url), socket = parsed.hostname === "" && process.env.PGHOST === "/var/run/postgresql";
const local = ["127.0.0.1", "localhost", "[::1]"].includes(parsed.hostname);
const production = parsed.pathname === "/wfsim";
if ((!local && !socket) || (production && (!socket || process.env.WFSIM_COMPUTE_MIGRATION !== "compute-identity-2026-11-11")))
  throw new Error("identity migration requires isolated local PG or the exact on-host production operation");
const sql = postgres(url, { max: 1, fetch_types: false, onnotice: () => {}, connection: { statement_timeout: 15000, application_name: "wfsim-compute-identity" },
  types: { bigint: { to: 20, from: [20], parse: Number, serialize: String } } });
const db = pgDb({ query: async (text, parameters) => { const rows = await sql.unsafe(text, parameters); return { rows: [...rows], affected: rows.count }; } });
try {
  if (command === "plan") {
    const base = new URL(options.normalizer || "http://127.0.0.1:8972");
    if (!["127.0.0.1", "localhost", "[::1]"].includes(base.hostname)) throw new Error("private input normalization must use the pinned local native engine");
    const normalize = async (kind, request, context) => {
      const response = await fetch(new URL("/api/computation/input",base), { method:"POST",headers:{"content-type":"application/json"},
        body:JSON.stringify({kind,request,context}),signal:AbortSignal.timeout(30000) });
      const value = await response.json();
      if (!response.ok || !value.ok || value.schema !== 1) throw new Error("retained computation refused by the normalization engine: " + (value.error || response.status));
      return value;
    };
    const plan = await identityPage(db, normalize, options);
    writeFileSync(file,JSON.stringify(plan,null,2)+"\n",{mode:0o600});
    console.log(JSON.stringify({schema:plan.schema,engine:plan.engine,after:plan.after,page_full:plan.page_full,
      counts:Object.fromEntries(["unchanged","rekey","remove_untraced"].map(action=>[action,plan.entries.filter(entry=>entry.action===action).length]))}));
  } else {
    await requireAdministrator(sql);
    await requireDatabaseFence(sql,url,{production});
    await sql.unsafe("SET default_transaction_read_only=off");
    const plan=JSON.parse(readFileSync(file,"utf8"));
    if (plan.schema!==1 || !Array.isArray(plan.entries) || plan.entries.length>250 || !plan.engine) throw new Error("invalid identity migration plan");
    const applied=[];
    try { for(const entry of plan.entries) applied.push({id:entry.id,action:entry.action,result:await applyIdentityEntry(db,entry)}); }
    finally { writeFileSync(file+".applied.json",JSON.stringify({schema:1,after:plan.after,applied},null,2)+"\n",{mode:0o600}); }
    console.log(JSON.stringify({applied:applied.length,after:plan.after}));
  }
} finally { await sql.end(); }
