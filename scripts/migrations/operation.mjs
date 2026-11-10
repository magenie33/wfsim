// Production mutation is local to the database host and frozen phases require the recorded two-database role fence.
import postgres from "../pg/node_modules/postgres/src/index.js";
export const OPERATION_ID="db-boundaries-2026-11-10";
export function migrationEndpoint(kind,url,productionConfig=false){
  const parsed=new URL(url || "invalid:"),name=parsed.pathname.slice(1);
  const socket=parsed.hostname==="" && (process.env.PGHOST || "").startsWith("/");
  const loopback=["127.0.0.1","localhost","[::1]"].includes(parsed.hostname);
  if(!["postgres:","postgresql:"].includes(parsed.protocol) || (!loopback && !socket))throw new Error("migration execution requires a local database transport");
  const expected=kind==="library" ? "wfsim" : kind==="accounts" ? "wfsim_accounts" : "postgres";
  const production=kind==="admin" ? productionConfig : name===expected;
  const local=kind==="library" ? /^wfsim_migration_[a-z0-9_]+$/.test(name)
    : kind==="accounts" ? /^wfsim_accounts_migration_[a-z0-9_]+$/.test(name) : name==="postgres";
  if(!local && !production)throw new Error("unexpected migration database name");
  if(production && (name!==expected || process.env.WFSIM_MIGRATION_PRODUCTION!==OPERATION_ID
      || !socket || process.env.PGHOST!=="/var/run/postgresql"))
    throw new Error("production execution requires the exact operation id and the on-host PostgreSQL socket");
  return {name,production,local_only:!production};
}

export async function requireAdministrator(sql){
  const [role]=await sql.unsafe("SELECT rolsuper FROM pg_roles WHERE rolname=current_user");
  if(!role?.rolsuper)throw new Error("the migration operator must be a retained PostgreSQL administrator");
}

export async function requireDatabaseFence(sql,url,mode){
  if(!mode.production && process.env.WFSIM_MIGRATION_REQUIRE_FENCE!=="1")return null;
  const id=process.env.WFSIM_MIGRATION_FENCE;
  if(!/^[a-z0-9_-]+$/.test(id || ""))throw new Error("a recorded active database fence is required");
  const parsed=new URL(url);parsed.pathname="/postgres";
  const admin=postgres(parsed.toString(),{max:1,fetch_types:false,onnotice:()=>{},connection:{application_name:"wfsim-migration-fence-check"}});
  try{
    const [{exists}]=await admin.unsafe("SELECT to_regclass('wfsim_migration_ops.state_database_fence') IS NOT NULL AS exists");
    if(!exists)throw new Error("no cross-database fence registry exists");
    const [held]=await admin.unsafe("SELECT phase,baseline FROM wfsim_migration_ops.state_database_fence WHERE id=$1",[id]);
    if(!held || held.phase!=="fenced")throw new Error("the exact cross-database fence is not active");
    const databases=held.baseline.databases.map(database=>database.name),roles=held.baseline.roles.map(role=>role.name);
    const [identity]=await sql.unsafe("SELECT current_database() AS name,system_identifier::text AS cluster FROM pg_control_system()");
    if(identity.cluster!==held.baseline.cluster || !databases.includes(identity.name))throw new Error("fence does not cover this database and cluster");
    const current=await admin.unsafe(`SELECT oid::text AS oid,rolname AS name,rolcanlogin AS login FROM pg_roles
      WHERE rolname IN(SELECT jsonb_array_elements_text(($1::text)::jsonb))`,[JSON.stringify(roles)]);
    if(current.length!==roles.length || current.some(role=>role.login)
        || held.baseline.roles.some(role=>!current.some(now=>now.oid===role.oid && now.name===role.name)))
      throw new Error("the application role fence is incomplete");
    const [{n:connections}]=await admin.unsafe(`SELECT count(*) AS n FROM pg_stat_activity
      WHERE datname IN(SELECT jsonb_array_elements_text(($1::text)::jsonb))
        AND usename IN(SELECT jsonb_array_elements_text(($2::text)::jsonb))`,[JSON.stringify(databases),JSON.stringify(roles)]);
    if(connections!=="0")throw new Error("application sessions remain inside the frozen boundary");
    const [{n:prepared}]=await admin.unsafe("SELECT count(*) AS n FROM pg_prepared_xacts WHERE database IN(SELECT jsonb_array_elements_text(($1::text)::jsonb))",[JSON.stringify(databases)]);
    if(prepared!=="0")throw new Error("prepared transactions remain inside the frozen boundary");
    const defaults=await admin.unsafe(`SELECT d.datname AS name,(SELECT split_part(item,'=',2)::boolean
      FROM pg_db_role_setting s CROSS JOIN unnest(s.setconfig)item WHERE s.setdatabase=d.oid AND s.setrole=0
      AND split_part(item,'=',1)='default_transaction_read_only') AS read_only FROM pg_database d
      WHERE d.datname IN(SELECT jsonb_array_elements_text(($1::text)::jsonb))`,[JSON.stringify(databases)]);
    if(defaults.length!==2 || defaults.some(database=>database.read_only!==true))throw new Error("both frozen databases must retain their read-only defaults");
    return id;
  }finally{await admin.end();}
}
