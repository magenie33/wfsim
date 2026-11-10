// A global role fence closes both restored databases; PostgreSQL stores the authoritative restoration state.
import {readFileSync} from "node:fs";
import {createHash} from "node:crypto";
import postgres from "../pg/node_modules/postgres/src/index.js";
const command=process.argv[2] || "status";
const configPath=process.argv.find(arg=>arg.startsWith("--config="))?.slice(9);
if(!configPath)throw new Error("an exact fence configuration is required");
const config=JSON.parse(readFileSync(configPath,"utf8"));
if(!/^[a-z0-9_-]+$/.test(config.id || "") || !/^wfsim_migration_[a-z0-9_]+$/.test(config.library || "")
    || !/^wfsim_accounts_migration_[a-z0-9_]+$/.test(config.accounts || "")
    || !Array.isArray(config.roles) || !config.roles.length || new Set(config.roles).size!==config.roles.length
    || config.roles.some(role=>!/^wfsim_[a-z0-9_]+$/.test(role)))throw new Error("isolated local database and application-role names are required");
const url=process.env.WFSIM_MIGRATION_URL,parsed=new URL(url || "invalid:");
if(!["localhost","127.0.0.1","[::1]"].includes(parsed.hostname) || parsed.pathname!=="/postgres")
  throw new Error("the fence rehearsal requires a localhost postgres administrator connection");
const sql=postgres(url,{max:1,fetch_types:false,onnotice:()=>{},connection:{application_name:"wfsim-db-fence-rehearsal"}});
const q=value=>'"'+value.replaceAll('"','""')+'"';
const literal=value=>"'"+value.replaceAll("'","''")+"'";
const configHash=createHash("sha256").update(JSON.stringify(config)).digest("hex");
const selectRoles=async tx=>[...await tx.unsafe(`SELECT oid::text AS oid,rolname AS name,rolcanlogin AS login,rolsuper AS superuser,
  rolbypassrls AS bypass FROM pg_roles WHERE rolname IN(SELECT jsonb_array_elements_text(($1::text)::jsonb)) ORDER BY rolname`,[JSON.stringify(config.roles)])];
const selectDatabases=async tx=>[...await tx.unsafe(`SELECT d.oid::text AS oid,d.datname AS name,d.datallowconn AS allowed,
  (SELECT split_part(item,'=',2) FROM pg_db_role_setting s CROSS JOIN unnest(s.setconfig)item
    WHERE s.setdatabase=d.oid AND s.setrole=0 AND split_part(item,'=',1)='default_transaction_read_only') AS read_only
  FROM pg_database d WHERE d.datname IN($1,$2) ORDER BY d.datname`,[config.library,config.accounts])];
const readState=async tx=>(await tx.unsafe("SELECT * FROM wfsim_migration_ops.state_database_fence WHERE id=$1 FOR UPDATE",[config.id]))[0];
const setup=async tx=>{
  await tx.unsafe("SELECT pg_advisory_xact_lock(hashtextextended('wfsim-cross-database-fence',0))");
  await tx.unsafe(`CREATE SCHEMA IF NOT EXISTS wfsim_migration_ops;REVOKE ALL ON SCHEMA wfsim_migration_ops FROM PUBLIC;
    CREATE TABLE IF NOT EXISTS wfsim_migration_ops.state_database_fence(id text PRIMARY KEY,phase text NOT NULL,
      config_hash text NOT NULL,baseline jsonb NOT NULL,at timestamptz NOT NULL DEFAULT clock_timestamp());
    CREATE UNIQUE INDEX IF NOT EXISTS database_fence_one_active ON wfsim_migration_ops.state_database_fence((true)) WHERE phase='fenced'`);
};
const checkPrepared=async tx=>{
  const [{n}]=await tx.unsafe("SELECT count(*) AS n FROM pg_prepared_xacts WHERE database IN($1,$2)",[config.library,config.accounts]);
  if(n!=="0")throw new Error("resolve prepared transactions before fencing either database");
};
const connections=async()=>[...await sql.unsafe(`SELECT pid,datname AS database,usename AS role,application_name FROM pg_stat_activity
  WHERE datname IN($1,$2) AND usename IN(SELECT jsonb_array_elements_text(($3::text)::jsonb)) AND pid<>pg_backend_pid()`,
  [config.library,config.accounts,JSON.stringify(config.roles)])];

