// The real sync client under delayed replies, retries and malformed cloud rows.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const source = readFileSync(new URL("../web/src/static/app/37-sync.js", import.meta.url), "utf8");
const list = "wfsim-presets-builder-builds";
function browser() {
  const handlers=[];
  const storage = {};
  Object.defineProperties(storage, {
    getItem: { value: (k) => storage[k] ?? null },
    setItem: { value: (k, v) => { storage[k] = String(v); } },
    removeItem: { value: (k) => { delete storage[k]; } },
  });
  const context = vm.createContext({ localStorage: storage, accountState: { account: { id: "a" } },
    REMINDERS_KEY: "wfsim-reminders", document: { addEventListener(name,fn) { handlers.push({name,fn}); } }, window: { addEventListener() {} },
    navigator: {}, setTimeout, clearTimeout, console, Response, structuredClone,
    flushPresetSaves() {}, ownerKeyed() { return null; }, customRefs() { return []; },
    presetNewId: (() => { let id = 0; return () => "copy" + ++id; })(), presetDoc() { return null; },
    presetToast() {}, noteInline() {}, tr: (s) => s, COLLECTIONS: [], undoStack: [], redoStack: [],
    absentHolds: new Map(), buildWanted: null, renderSyncStatus() {}, crypto: globalThis.crypto,
  });
  vm.runInContext(source, context);
  vm.runInContext("syncDevice=()=>({id:'device',label:'Browser'});syncReport=async()=>{};syncShow=()=>{}", context);
  return { context, storage, handlers, run: (code) => vm.runInContext(code, context) };
}
let failed = 0;
async function check(name, fn) {
  try { await fn(); console.log("  ok  " + name); }
  catch (e) { failed++; console.error("FAIL  " + name + ": " + e.message); }
}
await check("automatic first login changes no saved data and sends no sync request", async () => {
  const b = browser(); let calls = 0;
  b.context.fetch = async () => { calls++; return new Response(JSON.stringify({ ok: true, entries: [], cursor: 0 })); };
  b.storage.setItem(list, JSON.stringify([{ id: "x", name: "01", state: {} }]));
  const before = b.storage.getItem(list);
  await b.run("syncNow({automatic:true})");
  assert.equal(calls, 0); assert.equal(b.storage.getItem(list), before);
  assert.equal(b.run("syncStatus.state"), "paused");
});
await check("property order and save timestamps do not create content changes", () => {
  const b = browser();
  assert.equal(b.run("syncSig({id:'x',name:'01',state:{a:1,b:2},savedAt:1})"),
    b.run("syncSig({state:{b:2,a:1},name:'01',id:'x',savedAt:99})"));
});
await check("an identical stale-version reply creates no conflict copy", () => {
  const b = browser();
  b.storage.setItem(list, JSON.stringify([{ id: "x", name: "01", state: { a: 1 }, savedAt: 1 }]));
  b.run("syncConflict({known:{}},{id:'x',list:'" + list + "',body:{id:'x',name:'01',state:{a:1}}},{id:'x',list:'" + list + "',version:5,body:{state:{a:1},name:'01',id:'x',savedAt:9}},{lists:new Set(),ids:new Set(),removed:new Set()})");
  assert.equal(JSON.parse(b.storage.getItem(list)).length, 1);
});
await check("a genuine conflict keeps the local copy off the upload queue", () => {
  const b = browser();
  b.storage.setItem(list, JSON.stringify([{ id: "x", name: "01", state: { a: 1 } }]));
  b.run("syncConflict({known:{}},{id:'x',list:'" + list + "',body:{id:'x',name:'01',state:{a:1}}},{id:'x',list:'" + list + "',version:5,body:{id:'x',name:'01',state:{a:2}}},{lists:new Set(),ids:new Set(),removed:new Set()})");
  const rows = JSON.parse(b.storage.getItem(list));
  assert.equal(rows.length, 2); assert.equal(rows.find((r) => r.id !== "x").cloud_sync, false);
});
await check("a malformed pull cannot overwrite any local entry", () => {
  const b = browser();
  b.storage.setItem(list, JSON.stringify([{ id: "x", name: "01", state: { a: 1 } }]));
  const before = b.storage.getItem(list);
  try { b.run("syncApply({known:{}},[{id:'x',list:'" + list + "',version:2,body:{id:'wrong',state:[]}}],new Map())"); } catch (_) { /* rejected */ }
  assert.equal(b.storage.getItem(list), before);
});
await check("a reply to an account left during the round applies no cloud data", async () => {
  const b = browser(); let reply;
  b.storage.setItem("wfsim-sync", JSON.stringify({ account: "a", known: {}, cursor: 0 }));
  b.context.fetch = () => new Promise((ok) => { reply = ok; });
  const pending = b.run("syncNow()");
  while (!reply) await new Promise((ok) => setTimeout(ok, 0));
  b.context.accountState.account = { id: "b" };
  reply(new Response(JSON.stringify({ ok: true, cursor: 5, entries: [{ id: "private-a", list,
    version: 1, updated_at: 1, body: { id: "private-a", state: {} } }] })));
  await pending;
  assert.equal(b.storage.getItem(list), null);
});
await check("two tabs serialize their rounds and share the acknowledged base", async () => {
  const a=browser(), b=browser(); b.context.localStorage=a.storage;
  let chain=Promise.resolve(), release, pushes=0;
  const locks={request:(_,fn)=>{const task=chain.then(fn);chain=task.catch(()=>{});return task;}};
  a.context.navigator.locks=b.context.navigator.locks=locks;
  a.storage.setItem(list,JSON.stringify([{id:"x",name:"01",state:{}}]));
  const fetch=async (_,options)=>{
    const input=JSON.parse(options.body);
    if(input.changes?.length){pushes++;if(pushes===1)await new Promise(ok=>{release=ok;});}
    return new Response(JSON.stringify({ok:true,versions:{x:1},entries:[],cursor:0}));
  };
  a.context.fetch=b.context.fetch=fetch;
  const first=a.run("syncNow()"),second=b.run("syncNow()");
  while(!release)await new Promise(ok=>setTimeout(ok,0));
  await new Promise(ok=>setTimeout(ok,0));
  const concurrent=pushes;release();await Promise.all([first,second]);
  assert.equal(concurrent,1);assert.equal(pushes,1);
});
await check("pausing during a pull prevents that reply from changing local data", async () => {
  const b=browser();let reply;
  b.storage.setItem("wfsim-sync",JSON.stringify({account:"a",known:{},cursor:0}));
  b.context.fetch=()=>new Promise(ok=>{reply=ok;});
  const pending=b.run("syncNow()");while(!reply)await new Promise(ok=>setTimeout(ok,0));
  b.run("syncPause()");
  reply(new Response(JSON.stringify({ok:true,cursor:1,entries:[{id:"x",list,version:1,body:{id:"x",state:{}}}]})));
  await pending;assert.equal(b.storage.getItem(list),null);assert.equal(b.run("syncState().enabled"),false);
});
await check("overlap pages cannot replace an acknowledged newer version",()=>{
  const b=browser();b.storage.setItem(list,JSON.stringify([{id:"x",state:{n:3}}]));
  b.run("syncApply({known:{x:{v:3}}},[{id:'x',list:'"+list+"',version:2,body:{id:'x',state:{n:2}}}],new Map())");
  assert.equal(JSON.parse(b.storage.getItem(list))[0].state.n,3);
});
await check("restoring one cloud entry never enables a paused browser or uploads its local collection",async()=>{
  const b=browser();let pushes=0;const rendered=[];
  b.storage.setItem("wfsim-sync",JSON.stringify({account:"a",known:{},cursor:0,enabled:false}));
  b.storage.setItem(list,JSON.stringify([{id:"local",state:{}}]));
  b.context.location={pathname:"/account/sync"};
  b.context.authKindOf=()=>"sync";
  b.context.renderAuthPage=kind=>rendered.push(kind);
  b.context.onTyped=()=>{};
  b.context.fetch=async(_,o)=>{
    const input=JSON.parse(o.body);if(input.changes?.length)pushes++;
    return new Response(JSON.stringify({ok:true,restored:["gone"],entries:[],cursor:0}));
  };
  vm.runInContext(readFileSync(new URL("../web/src/static/app/19-cloud-page.js",import.meta.url),"utf8"),b.context);
  const button={dataset:{crestore:"gone"},disabled:false};
  await b.handlers.find(h=>h.name==="click").fn({target:{closest:()=>button}});
  assert.equal(b.run("syncState().enabled"),false);assert.equal(pushes,0);
  assert.deepEqual(rendered,["sync"]);
});
process.exitCode = failed ? 1 : 0;
