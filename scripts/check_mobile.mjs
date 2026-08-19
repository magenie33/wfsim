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
  // A PHONE HAS A FINGER, and `mobile: true` alone does not give it one:
  // `navigator.maxTouchPoints` stays 0 and every touch-only behaviour goes
  // untested. This check is the one that is about phones (2026-08-18).
  await send("Emulation.setTouchEmulationEnabled",
    { enabled: mobile, maxTouchPoints: mobile ? 5 : 1 });
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
    const out = {};
  // ---- A FINGER SCROLLS, IT DOES NOT DRAG THE FIGHT --------------------
  //
  // A browser decides who owns a gesture at pointerdown and never gives it
  // back, so a body that drags on touch means the finger that started on it
  // can no longer scroll. A 19x19 formation covers the canvas in bodies, so on
  // a phone almost every scroll past the arena dragged an enemy instead — the
  // fight moved silently and the result it had just produced was for a fight
  // nobody was in any more (owner, 2026-08-18).
  //
  // Only on the touch screens, because a mouse has no scroll to lose.
  if (navigator.maxTouchPoints > 0) {
    history.pushState({}, '', '/weapons/Torid/simulator'); route(); await sleep(2600);
    const add = document.querySelector('#preset-bar-simulator-scenarios .pchip.add');
    if (add) { add.click(); await sleep(1400); }
    // THE SCENE IS A CANVAS as of 2026-08-18, so the finger lands on the canvas
    // rather than on a circle - and touch-action is read from the canvas,
    // which is the element the browser asks about when it decides who owns the
    // gesture. The RULE has not moved: pan-y by default so the page still
    // scrolls, none while the move mode is on.
    const cv = () => document.querySelector('#sim-target-arena .arc-cv');
    out.dragToggle = !!document.querySelector('[data-touchdrag]');
    out.taDefault = cv() ? cv().style.touchAction : null;
    const swipe = async () => {
      const c = cv();
      if (!c) return;
      c.setPointerCapture = () => {};
      const host = document.querySelector('#sim-target-arena');
      const [ax, ay] = host.__arena.px(sim.target_at);
      const b = c.getBoundingClientRect();
      const x = b.left + ax, y = b.top + ay;
      const send = (type, cx, cy) => c.dispatchEvent(new PointerEvent(type,
        { bubbles: true, cancelable: true, pointerType: 'touch', pointerId: 1,
          isPrimary: true, button: 0, clientX: cx, clientY: cy }));
      send('pointerdown', x, y);
      for (let k = 1; k <= 6; k++) { send('pointermove', x, y - 14 * k); await sleep(30); }
      send('pointerup', x, y - 84);
      await sleep(600);
    };
    let was = JSON.stringify(sim.target_at);
    await swipe();
    out.swipeMovedTheFight = JSON.stringify(sim.target_at) !== was;
    // …AND THE MODE IS THERE FOR WHEN A READER DOES MEAN TO MOVE ONE.
    const tog = document.querySelector('[data-touchdrag]');
    if (tog) {
      tog.click(); await sleep(500);
      out.taWhileOn = cv() ? cv().style.touchAction : null;
      was = JSON.stringify(sim.target_at);
      await swipe();
      out.swipeMovedWhileOn = JSON.stringify(sim.target_at) !== was;
    }
  }

  return {
      ...out,
      vw,
      // The page's own sideways scroll — the symptom a reader actually feels.
      scrollW: document.documentElement.scrollWidth,
      slotsRight: Math.round(widest('#mod-slots .slot')),
      barsRight: Math.round(widest('.preset-bar, .pbar, #build-bar, #scenario-bar')),
      cols: getComputedStyle(document.getElementById('mod-slots')).gridTemplateColumns,
      narrowestName: names.length ? Math.min(...names) : 0,
      // WHICH SLOT IS WHICH, read in DOM order. The grid is two tracks wide,
      // so nothing on screen said whether "slot 5" was the third row's left
      // cell or the first column's fifth — and slot order is not cosmetic,
      // elements combine in it (player report, 2026-08-10).
      slotNos: [...document.querySelectorAll('#mod-slots .slot .slotno')]
        .map((e) => e.textContent.trim()).join(','),
      // THE TOPBAR'S BUDGET, and it is geometry rather than a style opinion:
      // at 360px the bar used to wrap to two rows and squeeze the weapon
      // SEARCH — the site's own navigation — to 29px, which is what the phone
      // menu exists to fix. So this measures the two halves of that claim.
      //
      // ONE: nothing was DELETED to make the bar fit (owner, 2026-08-07).
      // Every destination and every control is REACHABLE at every width — on a
      // desktop straight off the bar, on a phone after one tap on the
      // hamburger. Reachable is measured, not asserted from a display rule: a
      // real box on screen, inside the viewport on both sides.
      // (No backticks in here: this comment lives inside a template literal.)
      ...(() => {
        // THE BAR CARRIES WHAT A READER ACTS ON; THE REST IS ONE CLICK AWAY
        // (owner, 2026-08-19). Until then all eight sat on the desktop bar with
        // equal weight, which is three unrelated categories in one costume —
        // so this check asserted that all eight were visible with nothing
        // opened, and that assertion WAS the old design. Two sets now.
        // BAR: what is on the desktop bar with nothing opened. The ask is here
        // because it is the only entry that is not chrome.
        const BAR = ['.topnav .tnav[data-nav="home"]', '.topnav .tnav[data-nav="benchmark"]',
                     '#lang-select', '#theme-toggle', '.sup-link', '#tbmore-toggle'];
        // ALL: nothing was DELETED to make the bar fit — every destination and
        // every control is still REACHABLE at every width, which is the claim
        // this check has always existed to make. Only the number of clicks
        // moved. '.qq-link' / '.dc-link' are BOTH here: one is on the bar and one
        // is in the overflow, and which is which follows the display language.
        // ...and the overflow BUTTON is not in it. It is a container, not a
        // destination or a control: below 700px it is not drawn at all, because
        // the phone menu already holds what it would have opened.
        const ALL = BAR.filter((x) => x !== '#tbmore-toggle')
          .concat(['.gh-link', '.qq-link', '.dc-link', '#compute-select']);
        const missing = (SEL) => SEL.filter((s) => {
          const el = document.querySelector(s);
          if (!el) return true;
          const b = el.getBoundingClientRect();
          return b.width < 1 || b.height < 1 || b.right > vw + 0.5 || b.left < -0.5;
        });
        const tog = document.querySelector('.menu-toggle');
        const more = document.querySelector('#tbmore-toggle');
        const closed = missing(BAR);
        // Open BOTH: below 700px the hamburger holds everything and the '⋯' is
        // not drawn; above it the '⋯' holds the overflow and the hamburger is
        // not drawn. Clicking one that is not there is a no-op either way.
        if (tog) tog.click();
        if (more) more.click();
        const opened = missing(ALL);
        if (more) more.click();
        if (tog) tog.click();
        return {
          missingClosed: closed, missingOpen: opened,
          toggleDisplay: tog ? getComputedStyle(tog).display : 'absent',
          moreDisplay: more ? getComputedStyle(more).display : 'absent',
        };
      })(),
      // TWO: what the room bought. The search is the thing the old bar was
      // taking from, so it is the thing that has to be measurably better.
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
  // NOTHING IS LOST AT ANY WIDTH — the two destinations and the six controls
  // are all on screen and inside it, once the menu is open.
  check(`${tag} every topbar destination and control is reachable`,
    r.missingOpen.length === 0, `unreachable: ${r.missingOpen.join(", ") || "none"}`);
  // The hamburger is the phone's, and only the phone's: above the breakpoint
  // the same eight sit on the bar itself with nothing to open.
  check(`${tag} the menu is ${w <= 700 ? "how a phone reaches them" : "not in the way"}`,
    (r.toggleDisplay !== "none") === (w <= 700), `toggle display ${r.toggleDisplay}`);
  // The `⋯` is the DESKTOP's overflow and only the desktop's: below the
  // breakpoint it is `display:contents` and its children join the phone menu
  // directly, so there would be nothing left for it to open.
  check(`${tag} the overflow is ${w <= 700 ? "not a second menu on a phone" : "the desktop's"}`,
    (r.moreDisplay !== "none") === (w > 700), `⋯ display ${r.moreDisplay}`);
  if (w > 700) {
    check(`${tag} ...and the bar itself shows what a reader acts on`,
      r.missingClosed.length === 0, `hidden until opened: ${r.missingClosed.join(", ")}`);
  }
  // What the room bought. The old bar wrapped to two rows and left the search
  // at 29px on a 360px screen; one button in place of eight is what fixes it,
  // so the search has to come back with real width rather than merely exist.
  check(`${tag} the weapon search gets the room back`, r.searchW >= 120,
    `${r.searchW}px`);
  // A column squeezed to nothing is not a fixed layout. 90px is about a dozen
  // characters — enough to tell two mods apart, which is the job.
  // A FINGER SCROLLS, IT DOES NOT DRAG THE FIGHT. Only where a finger is
  // possible, because a mouse has no scroll to lose.
  if (mobile) {
    check(`${tag} a swipe that starts on a body does NOT move the fight`,
      r.swipeMovedTheFight === false, `touch-action ${r.taDefault}`);
    check(`${tag} ...and the scene still lets the page scroll through it`,
      r.taDefault === "pan-y", String(r.taDefault));
    check(`${tag} ...with a mode for when a reader does mean to move one`,
      r.dragToggle === true && r.swipeMovedWhileOn === true,
      `toggle ${r.dragToggle}, moved ${r.swipeMovedWhileOn}, on ${r.taWhileOn}`);
  }
  check(`${tag} a mod name still has room to be a name`, r.narrowestName >= 90,
    `narrowest name column ${r.narrowestName}px (grid: ${r.cols})`);
  // …and the grid says which cell is which. It belongs in the geometry check
  // because it IS a geometry question: the number exists only because two
  // columns leave the reading order ambiguous, and it is asserted at every
  // width because the wrapping is what changes between them.
  check(`${tag} every slot is numbered, 1..8 in reading order`,
    r.slotNos === "1,2,3,4,5,6,7,8", r.slotNos);
}

await app.finish("the page fits every screen it was measured on");
