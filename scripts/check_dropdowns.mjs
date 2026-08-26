// THE SEVENTEENTH CHECK — there is ONE dropdown.
//
// Owner, 2026-08-06. The site had eight native `<select>`s beside a rich
// searchable picker, so the quick calc's SCENARIO — a list that grows, since
// a scenario is a preset you make — was the plainest control on the page
// while a mod list two blocks away searched and sorted.
//
// What this asserts is the thing that decays: not that the component exists,
// but that nothing has quietly gone back to a native select, and that the four
// contracts it has to honour still hold.
//
//   1. NO native `<select>` is left except `#weapon`, which is hidden and is
//      the router's source of truth rather than a control.
//   2. The SEARCH BAR follows its own rule — forced where a list grows (the
//      scenario), absent where the whole list is two rows (the language). A
//      search box over two options is furniture; the panel and rows are what
//      make it "one look".
//   3. `data-k` still works. The scenario panel binds its fields generically —
//      it reads `el.value` and listens for `change` — so the replacement keeps
//      `HTMLButtonElement.value` reflecting and dispatches `change` itself.
//      This is the riskiest part of the swap and the least visible.
//   4. NESTING. The mod picker's Sort control lives INSIDE `#mod-popover`, so
//      opening it must not close the picker it belongs to. `closePopovers`
//      takes the anchor and spares any popover containing it.
import { openApp } from "./cdp.mjs";
const app = await openApp({ boot: 11000 });
const { evaluate, check, sleep, send, BASE } = app;
const script = String.raw`(async () => {
  const s = (ms) => new Promise(r => setTimeout(r, ms));
  const out = {};
  const pop = () => document.getElementById('dd-popover');
  try {
    out.nativeSelects = [...document.querySelectorAll('select')]
      .map(x => ({ id: x.id || '(anon)', hidden: !x.offsetParent }));
    // The search affordance is DRAWN, not typed. An emoji is the one glyph the
    // platform renders in its own colour and its own shape, so it was the only
    // colour thing in a monochrome UI and looked different on every OS.
    out.addbars = [...document.querySelectorAll('.addbar')].map(b => {
      const st = getComputedStyle(b, '::before');
      return {
        text: b.innerText.trim(),
        drawn: (st.maskImage || st.webkitMaskImage || 'none') !== 'none',
        tinted: st.backgroundColor,
      };
    });
    // the topbar language control — two options, so NO search bar
    const lang = document.getElementById('lang-select');
    out.langIsDD = !!(lang && lang.dataset.dd);
    lang.click(); await s(300);
    out.langOpen = !pop().hidden;
    out.langRows = [...pop().querySelectorAll('.opt')].map(r => r.dataset.v);
    out.langSearchShown = !document.getElementById('dd-addbar').hidden;
    document.body.click(); await s(200);
    // a data-k scenario field — the generic binding must still fire
    history.pushState({}, '', '/weapons/Burston_Prime/simulator'); route(); await s(3500);
    const dk = [...document.querySelectorAll('[data-dd][data-k]')][0];
    // The scenario fields come up disabled on this navigation path — a
    // PRE-EXISTING behaviour, identical in the build before the swap — so the
    // check forces one live rather than measuring that instead of the contract.
    if (dk) dk.disabled = false;
    out.dataKField = dk ? dk.dataset.k : null;
    out.dataKTag = dk ? dk.tagName : null;
    if (dk) {
      const before = dk.value;
      dk.click(); await s(400);
      const rows = [...pop().querySelectorAll('.opt')];
      const other = rows.find(r => r.dataset.v !== before);
      if (other) {
        const want = other.dataset.v;
        other.click(); await s(600);
        out.picked = want;
        out.simAfter = sim[dk.dataset.k];
        out.valueAfter = document.querySelector('[data-dd][data-k="' + dk.dataset.k + '"]').value;
      }
    }
    document.body.click(); await s(200);
    // THE QUICK CALC'S FIGHT IS NOT A DROPDOWN, and that is the assertion.
    // It was one until 2026-08-17 — a second, persisted pointer at a scenario,
    // sticky across weapons and sessions, silently disagreeing with the fight
    // the simulator was on. The control that replaced it STATES the fight and
    // cannot be picked from; see theFight().
    history.pushState({}, '', '/weapons/Burston_Prime'); route(); await s(2500);
    const scen = document.getElementById('gp-scen');
    out.scenExists = !!scen;
    out.scenIsDD = !!(scen && scen.dataset.dd);
    out.scenNames = scen ? scen.textContent.trim() : null;
    out.scenIsActiveFight = scen
      ? scen.textContent.trim() === gainScenario().name : null;
    document.body.click(); await s(200);
    // NESTED — the mod picker's Sort is inside #mod-popover
    const slot = document.querySelector('#mod-slots .slot');
    slot.click(); await s(1500);
    out.pickerOpen = !document.getElementById('mod-popover').hidden;
    const pk = document.getElementById('pk-sort');
    out.pkIsDD = !!(pk && pk.dataset.dd);
    if (pk) {
      pk.click(); await s(400);
      out.ddOpenInsidePicker = !pop().hidden;
      out.pickerStillOpen = !document.getElementById('mod-popover').hidden;
    }
  } catch (e) { out.threw = String((e && e.message) || e); }
  return out;
})()`;
const r = await send("Runtime.evaluate", { expression: script, awaitPromise: true, returnByValue: true });
const v = r.result?.result?.value;
if (!v || v.threw) {
  console.log("FAIL  the page threw:", (v && v.threw) || r.result?.exceptionDetails?.exception?.description?.slice(0, 400));
  check("the page did not throw", false);
  await app.finish("");   // unreachable: the check above already failed
}
const visibleSelects = v.nativeSelects.filter((x) => !x.hidden || x.id !== "weapon");
check("no native <select> is left on the page",
  visibleSelects.length === 0, JSON.stringify(v.nativeSelects));
