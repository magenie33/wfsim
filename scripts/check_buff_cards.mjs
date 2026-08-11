// BUFF CARDS: named in the display language, opened at the right stack count,
// and honest about coverage.
//
// Four things this guards, each of which has been wrong:
//   · a buff granted by an EVOLUTION was the only card left in English,
//     because the name lookup knew about mods and arcanes and nothing else;
//   · the earned-from-zero default has to REACH the card, not just the server;
//   · uptime was rounding 99.83% up to a flat "100%", which is the one number
//     a reader will not believe (user, 2026-08-03);
//   · a buff with no card at all: the Ocucor's TENDRILS, which are what its
//     only augment scales with. A tendril costs a kill, so against a target
//     that dies slowly the mod measured as nothing and there was no knob to
//     say otherwise (player report, 2026-08-08).
//
//   node scripts/check_buff_cards.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ lang: "zh" });
const { evaluate, check, sleep, send } = app;

const r = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  history.pushState({},'','/weapons/Laetum'); route(); await sleep(3000);
  evoSel = {1:'laetum_evo1_incarnon_form',2:'laetum_rapid_wrath',3:'laetum_lethal_rearmament',
            4:'laetum_caput_mortuum',5:'laetum_overwhelming_attrition'};
  markPresetDirty(); renderEvo(); refreshPanel(); await sleep(2500);
  document.querySelectorAll('.tab').forEach(x=>{ if(/Sim|模拟/i.test(x.textContent)) x.click(); });
  await sleep(1200);
  const cards = [...document.querySelectorAll('#sim-buffs .buff-card')].map(e=>({
    name: e.querySelector('.bn').textContent.trim(),
    stacks: e.querySelector('input[data-f="stacks"]').value,
    cap: e.querySelector('.bmax').textContent.trim(),
  }));
  sim.level = 300; sim.steel_path = true; sim.duration = 60; sim.runs = 6;
  markScenarioDirty(); await sleep(600);
  document.getElementById('run-sim').click();
  for (let k=0;k<40 && !document.querySelector('.rp-row'); k++) await sleep(1000);
  const rows = [...document.querySelectorAll('.rp-row')].map(e=>({
    name: e.querySelector('.rp-name').textContent.trim(),
    stat: e.querySelector('.rp-stat').textContent.split(/\\s+/).join(' ').trim(),
    now: e.querySelector('.rp-now').textContent.trim(),
    mean: !!e.querySelector('.rp-mean'),
    dead: e.querySelectorAll('.rp-dead').length,
  }));
  // Rewind to t=0, where every buff is off, and read the live counts.
  const sc = document.getElementById('rp-scrub');
  sc.value = 0; sc.dispatchEvent(new Event('input')); await sleep(400);
  const atZero = [...document.querySelectorAll('.rp-now')].map(e=>e.textContent.trim());
  // The UNCAPPED card: Secondary Enervate ramps with no ceiling, so its card
  // must show an infinity rather than a number somebody invented.
  history.pushState({},'','/weapons/Dual_Toxocyst'); route(); await sleep(3000);
  arcanes = ['secondary_enervate']; markPresetDirty(); renderMods(); refreshPanel(); await sleep(2500);
  document.querySelectorAll('.tab').forEach(x=>{ if(/Sim|模拟/i.test(x.textContent)) x.click(); });
  await sleep(1500);
  const un = [...document.querySelectorAll('#sim-buffs .buff-card')]
    .map(e=>({ name:e.querySelector('.bn').textContent.trim(),
               cap:e.querySelector('.bmax').textContent.trim(),
               stacks:e.querySelector('input[data-f="stacks"]').value,
               hasMax:e.querySelector('input[data-f="stacks"]').hasAttribute('max') }))
    .filter(c=>/失活|Enervate/.test(c.name));
  // THE WEAPON-PASSIVE CARD. The Ocucor's tendrils are a buff by every test
  // that matters — gained on a kill, cleared by a magazine event, capped by
  // the weapon — and Sentient Surge is the mod that reads the count. The card
  // has to exist, cap at the WEAPON's 4, and REACH the fight: set it and the
  // replay must show the count it was set to.
  history.pushState({},'','/weapons/Ocucor'); route(); await sleep(3000);
  slots[0] = { mod:'sentient_surge', pol:slots[0].pol, rank:null };
  markPresetDirty(); renderMods(); refreshPanel(); await sleep(2500);
  document.querySelectorAll('.tab').forEach(x=>{ if(/Sim|模拟/i.test(x.textContent)) x.click(); });
  await sleep(1500);
  const tend = [...document.querySelectorAll('#sim-buffs .buff-card')].map(e=>({
    name: e.querySelector('.bn').textContent.trim(),
    id: e.querySelector('input[data-f="stacks"]').dataset.b,
    stacks: e.querySelector('input[data-f="stacks"]').value,
    cap: e.querySelector('.bmax').textContent.trim(),
  }));
  sim.level = 300; sim.steel_path = true; sim.duration = 30; sim.runs = 4;
  markScenarioDirty(); await sleep(600);
  const set = (f, v) => {
    const el = document.querySelector('#sim-buffs input[data-b="tendrils"][data-f="'+f+'"]');
    if (el.type === 'checkbox') el.checked = v; else el.value = v;
    el.dispatchEvent(new Event('change'));
  };
  set('stacks', 4);
  // A tendril has no clock — what ends it is the magazine event — so "no
  // timeout" is what stops a reload from taking them.
  set('locked', true);
  await sleep(800);
  document.getElementById('run-sim').click();
  for (let k=0;k<40 && !document.querySelector('.rp-row'); k++) await sleep(1000);
  const tendRows = [...document.querySelectorAll('.rp-row')].map(e=>({
    name: e.querySelector('.rp-name').textContent.trim(),
    now: e.querySelector('.rp-now').textContent.trim(),
  }));
  return { cards, rows, atZero, un, tend, tendRows, lang: LANG };
})()`);

console.log("lang:", r.lang);
console.log("cards:", JSON.stringify(r.cards, null, 1));
console.log("rows :", JSON.stringify(r.rows, null, 1));
check("both evolution buffs have a card", r.cards.length === 2, JSON.stringify(r.cards.map(c=>c.name)));
check("their names are Chinese", r.cards.every(c => /[\u4e00-\u9fff]/.test(c.name)), r.cards.map(c=>c.name).join(","));
check("they open at 0 stacks", r.cards.every(c => c.stacks === "0"), r.cards.map(c=>c.stacks).join(","));
check("a capped buff shows its own ceiling", r.cards.every(c => /\/ ?\d+/.test(c.cap)), r.cards.map(c=>c.cap).join(","));
check("the coverage rows are Chinese too", r.rows.every(x => /[\u4e00-\u9fff]/.test(x.name)), r.rows.map(x=>x.name).join(","));
// ...and the "full at" figure only where there IS one: a buff that never
// reached its cap says so in words, and demanding a time off it was demanding
// the row lie. The Ocucor's tendrils against a target that dies slowly is
// exactly that row, which is the case the card was added for.
check("every figure carries two decimals",
  r.rows.every(x => /[\d]+\.\d\d\/\d/.test(x.stat) && /\d+\.\d\d%/.test(x.stat)
    && (/\d+\.\d\ds/.test(x.stat) || /never|未满层/.test(x.stat))),
  r.rows.map(x=>x.stat).join(" | "));
check("uptime is never a flat 100%", r.rows.every(x => !/(^|[^.\d])100%/.test(x.stat)), r.rows.map(x=>x.stat).join(" | "));
check("the average is drawn on the curve", r.rows.every(x => x.mean));
check("the inactive stretches are banded", r.rows.every(x => x.dead > 0), r.rows.map(x=>x.dead).join(","));
check("at t=0 every buff reads zero", r.atZero.every(x => /^0\//.test(x)), r.atZero.join(" | "));
check("Secondary Enervate has a card of its own", r.un.length === 1, JSON.stringify(r.un));
check("...uncapped, shown as infinity", r.un[0] && /∞/.test(r.un[0].cap), r.un[0] && r.un[0].cap);
check("...starting at 0, with no invented maximum",
  r.un[0] && r.un[0].stacks === "0" && !r.un[0].hasMax, JSON.stringify(r.un[0]));
console.log("tendrils:", JSON.stringify(r.tend), JSON.stringify(r.tendRows));
const td = (r.tend || []).find((c) => c.id === "tendrils");
check("the Ocucor's tendrils have a card", !!td, JSON.stringify(r.tend));
check("...named in Chinese, saying what a stack IS",
  td && /卷须/.test(td.name), td && td.name);
check("...capped at the WEAPON's tendril limit, and earned from zero",
  td && td.cap === "/ 4" && td.stacks === "0", JSON.stringify(td));
check("...and the count reaches the fight",
  (r.tendRows || []).some((x) => x.now === "4/4"), JSON.stringify(r.tendRows));
await app.finish("the buff cards read right in Chinese");
