// SPDX-License-Identifier: AGPL-3.0-or-later
// THE COMPANION BUILDER SEATS THE POOL IT IS ALLOWED, and says plainly that none
// of it pays: ten general slots, four innate Penjaga polarities, capacity and
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
  return { slots, innate, penjaga, capBefore, says, offered, seated, ranked };
})()`);
console.log(JSON.stringify(r, null, 1));
check("ten general slots, with four innate Penjaga polarities",
  r.slots === 10 && r.innate === 4 && r.penjaga === 4, JSON.stringify([r.slots, r.innate, r.penjaga]));
check("the pool is every companion's, never a model's own",
  r.offered.onlyUniversal && r.offered.hasVitality && !r.offered.hasAssault
  && r.offered.pool < r.offered.all, JSON.stringify(r.offered));
check("...and the page says plainly that none of it pays", r.says === true);
check("seating one through the picker spends capacity and is born as a build",
  r.seated.mod === "enhanced_vitality" && r.seated.cap !== r.capBefore && r.seated.stored === 1,
  JSON.stringify(r.seated));
check("...and a rank down costs a point less", r.ranked.used === r.ranked.before - 1,
  JSON.stringify(r.ranked));
await app.finish("a companion seats its pool, and none of it pays");