check("every search bar draws its icon rather than typing one",
  v.addbars.length > 0 && v.addbars.every((b) => b.drawn),
  JSON.stringify(v.addbars.map((b) => b.drawn)));
check("...and none of them contains an emoji",
  v.addbars.every((b) => !/\p{Extended_Pictographic}/u.test(b.text)),
  JSON.stringify(v.addbars.map((b) => b.text)));
check("the topbar language control is the shared dropdown",
  v.langIsDD && v.langOpen && JSON.stringify(v.langRows) === JSON.stringify(["en", "zh"]),
  JSON.stringify({ isDD: v.langIsDD, open: v.langOpen, rows: v.langRows }));
check("a two-option list gets NO search bar",
  v.langSearchShown === false, `search shown: ${v.langSearchShown}`);
check("the quick calc's fight is NOT a dropdown — there is nothing to pick",
  v.scenExists === true && v.scenIsDD === false,
  JSON.stringify({ exists: v.scenExists, isDD: v.scenIsDD }));
check("...it NAMES the fight the simulator is on instead",
  v.scenIsActiveFight === true, `on screen: ${v.scenNames}`);
check("a scenario field is a button that still carries data-k",
  v.dataKTag === "BUTTON" && !!v.dataKField, JSON.stringify({ tag: v.dataKTag, k: v.dataKField }));
check("picking writes through the GENERIC data-k binding",
  !!v.picked && v.simAfter === v.picked,
  JSON.stringify({ picked: v.picked, sim: v.simAfter }));
check("...and the trigger's own value reflects it",
  v.valueAfter === v.picked, JSON.stringify({ value: v.valueAfter, picked: v.picked }));
check("a dropdown INSIDE the mod picker opens",
  v.pkIsDD && v.pickerOpen && v.ddOpenInsidePicker,
  JSON.stringify({ isDD: v.pkIsDD, picker: v.pickerOpen, dd: v.ddOpenInsidePicker }));
check("...without closing the picker it belongs to",
  v.pickerStillOpen === true, `picker still open: ${v.pickerStillOpen}`);

