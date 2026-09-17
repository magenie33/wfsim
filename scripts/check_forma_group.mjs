// SPDX-License-Identifier: AGPL-3.0-or-later
// BUILDS OF ONE ITEM SHARE ITS POLARITIES, AND THE RULES ARE THE PLAYER'S.
//
// Two saved Torid builds planned together must come out wearing ONE main
// layout, each keeping its own mods; the open build keeps its positions. The
// rules are global: a Catalyst switched off halves the capacity line, a Forma
// limit refuses in the page's own words, and a Warframe plans through the same
// box. The box is a BLOCK OF ITS OWN under the build bar on both pages: it
// belongs to the item and its builds, not to one build's mods. With mods kept
// in place nothing moves. Planning ahead over the Torid's board draws a curve
// that climbs, saves a pick already placed on its point, and places the ticked
// builds onto a point for nothing more. docs/INVESTMENT.md §The planner.
//
//   node scripts/check_forma_group.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, lang: "zh", base: process.env.WFSIM_BASE });
const { evaluate, check, send, sleep } = app;

const setup = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  localStorage.clear();
  localStorage.setItem('wfsim-lang', 'zh');
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(3500);
  const pool = currentPool.filter((m) => !m.exilus && !m.stance && !m.family);
  const by = (p) => pool.filter((m) => m.polarity === p).sort((a, b) => b.drain - a.drain);
  const st = (ids) => ({ ...snapshotState(), slots: Array.from({ length: 10 }, (_, i) =>
    ({ mod: ids[i] || null, pol: null, rank: null })) });
  const a = by('Madurai').slice(0, 5).concat(by('Naramon').slice(0, 3)).map((m) => m.id);
  const b = by('Madurai').slice(5, 7).concat(by('Vazarin').slice(0, 3), by('Naramon').slice(3, 6)).map((m) => m.id);
  localStorage.setItem('wfsim-presets-torid-builder-builds', JSON.stringify([
    { name: 'A', savedAt: 1, state: st(a) }, { name: 'B', savedAt: 2, state: st(b) }]));
  localStorage.setItem('wfsim-preset-active-torid-builder-builds', 'A');
  localStorage.setItem('wfsim-forma-group-torid', JSON.stringify(['B']));
  return { a, b };
})()`);
await send("Page.reload");
await sleep(9000);

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  await sleep(1500);
  const box = document.getElementById('forma-plan');
  const under = (id) => (document.getElementById(id).previousElementSibling || {}).id;
  const out = { active: activePreset, live0: slots.slice(0, 8).map((s) => s.mod),
    under: under('forma-block'), inMods: !!document.querySelector('#mod-block .forma-plan') };
  box.querySelector('.fp-run').click();
  await sleep(3000);
  const note = formaNotes.builder;
  out.plan = note && note.r;
  out.live = slots.slice(0, 9).map((s) => [s.mod, s.pol]);
  out.stored = JSON.parse(localStorage.getItem('wfsim-presets-torid-builder-builds'))
    .map((p) => ({ name: p.name, slots: p.state.slots.slice(0, 9).map((s) => [s.mod, s.pol]) }));
  out.table = box.querySelectorAll('.fp-table tr').length;

  // THE RULES ARE GLOBAL and change what the page measures against.
  const set = (k, v) => { const el = document.getElementById('forma-plan').querySelector('[data-r="' + k + '"]');
    if (el.type === 'checkbox') el.checked = v; else el.value = v;
    el.dispatchEvent(new Event('change')); };
  set('catalyst', false);
  out.capOff = document.getElementById('capacity').textContent;
  set('catalyst', true);
  out.capOn = document.getElementById('capacity').textContent;
  set('forma_limit', '1');
  document.getElementById('forma-plan').querySelector('.fp-run').click();
  await sleep(3000);
  out.refusal = document.getElementById('forma-plan').querySelector('.fp-result').textContent;
  set('forma_limit', '');
  out.limit = formaRules().forma_limit;

  // MODS STAY IN PLACE: neither build's mods move.
  set('fixed_order', true);
  const mods0 = slots.slice(0, 8).map((s) => s.mod).join();
  const bOf = () => JSON.parse(localStorage.getItem('wfsim-presets-torid-builder-builds'))
    .find((p) => p.name === 'B').state.slots.slice(0, 8).map((s) => s.mod).join();
  const b0 = bOf();
  document.getElementById('forma-plan').querySelector('.fp-run').click();
  await sleep(3000);
  out.fixed = { fits: !!(formaNotes.builder && formaNotes.builder.r && formaNotes.builder.r.fits),
    open: slots.slice(0, 8).map((s) => s.mod).join() === mods0,
    b: bOf() === b0 };
  set('fixed_order', false);

  // PLAN AHEAD over the whole board, with A and B as hard loadouts.
  const fb = document.getElementById('forma-plan');
  fb.querySelector('.fp-reach-run').click();
  for (let i = 0; i < 150 && (!formaReach || formaReach.busy); i++) await sleep(200);
  const x = formaReach || {};
  const curve = (x.r && x.r.curve) || [];
  out.reach = { ok: !!(x.r && x.r.fits), groups: (x.groups || []).length,
    points: curve.map((p) => [p.plan.regular + p.plan.umbra + p.plan.omni, p.worst]),
    sel: x.sel };
  const pt = curve[x.sel];
  const save = document.getElementById('forma-plan').querySelector('[data-save]');
  if (pt && save) {
    const gi = Number(save.dataset.save);
    save.click(); await sleep(300);
    const ps = JSON.parse(localStorage.getItem('wfsim-presets-torid-builder-builds'));
    const made = ps[ps.length - 1];
    const row = x.groups[gi].builds[pt.picks[gi].build].row;
    out.saved = { pols: made.state.slots.slice(0, 8).map((s) => s.pol).join(),
      layout: pt.plan.layout.main.map((p) => p || null).join(),
      mods: made.state.slots.slice(0, 9).map((s) => s.mod).filter(Boolean).length,
      rowMods: (row.mods || []).length + (row.exilus && row.exilus !== 'none' ? 1 : 0) };
    document.getElementById('forma-plan').querySelector('[data-apply]').click();
    await sleep(3000);
    const n = formaNotes.builder && formaNotes.builder.r;
    out.applied = { fits: !!(n && n.fits), live: slots.slice(0, 8).map((s) => s.pol || null).join(),
      layout: pt.plan.layout.main.map((p) => p || null).join() };
  }

  // …AND A FRAME PLANS THROUGH THE SAME BOX, under the same rules.
  history.pushState({}, '', '/warframes/Valkyr'); route(); await sleep(4000);
  const heavy = WFCAT.mods.filter((m) => !m.aura && !m.exilus && m.polarity === 'Madurai')
    .sort((x, y) => y.drain - x.drain).slice(0, 8);
  heavy.forEach((m, i) => { wf.slots[i].mod = m.id; wf.slots[i].rank = null; });
  wfChanged();
  document.getElementById('wf-forma-plan').querySelector('.fp-run').click();
  await sleep(3000);
  out.frame = formaNotes.warframe && formaNotes.warframe.r;
  out.frameUnder = under('wf-forma-block');
  out.frameCap = document.getElementById('wf-capacity').textContent;
  return out;
})()`);