async function fence(){
  await sql.begin(async tx=>{
    await setup(tx);await checkPrepared(tx);
    const active=(await tx.unsafe("SELECT id FROM wfsim_migration_ops.state_database_fence WHERE phase='fenced' FOR UPDATE"))[0];
    if(active && active.id!==config.id)throw new Error("another database fence is active: "+active.id);
    const prior=await readState(tx);
    if(prior){
      if(prior.config_hash!==configHash || prior.phase!=="fenced")throw new Error("fence id cannot be reused for another configuration or restored operation");
      return;
    }
    const roles=await selectRoles(tx),databases=await selectDatabases(tx);
    if(roles.length!==config.roles.length || roles.some(role=>role.superuser || role.bypass))throw new Error("fence only existing non-privileged application roles");
    if(databases.length!==2 || databases.some(db=>!db.allowed))throw new Error("both target databases must exist and allow operator connections");
    const [identity]=await tx.unsafe("SELECT current_user AS actor,system_identifier::text AS cluster FROM pg_control_system()");
    if(config.roles.includes(identity.actor))throw new Error("the migration operator cannot fence its own role");
    const unknown=await tx.unsafe(`SELECT usename FROM pg_stat_activity WHERE datname IN($1,$2) AND usename<>current_user
      AND usename NOT IN(SELECT jsonb_array_elements_text(($3::text)::jsonb)) LIMIT 1`,[config.library,config.accounts,JSON.stringify(config.roles)]);
    if(unknown.length)throw new Error("an unlisted role is connected to a target database");
    const baseline={roles,databases,...identity};
    await tx.unsafe("INSERT INTO wfsim_migration_ops.state_database_fence(id,phase,config_hash,baseline) VALUES($1,'fenced',$2,($3::text)::jsonb)",[config.id,configHash,JSON.stringify(baseline)]);
    for(const role of roles)await tx.unsafe(`ALTER ROLE ${q(role.name)} NOLOGIN`);
    for(const database of databases)await tx.unsafe(`ALTER DATABASE ${q(database.name)} SET default_transaction_read_only='on'`);
  });
  let quiet=0;
  for(let attempt=0;attempt<50 && quiet<3;attempt++){
    const rows=await connections();
    if(rows.length){quiet=0;for(const row of rows)await sql.unsafe("SELECT pg_terminate_backend($1::integer)",[row.pid]);}
    else quiet++;
    await new Promise(resolve=>setTimeout(resolve,100));
  }
  if((await connections()).length)throw new Error("application connections remain; preserve the active fence and investigate");
  await checkPrepared(sql);
  return {phase:"fenced",application_connections:0,login_roles:(await selectRoles(sql)).filter(role=>role.login).length};
}

async function restore(){
  await sql.begin(async tx=>{
    await setup(tx);const held=await readState(tx);
    if(!held || held.config_hash!==configHash || held.phase!=="fenced")throw new Error("an exact active fence is required for restoration");
    const [{cluster}]=await tx.unsafe("SELECT system_identifier::text AS cluster FROM pg_control_system()");
    if(cluster!==held.baseline.cluster)throw new Error("fence belongs to another database cluster");
    const roles=await selectRoles(tx),databases=await selectDatabases(tx);
    if(roles.some(role=>role.login) || held.baseline.roles.some(role=>!roles.some(now=>now.oid===role.oid && now.name===role.name))
        || held.baseline.databases.some(db=>!databases.some(now=>now.oid===db.oid && now.name===db.name)))
      throw new Error("fenced role or database identity changed; do not guess restoration");
    for(const database of held.baseline.databases)await tx.unsafe(database.read_only===null
      ? `ALTER DATABASE ${q(database.name)} RESET default_transaction_read_only`
      : `ALTER DATABASE ${q(database.name)} SET default_transaction_read_only=${literal(database.read_only)}`);
    for(const role of held.baseline.roles)await tx.unsafe(`ALTER ROLE ${q(role.name)} ${role.login ? "LOGIN" : "NOLOGIN"}`);
    await tx.unsafe("UPDATE wfsim_migration_ops.state_database_fence SET phase='restored',at=clock_timestamp() WHERE id=$1",[config.id]);
  });return {phase:"restored",roles:await selectRoles(sql),databases:await selectDatabases(sql)};
}

try{
  let result;
  if(command==="fence")result=await fence();
  else if(command==="restore")result=await restore();
  else if(command==="status"){
    const [{exists}]=await sql.unsafe("SELECT to_regclass('wfsim_migration_ops.state_database_fence') IS NOT NULL AS exists");
    result={state:exists ? (await sql.unsafe("SELECT * FROM wfsim_migration_ops.state_database_fence WHERE id=$1",[config.id]))[0] : null,
      roles:await selectRoles(sql),databases:await selectDatabases(sql),connections:await connections()};
  }else throw new Error("unknown fence command: "+command);
  console.log(JSON.stringify({ok:true,command,id:config.id,local_only:true,production_changes:false,...result}));
}finally{await sql.end();}
