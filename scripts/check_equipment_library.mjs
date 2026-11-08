import { openApp } from "./cdp.mjs";
const app = await openApp({lang:"zh", boot:12000});
try {
  const result = await app.evaluate(`(async () => {
    const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
    await loadWarframeCatalog();
    const raw = {kind:'warframe', build:{frame:'volt',
      mods:['vitality','redirection','steel_fiber','intensify','continuity','stretch','streamline','flow'].map(id=>({id})),
      aura:{id:'energy_siphon'}, exilus:{id:'power_drift'},
      arcanes:WFCAT.arcanes.slice(0,2).map(a=>({id:a.id})),
      shards:Array.from({length:5},()=>({shard:'crimson_archon_shard',effect:'melee_crit_damage',tauforged:false}))}};
    const checked = await api('/api/equipment/build', raw);
    if (!checked.accepted) return {accepted:false,reason:checked.reason};
    const out = {accepted:true, tau:checked.record.build.shards.every(s=>s.tauforged)};
    const lower = JSON.parse(JSON.stringify(raw)); lower.build.mods[0].rank=0;
    out.ranks=(await api('/api/equipment/build',lower)).id===checked.id;
    const originalFetch = window.fetch, sent=[];
    let failNext = false;
    window.fetch = async (url, options={}) => {
      if (String(url) === '/api/equipment/submit') {
        if (failNext) { failNext=false; throw new Error('test network outage'); }
        sent.push(JSON.parse(options.body));
        return new Response(JSON.stringify({ok:true,queued:true}),{headers:{'content-type':'application/json'}});
      }
      if (String(url) === '/api/equipment/library') {
        const body=JSON.parse(options.body), builds=body.kind==='warframe' ? [{id:checked.id,...checked.record}] : [];
        return new Response(JSON.stringify({ok:true,builds,next:null}),{headers:{'content-type':'application/json'}});
      }
      return originalFetch(url,options);
    };
    await nav('/warframes/Volt/builder');
    localStorage.removeItem('wfsim-equipment-uploaded-warframe:volt');
    wf=equipmentState(checked.record); wf.shards.forEach(s=>s.tauforged=false);
    const originalState=JSON.stringify(wf);
    wfChanged(); await sleep(1900);
    out.auto=sent.length===1 && sent[0].build.shards.every(s=>s.tauforged);
    out.editorPreserved=JSON.stringify(wf)===originalState;
    wf.slots.slice(0,8).reverse().forEach((s,i)=>wf.slots[i]=s);
    wfChanged(); await sleep(1900); out.dedup=sent.length===1;
    out.incompleteChecks=true;
    for (const field of ['mods','arcanes','shards','aura','exilus']) {
      const incomplete=JSON.parse(JSON.stringify(raw));
      if (Array.isArray(incomplete.build[field])) incomplete.build[field].pop(); else incomplete.build[field]=null;
      const verdict=await api('/api/equipment/build',incomplete);
      if (verdict.accepted) out.incompleteChecks=false;
      scheduleEquipmentPublish('warframe',incomplete.build);
    }
    await sleep(1800); out.incomplete=sent.length===1;
    await nav('/warframes/Volt'); await sleep(300);
    out.publicCard=!!document.querySelector('#wf-library [data-equipment-copy]');
    out.noManualUpload=!document.querySelector('#wf-library [data-equipment-submit],#wf-library [data-upload]');
    const before=presetListWithIds(WF_BUILDS,'volt').length;
    document.querySelector('#wf-library [data-equipment-copy]').click(); await sleep(300);
    out.copy=location.pathname==='/warframes/Volt/builder' && presetListWithIds(WF_BUILDS,'volt').length===before+1;
    out.copyTau=wf.shards.every(s=>s.tauforged);
    const timers=window.setTimeout;
    window.setTimeout=(fn,ms,...args)=>timers(fn,ms===15000?25:ms,...args);
    localStorage.removeItem('wfsim-equipment-uploaded-warframe:volt');
    failNext=true; const uploaded=sent.length;
    wfChanged(); await sleep(2400);
    out.retry=sent.length===uploaded+1;
    window.setTimeout=timers;
    await nav('/companions/Sentinel'); await sleep(200);
    out.companionLibrary=!document.getElementById('comp-library').hidden && document.getElementById('comp-mods-block').hidden;
    await nav('/companions/Sentinel/builder');
    out.companionBuilder=document.getElementById('comp-library').hidden && !document.getElementById('comp-mods-block').hidden;
    const families=new Set();
    comp.slots=compPool('sentinel').filter(m=>families.has(m.family || m.id) ? false : (families.add(m.family || m.id),true))
      .slice(0,10).map(m=>({mod:m.id,rank:null,pol:m.polarity}));
    localStorage.removeItem('wfsim-equipment-uploaded-companion:sentinel');
    compChanged(); await sleep(1900);
    out.companionAuto=sent.some(s=>s.kind==='companion' && s.build.mods.length===10);
    await nav('/companions/Sentinel?build='+encodeURIComponent(compActive));
    out.legacyCompanionLink=document.getElementById('comp-library').hidden && !document.getElementById('comp-mods-block').hidden;
    window.fetch=originalFetch;
    return out;
  })()`);
  for (const [name,value] of Object.entries(result)) app.check(name,value === true,JSON.stringify(result));
  await app.finish("complete equipment builds upload as canonical records and both libraries open editable copies");
} catch(error) { await app.close(); throw error; }
