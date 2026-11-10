// Help examples must pass the bot's real parser and resolve the requested
// benchmark file, including numeric scene tags. No chat or appraisal is sent.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { loadEngine } from "../bot/engine.mjs";
import { cardDocument } from "../bot/card.mjs";
const {makeAnswer} = await import(process.env.WFSIM_BOT_ANSWER_MODULE || "../bot/answer.mjs");
const ui=JSON.parse(execFileSync(process.env.PYTHON || "python3",["-c",
  "import yaml,json;print(json.dumps(yaml.safe_load(open('data/i18n/zh/ui.yaml'))['ui']))"],{encoding:"utf8"}));
const published=JSON.parse(readFileSync("site/board/torid.json","utf8"));
const requested=[];
let fixtures=new Map(), checks=0;
const server=createServer((req,res)=>{
  requested.push(req.url);
  const rows=fixtures.get(req.url);
  if(!rows && req.url.includes("/heads/")) {res.writeHead(200,{"content-type":"text/html"});res.end("<!doctype html>");return;}
  res.writeHead(rows ? 200 : 404,{"content-type":"application/json"});res.end(JSON.stringify(rows || {}));
});
await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
const engine=await loadEngine({site:`http://127.0.0.1:${server.address().port}`}), h=engine.headless;
Object.assign(engine.zh.ui,ui);
const primary=engine.meta.benchmarks.find(b=>b.primary).id;
const seed=published.find(row=>row.benchmark===primary && row.riven);
const free=published.find(row=>row.benchmark===primary && !row.riven);
assert.ok(seed && free);
for(const [i,b] of engine.meta.benchmarks.entries()) {
  const rows=[{...free,benchmark:b.id,score:100+i,shown:String(100+i)},
    {...seed,benchmark:b.id,score:200+i,shown:String(200+i),riven:{bonuses:["critical_chance","critical_damage","multishot"],malus:"magazine_capacity",rolls:[1,1,1,1]}}];
  const path=h.headlessBoardFile(engine.meta,"torid",b.id);
  fixtures.set(path,[...(fixtures.get(path) || []),...rows]);
}
fixtures.set("/board/heads/torid.json",[...fixtures.values()].flat());
const check=async(name,fn)=>{await fn();checks++;console.log("  ok  "+name);};
try {
  const request=h.headlessNonaHelpRequest(engine.meta), riven=engine.api("/api/riven",request);
  assert.ok(riven.ok && !riven.illegal.length && riven.stats.length===4);
  const guides=["zh","en"].map(lang=>h.headlessNonaHelp(h.headlessSpeaker(engine.meta,engine.i18n,lang),riven));
  let calls=[],opened=[];
  const answer=makeAnswer({...engine,run:async(id,args)=>{calls.push({id,args});return {ok:false};}});
  const context={openAppraisal:async ask=>{opened.push(ask);return {code:"HELP1"};}};
  await check("every illustrated example reaches the real query or legal four-stat appraisal",async()=>{
    for(const guide of guides) for(const entry of guide.entries.filter(e=>e.command!=="gx")) for(const command of entry.examples) {
      calls=[];opened=[];const reply=await answer(command,context);
      const ruler=command.includes(guide.complex.id) ? guide.complex.id : "demolisher";
      if(entry.command==="fx") {
        assert.equal(opened.length,1,JSON.stringify({command,reply}));assert.equal(opened[0].ruler,ruler);
        assert.deepEqual(opened[0].riven.bonuses.map(s=>s.id),request.bonuses.map(s=>s.id));
        assert.equal(opened[0].riven.malus.id,"damage_to_corpus");
      } else {
        assert.equal(calls.length,1,JSON.stringify({command,reply}));assert.equal(calls[0].args.ruler,ruler);
        if(entry.command==="zk") {
          assert.deepEqual(calls[0].args.bonuses,["critical_chance","critical_damage","multishot"]);
          assert.equal(calls[0].args.malus,command===entry.examples[0]?"any":"magazine_capacity");
        } else {assert.equal(calls[0].args.limit,10);assert.equal(calls[0].args.riven,command===entry.examples[0]?"with":"without");}
      }
    }
  });
  await check("all benchmark IDs survive the count parser and reach zk, pz and fx exactly",async()=>{
    const example=guides[0].entries.find(e=>e.command==="fx").examples[0];
    for(const b of engine.meta.benchmarks) {
      for(const suffix of [""," 10"]) for(const cmd of ["zk","pz"]) {
        calls=[];const reply=await answer(`${cmd} Torid ${b.id}${suffix}`);
        assert.equal(calls.length,1,JSON.stringify({b:b.id,cmd,suffix,reply}));assert.equal(calls[0].args.ruler,b.id);
        if(cmd==="pz")assert.equal(calls[0].args.limit,suffix?10:3);
      }
      for(const label of [b.name,h.headlessBenchmarkName(engine.meta,b.id,s=>ui[s] || s)]) {
        calls=[];await answer(`zk Torid ${label} 10`);assert.equal(calls[0]?.args.ruler,b.id,label);
      }
      opened=[];await answer(example.replace(guides[0].complex.id,b.id),context);assert.equal(opened[0]?.ruler,b.id);
    }
    calls=[];await answer("zk Torid");assert.equal(calls[0].args.ruler,primary);
    for(const cmd of ["zk","pz"]) {calls=[];await answer(`${cmd} Torid ${primary}__unknown 10`);assert.equal(calls.length,0);}
  });
  await check("exact cell queries read the scene file and filter all-cell heads without switching boards",async()=>{
    for(const [i,b] of engine.meta.benchmarks.entries()) {
      const rv=await engine.run("builder.rivens.read",{weapon:"torid",ruler:b.id,pooled:true});
      assert.equal(rv.groups.length,1,b.id);assert.equal(rv.groups[0].ruler_id,b.id);assert.equal(rv.groups[0].rivens[0].score,String(200+i));
      const board=await engine.run("builder.board.read",{weapon:"torid",ruler:b.id,pooled:true,distinct:true,limit:10});
      assert.equal(board.rows.length,1,b.id);assert.equal(board.rows[0].ruler_id,b.id);assert.equal(board.rows[0].score,String(100+i));
      assert.ok(requested.includes(h.headlessBoardFile(engine.meta,"torid",b.id)));
    }
    const real=makeAnswer(engine), target=guides[0].complex.id;
    const result=await real(`zk Torid ${target} 双暴 多重 负任意 10`);
    assert.equal(new URL(result.card).searchParams.get("ruler"),target,JSON.stringify(result));
    const doc=await cardDocument(engine,result.card,{css:""});assert.ok(doc.html.includes(guides[0].complex.name));
    assert.equal(h.headlessBoardFile(engine.meta,"torid","missing"),null);
  });
  await check("help ships an image with copyable complex commands and a complete ruler index fallback",async()=>{
    for(const topic of [""," 标尺"]) {
      const reply=await answer("帮助"+topic), guide=reply.cardData.help;
      assert.equal(guide.topic,topic?"rulers":"commands");assert.ok(reply.card.startsWith("https://wfsim.app/nona/help/card"));
      assert.ok(reply.line.includes(guides[0].complex.id) && reply.text.includes(guides[0].complex.id));
      assert.ok(guide.groups.flatMap(g=>g.rows).length===engine.meta.benchmarks.length);
      if(topic)for(const b of engine.meta.benchmarks)assert.ok(reply.text.includes(b.id));
      const doc=await cardDocument(engine,reply.card,{css:"",help:guide});assert.ok(doc.html.includes("nona-help"));
      assert.ok(topic || doc.html.includes(guide.entries[2].examples[0]));
      assert.equal(doc.verdict,null);
    }
    assert.ok(!h.headlessNonaHelp(h.headlessSpeaker(engine.meta,engine.i18n,"zh"),null).entries[2].examples.length);
  });
  await check("a missing heads file served as the SPA falls back to the requested cell",async()=>{
    fixtures.delete("/board/heads/torid.json");
    const fallback=await loadEngine({site:engine.site}),ruler=guides[0].complex.id;
    const result=await fallback.run("builder.board.read",{weapon:"torid",ruler,limit:10});
    assert.ok(result.rows.length && result.rows.every(row=>row.ruler_id===ruler));
  });
  console.log(`ok  ${checks} help contracts, ${engine.meta.benchmarks.length} rulers and both locales`);
} finally {server.closeAllConnections();server.close();}
