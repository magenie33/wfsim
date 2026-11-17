import {openApp} from "./cdp.mjs";
import {mkdirSync, writeFileSync} from "node:fs";
import {join} from "node:path";

async function checkHomeHero(app) {
  const waitFor = async expression => {
    for (let i = 0; i < 200; i++) { if (await app.evaluate(expression)) return true; await app.sleep(100); }
    return false;
  };
  const errors = [];
  app.on("Runtime.exceptionThrown", event => errors.push(event.exceptionDetails.exception?.description || event.exceptionDetails.text));
  const previousConsent = await app.evaluate("localStorage.getItem('wfsim-compute-consent')");
  const fixture = await app.send("Page.addScriptToEvaluateOnNewDocument", { source: `
    localStorage.setItem("wfsim-compute-consent", JSON.stringify({ v: 3, on: false, at: new Date().toISOString() }));
    window.__homeFixture = { builds: 54321987, scores: 123456789, devices: 123, snapshotDevices: 9999 };
    window.__homeReads = [];
    window.__homeRealFetch = window.fetch;
    window.fetch = async (address, options) => {
      const url = new URL(typeof address === 'string' ? address : address.url, location.origin);
      const f = window.__homeFixture;
      const reply = body => new Response(JSON.stringify(body), { status: f.fail ? 503 : 200, headers: { 'content-type': 'application/json' } });
      if (url.pathname === '/board.meta.json') {
        window.__homeReads.push(['builds', url.href]);
        return reply({ boards: { a: { submissions: f.builds, listed: 777 }, b: { submissions: f.builds, listed: 999 } } });
      }
      if (url.pathname === '/api/board/pages/catalog') return reply({ ok: true, partitions: [], next: null });
      if (url.pathname.startsWith('/board/')) return reply(url.pathname.endsWith('/index.json') ? {} : []);
      if (url.pathname === '/stats/tally.json' || url.pathname === '/api/board/tally') {
        window.__homeReads.push(['scores', url.href]);
        return reply({ ok: true, v: 1, at: new Date().toISOString(), totals: { volunteers: f.scores, official: 20 }, per_hour: { volunteers: 60000 }, computing: f.snapshotDevices });
      }
      if (url.pathname === '/work.json') {
        window.__homeReads.push(['devices', url.href]);
        return reply({ v: 1, at: new Date().toISOString(), computing: f.devices, mode: 'wait' });
      }
      return window.__homeRealFetch(address, options);
    };
  ` });
  try {
    await app.send("Network.enable");
    await app.send("Network.setCacheDisabled", { cacheDisabled: true });
    await app.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
    let currentLang;
    for (const [name, lang, theme, width, height, font] of [
      ["desktop-dark", "zh", "dark", 1440, 1080],
      ["desktop-light", "zh", "light", 1440, 1080],
      ["mobile-dark", "zh", "dark", 390, 1000],
      ["small-english", "en", "light", 320, 1000],
      ["small-english-wide-header", "en", "light", 320, 1000, "Verdana, sans-serif"],
      ["desktop-english", "en", "dark", 1440, 1080]
    ]) {
      await app.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: width < 500 });
      if (currentLang !== lang) { await app.setLang(lang, 100); currentLang = lang; }
      const ready = await waitFor("window.__wfsimReady && document.querySelectorAll('#home-facts .hf,#home-tally-n .hf').length === 3");
      app.check(name + " hero and three sourced counters arrive", ready && await app.evaluate("!!document.querySelector('.home-hero')"));
      await app.evaluate("document.documentElement.dataset.theme = " + JSON.stringify(theme));
      await app.evaluate("document.querySelector('.topbar-inner').style.fontFamily = " + JSON.stringify(font || ""));
      await app.sleep(100);
      const state = await app.evaluate(`(() => {
        const stats = [...document.querySelectorAll('#home-facts .hf,#home-tally-n .hf')];
        return { lang: LANG, width: innerWidth, scroll: document.documentElement.scrollWidth,
          overflow: stats.filter(node => node.scrollWidth > node.clientWidth + 1).map(node => node.dataset.homeStat),
          labels: stats.map(node => node.querySelector('.metric-label')?.textContent || ''),
          search: !!document.querySelector('.home-entry #wsearch-input'), cards: document.querySelectorAll('#home-sections .wcard').length };
      })()`);
      app.check(name + " has no horizontal overflow or clipped counters", state.scroll <= width && state.width <= width && state.overflow.length === 0, JSON.stringify(state));
      app.check(name + " keeps the real search, equipment and localized labels", state.search && state.cards > 0 && state.lang === lang
        && state.labels.every(label => lang === "zh" ? /[一-鿿]/.test(label) : /^[\x00-\x7f]+$/.test(label)), JSON.stringify(state));
      app.check(name + " loads the actual equipment art", await waitFor("[...document.querySelectorAll('#home-sections .wcard img')].some(img => img.naturalWidth > 0)"));
      if (process.env.WFSIM_HOME_SCREENSHOTS) {
        mkdirSync(process.env.WFSIM_HOME_SCREENSHOTS, { recursive: true });
        const image = await app.send("Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
        writeFileSync(join(process.env.WFSIM_HOME_SCREENSHOTS, name + ".png"), Buffer.from(image.result.data, "base64"));
      }
    }
    for (const width of [1024, 768, 700, 360]) {
      await app.send("Emulation.setDeviceMetricsOverride", { width, height: 1000, deviceScaleFactor: 1, mobile: width < 500 });
      await app.sleep(100);
      const bounds = await app.evaluate("({ width: innerWidth, scroll: document.documentElement.scrollWidth, clipped: [...document.querySelectorAll('.home-stats .hf')].some(node => node.scrollWidth > node.clientWidth + 1) })");
      app.check(width + "px home stays inside the viewport", bounds.scroll <= width && bounds.width <= width && !bounds.clipped, JSON.stringify(bounds));
    }
    await app.evaluate("document.activeElement.blur()");
    await app.send("Input.dispatchKeyEvent", { type: "keyDown", key: "/", code: "Slash", text: "/" });
    await app.send("Input.dispatchKeyEvent", { type: "keyUp", key: "/", code: "Slash" });
    const focused = await app.evaluate("document.activeElement.id === 'wsearch-input'");
    await app.evaluate("document.getElementById('wsearch-input').focus(); document.getElementById('wsearch-input').value='Torid'; document.getElementById('wsearch-input').dispatchEvent(new Event('input',{bubbles:true}))");
    const resultsVisible = await waitFor("(() => {const row=document.querySelector('#wsearch-list [data-id=torid]');if(!row||document.getElementById('wsearch-panel').hidden)return false;const r=row.getBoundingClientRect();return row.contains(document.elementFromPoint(r.x+5,r.y+Math.min(r.height/2,10)));})()");
    app.check("hero shortcut opens the real visible search results", focused && resultsVisible);
    await app.evaluate("document.querySelector('#wsearch-list [data-id=torid]').click()");
    app.check("a result opens the weapon and returns search to the header", await waitFor("location.pathname.includes('/weapons/Torid') && !!document.querySelector('.topbar-inner #wsearch-input')"));
    await app.evaluate("nav('/')");
    const returnedHome = await waitFor("location.pathname==='/' && !document.getElementById('home-page').hidden");
    app.check("returning home restores the single real search", returnedHome && await app.evaluate("!!document.querySelector('.home-entry #wsearch-input') && document.querySelectorAll('#wsearch-input').length===1"));
    app.check("home design raises no page exceptions", errors.length === 0, JSON.stringify(errors));
    await app.evaluate(`(() => {
      window.__homeRefresh = async () => {
        await tallyLoading; computingAskedAt = 0;
        await Promise.all([loadBoard(), loadHomeTally(), navComputing()]);
      };
      window.__homeState = () => {
        const nodes = [document.querySelector('#home-facts .hf b'), document.querySelector('[data-tally="volunteers"]'), document.querySelector('[data-tally="devices"]')];
        const nav = document.querySelector('[data-live-device-number]') || document.getElementById('compute-count'), hero = nodes[2];
        const value = node => node ? Number(node.textContent.replace(/[^0-9]/g,'')) : null;
        return { values: nodes.map(value), nav: value(nav), hero: value(hero), active: nodes.map(node => node?.getAnimations().length || 0),
          navActive: nav?.getAnimations().length || 0, sameClock: hero?.getAnimations()[0]?.startTime === nav?.getAnimations()[0]?.startTime,
          duration: hero?.getAnimations()[0]?.effect.getTiming().duration,
          visible: !document.getElementById('compute-count').hidden && !document.getElementById('compute-link').hidden };
      };
      window.__homeFixture = { builds: 54321, scores: 123456, devices: 47, snapshotDevices: 9999 };
    })()`);
    const changed = await app.evaluate(`(async () => {
      await window.__homeRefresh(); const first = window.__homeState(), samples = [];
      for (let i=0;i<30;i++) { await new Promise(requestAnimationFrame); samples.push(window.__homeState()); }
      return { first, samples, reads: window.__homeReads };
    })()`);
    app.check("home displays three confirmed targets immediately", JSON.stringify(changed.first.values) === JSON.stringify([54321,123456,47]), JSON.stringify(changed.first));
    app.check("header and hero use live work rather than tally devices", changed.first.visible && changed.first.nav === 47 && changed.first.hero === 47
      && ["builds","scores","devices"].every(kind => changed.reads.some(row => row[0] === kind)), JSON.stringify(changed));
    app.check("all three confirmations animate for 380ms and device displays share a clock", changed.first.active.every(n => n > 0) && changed.first.navActive > 0 && changed.first.sameClock && changed.first.duration === 380, JSON.stringify(changed.first));
    app.check("every motion frame keeps confirmed text and identical device values", changed.samples.every(s => s.nav === s.hero && JSON.stringify(s.values) === JSON.stringify([54321,123456,47])), JSON.stringify(changed.samples));
    const same = await app.evaluate("(async()=>{await window.__homeRefresh();return window.__homeState();})()");
    app.check("unchanged reads leave all counters still", same.active.every(n => n === 0) && same.navActive === 0, JSON.stringify(same));
    const zero = await app.evaluate("(async()=>{window.__homeFixture.devices=0;window.__homeFixture.scores+=100;window.__homeFixture.builds+=10;await window.__homeRefresh();return window.__homeState();})()");
    app.check("zero live devices is visible and shared immediately", zero.visible && zero.nav === 0 && zero.hero === 0 && JSON.stringify(zero.values) === JSON.stringify([54331,123556,0]), JSON.stringify(zero));
    const failed = await app.evaluate("(async()=>{window.__homeFixture.fail=true;await new Promise(r=>setTimeout(r,450));await window.__homeRefresh();return window.__homeState();})()");
    app.check("failed reads retain last confirmations without motion", JSON.stringify(failed.values) === JSON.stringify([54331,123556,0]) && failed.nav === 0 && failed.active.every(n => n === 0) && failed.navActive === 0, JSON.stringify(failed));
    await app.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
    const reduced = await app.evaluate("(async()=>{window.__homeFixture={builds:54330,scores:123555,devices:32};await window.__homeRefresh();return window.__homeState();})()");
    app.check("reduced motion shows exact increases and decreases immediately", JSON.stringify(reduced.values) === JSON.stringify([54330,123555,32]) && reduced.nav === 32 && reduced.active.every(n => n === 0) && reduced.navActive === 0, JSON.stringify(reduced));
  } finally {
    await app.send("Page.removeScriptToEvaluateOnNewDocument", { identifier: fixture.result.identifier });
    await app.evaluate("if(window.__homeRealFetch)window.fetch=window.__homeRealFetch");
    await app.evaluate("if (" + JSON.stringify(previousConsent) + " === null) localStorage.removeItem('wfsim-compute-consent'); else localStorage.setItem('wfsim-compute-consent'," + JSON.stringify(previousConsent) + ")");
    await app.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
  }
}

