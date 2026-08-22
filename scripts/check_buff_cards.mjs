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
  // THE SHOT COMBO COUNTER — the second weapon-passive card, and the one the
  // gating matters on. It is UNCAPPED (the tiers keep climbing), it belongs to
  // the BASE form of an Incarnon cycle, and it exists only while the fight is
  // scoped in — so this walks all three: the card is there aiming, it is GONE
  // from the hip, and setting it moves the number.
  history.pushState({},'','/weapons/Vectis_Prime'); route(); await sleep(3000);
  slots.forEach(s => { s.mod = null; s.rank = null; });
  ['serration','split_chamber','point_strike','vital_sense'].forEach((id,i) => {
    slots[i] = { mod:id, pol:slots[i].pol, rank:null };
  });
  markPresetDirty(); renderMods(); refreshPanel(); await sleep(2500);
  document.querySelectorAll('.tab').forEach(x=>{ if(/Sim|模拟/i.test(x.textContent)) x.click(); });
  await sleep(1500);
  const card = () => [...document.querySelectorAll('#sim-buffs .buff-card')].map(e=>({
    name: e.querySelector('.bn').textContent.trim(),
    id: e.querySelector('input[data-f="stacks"]').dataset.b,
    stacks: e.querySelector('input[data-f="stacks"]').value,
    cap: e.querySelector('.bmax').textContent.trim(),
    hasMax: e.querySelector('input[data-f="stacks"]').hasAttribute('max'),
  })).find(c => c.id === 'sniper_combo');
  sim.level = 9999; sim.steel_path = true; sim.duration = 30; sim.runs = 6;
  sim.headshot_pct = 100; markScenarioDirty(); await sleep(600);
  const combo = card();
  const setC = (f, v) => {
    const el = document.querySelector('#sim-buffs input[data-b="sniper_combo"][data-f="'+f+'"]');
    if (el.type === 'checkbox') el.checked = v; else el.value = v;
    el.dispatchEvent(new Event('change'));
  };
  // The answer is taken off the WIRE rather than off the page: a DPS read from
  // a formatted cell is a test of the formatter.
  let shot = null;
  const realApi = window.api;
  window.api = async (path, body) => {
    const res = await realApi(path, body);
    if (path === '/api/simulate') shot = res;
    return res;
  };
  // …AND OFF THE FLEET, which is what Run Sim uses now: a sharded simulation
  // never touches api at all, so an interception of it alone came back null
  // and this file's whole point — that a card MOVES THE NUMBER — went untested
  // (2026-08-18).
  const realFleet = window.simulateFleet;
  window.simulateFleet = async (body, onProgress) => {
    const res = await realFleet(body, onProgress);
    shot = res;
    return res;
  };
  const runDps = async () => {
    shot = null;
    document.getElementById('run-sim').click();
    for (let k=0;k<60 && !shot; k++) await sleep(1000);
    return shot && shot.dps_mean;
  };
  const dpsCold = await runDps();
  setC('stacks', 405); setC('locked', true); await sleep(800);
  const dpsHeld = await runDps();
  const comboRows = [...document.querySelectorAll('.rp-row')].map(e=>e.querySelector('.rp-name').textContent.trim());
  // ...AND FROM THE HIP IT IS NOT THERE. The mechanic's own condition is
  // "requires being scoped in", answered once in resolve() — so this is the
  // assertion that the card follows the FIGHT rather than the weapon.
  sim.aiming = false; markScenarioDirty(); refreshPanel(); await sleep(2000);
  const hipCombo = card();
  sim.aiming = true; markScenarioDirty(); await sleep(300);
  // A ROW DRAWN AS A NUMBER. Hata-Satya is the one buff whose ceiling DE
  // publishes as a VALUE (500%) instead of as a stack count, so its curve is a
  // curve of per cent — and the stepper beside it is still in HITS, because a
  // hit is what a player would count. Galvanized Chamber rides along as the
  // negative control: an ordinary stack-capped buff in the same run must still
  // read a count out of a count.
  history.pushState({},'','/weapons/Soma_Prime'); route(); await sleep(3000);
  slots.forEach(s => { s.mod = null; s.rank = null; });
  slots[0] = { mod:'hata_satya', pol:slots[0].pol, rank:null };
  slots[1] = { mod:'galvanized_chamber', pol:slots[1].pol, rank:null };
  markPresetDirty(); renderMods(); refreshPanel(); await sleep(2500);
  document.querySelectorAll('.tab').forEach(x=>{ if(/Sim|模拟/i.test(x.textContent)) x.click(); });
  await sleep(1500);
  const hsCard = [...document.querySelectorAll('#sim-buffs .buff-card')].map(e=>({
    id: e.querySelector('input[data-f="stacks"]').dataset.b,
    cap: e.querySelector('.bmax').textContent.trim(),
  })).find(c => c.id === 'crit_per_hit');
  sim.level = 300; sim.steel_path = true; sim.duration = 30; sim.runs = 4;
  markScenarioDirty(); await sleep(600);
  document.getElementById('run-sim').click();
  for (let k=0;k<60 && !document.querySelector('.rp-row'); k++) await sleep(1000);
  const hsRows = [...document.querySelectorAll('.rp-row[data-buff]')].map(e=>({
    name: e.querySelector('.rp-name').textContent.trim(),
    stat: e.querySelector('.rp-stat').textContent.split(/\\s+/).join(' ').trim(),
    now: e.querySelector('.rp-now').textContent.trim(),
  }));
  return { cards, rows, atZero, un, tend, tendRows, lang: LANG,
           combo, hipCombo, comboRows, dpsCold, dpsHeld, hsCard, hsRows };
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
// …AGAINST A CEILING THAT MAY BE AN INFINITY. An uncapped row prints "0.20/∞"
// and this demanded a digit after the slash, so it had been red since the
// debuff table started drawing the uncapped DoT families (2026-08-19) — the
// two-decimal property it exists for was holding the whole time.
check("every figure carries two decimals",
  r.rows.every(x => /[\d]+\.\d\d\/(\d|∞)/.test(x.stat) && /\d+\.\d\d%/.test(x.stat)
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
console.log("combo:", JSON.stringify(r.combo), "hip:", JSON.stringify(r.hipCombo),
  "dps", r.dpsCold, "->", r.dpsHeld);
check("the sniper's Shot Combo Counter has a card", !!r.combo, JSON.stringify(r.combo));
// THE NAME ITSELF, not the grants line under it. A weapon passive has no mod
// to borrow a localized name from, which is exactly how this card shipped
// reading "Shot Combo Counter" over a Chinese subtitle and passed a laxer
// assertion.
check("...named in Chinese", r.combo && /^[一-鿿]/.test(r.combo.name), r.combo && r.combo.name);
check("...and what a stack buys is Chinese too, whole rather than half",
  r.combo && !/(hits?|every|Damage)/.test(r.combo.name), r.combo && r.combo.name);
// UNCAPPED, and for a stated reason: the wiki's tiers keep climbing (the
// eighth is 11025 hits), so any ceiling here would be invented.
check("...uncapped, with no invented maximum",
  r.combo && /∞/.test(r.combo.cap) && !r.combo.hasMax, JSON.stringify(r.combo));
check("...earned from zero", r.combo && r.combo.stacks === "0", r.combo && r.combo.stacks);
check("...and it is drawn as a curve in the replay",
  (r.comboRows || []).some((n) => /连击|Combo/i.test(n)), JSON.stringify(r.comboRows));
// The card has to MOVE THE NUMBER. A control that draws and does nothing is
// the failure this whole file exists for.
check("...holding it at 405 multiplies the fight",
  r.dpsHeld > r.dpsCold * 1.1, `${r.dpsCold} -> ${r.dpsHeld}`);
// THE GATE. "Building combo and benefiting from its multiplier requires being
// scoped in" — a hip-fired fight has no counter, so it must offer no card.
check("a sniper fired from the HIP is offered no counter at all",
  !r.hipCombo, JSON.stringify(r.hipCombo));

// A CEILING THAT IS A NUMBER IS DRAWN AS A NUMBER (owner, 2026-08-22).
// Hata-Satya publishes "capped at 500% at all mod ranks" and lets the counter
// run, so a stack count charted against 417 would be a chart of a quantity DE
// never printed — the row draws the per cent instead, and the ceiling it is
// drawn against is the card's own.
console.log("hata-satya:", JSON.stringify(r.hsCard), JSON.stringify(r.hsRows));
const hs = (r.hsRows || []).find((x) => /%\/500%$/.test(x.now));
check("the crit-per-hit row is drawn as a PER CENT against the published cap",
  !!hs, JSON.stringify(r.hsRows));
check("...and its average is the same quantity, not a stack count",
  hs && /\d+\.\d%\/500%/.test(hs.stat), hs && hs.stat);
// THE STEPPER IS STILL IN HITS, and its maximum is the first hit that reaches
// the ceiling — 417 at max rank, which is the wiki's own column and one more
// than the "last stack that fits under 500%" this used to compute.
check("...while the card that seeds it counts HITS, to the 417th",
  r.hsCard && r.hsCard.cap === "/ 417", JSON.stringify(r.hsCard));
// THE NEGATIVE CONTROL, in the same run: an ordinary stack-capped buff is
// still a count out of a count. A page that had simply started formatting
// every row as a percentage would pass everything above and fail here.
check("...and an ordinary buff beside it still reads a count",
  (r.hsRows || []).some((x) => /^\d+\/\d+$/.test(x.now)), JSON.stringify(r.hsRows));

await app.finish("the buff cards read right in Chinese");