// ---- 5. AND THERE IS ONE SLOT MENU -------------------------------------
//
// The same claim one level down. Every axis that can hold something draws the
// same card with the same ⋯ at its top right, and the ⋯ opens the same
// Swap/Remove menu — so the menu has to arrive in the same place. It did not:
// a MOD slot passed its own button as the anchor while the arcane, evolution,
// kitgun-part and valence slots passed the whole CARD, and `place` puts a
// popover under its anchor's bottom-LEFT. Same control, same gesture, and the
// menu appeared under the ⋯ on one and at the card's BOTTOM-LEFT on the other
// four (owner, 2026-08-26). Measured on an evolution row: the fix moves it
// 152px right and 46px up.
//
// ASSERTED AS A RELATION, not as coordinates. Where the menu lands depends on
// the width, the clamp and where the card sits, none of which this is about —
// what has to hold is that it hangs off ITS OWN BUTTON, which is one number
// (the gap under the button) and one that no layout change can drift.
const MENUS = [
  // Laetum carries three of the five: mods, arcanes and Incarnon evolutions.
  ["Laetum", "mod-slots"], ["Laetum", "arcane-slots"], ["Laetum", "evo-rows"],
  // …and the two that are a weapon KIND rather than a slot everyone has.
  ["Tombfinger", "assembly-row"], ["Kuva_Nukor", "element-cfg"],
];
let lastWeapon = null;
for (const [weapon, host] of MENUS) {
  if (weapon !== lastWeapon) {
    await send("Page.navigate", { url: BASE + "/weapons/" + weapon });
    await sleep(11000);
    lastWeapon = weapon;
  }
  const m = await evaluate(`(async () => {
    const nap = (ms) => new Promise((r) => setTimeout(r, ms));
    // FILL WHAT IS EMPTY, because an empty slot has no ⋯ at all — the whole
    // plate opens the list — so this can only be asked of a filled one.
    const pool = poolWithRivens().filter((x) => !x.exilus);
    for (let i = 0; i < 8 && i < pool.length; i++) { slots[i].mod = pool[i].id; slots[i].rank = null; }
    renderMods();
    try { const ap = arcanePool(); if (ap && ap.length) { arcanes[0] = ap[0].id; renderArcanes(); } } catch (e) {}
    try {
      const wid = document.getElementById('weapon').value;
      const tiers = ((META.weapons.find((x) => x.id === wid) || {}).evolutions) || [];
      if (tiers.length && (tiers[0].options || []).length)
        pickEvolution(tiers[0].tier, tiers[0].options[0].id);
    } catch (e) {}
    await nap(900);
    const dots = document.querySelector('#' + ${JSON.stringify(host)} + ' .slot.filled .dots');
    if (!dots) return { missing: true };
    dots.click(); await nap(400);
    const menu = [...document.querySelectorAll('.popover')].find((p) => !p.hidden);
    if (!menu) return { noMenu: true };
    const d = dots.getBoundingClientRect(), b = menu.getBoundingClientRect();
    const card = dots.closest('.slot').getBoundingClientRect();
    return { gapUnderButton: Math.round(b.top - d.bottom),
             atCardBottom: Math.round(b.top - card.bottom),
             leftOfButton: Math.round(b.left - d.left) };
  })()`);
  check(`the ${host} card's ⋯ opens its menu`, !m.missing && !m.noMenu,
    JSON.stringify(m));
  if (m.missing || m.noMenu) continue;
  // The one number that is the claim: the menu sits just under the BUTTON.
  check(`...directly under that button, like every other axis`,
    m.gapUnderButton === 4, `${m.gapUnderButton}px under the ⋯`);
  // And the negative control, which is what the four broken ones scored: a
  // menu hung off the CARD lands under the card's bottom edge instead. Stated
  // so a future `place` that happens to put both in the same spot cannot make
  // the assertion above vacuous.
  check(`...rather than at the card's bottom-left`, m.atCardBottom !== 4,
    `${m.atCardBottom}px under the card, left offset ${m.leftOfButton}`);
}

await app.finish("every dropdown on the site is the same dropdown");
