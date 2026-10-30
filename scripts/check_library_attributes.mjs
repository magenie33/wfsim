import {openApp} from './cdp.mjs';
const app=await openApp({boot:12000,lang:'en'});
const {evaluate,check,sleep}=app;
try {
  await app.load('/weapons/Laetum',8000);
  check('the advanced entry is available beside simple filters',await evaluate("!!document.querySelector('[data-library-advanced]')"));
  const ready=await evaluate(`(async()=>{
    const all=BOARD.laetum,step=Math.max(1,Math.floor(all.length/160));
    BOARD.laetum=all.filter((r,i)=>i%step===0).slice(0,160);renderBuildFinder();
    document.querySelector('[data-library-filters]').click();
    document.querySelector('[data-library-advanced]').click();
    document.querySelector('[data-query-add]').click();
    const input=document.querySelector('[data-query-value]');input.value='0';input.dispatchEvent(new Event('change',{bubbles:true}));
    for(let i=0;i<300&&libraryStatRun?.running;i++)await new Promise(r=>setTimeout(r,100));
    return {done:!libraryStatRun?.running&&!libraryStatRun?.error,static:libraryStatCache.size,expr:$('library-query-text').value,
      simpleHidden:document.querySelector('.library-simple-controls').hidden,sortSimple:!finder.advancedSort&&!document.querySelector('[data-library-sort]').hidden};
  })()`);
  check('adding an attribute runs only its permanent-stat calculation',ready.done&&ready.static>0);
  check('graphical edits compile to the advanced language',ready.expr.includes('crit_chance >= 0')&&ready.simpleHidden);
  check('advanced filters work with the original ordinary sort control',ready.sortSimple);
  const text='crit_chance <= 50 AND (status_chance >= 50 OR equipment != "mod:galvanized_diffusion") ORDER BY score DESC, crit_chance ASC';
  await evaluate(`$('library-query-text').value=${JSON.stringify(text.split(' ORDER BY ')[0])};document.querySelector('[data-query-apply="filter"]').click();document.querySelector('[data-library-sort-advanced]').click();$('library-sort-text').value='ORDER BY score DESC, crit_chance ASC';document.querySelector('[data-query-apply="sort"]').click()`);
  await sleep(500);
  const result=await evaluate(`(()=>{
    const w=weaponInfo('laetum'),all=builtinBuilds().filter(p=>p.benchmark===finder.b),get=p=>libraryStatCache.get(libraryStatKey(w,p))?.values;
    const expected=p=>{const v=get(p);return v&&v.crit_chance*100<=50+1e-9&&(v.status_chance*100>=50-1e-9||!finderTokens(p).includes('mod:galvanized_diffusion'));};
    const rows=[...document.querySelectorAll('#build-finder tr.fr:not(.mem)')].map(el=>builtinBuilds().find(p=>presetId(p)===el.dataset.frow));
    return {matched:all.filter(expected).length,rejected:all.filter(p=>!expected(p)).length,
      valid:rows.length>0&&rows.every(expected),order:rows.every((p,i)=>!i||rows[i-1].board.score>=p.board.score),
      keys:finder.query.orderBy.map(x=>x.field),url:location.pathname+location.search};
  })()`);
  check('AND, OR and parentheses match independently calculated attributes',result.valid&&result.matched>0&&result.rejected>0,JSON.stringify(result));
  check('advanced sort retains multiple priorities',result.order&&result.keys.join(',')==='score,crit_chance');
  const switched=await evaluate(`(()=>{
    const before=libraryQueryLanguage(finder.query);document.querySelector('[data-library-advanced]').click();
    const filterSimpleSortAdvanced=!finder.advanced&&finder.advancedSort&&!!document.querySelector('.library-retained-filter')&&!!document.querySelector('[data-query-sort-field]');
    document.querySelector('[data-library-sort-advanced]').click();
    const kept=before===libraryQueryLanguage(finder.query)&&!!document.querySelector('.library-expression-chip')&&!!document.querySelector('.library-retained-sort');
    $('fd-input').value='';finder.req.add('mod:galvanized_diffusion');renderBuildFinder();
    const merged=libraryQueryLanguage(finder.query);
    document.querySelector('[data-library-advanced]').click();
    const filterAdvancedSortSimple=finder.advanced&&!finder.advancedSort&&!document.querySelector('[data-library-sort]').hidden;
    document.querySelector('[data-library-sort-advanced]').click();
    return {kept,merged,query:libraryQueryLanguage(finder.query),advanced:finder.advanced,filterSimpleSortAdvanced,filterAdvancedSortSimple};
  })()`);
  check('ordinary filters and advanced sorting can be combined',switched.filterSimpleSortAdvanced);
  check('advanced filters and ordinary sorting can be combined',switched.filterAdvancedSortSimple);
  check('ordinary mode explicitly retains unprojected filters and every sort priority',switched.kept);
  check('a simple equipment edit adds to the same query tree',switched.advanced&&switched.merged.includes('equipment = "mod:galvanized_diffusion"')&&switched.merged.includes(' OR '));
  await evaluate(`$('library-query-text').value='crit_chance >= window.evil()';document.querySelector('[data-query-apply]').click()`);
  check('invalid text reports an inline error and preserves the active query',await evaluate(`
    !!$('library-query-error').textContent&&libraryQueryLanguage(finder.query)===${JSON.stringify(switched.query)}`));
  const syntax=await evaluate(`(()=>{
    const cases=['crit_chance >= 50 OR status_chance > 100 AND multishot >= 2','(crit_chance >= 50 OR status_chance > 100) AND multishot >= 2','ORDER BY crit_chance ASC, score DESC'];
    return cases.every(text=>{const one=libraryParseQuery(text),two=libraryParseQuery(libraryQueryLanguage(one));return JSON.stringify(one)===JSON.stringify(two);});
  })()`);
  check('precedence and sort-only queries survive language round trips',syntax);
  const retained = await evaluate(`(()=>{
    const saved=finder.query;
    try {
      finder.query=libraryParseQuery('equipment = "mod:a" OR equipment = "mod:b"');libraryProjectQuery();
      finder.req.add('mod:c');librarySyncQuery();
      const matches=tokens=>libraryEvaluateQuery(finder.query.where,{},tokens,new Map());
      return matches(['mod:a'])===false&&matches(['mod:c'])===false
        &&matches(['mod:a','mod:c'])===true&&matches(['mod:b','mod:c'])===true;
    } finally {finder.query=saved;libraryProjectQuery();}
  })()`);
  check('a simple edit preserves an original OR group and joins it with AND',retained);
  const sorted=await evaluate(`(async()=>{
    const before=libraryExpressionText(finder.query.where);
    $('library-sort-text').value='ORDER BY crit_chance ASC, score DESC';document.querySelector('[data-query-apply="sort"]').click();
    if(libraryExpressionText(finder.query.where)!==before)return false;
    const order=JSON.stringify(finder.query.orderBy);
    $('library-query-text').value='';document.querySelector('[data-query-apply="filter"]').click();
    if(JSON.stringify(finder.query.orderBy)!==order)return false;
    const rows=[...document.querySelectorAll('#build-finder tr.fr:not(.mem)')].map(el=>builtinBuilds().find(p=>presetId(p)===el.dataset.frow));
    const cc=p=>libraryStatCache.get(libraryStatKey(weaponInfo('laetum'),p)).values.crit_chance;
    return rows.length>1&&rows.every((p,i)=>!i||cc(rows[i-1])<cc(p)||Math.abs(cc(rows[i-1])-cc(p))<1e-9&&rows[i-1].board.score>=p.board.score);
  })()`);
  check('each area edits only its own rules and attribute sorting uses the next priority',sorted);
  const retried=await evaluate(`(async()=>{
    const original=api;let failed=false;
    api=(path,...args)=>{if(path==='/api/build/static'&&!failed){failed=true;return Promise.resolve({ok:false,error:'test unavailable'});}return original(path,...args);};
    try {
      libraryStatCache.clear();libraryStatRun=null;renderBuildFinder();
      for(let i=0;i<100&&!libraryStatRun?.error;i++)await new Promise(r=>setTimeout(r,50));
      const missing=!!libraryStatRun?.error&&!document.querySelector('#build-finder tr.fr')&&!!document.querySelector('[data-library-stats-retry]');
      api=original;document.querySelector('[data-library-stats-retry]').click();
      for(let i=0;i<300&&libraryStatRun?.running;i++)await new Promise(r=>setTimeout(r,100));
      return missing&&!libraryStatRun?.error&&!!document.querySelector('#build-finder tr.fr');
    } finally {api=original;}
  })()`);
  check('unavailable attributes stay distinct from zero and can be retried',retried);
  await app.load(result.url,8000);
  check('a copied URL restores the complete filter and sort expression',await evaluate(`libraryQueryLanguage(finder.query)===${JSON.stringify(text)}`));
  check('a copied URL restores the two editor modes independently',await evaluate('finder.advanced&&finder.advancedSort'));
  await app.send('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:1,mobile:true});
  await evaluate("document.querySelector('[data-library-advanced]').click()");
  check('the advanced editor fits a phone',await evaluate('document.documentElement.scrollWidth<=innerWidth'));
  await app.finish('one query language powers filters and sorting');
} finally {await app.close();}
