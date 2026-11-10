// Drain accepts in-flight work; the final maintenance fence opens no database or scheduled work.
import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
import worker from "../worker/index.js";
import {createDispatch} from "../worker/task_dispatch.js";
import {testDb} from "./pg/test_db.mjs";
import {syncRoute,forgetWitnesses} from "../worker/sync.js";
import {verifyRoute} from "../worker/verify.js";
import {boardTasksRoute} from "../worker/board_tasks.js";
import {facts} from "../worker/factory.js";
import {setDial,forgetDials} from "../worker/dials.js";

let passed=0;
const check=(name,ok)=>{assert.ok(ok,name);passed++;console.log("ok "+name);};
const blocked={DATABASE_MAINTENANCE:"1",get HYPERDRIVE(){throw new Error("database handle opened");},ASSETS:{fetch:()=>new Response("static")}};
const request=path=>new Request("https://wfsim.app"+path);
const response=await worker.fetch(request("/api/notices/everyone"),blocked);
check("maintenance rejects database APIs before opening their handles",response.status===503 && response.headers.get("retry-after")==="60" && (await response.json()).retryable===true);
let scheduled=false;await worker.scheduled({cron:"* * * * *"},blocked,{waitUntil(){scheduled=true;}});
check("maintenance starts no scheduled database job",!scheduled);
check("static pages remain independent of database handles during maintenance",await (await worker.fetch(request("/weapons/Braton"),blocked)).text()==="static");
check("dynamic share images are fenced before their database read",(await worker.fetch(request("/og/s/abcdefghij.png"),blocked)).status===503);
let dispatch=createDispatch;
if(process.argv.includes("--break-drain")) {
  const module=new URL("../worker/task_dispatch.js",import.meta.url);
  const source=readFileSync(module,"utf8").replace('if(env.FACTORY_DRAINING==="1")return null;','')
    .replace(/from "(\.\/[^\"]+)"/g,(_whole,path)=>'from "'+new URL(path,module).href+'"');
  dispatch=(await import("data:text/javascript;base64,"+Buffer.from(source).toString("base64"))).createDispatch;
}
const dispatcher=dispatch({verbs:{}});
const draining={FACTORY_DRAINING:"1",get LIBRARY(){throw new Error("new work touched the database");}};
check("drain closes single, pack and official admission at their common dispatcher",await dispatcher.next(draining)===null
  && await dispatcher.nextPack(draining,{},"e",{})===null && (await dispatcher.claim(draining,{},{})).length===0);

for(const settleAsync of [0,1]) {
  forgetDials();forgetWitnesses();
  const db=await testDb("library"),accounts=await testDb("accounts");
  const env={LIBRARY:db,ACCOUNTS:accounts,ASSETS:{fetch:async()=>Response.json({engine:"e1",release:"r1"})}};
  await setDial(env,"sync_proc",1,"check");await setDial(env,"settle_async",settleAsync,"check");
  const batch="arrivals-2026-11-10",A="a".repeat(24),B="b".repeat(24);
  await db.run("INSERT INTO entity_compute_batches(id,at,why,total) VALUES(?,'x','check',0)",batch);
  for(const id of ["one","two","three","four"]){
    await db.run("INSERT INTO entity_weapon_builds(id,at,record) VALUES(?,'x',?)",id,JSON.stringify({weapon:"braton_prime",mods:[id]}));
    await db.run("INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES(?,?,'standard_single_target','base')",batch,id);
  }
  const post=async(route,path,body)=>{
    const response=await route(new Request("https://wfsim.app"+path,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(body)}),env,path);
    return response.json();
  };
  env.FACTORY_TOKEN="token";
  const askRequest=new Request("https://wfsim.app/api/board/ask",{method:"POST",headers:{authorization:"Bearer token"},
    body:JSON.stringify({batch,rows:["one","two","three","four"].map(identity=>({identity,ruler:"standard_single_target",mode:"base"}))})});
  assert.equal((await boardTasksRoute(askRequest,env,"/api/board/ask")).status,200);
  const who=id=>({device:id,owner:id,siblings:[id],net:id,lanes:4});
  const first=await facts().nextPack(env,who(A),"e1",{n:2,lease_ms:600000});
  const second=await facts().nextPack(env,who(B),"e1",{n:1,lease_ms:600000});
  assert.equal(first.questions.length,2);assert.equal(second.questions.length,1);
  env.FACTORY_DRAINING="1";
  const actual=db.prepare.bind(db);
  env.LIBRARY=new Proxy(db,{get(target,key){
    if(key==="prepare")return text=>{assert.ok(!/SELECT \* FROM board_sync\(/.test(text),"drain must bypass the lease-bearing SQL procedure");return actual(text);};
    return typeof target[key]==="function" ? target[key].bind(target) : target[key];
  }});
  const item=question=>({question,score:1,metric:"kpm",work:17});
  const kept=first.questions[1].question;
  const result=await post(syncRoute,"/api/board/sync",{verifier:A,engine:"e1",protocol:8,consent:{v:1,at:"2026-11-01T00:00:00.000Z"},
    lanes:4,want_ms:600000,results:[{lease:first.lease,...item(first.questions[0].question)}],held:[{lease:first.lease,rows:[kept]}]});
  check("protocol8 drain receives answers and renews held work with settle_async="+settleAsync,result.taken?.includes(first.questions[0].question)
    && result.held?.some(row=>row.question===kept && row.held) && !result.pack && !result.work);
  const old=await post(verifyRoute,"/api/board/answers",{verifier:B,engine:"e1",lease:second.lease,answers:[item(second.questions[0].question)],done:true});
  check("protocol7 drain keeps accepting its existing answer with settle_async="+settleAsync,old.ok===true
    && (await db.get("SELECT count(*) AS n FROM record_candidate_answers")).n===2);
  check("drain leaves remaining work unleased with settle_async="+settleAsync,(await facts().claim(env,{device:"official:check"},{},"e1")).length===0
    && (await db.get("SELECT count(*) AS n FROM state_compute_work WHERE lease IS NOT NULL")).n===1);
  env.FACTORY_DRAINING="0";
  check("clearing drain resumes lease admission with settle_async="+settleAsync,!!await facts().next(env,who("c".repeat(24)),"e1"));
  const resumed=await worker.fetch(request("/api/notices/everyone"),{...env,DATABASE_MAINTENANCE:"0"});
  check("clearing maintenance restores the normal database API with settle_async="+settleAsync,resumed.status===200 && (await resumed.json()).ok===true);
  await db.close();await accounts.close();
}
console.log(JSON.stringify({ok:true,checks:passed}));
