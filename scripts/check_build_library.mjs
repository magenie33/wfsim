import {openApp} from "./cdp.mjs";
const app = await openApp({boot: 12000, lang: "en"});
const {evaluate, check, sleep} = app;
const interactions = `(async () => {
  const wait=()=>new Promise(r=>setTimeout(r,250)), out={board:true,stable:true,order:true,states:true};
  for(const axis of ['base','scene','shooting']) {
    const button=document.querySelector('#library-board [data-board-axis="'+axis+'"] button:not(.sel):not(:disabled)');
    if(!button)continue;
    scrollTo(0,60);await wait();const y=scrollY;
    button.click();await wait();out.board=out.board&&Math.abs(scrollY-y)<2;
  }
  finder.b=libraryDefaultBenchmark();renderBuildFinder();await wait();
  const overview=()=>document.querySelector('.fd-ov');
  if(overview().classList.contains('shut'))overview().querySelector('.fold-h').click();
  const buttons=()=>[...overview().querySelectorAll('[data-fcyc]')];
  const original=buttons().map(el=>el.dataset.fcyc), token=original[Math.min(5,original.length-1)];
  const selected=()=>buttons().find(el=>el.dataset.fcyc===token);
  if(!selected())return {...out,stable:false,order:false,states:false,images:false};
  selected().scrollIntoView({block:'center'});await wait();
  const rect=selected().getBoundingClientRect();
  for(let i=0;i<3;i++) {
    selected()?.click();await wait();const el=selected(), after=el?.getBoundingClientRect();
    out.stable=out.stable&&!!after&&Math.abs(after.x-rect.x)<2&&Math.abs(after.y-rect.y)<2
      &&Math.abs(after.width-rect.width)<2&&Math.abs(after.height-rect.height)<2;
    out.order=out.order&&JSON.stringify(buttons().map(el=>el.dataset.fcyc))===JSON.stringify(original);
    out.states=out.states&&(i===0?finder.req.has(token)&&!finder.exc.has(token):
      i===1?!finder.req.has(token)&&finder.exc.has(token):!finder.req.has(token)&&!finder.exc.has(token));
  }
  const images=[...overview().querySelectorAll('[data-fcyc^="mod:"] img')];
  images.forEach(img=>img.loading='eager');
  await Promise.race([Promise.all(images.map(img=>img.decode().catch(()=>{}))),new Promise(r=>setTimeout(r,5000))]);
  out.images=images.length>0&&images.every(img=>img.naturalWidth>0&&new URL(img.src).origin===location.origin);
  out.fit=document.documentElement.scrollWidth<=innerWidth;
  overview().querySelector('.fold-h').click();scrollTo(0,0);return out;
})()`;
try {
  await app.send('Page.addScriptToEvaluateOnNewDocument',{source: `window.libraryUrlWrites=[];
    for(const name of ['replaceState','pushState']){const original=history[name];history[name]=function(...args){
      window.libraryUrlWrites.push(new URL(args[2],location.href).href);return original.apply(this,args);
    };}`});
  await app.load('/weapons/Laetum?filter=equipment+%21%3D+%22mod%3Alethal_torrent%22&depth=0&view=library',8000);
  const linked=await evaluate(`(()=>{
    const intact=()=>new URL(location.href).searchParams.get('filter')==='equipment != "mod:lethal_torrent"';
    const cold=intact()&&finder.exc.has('mod:lethal_torrent');
    boardRowsLanded('laetum');boardRowsLanded('laetum');
    return {cold,refresh:intact()&&finder.exc.has('mod:lethal_torrent'),
      writes:window.libraryUrlWrites.every(url=>new URL(url).searchParams.get('filter')==='equipment != "mod:lethal_torrent"'),
      rows:[...document.querySelectorAll('#build-finder tr.fr')].every(el=>!finderTokens(builtinBuilds().find(p=>presetId(p)===el.dataset.frow)).includes('mod:lethal_torrent'))};
  })()`);
  check('a cold library link applies its equipment exclusion',linked.cold&&linked.rows);
  check('board arrivals preserve the query without a transient bare address',linked.refresh&&linked.writes);
  await app.load("/weapons/Furis/", 8000);
  const ready = await evaluate("document.body.classList.contains('on-build-library') && !!document.querySelector('.library-toolbar')");
  check("the weapon root is Build Library", ready);
  if (!ready) { await app.finish("library missing"); }
  const initial = await evaluate(`(() => {
    const b=benchList().find(b=>b.id===finder.b);
    const first=document.querySelector('#build-finder tr.fr'), p=builtinBuilds().find(p=>presetId(p)===first.dataset.frow);
    const heads=[...first.querySelectorAll('.sb-h')].map(el=>el.textContent);
    return { default: b.primary && b.id==='standard_single_target', all:finder.mo===FINDER_EVERY_MODE,
      panels:['library-board','library-filters','library-sort','library-results'].every((id,i)=>{
        const el=$(id);return el?.parentElement===$('build-finder')&&el.offsetParent!==null
          &&$('build-finder').children[i]===el&&getComputedStyle(el).borderTopWidth!=='0px';
      }),
      filterScope:!!$('library-filters')?.querySelector('.fd-ov')&&!!$('library-filters')?.querySelector('#fd-input')
        &&!$('library-filters')?.querySelector('[data-library-sort]'),
      sortScope:!!$('library-sort')?.querySelector('[data-library-sort]')&&!!$('library-sort')?.querySelector('[data-library-sort-advanced]'),
      resultScope:!!$('library-results')?.querySelector('.fd-main')&&!$('library-results')?.querySelector('.fd-ov')
        &&!$('library-results')?.querySelector('[data-library-sort]'),
      builderHidden:$('mod-block').offsetParent===null, left:document.querySelector('#module-tabs a.sel')?.textContent==='Build Library',
      rows:document.querySelectorAll('#build-finder tr.fr').length,
      card:heads[0]===tr('Mode') && heads[1].startsWith(tr('Mods')+' ·') && heads.includes(tr('Exilus')),
      summary:$('doc').textContent.includes(String(p.board.shown)) && !!$('doc').querySelector('[data-library-build]'),
      noButton:!document.querySelector('[data-fmore]'), compact:$('doc').querySelector('.library-summary').classList.contains('shut'),
    };
  })()`);
  check("fresh visits default to Standard Single Target Steel9999", initial.default);
  check("all modes and twenty places are available immediately", initial.all && initial.rows===20);
  check("board, filters, sort and results occupy four separate sibling cards",initial.panels);
  check("equipment search and Overview belong to filters",initial.filterScope);
  check("ordinary and advanced sort controls belong to sort",initial.sortScope);
  check("the final result card contains builds and navigation",initial.resultScope);
  check("the library is leftmost and the editor is separate", initial.left && initial.builderHidden);
  check("shared info cards retain mode and separate Exilus", initial.card);
  check("a compact generated summary links to its published build", initial.summary && initial.compact);
  check("normal browsing needs no load-more button", initial.noButton);
  await app.send('Emulation.setDeviceMetricsOverride', {width:800,height:900,deviceScaleFactor:1,mobile:false});
  await sleep(300);
  check("a narrow desktop keeps navigation compact and search usable", await evaluate(`(() => {
    const menu=$('menu-toggle').getBoundingClientRect(), search=$('wsearch-input').getBoundingClientRect();
    return menu.width>0&&menu.right<=innerWidth&&search.width>=140&&document.documentElement.scrollWidth<=innerWidth;
  })()`));
  const desktop=await evaluate(interactions);
  for(const [key,ok] of Object.entries(desktop))check("desktop board and Overview interaction: "+key,ok);

  const browsing = await evaluate(`(async () => {
    const before=JSON.stringify(snapshotState()), fight=JSON.stringify(snapshotScenario()), active=activePreset;
    const b=benchList().find(b=>!b.primary), m=weaponInfo(presetWeapon()).modes[0];
    const query=new URLSearchParams({view:'library',bench:b.id,mode:m});
    history.pushState({},'',weaponPath(presetWeapon())+'?'+query); await route();
    const kept=finder.b===b.id&&finder.mo===m&&JSON.stringify(snapshotState())===before
      &&JSON.stringify(snapshotScenario())===fight&&activePreset===active;
    document.querySelector('[data-library-remove="mo"]').click();
    const removed=finder.mo===FINDER_EVERY_MODE&&finder.b===b.id;
    history.pushState({},'',weaponPath(presetWeapon()));await route();
    return {kept,removed};
  })()`);
  check("library conditions leave the active build and fight untouched", browsing.kept);
  check("a condition chip removes only its own filter", browsing.removed);

  const sorts = await evaluate(`(async () => {
    const sleep=ms=>new Promise(r=>setTimeout(r,ms));
    const read=()=>[...document.querySelectorAll('#build-finder tr.fr:not(.mem)')]
      .map(el=>builtinBuilds().find(p=>presetId(p)===el.dataset.frow));
    const out={};
    for(const sort of ['score-asc','recent','drain','score']) {
      const el=document.querySelector('[data-library-sort]');
      if(!el.matches('[data-dd]')){out[sort]=false;continue;}
      el.click();$('dd-menu').querySelector('[data-v="'+sort+'"]').click();await sleep(100);
      const rows=read();
      const value=p=>sort==='recent'?(Date.parse(p.board.measured_at)||0):sort==='drain'?libraryBaseDrain(p):p.board.score;
      out[sort]=rows.length>1 && rows.every((p,i)=>i===0||(sort==='score-asc'||sort==='drain'?value(rows[i-1])<=value(p):value(rows[i-1])>=value(p)));
    }
    return out;
  })()`);
  for (const [sort, ok] of Object.entries(sorts)) check("published results sort correctly: " + sort, ok);

  const filtering = await evaluate(`(async () => {
    const sleep=ms=>new Promise(r=>setTimeout(r,ms));
    document.querySelector('[data-library-filters]').click();
    const available=!document.querySelector('.fd-scope').hidden;
    document.querySelector('[data-fseg="rv"][data-v="plain"]').click();
    const rows=()=>[...document.querySelectorAll('#build-finder tr.fr:not(.mem)')].map(el=>builtinBuilds().find(p=>presetId(p)===el.dataset.frow));
    const plain=rows().length>0&&rows().every(p=>!p.riven);
    const mod=rows()[0].board.mods.find(id=>id!=='riven');
    const input=$('fd-input');input.value=modById(splitRank(mod)[0]).name;input.dispatchEvent(new Event('input'));
    const must=document.querySelector('[data-freq="mod:'+mod+'"]');must.click();await sleep(100);
    const required=rows().length>0&&rows().every(p=>finderTokens(p).includes('mod:'+mod));
    const cap=Math.min(...rows().map(libraryBaseDrain));
    const field=document.querySelector('[data-library-number="maxDrain"]');field.value=cap;field.dispatchEvent(new Event('change',{bubbles:true}));
    const capacity=rows().length>0&&rows().every(p=>libraryBaseDrain(p)<=cap);
    const min=Math.min(...rows().map(libraryScore));
    const score=document.querySelector('[data-library-number="minScore"]');score.value=min;score.dispatchEvent(new Event('change',{bubbles:true}));
    const threshold=rows().length>0&&rows().every(p=>libraryScore(p)>=min);
    const url=location.pathname+location.search;
    return {available,plain,required,capacity,threshold,url,mod,cap,min};
  })()`);
  for (const key of ["available","plain","required","capacity","threshold"]) check("composable library filters: " + key, filtering[key]);
  await app.load(filtering.url, 8000);
  check("a copied query restores its equipment, budget and score filters", await evaluate(`
    finder.rv==='plain' && finder.req.has('mod:'+${JSON.stringify(filtering.mod)})
      && finder.maxDrain===${filtering.cap} && finder.minScore===${filtering.min}`));
  check("every selected filter remains visible in the filter card", await evaluate(`
    ['rv','minScore','maxDrain'].every(key=>!!document.querySelector('#library-filters [data-library-remove="'+key+'"]'))
      &&!!document.querySelector('#library-filters [data-funtok]')`));
  await evaluate("document.querySelector('[data-library-clear]').click()");
  await sleep(300);
  const prefix = await evaluate("[...document.querySelectorAll('#build-finder tr.fr')].map(el=>el.dataset.frow)");
  await evaluate("document.querySelector('[data-library-more]').scrollIntoView({block:'center'})");
  await sleep(1200);
  const stream = await evaluate(`(() => {
    const rows=[...document.querySelectorAll('#build-finder tr.fr')].map(el=>el.dataset.frow);
    const r=document.querySelector('.library-toolbar').getBoundingClientRect();
    const tabs=$('module-tabs').getBoundingClientRect();
    return {shown:finder.shown,rows,pinned:Math.abs(r.top-tabs.bottom)<2,y:scrollY};
  })()`);
  check("scrolling appends another batch while retaining the existing prefix", stream.shown>=40 && prefix.every((id,i)=>stream.rows[i]===id));
  check("the result navigation stays below the site header", stream.pinned);
  const opened = await evaluate(`(() => {
    const row=document.querySelector('#build-finder tr.fr');const id=row.dataset.frow;
    row.querySelector('[data-fopen]').click();return id;
  })()`);
  await sleep(1200);
  check("a result opens the exact build in explicit Builder", await evaluate(`
    location.pathname==='/weapons/Furis/builder' && activePreset===${JSON.stringify(opened)} && $('mod-block').offsetParent!==null`));
  await app.send("Page.navigateToHistoryEntry", {entryId: (await app.send("Page.getNavigationHistory")).result.entries.slice(-2)[0].id});
  await sleep(1600);
  check("Back restores the loaded query and its scroll position", await evaluate(`
    modSuffix()==='build-library' && finder.shown>=${stream.shown} && Math.abs(scrollY-${stream.y})<120`));
  await evaluate("libraryTop()"); await sleep(600);
  check("the pinned navigation returns to the page top", await evaluate("scrollY<2"));

  const race = await evaluate(`(async () => {
    history.pushState({},'','/weapons/Furis/builder');await route();
    copyActivePreset(buildBarCfg());
    equipMod(0,'hornet_strike',0);markPresetDirty();
    const before=JSON.stringify(snapshotState());
    boardRowsLanded('furis');
    const kept=JSON.stringify(snapshotState())===before
      &&sameState(buildNamed(activePreset).state,snapshotState());
    history.pushState({},'','/weapons/Furis');await route();
    return kept;
  })()`);
  check("late board data preserves an edit still waiting to save", race);

  await app.send("Emulation.setDeviceMetricsOverride", {width:390,height:844,deviceScaleFactor:1,mobile:true});
  await sleep(300);
  check("the phone keeps library controls inside its viewport", await evaluate("document.documentElement.scrollWidth<=innerWidth && document.querySelector('.library-toolbar').getBoundingClientRect().right<=innerWidth"));
  await evaluate("document.querySelector('[data-library-filters]').click()");
  check("detailed conditions expand on a phone", await evaluate("!document.querySelector('.fd-scope').hidden"));
  const phone=await evaluate(interactions);
  for(const [key,ok] of Object.entries(phone))check("phone board and Overview interaction: "+key,ok);
  const empty = await evaluate(`(() => {
    const b=benchList().find(b=>!(BOARD[presetWeapon()]||[]).some(r=>r.benchmark===b.id));
    if(!b)return true;finder.b=b.id;renderBuildFinder();
    return $('doc').textContent.includes(tr('No build has been measured under these board conditions.'))
      && !document.querySelector('#doc [data-library-build]');
  })()`);
  check("unpublished conditions never borrow another ruler's winner", empty);
  await app.finish("the Build Library finds and preserves usable builds");
} finally { await app.close(); }
