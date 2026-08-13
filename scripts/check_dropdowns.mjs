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
    // the quick calc scenario — search FORCED, because the list grows
    history.pushState({}, '', '/weapons/Burston_Prime'); route(); await s(2500);
    const scen = document.getElementById('gp-scen');
    out.scenIsDD = !!(scen && scen.dataset.dd);
    if (scen) {
      scen.click(); await s(300);
      out.scenOpen = !pop().hidden;
      out.scenSearchShown = !document.getElementById('dd-addbar').hidden;
    }
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
check("the quick calc's scenario IS the shared dropdown",
  v.scenIsDD && v.scenOpen, JSON.stringify({ isDD: v.scenIsDD, open: v.scenOpen }));
check("...and forces its search bar, because a scenario list grows",
  v.scenSearchShown === true, `search shown: ${v.scenSearchShown}`);
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

await app.finish("every dropdown on the site is the same dropdown");
