// Real IndexedDB upgrades preserve existing reports and delivery state, including an aborted upgrade.
import {createServer} from "node:http";
import {readFileSync} from "node:fs";
import {openApp} from "./cdp.mjs";

const source=name=>readFileSync(new URL("../web/src/static/"+name,import.meta.url),"utf8");
let task=source("app/53-task-store.js");
if(process.argv.includes("--break-atomic-write"))task=task.replace("tx.abort();reject(error);return;","reject(error);return;");
const result=source("app/35-results.js"), compute=source("app/69-board-work.js");
const files={
  "/source/task":task,
  "/source/result":result.slice(result.indexOf("const RESULTS_DB"),result.indexOf("function resultContext()")),
  "/source/compute":compute.slice(compute.indexOf("const COMPUTE_DB ="),compute.indexOf("/// SESSION-LOCAL TIMINGS")),
};
const server=createServer((request,response)=>{
  const path=new URL(request.url,"http://localhost").pathname;
  let body=files[path];
  if(path.startsWith("/nona/") && !path.includes("..")) {
    try{body=source(path.slice(1));}catch{response.writeHead(404);response.end();return;}
  }
  response.setHeader("content-type",body===undefined ? "text/html" : "text/javascript");
  response.end(body===undefined ? "<!doctype html><script>window.__wfsimReady=true</script>" : body);
});
await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
let app;
try {
  app=await openApp({base:"http://127.0.0.1:"+server.address().port});
  const checks=await app.evaluate(`(async()=>{
    const checks=[];
    const assert=(name,ok)=>{if(!ok)throw new Error(name);checks.push(name);};
    const equal=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
    const done=tx=>new Promise((ok,no)=>{tx.oncomplete=ok;tx.onabort=tx.onerror=()=>no(tx.error);});
    const open=(name,version,upgrade)=>new Promise((ok,no)=>{
      const request=indexedDB.open(name,version);request.onupgradeneeded=()=>upgrade(request.result,request.transaction);
      request.onsuccess=()=>ok(request.result);request.onerror=()=>no(request.error);
    });
    const all=(db,name)=>new Promise((ok,no)=>{
      const request=db.transaction(name).objectStore(name).getAll();request.onsuccess=()=>ok(request.result);request.onerror=()=>no(request.error);
    });
    const state={tasks:[{id:"second",request:{text:"原文"},state:"succeeded",result_ref:"report"},{id:"first",request:{text:"immutable"},state:"queued"}],
      events:[{id:"z-completion",task:"second",delivered_at:null},{id:"a-completion",task:"first",delivered_at:null}],
      commands:[{id:"z-cancel",remote:"original",delivered_at:null},{id:"a-cancel",remote:"next",delivered_at:null}]};
    const artifact={id:"report",result:{score:42},origin:{weapon:"original"},engine:"old"};
    const upload={build:{weapon:"torid"},source:{explicit:true},marker:"optimization-build-seen:one"};
    let db=await open("wfsim-user-tasks",1,db=>db.createObjectStore("queue"));
    let tx=db.transaction("queue","readwrite"),ended=done(tx),store=tx.objectStore("queue");
    store.put(state,"state");store.put(artifact,"artifact:report");store.put(upload,"optimization-build-upload:explicit:one");
    store.put(true,"optimization-build-seen:one");store.put({cursor:17},"optimization-build-source:stream");store.put("owner-secret","community-owner");
    await ended;db.close();
    const code=await (await fetch("/source/task")).text();
    const broken=code.replace("store.add(row,row.id)","store.add(row,'collision')");
    assert("the upgrade control alters the actual task-row write",broken!==code);
    let rejected=false;
    try{await new Function(broken+";return UserTaskBrowserStore;")().open();}catch{rejected=true;}
    db=await open("wfsim-user-tasks",1);
    const old=await all(db,"queue");
    assert("a failed upgrade retains the complete original database and version",rejected && db.version===1
      && equal([...db.objectStoreNames],["queue"]) && old.length===6 && old.some(row=>equal(row,state)) && old.some(row=>equal(row,artifact)));
    db.close();
    const BrowserStore=new Function(code+";return UserTaskBrowserStore;")();
    let saved=await BrowserStore.open();
    assert("tasks, completion events and pending commands survive with their ordering",equal(await saved.read(),state));
    assert("artifacts and upload payloads retain their original content and identity",equal(await saved.value("artifact:report"),artifact)
      && equal((await saved.takeValues("optimization-build-upload:",10))[0].value,upload)
      && (await saved.value("optimization-build-source:stream")).cursor===17 && await saved.value("community-owner")==="owner-secret");
    assert("physical stores separate individual tasks, reports and coordination",(await all(saved.db,"entity_calculation_tasks")).length===2
      && (await all(saved.db,"entity_calculation_reports")).length===1 && !saved.db.objectStoreNames.contains("queue")
      && [...saved.db.objectStoreNames].every(name=>/^(entity|record|relation|state)_/.test(name)));
    await saved.putOnce([{key:"optimization-build-upload:explicit:one",marker:"optimization-build-seen:one",value:{changed:true}}]);
    assert("existing delivery deduplication receipts still prevent duplicate uploads",equal((await saved.takeValues("optimization-build-upload:",10))[0].value,upload));
    rejected=false;
    try{await saved.writeValues([{key:"artifact:partial",value:{would:"commit"}},{key:"optimization-build-upload:broken",value:()=>{}}]);}catch{rejected=true;}
    assert("a failed cross-store write commits none of its valid earlier writes",rejected && await saved.value("artifact:partial")===null);
    await saved.change(state=>{state.tasks=state.tasks.filter(task=>task.id!=="second");});
    saved.db.close();saved=await BrowserStore.open();
    assert("removing task history and reopening preserves independent reports and delivery",(await saved.read()).tasks.length===1
      && equal(await saved.value("artifact:report"),artifact) && await saved.countValues("optimization-build-upload:")===1);
    saved.db.close();

    const simulation={id:"simulation",at:17,kept:true,r:{score:7,replay:null},engine:"original"};
    db=await open("wfsim",1,db=>db.createObjectStore("results",{keyPath:"id"}));
    tx=db.transaction("results","readwrite");ended=done(tx);tx.objectStore("results").put(simulation);await ended;db.close();
    const resultSource=await (await fetch("/source/result")).text();
    const results=new Function(resultSource+";return {open:resultsDb};")();db=await results.open();
    assert("simulation history retains pins, identity, original time and output",db.version===2
      && !db.objectStoreNames.contains("results") && equal(await all(db,"entity_simulation_reports"),[simulation]));db.close();

    const answer={question:"original-question",lease:"original-lease",engine:"old",verifier:"same-device",work:17,score:3,at:19};
    const pending={question:"pending-question",engine:"old",verifier:"same-device",work:23,at:29};
    db=await open("wfsim-compute",1,db=>{db.createObjectStore("results",{keyPath:"question"});db.createObjectStore("pending",{keyPath:"question"});});
    tx=db.transaction(["results","pending"],"readwrite");ended=done(tx);tx.objectStore("results").put(answer);tx.objectStore("pending").put(pending);await ended;db.close();
    const computeSource=await (await fetch("/source/compute")).text();
    const outputs=new Function(computeSource+";return {open:openResults};")();db=await outputs.open();
    assert("unreturned answers and unconfirmed credit retain their exact protocol payloads",db.version===2
      && equal(await all(db,"entity_client_compute_results"),[answer]) && equal(await all(db,"state_compute_credit_pending"),[pending])
      && !db.objectStoreNames.contains("results") && !db.objectStoreNames.contains("pending"));db.close();

    const conversation={id:"conversation",v:1,pinned:true,updated_at:17,messages:[{role:"user",text:"keep original"}]};
    db=await open("wfsim-nona",1,db=>db.createObjectStore("conversations",{keyPath:"id"}));
    tx=db.transaction("conversations","readwrite");ended=done(tx);tx.objectStore("conversations").put(conversation);await ended;db.close();
    const nona=await import("/nona/runtime/store.js");await nona.conversations();
    db=await open("wfsim-nona",2);
    assert("conversation storage upgrades without rewriting its saved document",db.version===2
      && equal(await all(db,"entity_conversations"),[conversation]) && !db.objectStoreNames.contains("conversations"));db.close();
    return checks;
  })()`);
  for(const name of checks)app.check(name,true);
  console.log(JSON.stringify({ok:true,checks:checks.length,transport:"chromium-indexeddb",scope:"storage migration; no calculator UI or engine assertion"}));
} finally {await app?.close();await new Promise(resolve=>server.close(resolve));}
