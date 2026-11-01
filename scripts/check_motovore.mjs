import { openApp } from "./cdp.mjs";

const app = await openApp({ lang: "zh", boot: 14000 });
const { evaluate, check } = app;
const offered = await evaluate(`(async () => {
  history.pushState({}, '', '/weapons/Coda_Motovore/builder'); await route();
  const w = weaponInfo('coda_motovore');
  return { spec: w?.valence, shown: !document.getElementById('element-block').hidden,
    lines: w?.passives, state: snapshotState(), capacity: capOf('coda_motovore') };
})()`);
check('Motovore offers the shared mandatory progenitor control', offered.shown && offered.spec?.elements.length === 7, JSON.stringify(offered));
if (!offered.spec) await app.finish('Motovore has no valence spec');
check('the build keeps its progenitor element and rank40 capacity', offered.state.valence.element === 'impact' && offered.state.valence.bonus === 0.6 && offered.capacity === 80, JSON.stringify(offered.state.valence));
const r = await evaluate(`(async () => {
  const body = (mods, element='impact', mode='base') => ({ ...buildPayload(), ...theFight(),
    weapon:'coda_motovore', mode, mods:[...mods,'crushing_ruin'], arcanes:[], evolutions:[],
    valence_element:element, valence_bonus:0.6, duration:1, runs:1 });
  const at = async (mods, element, mode) => {
    const out = await api('/api/simulate',body(mods,element,mode));
    if (!out.ok) throw new Error(JSON.stringify(out));
    return out.panel;
  };
  const bare = await at([]), impact = await at(['primed_heavy_trauma','fury']),
    slash = await at(['buzz_kill','melee_prowess'],'heat'), heavy = await at([],'heat','heavy_slam');
  const info = await api('/api/panel',body(['primed_heavy_trauma']));
  const sources = JSON.stringify(info);
  const heavyBase = info.forms.find(f=>f.label==='Heavy Slam')?.parts
    .find(p=>p.label==='Radial explosion')?.stats.find(s=>s.key==='base_damage')?.final;
  const before = snapshotState();
  const restored = stateFromBuild({mods:[],arcane:[],evolutions:[],valence_element:'heat',valence_bonus:0.6,mode:'base'},'coda_motovore');
  return { bare, impact, slash, heavy, heavyBase, sources, original:before.valence, restored:restored.valence };
})()`);
const near = (actual, expected) => Math.abs(actual - expected) < 1e-4;
check('60% progenitor Impact raises base250 to400 while the default wheel gives60% status', near(r.bare.modified_base,400) && near(r.bare.status_chance,0.6) && near(r.bare.fire_rate,1),JSON.stringify(r.bare));
check('the Impact wheel joins Fury in the attack-speed bracket', near(r.impact.fire_rate,1.7) && near(r.impact.status_chance,0.3),JSON.stringify(r.impact));
check('the Slash wheel joins Melee Prowess in the status bracket', near(r.slash.status_chance,0.87),JSON.stringify(r.slash));
check('a pure-radial heavy slam keeps zero direct damage and1200 radial base with valence', near(r.heavy.modified_base,0) && near(Number(r.heavyBase),1200),JSON.stringify({direct:r.heavy,radial:r.heavyBase}));
check('panel sources attribute amplified physical mods and the active wheel', r.sources.includes('990%') && r.sources.includes('Physical mod passive'),r.sources.slice(0,250));
check('importing a build payload keeps its progenitor element', r.original.element==='impact' && r.restored?.element==='heat',JSON.stringify(r.restored));
await app.finish('Motovore uses shared build, panel and simulation paths');
