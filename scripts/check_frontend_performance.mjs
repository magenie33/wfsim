import {openApp} from "./cdp.mjs";

const app=await openApp({base:process.env.WFSIM_BASE,lang:"en",boot:8000});
const {evaluate,check,sleep}=app;
try {
  const meta=await evaluate("({benchmarks:META.benchmarks,weapons:META.weapons.map(w=>({id:w.id,modes:w.modes,mods:w.mods}))})");
  const primary=meta.benchmarks.find(b=>b.primary)||meta.benchmarks[0];
  const dirs=[...new Set(meta.benchmarks.map(b=>b.cell_dir||""))];
  const requests=[],rowsPerCell=1800;
  const payloads=new Map();
  function rows(weapon,dir) {
    const key=weapon+":"+dir;
    if(!payloads.has(key)) {
      const w=meta.weapons.find(w=>w.id===weapon)||meta.weapons[0];
      const mods=w.mods.slice(0,8);
      payloads.set(key,JSON.stringify(meta.benchmarks.filter(b=>(b.cell_dir||"")===dir).flatMap(b=>
        Array.from({length:rowsPerCell},(_,i)=>({benchmark:b.id,mode:w.modes[0],score:rowsPerCell-i,
          mods,arcanes:[],evolutions:[],fixture:i})))));
    }
    return payloads.get(key);
  }
  app.on("Fetch.requestPaused",async event=>{
    const path=new URL(event.request.url).pathname;
    if(path.startsWith('/board/heads/')) {
      await app.send("Fetch.fulfillRequest",{requestId:event.requestId,responseCode:404,body:""});return;
    }
    let body;
    if(path==="/board.meta.json")body=JSON.stringify({scene_weapons:Object.fromEntries(dirs.filter(Boolean).map(dir=>[dir,["torid","furis"]]))});
    else {
      const parts=path.slice("/board/".length).split("/"),weapon=parts.at(-1).replace(/\.json$/,"");
      const dir=parts.length>1?parts[0]:"";
      requests.push({weapon,dir});body=rows(weapon,dir);
    }
    await app.send("Fetch.fulfillRequest",{requestId:event.requestId,responseCode:200,
      responseHeaders:[{name:"Content-Type",value:"application/json"}],body:Buffer.from(body).toString("base64")});
  });
  await app.send("Fetch.enable",{patterns:[{urlPattern:"*/board/*.json",requestStage:"Request"},
    {urlPattern:"*/board.meta.json",requestStage:"Request"}]});
  await app.load("/",4000);
  const cold=await evaluate("({ready:window.__wfsimReady,editor:weaponEditorReady,catalog:WFCAT,rows:Object.keys(BOARD).length,usable:window.__wfsimPerformance?.usable})");
  check("home becomes usable without initializing a weapon editor or catalog",cold.ready&&!cold.editor&&!cold.catalog&&cold.rows===0&&cold.usable>0,JSON.stringify(cold));
  check("home downloads no weapon board payloads",requests.length===0,JSON.stringify(requests));
  const catalog=await evaluate(`(async()=>{
    const original=api, saved=WFCAT, answer=await original('/api/warframe/catalog',{});let calls=0;
    WFCAT=null;warframeCatalogPending=null;
    api=async(path,...args)=>{if(path!=='/api/warframe/catalog')return original(path,...args);calls++;await new Promise(r=>setTimeout(r,20));return answer;};
    try {
      const pending=Array.from({length:12},()=>loadWarframeCatalog()),shared=pending.every(p=>p===pending[0]);
      const results=await Promise.all(pending),once=calls===1&&results.every(result=>result===results[0]);
      WFCAT=null;let fail=true;api=async(path,...args)=>{if(path!=='/api/warframe/catalog')return original(path,...args);calls++;if(fail){fail=false;throw Error('fixture');}return answer;};
      let rejected=false;try{await loadWarframeCatalog();}catch{rejected=true;}
      await loadWarframeCatalog();return {shared,once,retry:rejected&&calls===3};
    } finally {api=original;WFCAT=saved;warframeCatalogPending=null;}
  })()`);
  check("concurrent catalog readers share one load and one result",catalog.shared&&catalog.once);
  check("a failed catalog load can be retried",catalog.retry);
  const animation=await evaluate(`(async()=>{
    tallyFrom={volunteers:100,official:200};tallyTo={volunteers:200,official:300};tallyAt=Date.now();
    renderHomeTally();await new Promise(r=>requestAnimationFrame(r));
    const node=$('home-tally-n').querySelector('[data-tally="volunteers"]'), text=node.firstChild;let mutations=0;
    const observer=new MutationObserver(records=>{mutations+=records.length;});observer.observe($('home-tally-n'),{childList:true,subtree:true});
    for(let i=0;i<10;i++){tallyAt-=1000;renderHomeTally();}
    await new Promise(r=>setTimeout(r,50));observer.disconnect();
    const stable=node===$('home-tally-n').querySelector('[data-tally="volunteers"]')&&text===node.firstChild&&mutations===0;
    const before=node.textContent;$('home-page').hidden=true;tallyAt-=1000;renderHomeTally();const hidden=before===node.textContent;$('home-page').hidden=false;
    return {stable,moving:Number(node.textContent)>100,hidden};
  })()`);
  check("animated tally numbers move without rebuilding their DOM",animation.stable&&animation.moving);
  check("a hidden home tally does not repaint",animation.hidden);
  await app.send("Emulation.setCPUThrottlingRate",{rate:4});
  if(process.env.WFSIM_PERF_EAGER)await evaluate("window.perfEagerRead=loadSceneRows;loadSceneRows=(id,base)=>window.perfEagerRead(id,base,benchScenes())");
  await evaluate("window.perfCheckLag=0;window.perfCheckLast=performance.now();window.perfCheckTimer=setInterval(()=>{const now=performance.now();window.perfCheckLag=Math.max(window.perfCheckLag,now-window.perfCheckLast-25);window.perfCheckLast=now;},25)");
  await evaluate("history.pushState(null,'','/weapons/Torid/');route()");
  const waitFor=async expression=>{
    for(let i=0;i<100;i++){if(await evaluate(expression))return true;await sleep(100);}return false;
  };
  check("the large published fixture opens",await waitFor("document.body.classList.contains('on-build-library')&&!!document.querySelector('#build-finder tr.fr')"));
  check("opening a weapon downloads only its selected scene",requests.length===1&&requests[0].dir===(primary.cell_dir||""),JSON.stringify(requests));
  const view=await evaluate("({rows:builtinBuilds(finder.b).length,raw:BOARD.torid.length,visible:document.querySelectorAll('#build-finder tr.fr').length})");
  check("ranking covers the complete selected ruler while DOM results stay bounded",view.rows===rowsPerCell&&view.raw>=view.rows&&view.visible===20,JSON.stringify(view));
  for(const dir of dirs.filter(dir=>dir!==(primary.cell_dir||""))) {
    const b=meta.benchmarks.find(b=>(b.cell_dir||"")===dir);
    await evaluate("finder.b="+JSON.stringify(b.id)+";renderBuildFinder()");
    check("scene remains reachable: "+(dir||"base"),await waitFor("boardHasCell('torid',"+JSON.stringify(b.id)+")&&finder.b==="+JSON.stringify(b.id)+"&&document.querySelectorAll('#build-finder tr.fr').length>0"));
  }
  check("each requested scene downloads once",requests.filter(r=>r.weapon==="torid").length===dirs.length&&new Set(requests.filter(r=>r.weapon==="torid").map(r=>r.dir)).size===dirs.length,JSON.stringify(requests));
  const bounded=await evaluate("({loaded:boardFiles.get('torid').loaded.size,rows:BOARD.torid.length,lag:window.perfCheckLag})");
  check("scene switches release inactive scene payloads",bounded.loaded<=2&&bounded.rows<=rowsPerCell*8,JSON.stringify(bounded));
  check("large scene switches remain responsive under CPU throttling",bounded.lag<1500,JSON.stringify(bounded));
  await app.send("Emulation.setCPUThrottlingRate",{rate:1});
  const complete=await evaluate("(async()=>{await loadAllWeaponBoard('torid');return BOARD.torid.length;})()");
  check("whole-board tools can explicitly read every scene",complete===rowsPerCell*meta.benchmarks.length,String(complete));
  const fallback=await evaluate("(async()=>{releaseBoardCache(null);history.pushState(null,'','/weapons/Torid/card?kind=pz');await loadWeaponBoard('torid');return BOARD.torid.length;})()");
  check("a card without heads falls back to complete scene files",fallback===rowsPerCell*meta.benchmarks.length,String(fallback));
  await evaluate("clearInterval(window.perfCheckTimer);history.pushState(null,'','/weapons/Furis/builder');route()");
  check("switching weapon releases the previous public board",await waitFor("routeWeaponId()==='furis'&&BOARD_HAVE.has('furis')&&!BOARD.torid&&finderDataMemo===null"));
  await evaluate("history.pushState(null,'','/');route()");
  check("leaving weapons releases board and derived caches",await waitFor("!Object.keys(BOARD).length&&!boardFiles.size&&builtinMemo===null&&builtinRowsMemo===null&&finderDataMemo===null&&pageTiming.kind==='home'&&pageTiming.usable>0"));
  const timing=await evaluate(`(()=>{
    const original=track,sent=[];track=(event,subject,n)=>sent.push({event,subject,n});
    try{pageTiming.interaction=128;flushPageTiming();return {usable:pageTiming.usable,sent};}finally{track=original;}
  })()`);
  check("performance observations carry page kinds and numeric durations",timing.usable>0&&["app.longtask","app.blocking","app.interaction"].every(event=>timing.sent.some(row=>row.event===event&&row.subject==="home"&&Number.isFinite(row.n))),JSON.stringify(timing));
  await app.finish("startup, scene work, cache lifetimes and animation stay bounded");
} catch(error) {await app.close();throw error;}
