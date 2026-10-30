import {openApp} from './cdp.mjs';
const app=await openApp({boot:9000,lang:'en'});
try {
  await app.load('/weapons/Laetum/builder',6500);
  const ready=await app.evaluate("!!document.querySelector('[data-calculation-edit]')");
  app.check('quick calculation preferences are available in Builder',ready);
  if(!ready){await app.finish('shared calculation conditions missing');}
  const first=await app.evaluate(`(async()=>{
    gainPrefs.on=false;saveGainPrefs();renderQuickCalc();
    const runs=$('gp-runs');runs.value=30;runs.dispatchEvent(new Event('change',{bubbles:true}));
    document.querySelector('[data-calculation-edit]').click();
    await new Promise(r=>setTimeout(r,200));
    const locked=officialScenarioActive()&&$('sim-target').querySelector('[data-k="level"]').disabled;
    $('sim-official-copy').click();
    $('sim-whole-fight').open=true;
    const edit=(key,value)=>{const el=$('sim-block').querySelector('[data-k="'+key+'"]');
      if(el.type==='checkbox')el.checked=value;else el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}));};
    edit('level',4321);edit('headshot_pct',23);
    setScenarioFields({buffs:{'global-check':{stacks:9,locked:true}},extra_stats:{crit_chance:.42}});
    const simCount=$('sim-runs-input');simCount.value=17;simCount.dispatchEvent(new Event('change',{bubbles:true}));
    window.globalScenarioEditor=$('calculation-scenario-editor');
    return {locked,editor:$('sim-block').contains(globalScenarioEditor)&&!$('quick-calc').contains(globalScenarioEditor),
      id:activeScenario,count:loadPresetList(SCENARIOS).length,snapshot:snapshotScenario(),
      ranks:JSON.stringify(everyRank()),offers:everyRankOffers().map(([k,c])=>k+':'+c.id).sort()};
  })()`);
  app.check('official rulers stay locked and editing uses their ordinary copy',first.locked);
  app.check('scenario editing stays in its original section',first.editor);
  const catalog=await app.evaluate(`(async()=>{
    await fetchScenarioCatalog();setScenarioView('all');
    const weapons=new Set(META.weapons.filter(w=>w.slot==='archgun').map(w=>w.id));
    const buff=scenarioCatalog.find(b=>!b.weapons.includes('laetum')&&b.weapons.some(id=>weapons.has(id)));
    const source=structuredClone(sim.buffs),input=$('sim-buffs').querySelector('[data-b="'+buff.id+'"][data-f="stacks"]');
    const complete=!!input&&!input.disabled;
    input.value=9;input.dispatchEvent(new Event('change',{bubbles:true}));
    const weapon=buff.weapons.find(id=>weapons.has(id));
    const selector=$('dd-deployment'),deployment=weaponInfo(weapon).deployments[0];
    selector.value=deployment;selector.dispatchEvent(new Event('change',{bubbles:true}));
    history.pushState(null,'',weaponPath(weapon)+'/simulator');await route();
    const preserved=sim.buffs[buff.id].stacks===9&&sim.deployment===deployment&&scenarioBuffProjection().find(b=>b.id===buff.id).applicable;
    history.pushState(null,'','/weapons/Laetum/simulator');await route();
    scenarioQuery='frenzy';renderSim();renderSimBuffs();
    const shown=[...$('sim-buffs').querySelectorAll('.buff-card')].filter(card=>!card.hidden);
    const searched=shown.length>0&&shown.length<scenarioCatalog.length&&shown.every(card=>card.dataset.search.toLowerCase().includes('frenzy'));
    scenarioQuery='';setScenarioView('context');
    const candidate=scenarioBuffProjection().find(b=>b.trigger),old=sim.buff_triggers_off;
    sim.buff_triggers_off=[candidate.trigger];renderSimBuffs();
    const denied=$('sim-buffs').querySelector('[data-b="'+candidate.id+'"][data-f="stacks"]');
    const editable=!denied.disabled;sim.buff_triggers_off=old;renderSimBuffs();
    return {complete,preserved,searched,editable,id:buff.id,sourceUnchanged:sim.buffs['global-check'].stacks===source['global-check'].stacks};
  })()`);
  app.check('the global catalog can introduce an unrelated Arch-Gun buff',catalog.complete);
  app.check('Arch-Gun buff and deployment edits survive weapon changes',catalog.preserved&&catalog.sourceUnchanged);
  app.check('global search survives asynchronous buff-card redraws',catalog.searched);
  app.check('a denied trigger leaves its global input editable',catalog.editable);
  for(const module of ['simulator','optimizer']){
    const view=await app.evaluate(`(async()=>{
      history.pushState(null,'','/weapons/Laetum/${module}');await route();
      return {same:$('calculation-scenario-editor')===window.globalScenarioEditor&&document.querySelectorAll('#calculation-scenario-editor').length===1,
        visible:$('quick-calc').offsetParent===null&&($('calculation-scenario-editor').offsetParent!==null)===('${module}'==='simulator'),
        value:sim.level===4321&&sim.headshot_pct===23&&$('gp-runs').value==='30'&&!$('gp-on').checked&&simRuns()===17};
    })()`);
    app.check(module+' retains global state while controls stay in their modules',view.same&&view.visible&&view.value);
  }
  const other=await app.evaluate(`(async()=>{
    history.pushState(null,'','/weapons/Artax/simulator');await route();
    const raw=$('sim-technique').querySelector('[data-k="headshot_pct"]');
    const preserved=raw.value==='23'&&!raw.disabled&&sim.buffs['global-check'].stacks===9&&sim.extra_stats.crit_chance===.42;
    setScenarioView('all');
    const retained=$('sim-buffs').querySelector('[data-b="global-check"][data-f="stacks"]');
    const editableRetained=retained?.value==='9'&&!retained.disabled;
    if(retained){retained.value=11;retained.dispatchEvent(new Event('change',{bubbles:true}));}
    const headshot=$('sim-technique').querySelector('[data-k="headshot_pct"]');
    headshot.value=43;headshot.dispatchEvent(new Event('change',{bubbles:true}));
    const row=$('sim-technique').querySelector('[data-k="headshot_pct"]').closest('label');
    const forced=row.querySelector('.scenario-effective').textContent.includes('0')&&row.querySelector('input').value==='43';
    history.pushState(null,'','/weapons/Artax/builder');await route();
    $('gp-ranks').click();const list=everyRank();
    return {preserved,forced,editableRetained,id:activeScenario,count:loadPresetList(SCENARIOS).length,
      ranks:JSON.stringify(list),shown:$('quick-calc').querySelectorAll('.rk-x').length,
      total:list.mods.length+list.arcanes.length,offers:everyRankOffers().map(([k,c])=>k+':'+c.id).sort()};
  })()`);
  app.check('a companion view retains unrelated global scenario values',other.preserved&&other.id===first.id&&other.count===first.count);
  app.check('settings unused by the current build remain globally editable',other.editableRetained);
  app.check('global values remain editable while the effective forced value is explained',other.forced);
  app.check('global rank selection and candidates do not narrow to the weapon',other.ranks===first.ranks&&other.shown===other.total&&JSON.stringify(other.offers)===JSON.stringify(first.offers));
  const buffs=await app.evaluate(`(()=>{
    const b={id:'global-buff',name:'Global buff',max_stacks:2,default_stacks:0,default_locked:false};
    const check=draw=>{const cfg={'global-buff':{stacks:9,locked:true}},before=JSON.stringify(cfg),box=document.createElement('div');
      draw(box,[b],cfg);return before===JSON.stringify(cfg)&&box.querySelector('[data-f="stacks"]').value==='9';};
    const stable=check(renderBuffCards),negative=check((box,list,cfg)=>{cfg['global-buff'].stacks=2;renderBuffCards(box,list,cfg);});
    const empty={},box=document.createElement('div');renderBuffCards(box,[b],empty);
    return {stable,rejectsClamp:!negative,sparse:Object.keys(empty).length===0};
  })()`);
  app.check('rendering a contextual buff view neither clamps nor seeds the shared map',buffs.stable&&buffs.sparse);
  app.check('the preservation check rejects a view that clamps stored stacks',buffs.rejectsClamp);
  await app.evaluate('flushPresetSaves()');
  await app.load('/weapons/Laetum/builder',6500);
  app.check('global edits made from another weapon survive reload',await app.evaluate(`activeScenario===${JSON.stringify(first.id)}&&sim.level===4321&&sim.headshot_pct===43&&sim.buffs['global-check'].stacks===11&&sim.extra_stats.crit_chance===.42&&gainRuns()===30&&gainPrefs.on===false&&simRuns()===17`));
  await app.send('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:1,mobile:true});
  await app.evaluate("(async()=>{history.pushState(null,'','/weapons/Laetum/simulator');await route();})()");
  app.check('the shared editor and all buff cards fit a phone',await app.evaluate("(()=>{setScenarioView('all');return document.documentElement.scrollWidth<=innerWidth&&[...$('sim-buffs').querySelectorAll('.buff-card')].every(card=>card.scrollWidth<=card.clientWidth+1);})()"));
  await app.finish('global conditions are edited once and viewed in context');
}finally{await app.close();}
