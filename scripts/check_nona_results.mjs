import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { testDb } from "./pg/test_db.mjs";
import { serveSite } from "./cdp.mjs";
import { loadEngine } from "../bot/engine.mjs";
import { contributorProfilesRoute } from "../worker/contributor_profiles.js";
import { clientSource } from './ci_client_inputs.mjs';
const { makeAnswer } = await import(process.env.WFSIM_BOT_ANSWER_MODULE || "../bot/answer.mjs");
const ui = JSON.parse(execFileSync("python3", ["-c", "import yaml,json,sys;print(json.dumps(yaml.safe_load(sys.stdin)['ui']))"],
  {encoding:"utf8", input: clientSource('scripts/check_nona_results.mjs')}));
const server = await serveSite("site"), site = "http://127.0.0.1:" + server.address().port;
const engine = await loadEngine({site}), h = engine.headless;
const env = {ACCOUNTS:await testDb("accounts"),LIBRARY:await testDb("library")};
const say = h.headlessSpeaker(engine.meta,{zh:{...engine.zh,ui}},"zh"), weapon = engine.meta.weapons.find(w=>w.id === "laetum");
const published = await engine.host.board(weapon.id), ask = h.headlessCardAsk("?kind=zk&n=2");
const selected = h.headlessCardBoard(say,weapon,ask,published);
let checks = 0, batches = 0, requested = [], firstCardVersion;
const check = async (name, fn) => {await fn();checks++;console.log("  ok  " + name);};
try {
  for (const [i,id] of ["first","second"].entries()) {
    await env.ACCOUNTS.run("INSERT INTO accounts(id,created_at,username,display_name,profile_id) VALUES (?,'2026-01-01',?,?,?)",id,id,id,String(i+1).repeat(32));
    await env.ACCOUNTS.run("INSERT INTO contribution_choice(account,named,chosen_at) VALUES (?,1,'initial')",id);
    await env.ACCOUNTS.run("INSERT INTO device_spans(verifier,account,from_hour) VALUES (?,?,'')",String(i+1).repeat(24),id);
  }
  const keys = engine.api("/api/build/keys",{kind:"build",builds:selected.rows.map(row=>h.headlessBoardRecord(weapon.id,row))}).keys;
  assert.ok(selected.entries.length === 2 && selected.top);
  for (const [i,row] of selected.rows.entries()) {
    const at = new Date(row.measured_at).toISOString(), identity = keys[i], mode = row.mode || "base";
    await env.LIBRARY.run("INSERT INTO builds(id,at,record) VALUES (?,?,?) ON CONFLICT DO NOTHING",identity,at,JSON.stringify(h.headlessBoardRecord(weapon.id,row)));
    await env.LIBRARY.run("INSERT INTO scores(identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at) VALUES (?,?,?,?,?,'kpm',0,?,?)",
      identity,row.benchmark,mode,"verified:"+row.measured_by,row.score,at,at);
    for (const [position,id] of ["first","second"].entries()) await env.LIBRARY.run(`INSERT INTO score_witnesses
      (identity,ruler,mode,measured_at,engine,owner,device,official,completed_at,credited_at,position)
      VALUES (?,?,?,?,?,?,?,0,?,?,?)`,identity,row.benchmark,mode,at,row.measured_by,id,String(position+1).repeat(24),at,at,position);
  }
  const names = async body => {
    batches++; requested=body.rows;
    const response = await contributorProfilesRoute(new Request("https://wfsim.test/api/computations/names",{method:"POST",
      headers:{"content-type":"application/json"},body:JSON.stringify(body)}),env,"/api/computations/names");
    assert.equal(response.status,200);return response.json();
  };
  const lookup = rows => h.headlessMeasurementWitnesses(weapon.id,rows,{api:async(path,body)=>engine.api(path,body),names});
  let witnesses;
  await check("zk resolves only the displayed entries and baseline in one exact-measurement batch",async()=>{
    witnesses=await lookup(selected.rows);
    assert.equal(batches,1);assert.equal(requested.length,3);
    assert.deepEqual(witnesses.get(selected.entries[0].row).map(w=>w.name),["first","second"]);
  });
  await check("the shared card shows calculation and verification roles with public profile links",()=>{
    const card=h.headlessLongCard(say,weapon,ask,{rows:published,witnesses,origin:"https://wfsim.app",
      riven:(w,spec)=>h.headlessRivenAnswer(say,engine.api("/api/riven",{weapon:w.id,...spec}))});
    const html=card.draw("");
    assert.ok(html.includes("计算:") && html.includes("验证:"));
    assert.ok(html.includes("/user/"+"1".repeat(32)) && html.includes("/user/"+"2".repeat(32)));
    assert.ok(html.indexOf("first") < html.indexOf("second"));
    assert.ok(!html.includes("wfsim_session"));
    assert.ok([...witnesses.values()].every(list=>list.every(w=>!("device" in w) && !("owner" in w))));
  });
  engine.host.computationNames=(id,rows)=>lookup(rows);
  const answer=makeAnswer({...engine,zh:{...engine.zh,ui}});
  await check("zk fallback text retains both actual witnesses and makes no personal-computation claim",async()=>{
    const reply=await answer("zk Laetum 2");firstCardVersion=reply.cardVersion;
    assert.ok(reply.text.includes("计算: first") && reply.text.includes("验证: second"),JSON.stringify(reply));
    assert.ok(reply.cardData.witnesses instanceof Map && reply.cardData.rows === published);
    assert.ok(!/每张都跑|I ran every/i.test(reply.line));
  });
  await check("all fixed reply and menu keys have Chinese copy and preserve template variables",()=>{
    for(const path of ["bot/answer.mjs","bot/menus.mjs"]) {
      const source=readFileSync(path,"utf8");
      const keys=[...source.matchAll(/\bt\(\s*("(?:[^"\\]|\\.)*")/g),...source.matchAll(/const (?:UNREAD|NO_WEAPON|NOT_FOUND|NOT_MEASURED) = ("(?:[^"\\]|\\.)*")/g)].map(m=>JSON.parse(m[1]));
      for(const key of keys) {
        assert.equal(typeof ui[key],"string",key);
        assert.deepEqual([...key.matchAll(/\{([a-z_]+)\}/g)].map(m=>m[1]).sort(),[...ui[key].matchAll(/\{([a-z_]+)\}/g)].map(m=>m[1]).sort(),key);
      }
      assert.ok(!/usually a (?:minute|few)|Scan the code|I ran every one/.test(source));
    }
  });
  await check("FX receipt, running and delivered messages distinguish replay from independent verification",async()=>{
    const help=(await answer("帮助")).text;
    for(const command of ["zk ","pz ","fx ","gx "]) assert.ok(help.includes(command));
    const rv=selected.entries[0].row.riven;
    const card=engine.api("/api/riven",{weapon:weapon.id,rank:8,bonuses:rv.bonuses.map((id,i)=>({id,roll:rv.rolls[i]})),
      malus:rv.malus ? {id:rv.malus,roll:rv.rolls[rv.bonuses.length]} : null});
    const receipt=await answer("fx Laetum "+card.stats.map(stat=>stat.text).join(" "),{openAppraisal:async()=>({code:"CHECK"})});
    assert.ok(receipt.line?.includes("CHECK") && receipt.line.includes("复算") && !/分钟|扫码/.test(receipt.line),JSON.stringify(receipt));
    const started=answer.started({code:"CHECK"});
    assert.ok(started.includes("接手") && started.includes("复算") && !/分钟|验证完成/.test(started));
    const item={weapon:weapon.id,calculator:{name:"first",mark:{label:{en:"Fixture",zh:"测试身份"}}},verdict:{shown:"12.34",gain:0.2,grade:42,of:7}};
    const delivered=answer.told(item,false);
    for(const text of ["复算通过","12.34","+20.0%","42/100","7","交付：first · 测试身份"]) assert.ok(delivered.includes(text),delivered);
    assert.ok(!/双人|两人|验证完成|志愿者的设备/.test(delivered));
    assert.ok(answer.told({...item,calculator:{name:null}},true).includes(ui["Anonymous contributor"]));
  });
  await check("anonymity, renames and no-label selection apply to zk without changing the score",async()=>{
    await env.ACCOUNTS.run("UPDATE contribution_choice SET named=0 WHERE account='first'");
    let changed=await lookup(selected.rows), one=changed.get(selected.entries[0].row)[0];
    assert.equal(one.name,null);assert.equal(one.profile,null);assert.ok(!one.mark);
    await env.ACCOUNTS.run("UPDATE accounts SET display_name='second new' WHERE id='second'");
    changed=await lookup(selected.rows);assert.equal(changed.get(selected.entries[0].row)[1].name,"second new");
    assert.equal(requested[1].score,selected.entries[0].row.score);
    assert.notEqual((await answer("zk Laetum 2")).cardVersion,firstCardVersion);
  });
  await check("a renamed or anonymous contributor invalidates the cached image even on the same board",async()=>{
    const source=readFileSync("bot/qq.mjs","utf8"),start=source.indexOf("async function cardFor("),end=source.indexOf("\n}\n",start)+2;
    assert.ok(start>=0 && end>start);
    let draws=0;
    const cached=new Function("boardDigest","cards","drawCard","CARD_KEEP_MS","CARD_KEEP_MAX",source.slice(start,end)+";return cardFor;")(
      async()=>"same-board",new Map(),async(url,snapshot)=>({png:JSON.stringify({number:++draws,snapshot})}),600000,64);
    const first=await cached("card","public-name",{label:"first"});
    assert.equal(await cached("card","public-name",{label:"first"}),first);assert.equal(draws,1);
    const hidden=await cached("card","anonymous",{label:"hidden"});
    assert.notEqual(hidden,first);assert.equal(draws,2);assert.ok(hidden.includes("hidden"));
  });
  await check("command failures return a short retry message without internal details",async()=>{
    const broken=makeAnswer({...engine,zh:{...engine.zh,ui},run:async()=>{throw new Error("internal fixture detail");}});
    const before=console.error;console.error=()=>{};
    try {const reply=await broken("pz Laetum");assert.ok(reply.text.includes("稍后") && !reply.text.includes("fixture"));}
    finally {console.error=before;}
  });
  await check("stale measurement tuples cannot borrow the current witnesses",async()=>{
    const stale={...selected.entries[0].row,score:selected.entries[0].row.score+100};
    assert.deepEqual((await lookup([stale])).get(stale),[]);
  });
  await check("lookup failure keeps the result and explicitly reports unavailable attribution",async()=>{
    const row=selected.entries[0].row, unavailable=await h.headlessMeasurementWitnesses(weapon.id,[row],{
      api:async(path,body)=>engine.api(path,body),names:async()=>{throw new Error("offline");}});
    assert.equal(unavailable.get(row),null);
    assert.equal(h.headlessWitnessesText(say,unavailable.get(row)),ui["Attribution is unavailable."]);
    assert.ok(!h.headlessWitnessesHtml(say,[{name:null,profile:"1".repeat(32)}],"https://wfsim.app").includes("/user/"));
  });
} finally {await env.ACCOUNTS.close();await env.LIBRARY.close();server.closeAllConnections();server.close();}
console.log(`\n${checks} Nona result checks passed`);
