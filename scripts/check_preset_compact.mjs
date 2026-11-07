import { openApp } from './cdp.mjs';

const app = await openApp({lang:'zh',boot:9000});
const {evaluate,check}=app;
await app.load('/weapons/Laetum/builder',5000);
const scale = await evaluate(`(async () => {
  await new Promise(r=>setTimeout(r,500));
  localStorage.clear();
  const cfg = buildBarCfg(), blank = snapshotState();
  const list = Array.from({length:100}, (_,i) => presetEntry(String(i+1).padStart(2,'0'),
    {...blank, slots:blank.slots.map((slot,index)=>i===99 && index===0 ? {...slot,mod:'hornet_strike',rank:5} : {...slot})}));
  list.forEach((p,i)=>p.savedAt=1000+i);
  cfg.store(list); pickPreset(cfg,list[0].id);
  const bar = document.getElementById('preset-bar-builder-builds');
  const manyHeight = bar.getBoundingClientRect().height;
  const currentCount = bar.querySelectorAll('.pchip[data-name]').length;
  cfg.store(list.slice(0,1)); cfg.rerender();
  const oneHeight = bar.getBoundingClientRect().height;
  cfg.store(list); cfg.rerender();
  list[99].state.slots[0].mod='lethal_torrent'; cfg.store(list);
  window.compactFixture = {list,cfg};
  return {currentCount,manyHeight,oneHeight,view:document.body.className,path:location.pathname};
})()`);
check('100 builds show one current entry and use the same bar height as one build',scale.currentCount === 1 && scale.manyHeight > 0 && scale.manyHeight === scale.oneHeight,JSON.stringify(scale));
if (scale.currentCount !== 1) await app.finish('the preset bar must stay compact');
const r = await evaluate(`(async () => {
  const {list,cfg} = compactFixture, bar=document.getElementById('preset-bar-builder-builds');
  const open=()=>bar.querySelector('[data-dd="dd-preset-builder-builds"]').click();
  open();
  const limited=document.querySelectorAll('#dd-menu .opt[data-v]').length===8;
  const manage=!!document.querySelector('#dd-menu [data-dd-footer]');
  const query=document.getElementById('dd-search');query.value='lethal torrent';query.dispatchEvent(new Event('input',{bubbles:true}));
  const hit=document.querySelector('#dd-menu .opt[data-v="'+list[99].id+'"]');
  const allSearch=!!hit && document.querySelectorAll('#dd-menu .opt[data-v]').length===1;
  hit.querySelector('[data-dd-info]').click();
  const inert=cfg.active()===list[0].id;
  hit.click();
  const selected=cfg.active()===list[99].id;
  open();
  const find=document.getElementById('dd-search');find.value='50';find.dispatchEvent(new Event('input',{bubbles:true}));
  document.querySelector('#dd-menu .opt[data-v="'+list[49].id+'"]').click();
  open();
  const recent=[...document.querySelectorAll('#dd-menu .opt[data-v]')].map(row=>row.dataset.v);
  const lastUsed=recent[0]===list[49].id && recent[1]===list[99].id && presetRecentUses(BUILDS)[list[99].id]>0;
  closePopovers();
  const menu=bar.querySelector('.preset-actions');menu.querySelector('summary').click();await new Promise(r=>setTimeout(r,50));
  const ops=menu.open && menu.querySelector('.preset-action-menu').getBoundingClientRect().width>0;
  document.body.click();
  const dismissed=!menu.open;
  openPresetLibrary(cfg,bar);
  const manager=document.querySelector('.preset-library-window');
  const collapsed=manager.querySelectorAll('.preset-library-info[open]').length===0;
  const row=manager.querySelector('[data-entry]');
  const summary=!!row.querySelector('.preset-library-summary').textContent;
  const quiet=row.querySelector('[data-library-rename]').offsetHeight===0;
  const results=manager.querySelector('.preset-library-results'), count=results.querySelectorAll('[data-entry]').length;
  results.scrollTop=results.scrollHeight; results.dispatchEvent(new Event('scroll'));await new Promise(r=>setTimeout(r,100));
  const loads=results.querySelectorAll('[data-entry]').length>count;
  manager.querySelector('[data-library-close]').click();
  await loadWarframeCatalog(); wf=wfBlank(wfFrames()[0].id); comp=compBlank(compHosts()[0].id); op=opBlank();
  const configs=[buildBarCfg(),scenarioBarCfg(),optBarCfg(),wfBarCfg(),compBarCfg(),opBarCfg()];
  const all=configs.map(c=>{
    const state=c.snapshot();
    c.store(Array.from({length:100},(_,i)=>presetEntry(String(i+1).padStart(2,'0'),state)));
    c.setActive(c.load()[0].id);
    const host=document.createElement('div');host.id='compact-test-'+c.domain;
    renderPresetBarIn(host,c);
    return {domain:c.domain,one:host.querySelectorAll('.pchip[data-name]').length===1,
      items:ddReg.get('dd-preset-'+c.domain).items.length>=100,info:!!host.querySelector('[data-preset-info]')};
  });
  return {limited,manage,allSearch,inert,selected,lastUsed,ops,dismissed,collapsed,summary,quiet,loads,all};
})()`);
check('the switcher starts with eight recent choices and a management entry',r.limited&&r.manage);
check('equipment search finds a build outside the recent choices',r.allSearch);
check('info keeps the selection and choosing its row opens the saved build',r.inert&&r.selected);
check('recent usage is independent of modification times and survives as a preference',r.lastUsed);
check('the action menu opens and closes on an outside click',r.ops&&r.dismissed);
check('management rows show summaries, with info and secondary actions folded',r.collapsed&&r.summary&&r.quiet);
check('scrolling loads the remaining rows',r.loads);
check('all six domains keep one current entry, search 100 presets and retain info',r.all.every(x=>x.one&&x.items&&x.info),JSON.stringify(r.all));
const shares = await evaluate(`(async () => {
  const sleep = ms => new Promise(resolve => setTimeout(resolve,ms));
  const {list,cfg} = compactFixture;
  cfg.store([list[0],list[99]]); pickPreset(cfg,list[0].id);
  const bar=document.getElementById('preset-bar-builder-builds');
  const visible=el=>!!el && el.getBoundingClientRect().height>0 && el.checkVisibility();
  const direct=visible(bar.querySelector(':scope > .share'));
  if (!direct) return {direct};
  const waitLink=async host=>{
    for(let i=0;i<50&&!host.querySelector('.sh-url');i++) await sleep(100);
    const url=host.querySelector('.sh-url')?.value;
    return url ? {url,build:await decodeShare(new URL(url).searchParams.get(SHARE_PARAM))} : null;
  };
  bar.querySelector('.share').click();
  const current=await waitLink(bar);
  bar.querySelector('.pshare').hidden=true;
  const weapon='braton_prime', tiers=weaponEvos(weapon);
  const evoSel=Object.fromEntries(tiers.map(t=>[t.tier,t.options.at(-1).id]));
  const other=presetEntry('other weapon',{...list[0].state,weapon,evoSel,
    slots:list[0].state.slots.map((s,i)=>i===0?{...s,mod:'serration',rank:3}:{...s})});
  storePresetList(BUILDS,[other],weapon);
  const before=JSON.stringify([snapshotState(),snapshotScenario(),loadPresetWhole(BUILDS),cfg.active()]);
  openPresetLibrary(cfg,bar);
  const overlay=document.querySelector('.preset-library-window');
  const row=overlay.querySelector('[data-entry="'+list[99].id+'"]');
  const rowVisible=visible(row.querySelector('[data-library-share]'));
  if (!rowVisible) {presetLibraryWindow.close();return {direct,current:!!current,rowVisible};}
  row.querySelector('[data-library-share]').click();
  const saved=await waitLink(row);
  document.getElementById('preset-library-scope').click();
  document.querySelector('#dd-menu [data-v="all"]').click();
  const otherRow=overlay.querySelector('[data-entry="'+other.id+'"]');
  otherRow.querySelector('[data-library-share]').click();
  const cross=await waitLink(otherRow);
  const expected=tiers.map(t=>evoSel[t.tier]);
  const crossEvos=cross?.build.evos.map(id=>id.startsWith(weapon+'_')?id:weapon+'_'+id);
  const intact=before===JSON.stringify([snapshotState(),snapshotScenario(),loadPresetWhole(BUILDS),cfg.active()]);
  const out={direct,current:current?.build.w==='laetum' && !current.build.slots.some(s=>s.mod),rowVisible,
    saved:saved?.build.w==='laetum' && saved.build.slots[0].mod==='lethal_torrent',
    cross:cross?.build.w===weapon && new URL(cross.url).pathname===weaponPath(weapon)
      && cross.build.slots[0].mod==='serration' && cross.build.slots[0].rank===3
      && JSON.stringify(crossEvos)===JSON.stringify(expected),intact,
    noResult:!otherRow.querySelector('.sh-result,.sh-full'),shown:visible(otherRow.querySelector('.pshare'))};
  presetLibraryWindow.close();return out;
})()`);
check('current build sharing is directly visible and encodes the current build',shares.direct&&shares.current,JSON.stringify(shares));
check('management has a visible share button for each saved build',shares.rowVisible&&shares.saved,JSON.stringify(shares));
check('all-item sharing carries that weapon, ranked mod and evolutions',shares.cross,JSON.stringify(shares));
check('sharing saved builds preserves the active build, fight and presets',shares.intact&&shares.noResult&&shares.shown,JSON.stringify(shares));
await app.finish('100 presets fit the same switcher as one');
