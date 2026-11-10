// Prove read-only defaults are overridable and the role/session fence closes both application databases.
import assert from "node:assert/strict";
import {randomBytes} from "node:crypto";
import {mkdirSync,writeFileSync} from "node:fs";
import {spawnSync} from "node:child_process";
import postgres from "./pg/node_modules/postgres/src/index.js";
const url=process.env.WFSIM_MIGRATION_TEST_URL;
if(!url || !["127.0.0.1","localhost","[::1]"].includes(new URL(url).hostname) || new URL(url).pathname!=="/postgres")
  throw new Error("isolated localhost postgres administrator URL is required");
const id=randomBytes(6).toString("hex"),library="wfsim_migration_fence_"+id,accounts="wfsim_accounts_migration_fence_"+id;
const roles=["wfsim_fence_a_"+id,"wfsim_fence_b_"+id,"wfsim_fence_disabled_"+id];
const config={id:"fence-test-"+id,library,accounts,roles};
const directory="target/db-migration/fence-"+id;mkdirSync(directory,{recursive:true});
const path=directory+"/config.json";writeFileSync(path,JSON.stringify(config));
const options={max:1,fetch_types:false,onnotice:()=>{},connect_timeout:2};
const connect=(database,role)=>{const target=new URL(url);target.pathname="/"+database;if(role)target.username=role;return postgres(target.toString(),options);};
const admin=postgres(url,options),clients=[];let complete=false,fenced=false,checks=0;
const check=(name,value)=>{assert.ok(value,name);checks++;console.log("ok "+name);};
const run=(command,configPath=path)=>spawnSync(process.execPath,["scripts/migrations/db_fence.mjs",command,"--config="+configPath],
  {env:{...process.env,WFSIM_MIGRATION_URL:url},encoding:"utf8",timeout:30000});
const good=command=>{const r=run(command);if(r.status!==0)throw new Error(r.stderr);return JSON.parse(r.stdout);};
try{
  await admin.unsafe(`CREATE ROLE ${roles[0]} LOGIN`);await admin.unsafe(`CREATE ROLE ${roles[1]} LOGIN`);await admin.unsafe(`CREATE ROLE ${roles[2]} NOLOGIN`);
  await admin.unsafe(`CREATE DATABASE ${library}`);await admin.unsafe(`CREATE DATABASE ${accounts}`);
  const lib=connect(library),acc=connect(accounts);clients.push(lib,acc);
  for(const [db,role] of [[lib,roles[0]],[acc,roles[1]]]){
    await db.unsafe(`CREATE TABLE entity_fence_probes(id text PRIMARY KEY);GRANT SELECT,INSERT ON entity_fence_probes TO ${role}`);
  }
  await admin.unsafe(`ALTER DATABASE ${accounts} SET default_transaction_read_only='on'`);
  const weak=connect(accounts,roles[1]);clients.push(weak);
  check("a new application connection inherits the database read-only default",(await weak.unsafe("SHOW transaction_read_only"))[0].transaction_read_only==="on");
  await weak.unsafe("SET default_transaction_read_only='off'");
  await weak.unsafe("INSERT INTO entity_fence_probes VALUES('weak-default-overridden')");
  check("the same non-superuser can override a read-only default",(await acc.unsafe("SELECT count(*) AS n FROM entity_fence_probes"))[0].n==="1");
  await weak.end();await acc.unsafe("DELETE FROM entity_fence_probes");
  await admin.unsafe(`ALTER DATABASE ${accounts} SET default_transaction_read_only='off'`);
  const first=connect(library,roles[0]),second=connect(accounts,roles[1]);clients.push(first,second);
  await first.unsafe("BEGIN");await first.unsafe("INSERT INTO entity_fence_probes VALUES('must-roll-back')");
  await second.unsafe("SELECT 1");
  const result=good("fence");fenced=true;
  check("one fence closes both roles and terminates their existing sessions",result.login_roles===0 && result.application_connections===0);
  check("terminating an in-flight writer rolls back its uncommitted row",(await lib.unsafe("SELECT count(*) AS n FROM entity_fence_probes"))[0].n==="0");
  const concurrent=directory+"/concurrent.json";writeFileSync(concurrent,JSON.stringify({...config,id:"competing-"+id}));
  const overlap=run("fence",concurrent);
  check("a second operation cannot replace the active restoration baseline",overlap.status!==0 && overlap.stderr.includes("another database fence is active"));
  let refused=false;
  const blocked=connect(accounts,roles[1]);clients.push(blocked);
  try{await blocked.unsafe("SET default_transaction_read_only='off'");}catch(error){refused=error.code==="28000";}
  check("a fenced application cannot reconnect to override transaction settings",refused);
  const operator=connect(library);clients.push(operator);
  await operator.begin(async tx=>{await tx.unsafe("SET TRANSACTION READ WRITE");await tx.unsafe("INSERT INTO entity_fence_probes VALUES('controlled-operator-write')");});
  check("the retained administrator can perform an explicit controlled migration write",(await lib.unsafe("SELECT count(*) AS n FROM entity_fence_probes"))[0].n==="1");
  const restored=good("restore");fenced=false;
  check("restoration keeps the originally disabled role disabled",restored.roles.filter(role=>role.login).length===2
    && restored.roles.find(role=>role.name===roles[2]).login===false);
  check("both original database defaults are restored exactly",restored.databases.find(db=>db.name===library).read_only===null
    && restored.databases.find(db=>db.name===accounts).read_only==="off");
  const resumed=connect(accounts,roles[1]);clients.push(resumed);await resumed.unsafe("INSERT INTO entity_fence_probes VALUES('resumed-application')");
  check("restored application admission writes normally",(await acc.unsafe("SELECT count(*) AS n FROM entity_fence_probes"))[0].n==="1");
  complete=true;writeFileSync(directory+"/receipt.json",JSON.stringify({ok:true,checks,local_only:true,production_changes:false},null,2)+"\n");
  console.log(JSON.stringify({ok:true,checks}));
}finally{
  if(fenced){const r=run("restore");if(r.status!==0)console.error("retained active local fence: "+path);}
  for(const client of clients)await client.end({timeout:1});
  if(complete){await admin.unsafe(`DROP DATABASE ${library}`);await admin.unsafe(`DROP DATABASE ${accounts}`);for(const role of roles)await admin.unsafe(`DROP ROLE ${role}`);}
  else console.error("preserved failed local fence databases and configuration: "+directory);
  await admin.end();
}
