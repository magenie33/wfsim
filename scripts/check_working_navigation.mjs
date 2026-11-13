import {openApp} from './cdp.mjs';
const app=await openApp({boot:8000,lang:'en',base:process.env.WFSIM_BASE});
try {
  await app.send('Emulation.setDeviceMetricsOverride',{width:1440,height:1000,deviceScaleFactor:1,mobile:false});
  await app.load('/weapons/Laetum/builder',6000);
  const initial=await app.evaluate(`(()=>{
    window.navigationRunButtons=[$('run-sim'),$('run-opt')];
    const count=$('compute-count'),mark=$('compute-mark'),original={count:count.innerHTML,countHidden:count.hidden,markHidden:mark.hidden};
    count.innerHTML='<span class="online-dot"></span>45';count.hidden=false;mark.hidden=false;computeLinkPaint();
    const state={ready:!!$('execution-dock')&&!$('execution-dock').hidden,
      separate:$('quick-calc').offsetParent!==null&&$('sim-block').contains($('calculation-scenario-editor'))&&!$('quick-calc').contains($('calculation-scenario-editor')),
      header:document.querySelector('.topbar').offsetHeight,tabs:$('module-tabs').offsetHeight};
    state.compact=state.header<=52&&state.tabs<=36;
    count.innerHTML=original.count;count.hidden=original.countHidden;mark.hidden=original.markHidden;computeLinkPaint();
    return state;
  })()`);
  app.check('Builder keeps quick calculation and the separate scenario editor',initial.separate);
  app.check('the compact floating execution controls are available with both compute indicators',initial.ready&&initial.compact,JSON.stringify(initial));
  const inline=await app.evaluate(`(()=>{
    window.navigationResults=[$('sim-results-block'),$('opt-results')].map(node=>({node,parent:node.parentNode}));
    return $('sim-run-block')?.contains($('run-sim'))&&$('opt-runbar').contains($('run-opt'))&&!$('execution-dock').contains($('run-sim'));
  })()`);
  app.check('original run controls stay in their separate bottom blocks',inline);
  const requests=await app.evaluate(`(async()=>{
    const queue=await initUserTasks(),create=queue.create,wait=queue.wait,calls=[];
    queue.create=async(kind,request,origin)=>{calls.push({kind,request,origin});return {id:'navigation-probe',state:'queued'};};
    queue.wait=async()=>({});
    try{
      for(const id of ['run-sim','run-opt']){
        document.querySelector('[data-execution-run="'+id+'"]').click();await new Promise(r=>setTimeout(r,100));
        $(id).click();await new Promise(r=>setTimeout(r,100));
      }
      return calls.map(c=>({kind:c.kind,weapon:c.request.weapon,event:'isTrusted' in c.request}));
    }finally{queue.create=create;queue.wait=wait;}
  })()`);
  for(const kind of ['simulation','optimization']) app.check('original and side '+kind+' controls submit the current request',requests.filter(c=>c.kind===kind&&c.weapon==='laetum'&&!c.event).length===2,JSON.stringify(requests));
  const retained=await app.evaluate(`navigationResults.every(({node,parent})=>node.parentNode===parent)&&navigationRunButtons[0]===$('run-sim')&&navigationRunButtons[1]===$('run-opt')`);
  app.check('running from the side leaves the original controls and results in place',retained);
  const disabled=await app.evaluate(`(async()=>{
    const source=$('run-opt'),original=source.disabled;source.disabled=true;
    await new Promise(r=>setTimeout(r,80));const disabled=document.querySelector('[data-execution-run="run-opt"]').disabled;
    source.disabled=original;await new Promise(r=>setTimeout(r,80));return disabled;
  })()`);
  app.check('side shortcuts follow the original disabled state',disabled);
  await app.evaluate(`(async()=>{history.pushState(null,'','/weapons/Laetum/simulator');await route();})()`);await app.sleep(150);
  const blocks=await app.evaluate(`(()=>{
    const sim=$('sim-run-block'),build=$('sim-build-block'),scenario=$('sim-block');
    return {indexed:pageFolds().some(f=>f.box===sim),equal:Math.abs(sim.getBoundingClientRect().width-build.getBoundingClientRect().width)<1&&Math.abs(scenario.getBoundingClientRect().width-build.getBoundingClientRect().width)<1};
  })()`);
  app.check('simulation has an indexed run block equal in width to the cards above',blocks.indexed&&blocks.equal);
  await app.evaluate('scrollTo(0,700)');await app.sleep(150);
  const pinned=await app.evaluate(`(()=>{const h=document.querySelector('.topbar').getBoundingClientRect(),t=$('module-tabs').getBoundingClientRect();return Math.abs(h.top)<1&&Math.abs(t.top-h.bottom)<2&&t.height<=36;})()`);
  app.check('page tabs remain pinned below the compact shell',pinned);
  await app.send('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:1,mobile:true});
  await app.evaluate(`(async()=>{history.pushState(null,'','/weapons/Laetum/optimizer');await route();scrollTo(0,500);})()`);await app.sleep(200);
  const phone=await app.evaluate(`(()=>{const t=$('module-tabs').getBoundingClientRect(),a=$('module-tabs').querySelector('.sel').getBoundingClientRect(),d=$('execution-dock').getBoundingClientRect();return {fits:document.documentElement.scrollWidth<=innerWidth&&d.left>=0&&d.right<=innerWidth,selected:a.left>=t.left-1&&a.right<=t.right+1,oneRow:t.height<=36};})()`);
  app.check('phone tabs stay in one row with the active tab in view',phone.selected&&phone.oneRow);
  app.check('phone execution controls stay inside the viewport',phone.fits);
  const utility=await app.evaluate(`(async()=>{
    history.pushState(null,'','/utility/'+UTILITY_TABS[0][0]);await route();
    const spacer=document.createElement('div');spacer.style.height='1600px';$('utility-page').append(spacer);scrollTo(0,600);
    await new Promise(r=>setTimeout(r,120));const t=$('utility-tabs').getBoundingClientRect(),h=document.querySelector('.topbar').getBoundingClientRect();
    return {tabs:Math.abs(t.top-h.bottom)<2,hidden:$('execution-dock').hidden,restored:$('sim-run-block').contains($('run-sim'))&&$('opt-runbar').contains($('run-opt'))};
  })()`);
  app.check('other pages share the sticky tab behavior',utility.tabs);
  app.check('leaving the weapon workspace hides shortcuts and keeps the original buttons',utility.hidden&&utility.restored);
  await app.send('Emulation.setDeviceMetricsOverride',{width:1440,height:1000,deviceScaleFactor:1,mobile:false});
  await app.load('/weapons/Laetum/optimizer',5000);
  const layout=await app.evaluate(`(()=>{
    const node=$('opt-run-block'),build=JSON.stringify(snapshotState()),fight=JSON.stringify(snapshotScenario());
    document.querySelector('[data-layout-edit]').click();
    node.querySelector('[data-layout-up]').click();
    node.querySelector('.layout-size-handle').dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',shiftKey:true,bubbles:true}));
    const saved=JSON.parse(localStorage.getItem('wfsim-layout'));
    return {first:[...node.parentElement.children].filter(n=>n.dataset.module==='optimizer').map(n=>n.id),width:node.offsetWidth,
      separate:build===JSON.stringify(snapshotState())&&fight===JSON.stringify(snapshotScenario()),saved:!!saved.orders['weapon:optimizer']&&!!saved.cards['opt-run-block']};
  })()`);
  app.check('layout changes preserve build and scenario data',layout.separate&&layout.saved);
  await app.load('/weapons/Laetum/optimizer',5000);
  const persisted=await app.evaluate(`(()=>{const n=$('opt-run-block');return {order:[...n.parentElement.children].filter(n=>n.dataset.module==='optimizer').map(n=>n.id),width:n.offsetWidth};})()`);
  app.check('card order and dimensions survive reload',JSON.stringify(persisted.order)===JSON.stringify(layout.first)&&persisted.width===layout.width);
  const reset=await app.evaluate(`(()=>{document.querySelector('[data-layout-reset]').click();const n=$('opt-run-block');return [...n.parentElement.children].filter(n=>n.dataset.module==='optimizer').map(n=>n.id).join(',')==='opt-block,opt-fight-block,opt-run-block'&&n.style.width==='';})()`);
  app.check('default layout restores authored order and natural dimensions',reset);
  for (const lang of ['en','zh']) {
    if (lang==='zh') await app.setLang(lang,2000);
    await app.load('/weapons/Laetum?filter=score%20%3E%201e30&view=library',3000);
    app.check('the scroll fixture has an empty library',await app.evaluate(`!!document.querySelector('#library-results .fd-empty')&&!document.querySelector('#build-finder tr.fr')`));
    for (const [width,height] of [[1353,919],[864,896],[1440,1000],[1353,959],[864,936],[390,844]]) {
      await app.send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});
      const empty=await app.evaluate(`(async()=>{
        const wait=async(n)=>{for(let i=0;i<n;i++)await new Promise(requestAnimationFrame);};
        scrollTo(0,0);await wait(4);
        const root=document.documentElement,bar=document.querySelector('#library-results .library-toolbar');
        const fullHeight=root.scrollHeight,expanded=bar.offsetHeight,frames=[];
        scrollTo(0,1e6);const bottom=scrollY;
        for(let i=0;i<48;i++){
          await wait(1);frames.push({y:scrollY,height:root.scrollHeight,stuck:root.classList.contains('page-tabs-stuck')});
        }
        const compact=bar.offsetHeight,stuck=root.classList.contains('page-tabs-stuck');
        const tabs=$('module-tabs').getBoundingClientRect(),header=document.querySelector('.topbar').getBoundingClientRect();
        scrollTo(0,0);await wait(4);
        return {fullHeight,bottom,expanded,compact,stuck,pinned:Math.abs(tabs.top-header.bottom)<2,
          changes:frames.filter((f,i)=>i&&f.stuck!==frames[i-1].stuck).length,
          stable:frames.every(f=>f.height===fullHeight&&Math.abs(f.y-bottom)<1),
          top:!root.classList.contains('page-tabs-stuck')&&bar.offsetHeight===expanded&&root.scrollHeight===fullHeight};
      })()`);
      const label=lang+' '+width+'x'+height;
      app.check('empty library preserves page height and scroll position at '+label,empty.stable,JSON.stringify(empty));
      app.check('empty library settles its sticky state at '+label,empty.changes<=1&&(!empty.stuck||empty.pinned&&empty.compact<empty.expanded),JSON.stringify(empty));
      app.check('scrolling to the top restores the toolbar at '+label,empty.top,JSON.stringify(empty));
    }
    await app.evaluate('scrollTo(0,1e6)');await app.sleep(100);
    await app.send('Emulation.setDeviceMetricsOverride',{width:1353,height:919,deviceScaleFactor:1,mobile:false});
    await app.sleep(100);
    const changed=await app.evaluate(`(async()=>{
      const root=document.documentElement,status=$('library-stat-status'),bar=document.querySelector('#library-results .library-toolbar');
      status.textContent='Checking build attributes';status.hidden=false;
      for(let i=0;i<6;i++)await new Promise(requestAnimationFrame);
      const stuck=root.classList.contains('page-tabs-stuck'),height=root.scrollHeight,compact=bar.offsetHeight;
      scrollTo(0,0);for(let i=0;i<6;i++)await new Promise(requestAnimationFrame);
      return {stuck,height,expandedHeight:root.scrollHeight,compact,expanded:bar.offsetHeight};
    })()`);
    app.check('resizing and changing toolbar content preserve its document space',changed.stuck&&changed.height===changed.expandedHeight&&changed.compact<changed.expanded,JSON.stringify(changed));
  }
  await app.load('/warframes/Prototype/builder',5000);
  await app.evaluate(`(()=>{const state=wfBlank('prototype');storePresetList(WF_BUILDS,[{id:'library-probe',name:'Library probe',savedAt:1,state}],'prototype');})()`);
  await app.load('/warframes/Prototype',4000);
  const frameLibrary=await app.evaluate(`location.pathname==='/warframes/Prototype'&&!$('wf-library').hidden&&$('wf-library').querySelectorAll('.wf-library-entry').length===1&&$('wf-module-tabs').querySelectorAll('.mtab').length===2`);
  app.check('Warframe root opens saved builds with Library and Builder tabs',frameLibrary);
  await app.evaluate(`document.querySelector('[data-wf-library-open="library-probe"]').click()`);await app.sleep(400);
  app.check('opening a Warframe library card reuses its existing builder and preset',await app.evaluate(`location.pathname==='/warframes/Prototype/builder'&&wfActive==='library-probe'&&$('wf-library').hidden`));
  await app.finish('settings stay in their modules while execution and navigation remain within reach');
}finally {await app.close();}