const app=await openApp({base:process.env.WFSIM_BASE,boot:8000});
const {evaluate,check,sleep}=app;
try {
  await checkHomeHero(app);
  if (process.argv.includes("--home-hero")) await app.finish("home hero layout and confirmed-value motion preserve the real sources");
  const meta=await evaluate("({benchmarks:META.benchmarks,weapons:META.weapons.map(w=>({id:w.id,modes:w.modes,mods:w.mods}))})");
  const primary=meta.benchmarks.find(b=>b.primary)||meta.benchmarks[0];
  const dirs=[...new Set(meta.benchmarks.map(b=>b.cell_dir||""))];
  const requests=[],missingHeads=[],rowsPerCell=1800;
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
      missingHeads.push(path);
      await app.send("Fetch.fulfillRequest",{requestId:event.requestId,responseCode:404,body:""});return;
    }
    let body;
    if(path==="/board.meta.json")body=JSON.stringify({scene_weapons:Object.fromEntries(dirs.filter(Boolean).map(dir=>[dir,["torid","furis"]]))});
    else {
      const parts=path.slice("/board/".length).split("/"),weapon=parts.at(-1).replace(/\.json$/,"");
      const dir=parts.length>1?parts[0]:"";
      requests.push({weapon,dir});body=rows(weapon,dir);
      if(process.env.WFSIM_PERF_DROP_CARD_ROW && missingHeads.length) body=JSON.stringify(JSON.parse(body).slice(1));
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
    return {stable,confirmed:Number(node.textContent)===200,hidden};
  })()`);
  check("confirmed tally values retain their DOM while presentation moves",animation.stable&&animation.confirmed);
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
  for(const requested of [null,...dirs]) {
    const dir=requested??(primary.cell_dir||"");
    const cells=meta.benchmarks.filter(b=>(b.cell_dir||"")===dir).map(b=>b.id),before=requests.length,headsBefore=missingHeads.length;
    const address='/weapons/Torid/card?kind=pz'+(requested===null?'':'&ruler='+encodeURIComponent(cells[0]));
    const fallback=await evaluate(`(async()=>{
      releaseBoardCache(null);history.pushState(null,'',${JSON.stringify(address)});await loadWeaponBoard('torid');
      const rows=BOARD.torid||[];
      return {rows:rows.length,loaded:[...boardFiles.get('torid').loaded],
        cells:Object.fromEntries([...new Set(rows.map(row=>row.benchmark))].map(id=>[id,rows.filter(row=>row.benchmark===id).length]))};
    })()`);
    const reads=requests.slice(before);
    check("a card without heads reads its complete requested scene: "+(requested===null?'default':dir||"base"),missingHeads.length>headsBefore
      && reads.length===1&&reads[0].weapon==='torid'&&reads[0].dir===dir
      && fallback.rows===rowsPerCell*cells.length&&Object.keys(fallback.cells).length===cells.length
      && cells.every(id=>fallback.cells[id]===rowsPerCell)&&fallback.loaded.length===1&&fallback.loaded[0]===dir,JSON.stringify({fallback,reads}));
  }
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
