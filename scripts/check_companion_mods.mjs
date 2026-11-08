// SPDX-License-Identifier: AGPL-3.0-or-later
// THE COMPANION BUILDER SEATS THE POOL IT IS ALLOWED, and says plainly what of
// it pays: ten general slots, four innate Penjaga polarities, capacity and
// Forma answered, a card that names a model this host is not never offered.
//
//   node scripts/check_companion_mods.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear(); sessionStorage.clear();
  history.pushState({}, '', '/companions/Prototype_Companion'); route(); await sleep(4500);
  const slots = document.querySelectorAll('#comp-mod-slots .slot').length;
  // A POLARITY DRAWS AS ART, so an empty one is the nopol glyph rather than
  // empty text — counting text counted none of them.
  const innate = [...document.querySelectorAll('#comp-mod-slots .slot .pol-btn')]
    .filter((b) => !b.querySelector('.nopol')).length;
  const penjaga = comp.slots.filter((s) => s.pol === 'Penjaga').length;
  const capBefore = document.getElementById('comp-capacity').textContent;
  // THE PAGE SAYS IT IN WHATEVER LANGUAGE IT IS IN, so the assertion is that the
  // line is there and carries the sentence, not which words it is made of.
  const hint = document.querySelector('#comp-mods-block .exhint');
  const says = !!hint && hint.textContent.trim().length > 20 && hint.hasAttribute('data-i18n');
  // THE POOL: every card is unmodelled, and a model's own card is not offered.
  const pool = compPool('prototype_companion');
  const all = WFCAT.companion_mods;
  const offered = { pool: pool.length, all: all.length,
    onlyUniversal: pool.every((m) => m.compat === 'companion' || m.compat === 'robotic'),
    hasAssault: pool.some((m) => m.id === 'assault_mode'),
    hasVitality: pool.some((m) => m.id === 'enhanced_vitality') };
  // SEAT ONE, through the picker the reader uses.
  document.querySelector('#comp-mod-slots .slot').click(); await sleep(700);
  const rows = [...document.querySelectorAll('#wf-menu .opt[data-id]')];
  const row = rows.find((o) => o.dataset.id === 'enhanced_vitality');
  row.click(); await sleep(1200);
  const seated = { mod: comp.slots[0].mod, cap: document.getElementById('comp-capacity').textContent,
    stored: loadPresetList('companions', 'prototype_companion').length,
    forma: document.getElementById('comp-forma').textContent };
  // …AND A RANK COSTS A POINT.
  const before = compUsed();
  document.querySelector('#comp-mod-slots .slot.filled .rk[data-d="-1"]').click(); await sleep(900);
  const ranked = { used: compUsed(), before };
  const catalog = document.getElementById('comp-catalogue');
  const catalogue = { all: catalog.querySelectorAll('.opt').length,
    hasBite: all.some((m) => m.id === 'bite' && m.compat === 'beast_claws'),
    noBiteInHost: !pool.some((m) => m.id === 'bite') };
  const search = document.getElementById('comp-catalogue-search');
  search.value = 'bite'; search.dispatchEvent(new Event('input', { bubbles: true }));
  catalogue.filtered = catalog.querySelectorAll('.opt').length;
  return { slots, innate, penjaga, capBefore, says, offered, seated, ranked, catalogue };
})()`);
console.log(JSON.stringify(r, null, 1));
check("ten general slots, with four innate Penjaga polarities",
  r.slots === 10 && r.innate === 4 && r.penjaga === 4, JSON.stringify([r.slots, r.innate, r.penjaga]));
check("the pool is every companion's, never a model's own",
  r.offered.onlyUniversal && r.offered.hasVitality && !r.offered.hasAssault
  && r.offered.pool < r.offered.all, JSON.stringify(r.offered));
check("...and the page says plainly what of it pays", r.says === true);
check("seating one through the picker spends capacity and is born as a build",
  r.seated.mod === "enhanced_vitality" && r.seated.cap !== r.capBefore && r.seated.stored === 1,
  JSON.stringify(r.seated));
check("...and a rank down costs a point less", r.ranked.used === r.ranked.before - 1,
  JSON.stringify(r.ranked));
check("the searchable catalogue includes beast claws outside the robotic host's pool",
  r.catalogue.all === r.offered.all && r.catalogue.hasBite && r.catalogue.noBiteInHost
  && r.catalogue.filtered > 0 && r.catalogue.filtered < r.catalogue.all,
  JSON.stringify(r.catalogue));
const claws = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  history.pushState({}, '', weaponPath('adarza_claws') + '/builder');
  await route();
  for (let i = 0; i < 100 && document.getElementById('weapon').value !== 'adarza_claws'; i++) await sleep(100);
  await sleep(1500);
  const w = weaponInfo('adarza_claws');
  const axes = weaponAxes('adarza_claws');
  const pool = axes.mods;
  history.pushState({}, '', weaponPath('adarza_claws') + '/simulator');
  await route();
  await sleep(1500);
  document.querySelector('#preset-bar-simulator-scenarios .pchip.add').click();
  await sleep(500);
  renderSim();
  const field = document.querySelector('[data-k="attack_interval_seconds"]');
  const defaultValue = Number(field?.value);
  const assumedCard = fightCardHtml(sim).includes(tr('Assumed attack interval (s)'));
  if (field) {
    field.value = '0.2';
    field.dispatchEvent(new Event('change', { bubbles: true }));
  }
  const req = { weapon: 'adarza_claws', duration: 1.01, runs: 1, distance: 0,
    headshot_pct: 0, mods: ['maul', 'bite'] };
  const assumed = await api('/api/simulate', req);
  const result = await api('/api/simulate', { ...req, attack_interval_seconds: 0.2 });
  await sleep(600);
  const persisted = loadPresetList(SCENARIOS).find(p => p.id === activeScenario)?.state.attack_interval_seconds;
  return { count: META.weapons.filter(w => w.mod_class === 'beast_claws').length,
    manual: w.refire.kind, body: w.wielder_modelled,
    mainHasBite: pool.some(m => m.id === 'bite'), mainHasMaul: pool.some(m => m.id === 'maul'),
    noRifle: !pool.some(m => m.id === 'serration'), stance: axes.stance.length,
    noExilus: !axes.hasExilus, noArcane: axes.arcanes.length === 0, noRiven: w.no_riven,
    declared: META.scenario_axes.some(a => a.id === 'attack_interval_seconds'),
    field: !!field, defaultValue, assumedCard, assumedShots: assumed.shots,
    changed: sim.attack_interval_seconds, persisted,
    shots: result.shots, damage: result.dps,
    card: fightCardHtml(sim).includes('0.2') };
})()`);
check("all seventeen claw weapons expose their supported card pool", claws.count === 17
  && claws.mainHasBite && claws.mainHasMaul && claws.noRifle, JSON.stringify(claws));
check("postures use their own slot; claws have no exilus or arcane", claws.stance === 6
  && claws.noExilus && claws.noArcane && claws.noRiven, JSON.stringify(claws));
check("claws show an adjustable assumed interval and keep it in the scenario", claws.manual === "supplied_interval"
  && claws.body === false && claws.field && claws.changed === 0.2 && claws.persisted === 0.2
  && claws.card && claws.declared, JSON.stringify(claws));
check("claws run with the one-second assumption or an explicit interval", claws.defaultValue === 1
  && claws.assumedCard && claws.assumedShots === 2 && claws.shots === 6 && claws.damage > 0, JSON.stringify(claws));
await app.finish("a companion seats its pool and claws use adjustable assumed timing");
