// The FIFTEENTH check: the page FITS THE SCREEN IT IS ON.
//
// The failure it exists for, reported on a phone (owner, 2026-08-05): the mod
// grid was two columns at every width, and `grid-template-columns:repeat(2,1fr)`
// floors each track at its MIN-CONTENT. A slot's min-content is 198px, so two
// of them plus the gap is 404px inside a 326px column — the right-hand slots
// (2, 4, 6, 8) hung ~55px past the screen edge and their ⋯ button could only be
// reached by panning the page sideways.
//
// Why a check and not a one-line fix left to stand: horizontal overflow is
// INVISIBLE on the machine it is written on. Every desktop width has room, the
// browser silently allows the pan, and nothing in the other fourteen checks
// looks at geometry at all — they assert what the DOM SAYS, and this is a class
// of bug where the DOM says everything correctly and the layout is still wrong.
//
// It asserts two things at each width, for the builder's mod grid and for the
// three preset bars, which are the other wide row on the page:
//   1. nothing sticks out past the viewport, and
//   2. the page does not scroll sideways at all.
// Plus one thing that is not about overflow: that a mod's NAME still has room
// to be a name, because the cheapest way to stop an overflow is to squeeze a
// column to nothing and call it fixed.
import { openApp } from "./cdp.mjs";

// boot: 0 — this check navigates once per SCREEN SIZE in its own loop,
// with its own wait each time, so the opener has nothing to wait for.
const app = await openApp({ boot: 0 });
const { evaluate, check, sleep, send, BASE } = app;

const SCREENS = [
  ["iPhone SE", 375, 667, true],
  ["Android", 360, 800, true],
  ["iPhone 14", 390, 844, true],
  ["tablet", 768, 1024, false],
  ["desktop", 1280, 900, false],
];

for (const [label, w, h, mobile] of SCREENS) {
  await send("Emulation.setDeviceMetricsOverride",
    { width: w, height: h, deviceScaleFactor: mobile ? 2 : 1, mobile });
  await send("Page.navigate", { url: BASE });
  await sleep(mobile ? 11000 : 9000);

  const r = await evaluate(`(async () => {
    const sleep = (ms) => new Promise(r => setTimeout(r, ms));
    history.pushState({}, '', '/weapons/Ocucor'); route(); await sleep(2600);
    // A FULL build, because an empty slot is narrow and proves nothing: the
    // overflow came from the content of a filled card.
    const pool = (META.weapons.find(w => w.id === 'ocucor') || {}).mods || [];
    for (let i = 0; i < 8 && i < pool.length; i++) slots[i] = { mod: pool[i], pol: null, rank: null };
    renderMods(); await sleep(1400);

    const vw = document.documentElement.clientWidth;
    // Everything that could stick out, measured where the reader meets it.
    const widest = (sel) => [...document.querySelectorAll(sel)]
      .filter(el => el.getBoundingClientRect().width > 0)
      .reduce((m, el) => Math.max(m, el.getBoundingClientRect().right), 0);
    const names = [...document.querySelectorAll('#mod-slots .slot .mn')]
      .map(el => Math.round(el.getBoundingClientRect().width)).filter(x => x > 0);
    return {
      vw,
      // The page's own sideways scroll — the symptom a reader actually feels.
      scrollW: document.documentElement.scrollWidth,
      slotsRight: Math.round(widest('#mod-slots .slot')),
      barsRight: Math.round(widest('.preset-bar, .pbar, #build-bar, #scenario-bar')),
      cols: getComputedStyle(document.getElementById('mod-slots')).gridTemplateColumns,
      narrowestName: names.length ? Math.min(...names) : 0,
      // THE TOPBAR'S BUDGET. It is the one strip where a new icon is taken
      // from something else rather than added: at 360px it already wraps to
      // two rows and the weapon SEARCH — the site's own navigation — is down
      // to 29px. So the support link is desktop-only, and that is geometry,
      // which makes it this check's business rather than a style opinion.
      // RESOLVED display, not offsetParent. The rule under test IS a display
      // rule, and offsetParent is a layout-dependent proxy for it — it read
      // null once on a freshly navigated tablet and passed on the next run,
      // which is a check that teaches people to ignore it. Same lesson as the
      // meter's collapse: measure the property, not a symptom.
      // (No backticks in here: this comment lives inside a template literal.)
      supDisplay: document.querySelector('.sup-link')
        ? getComputedStyle(document.querySelector('.sup-link')).display : 'absent',
      searchW: Math.round((document.querySelector('.wsearch') || { getBoundingClientRect: () => ({ width: 0 }) })
        .getBoundingClientRect().width),
    };
  })()`);

  const tag = `[${label} ${w}px]`;
  check(`${tag} the mod grid stays on screen`, r.slotsRight <= r.vw + 0.5,
    `rightmost slot edge ${r.slotsRight} vs viewport ${r.vw}`);
  check(`${tag} nothing else sticks out either`, r.barsRight <= r.vw + 0.5,
    `rightmost bar edge ${r.barsRight} vs viewport ${r.vw}`);
  check(`${tag} the page does not scroll sideways`, r.scrollW <= r.vw + 0.5,
    `scrollWidth ${r.scrollW} vs clientWidth ${r.vw}`);
  // A column squeezed to nothing is not a fixed layout. 90px is about a dozen
  // characters — enough to tell two mods apart, which is the job.
  // A phone must not spend its bar on the ask; a desktop has room and shows it.
  check(`${tag} the support link is ${w <= 700 ? "kept off" : "on"} this bar`,
    (r.supDisplay !== 'none') === (w > 700), `display ${r.supDisplay}, vw ${r.vw}`);
  // ...and the weapon search must still be reachable, which is what the budget
  // is FOR. Below 400px the bar is genuinely tight, so this only asserts the
  // search did not vanish outright.
  check(`${tag} the weapon search keeps its place in the bar`, r.searchW >= 20,
    `${r.searchW}px`);
  check(`${tag} a mod name still has room to be a name`, r.narrowestName >= 90,
    `narrowest name column ${r.narrowestName}px (grid: ${r.cols})`);
}

await app.finish("the page fits every screen it was measured on");