const pols = (xs) => xs.slice(0, 8).map((x) => x[1]).join();
const mods = (xs) => xs.map((x) => x[0]).filter(Boolean).sort().join();
const b = (r.stored || []).find((p) => p.name === "B");
check("the Forma block sits under the build bar", r.under === "preset-bar-builder-builds" && !r.inMods,
  `${r.under} ${r.inMods}`);
check("…on the Warframe page too", r.frameUnder === "preset-bar-warframes", r.frameUnder);
check("the open build is A", r.active === "A", r.active);
check("both builds fit one plan", r.plan && r.plan.fits && r.plan.loadouts.length === 2, JSON.stringify(r.plan));
check("the open build kept its positions",
  r.live.slice(0, 8).map((x) => x[0]).join() === r.live0.join(), JSON.stringify(r.live));
check("B wears the open build's main layout", b && pols(b.slots) === pols(r.live), `${b && pols(b.slots)} vs ${pols(r.live)}`);
check("B keeps exactly its own mods", b && mods(b.slots) === [...setup.b].sort().join(), JSON.stringify(b));
check("the table has a row per build", r.table === 3, String(r.table));
check("a Catalyst switched off halves the capacity", /\/ 30$/.test(r.capOff) && /\/ 60$/.test(r.capOn), `${r.capOff} ${r.capOn}`);
check("a limit refuses in the page's own words", /超过你设的上限 1/.test(r.refusal), r.refusal);
check("clearing the limit stores none", r.limit === null, String(r.limit));
check("with mods in place, nothing moves", r.fixed.fits && r.fixed.open && r.fixed.b, JSON.stringify(r.fixed));
const pts = (r.reach && r.reach.points) || [];
check("plan ahead reads every ruler of the board", r.reach.ok && r.reach.groups >= 2, JSON.stringify(r.reach));
check("…and each point costs more and reaches further than the last",
  pts.length > 1 && pts.every((p, i) => i === 0 || (p[0] > pts[i - 1][0] && p[1] > pts[i - 1][1])), JSON.stringify(pts));
check("…and the marked point is the first on the line",
  pts.findIndex((p) => p[1] >= 0.8 - 1e-9) === r.reach.sel, JSON.stringify(r.reach));
check("a saved pick wears the point's layout", r.saved && r.saved.pols === r.saved.layout, JSON.stringify(r.saved));
check("…with every card of its row", r.saved && r.saved.mods === r.saved.rowMods, JSON.stringify(r.saved));
check("the ticked builds are placed onto the point", r.applied && r.applied.fits
  && r.applied.live === r.applied.layout, JSON.stringify(r.applied));
check("a frame plans through the same box", r.frame && r.frame.fits, JSON.stringify(r.frame));
check("…and its capacity line agrees", r.frame && r.frameCap === `${r.frame.loadouts[0].drain} / ${r.frame.capacity}`, r.frameCap);

await app.finish("builds planned together share one layout, under the player's rules");
