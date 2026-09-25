// EVERY BLOCK AND EVERY SECTION ON A WEAPON PAGE FOLDS, AND THE JUMP MENU IS
// READ OFF THEM.
//
// The page is one long scroll and the reader wants three lines of it, so each
// box shuts from the header it already has and the menu indexes what is there.
// Both halves derive from the DOM — `wireStaticFolds` stamps a block's own id
// as its fold id, `pageFolds` walks the module's blocks and the sections inside
// them — which is what makes a section added tomorrow foldable, listed and
// reachable with no edit here and none in `app.js`.
//
// So this asserts the PROPERTY, never a list of sections:
//
//   * every block on the page carries a fold id and a caret, and shutting one
//     hides its body;
//   * ...and so does every section inside one;
//   * a control in a heading (the Buffs section's "all", the mods filter) is
//     that control's click and not a fold toggle — the one way this feature
//     can make the page worse;
//   * what you folded survives a reload;
//   * the menu lists exactly the folds the page has, opens what it jumps to
//     (target and every fold above it), and collapses/expands all of them;
//   * it drags, a drag is not a click, and it cannot be dragged out of reach.
//
// It runs on the SIMULATOR, whose seven sections are the deepest nesting the
// page has, and then on the OPTIMIZER, where the sections sit two levels down
// inside the two halves.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 13000 });
const { evaluate, check, finish, send, sleep } = app;

// A REAL POINTER, not a synthetic event: `PointerEvent` from the page cannot
// carry an active pointer id, and the drag is wired to the window precisely so
// that it does not need one.
async function drag(from, to) {
  await send("Input.dispatchMouseEvent",
    { type: "mousePressed", x: from[0], y: from[1], button: "left", clickCount: 1 });
  await send("Input.dispatchMouseEvent",
    { type: "mouseMoved", x: to[0], y: to[1], button: "left" });
  await send("Input.dispatchMouseEvent",
    { type: "mouseReleased", x: to[0], y: to[1], button: "left", clickCount: 1 });
  await sleep(200);
}

// ---- the simulator: every box folds ----------------------------------------

await app.load("/weapons/Torid/simulator");

const r = await evaluate(`(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const out = {};
  localStorage.removeItem('wfsim-folds');
  route(); await sleep(2500);

  const blocks = () => [...document.querySelectorAll('.config-page section.block')]
    .filter(b => b.offsetParent !== null);
  window.sectLeaks = (list) => list.flatMap(s => [...s.children]
    .filter(c => !c.classList.contains('fold-h') && getComputedStyle(c).display !== 'none')
    .map(c => s.dataset.fold + ' > ' + (c.id || c.className || c.tagName)));
  // THE VISIBLE MODULE'S SECTIONS, not the page's: a block belonging to another
  // tab is hidden by a CSS rule rather than by the attribute, so its sections
  // are still 'not inside anything hidden' and would be counted here while
  // nothing on screen can reach them.
  const sects = () => blocks().flatMap(b =>
    [...b.querySelectorAll('.fold.sect')].filter(s => !s.closest('[hidden]')));
  out.blocks = blocks().length;
  out.sects = sects().length;
  out.allSects = document.querySelectorAll('.fold.sect').length;
  // A FOLD ID AND A CARET ON EVERY ONE. Both are applied by the wiring pass
  // rather than written into the markup, so a box that the pass never reached
  // looks exactly like one that folds and does nothing when clicked.
  out.unwired = [
    ...blocks().filter(b => !b.dataset.fold || !b.querySelector(':scope > .bh > .fold-c'))
      .map(b => 'block ' + b.id),
    ...[...document.querySelectorAll('.fold.sect')]
      .filter(s => !s.dataset.fold || !s.querySelector(':scope > .fold-h > .fold-c'))
      .map(s => 'sect ' + (s.dataset.fold || s.id || '(anonymous)')),
  ];

  // SHUTTING ONE HIDES ITS BODY — asked of the computed style, because a rule
  // that stopped applying would leave every class exactly as it is.
  const sect = document.querySelector('[data-fold="sim-fight"]');
  sect.querySelector(':scope > .fold-h').click(); await sleep(120);
  out.sectShut = sect.classList.contains('shut');
  out.sectBodyGone = document.getElementById('sim-target').offsetParent === null;
  sect.querySelector(':scope > .fold-h').click(); await sleep(120);
  out.sectOpenAgain = !sect.classList.contains('shut')
    && document.getElementById('sim-target').offsetParent !== null;

  const blk = document.getElementById('sim-block');
  blk.querySelector(':scope > .bh').click(); await sleep(120);
  out.blockShut = blk.classList.contains('shut');
  out.blockBodyGone = getComputedStyle(blk.querySelector(':scope > .bb')).display === 'none';
  blk.querySelector(':scope > .bh').click(); await sleep(120);
  out.blockOpenAgain = !blk.classList.contains('shut');

  // A CONTROL IN A HEADING IS THAT CONTROL'S.
  const buffs = document.querySelector('[data-fold="sim-buffs"]');
  document.getElementById('sim-buffs-all').click(); await sleep(150);
  out.headingButtonKept = !buffs.classList.contains('shut');

  // WHAT THE MENU LISTS IS WHAT THE PAGE HAS. Both sides are read here; the
  // menu's own side comes from its rendered rows, so a menu built off a hand
  // written list would show up as a mismatch.
  jump.open = true; renderJump(); await sleep(150);
  out.rows = [...document.querySelectorAll('.jump-row')].map(b => b.dataset.jump);
  out.want = [...blocks().flatMap(b => [b.dataset.fold,
    ...[...b.querySelectorAll('.fold.sect')].filter(s => !s.closest('[hidden]')).map(s => s.dataset.fold)])];
  // ...and each row is NAMED, never left as its id: the id is ours and the
  // heading is the reader's, already translated.
  out.unnamed = [...document.querySelectorAll('.jump-row')]
    .filter(b => { const n = b.querySelector('.jr-n').textContent.trim();
                   return !n || n === b.dataset.jump; })
    .map(b => b.dataset.jump);

  // COLLAPSE ALL, then jump into the deepest section: it opens, and so does
  // the block above it — a jump that landed inside something shut would not
  // move the page at all.
  //
  // A CONTROL THE MENU DID NOT DRAW IS REPORTED, NOT THROWN ON. The most
  // likely regression here is a menu that lists fewer rows than the page has,
  // and a null dereference would abort every assertion after it — naming a
  // backtick in this file, which is the one thing it would not be.
  const hit = (sel) => { const el = document.querySelector(sel);
    if (el) el.click(); else out.missing = (out.missing || []).concat(sel);
    return !!el; };
  hit('[data-jump-all="shut"]'); await sleep(200);
  out.allShut = blocks().every(b => b.classList.contains('shut'))
    && sects().every(s => s.classList.contains('shut'));
  // EVERY SHUT SECTION'S BODY IS GONE, asked of each child's computed style: a
  // child's own id rule (the Buffs grid) outranks the class rule that hides it.
  out.leaks = sectLeaks(sects());
  out.shortPage = document.documentElement.scrollHeight;
  hit('[data-jump="sim-measure"]'); await sleep(400);
  out.jumpedOpen = document.querySelector('[data-fold="sim-measure"]').classList.contains('shut')
    && !document.getElementById('sim-block').classList.contains('shut');
  out.jumpedInView = document.querySelector('[data-fold="sim-measure"]').offsetParent !== null;
  // THE ROW'S CARET IS THE FOLD CONTROL: it opens that section and moves nothing.
  // Measured once the jump's smooth scroll has settled.
  await sleep(1200);
  const y0 = window.scrollY;
  hit('[data-jump-fold="sim-measure"]'); await sleep(200);
  out.caret = { open: !document.querySelector('[data-fold="sim-measure"]').classList.contains('shut'),
    moved: Math.round(window.scrollY - y0) };
  out.caretOpened = out.caret.open && Math.abs(out.caret.moved) < 2;
  hit('[data-jump-all="open"]'); await sleep(200);
  out.allOpen = blocks().every(b => !b.classList.contains('shut'))
    && sects().every(s => !s.classList.contains('shut'));
  out.tallPage = document.documentElement.scrollHeight;

  // ONE SECTION SHUT, TO SEE IT COME BACK SHUT after the reload below.
  hit('[data-fold="sim-limits"] > .fold-h'); await sleep(120);
  out.stored = JSON.parse(localStorage.getItem('wfsim-folds') || '{}')['sim-limits'] === true;
  return out;
})()`);

check(`the page is blocks and sections, and every one is wired (${r.blocks} + ${r.sects})`,
  r.blocks >= 2 && r.sects >= 5 && r.unwired.length === 0, JSON.stringify(r.unwired));
check("...and the unwired sweep saw the other modules' sections too",
  r.allSects > r.sects, `${r.allSects} on the page, ${r.sects} on this tab`);
check("a section shuts and its body goes", r.sectShut && r.sectBodyGone);
check("...and opens again", r.sectOpenAgain);
check("a block shuts from its own header and its body goes", r.blockShut && r.blockBodyGone);
check("...and opens again", r.blockOpenAgain);
check("a button in a heading is not a fold toggle", r.headingButtonKept);
check("the menu lists exactly the folds the page has",
  JSON.stringify(r.rows) === JSON.stringify(r.want),
  `menu: ${r.rows.join(",")}\n    page: ${r.want.join(",")}`);
check("...each under the heading's own name", r.unnamed.length === 0, JSON.stringify(r.unnamed));
check("collapse all reaches every block and every section", r.allShut);
check("...and nothing inside a shut section is still drawn", r.leaks.length === 0, JSON.stringify(r.leaks));
check("...and it is the point: the page gets shorter",
  r.shortPage * 2 < r.tallPage, `${r.shortPage}px shut vs ${r.tallPage}px open`);
check("a jump opens every fold above its target and leaves the target's own", r.jumpedOpen && r.jumpedInView);
check("...and the row's caret is what opens it, in place", r.caretOpened, JSON.stringify(r.caret));
check("expand all reaches every one back", r.allOpen);
check("what you folded is stored", r.stored === true);
check("every control the menu was asked for was drawn", !r.missing, JSON.stringify(r.missing));

// ---- ...and it is still folded after a reload ------------------------------

await app.load("/weapons/Torid/simulator");
const back = await evaluate(`(() => {
  const s = document.querySelector('[data-fold="sim-limits"]');
  return { shut: s.classList.contains('shut'),
    bodyGone: document.getElementById('sim-limits').offsetParent === null,
    others: [...document.querySelectorAll('.config-page section.block')]
      .filter(b => b.offsetParent !== null)
      .flatMap(b => [...b.querySelectorAll('.fold.sect')])
      .filter(x => x !== s && !x.closest('[hidden]') && x.classList.contains('shut'))
      .map(x => x.dataset.fold) };
})()`);
check("a section comes back shut, and only that one",
  back.shut && back.bodyGone && back.others.length === 0, JSON.stringify(back));

// ---- the menu is draggable and cannot be dragged out of reach --------------

const centre = (id) => evaluate(`(() => { const g = document.getElementById('${id}').getBoundingClientRect();
  return { x: Math.round(g.left + g.width / 2), y: Math.round(g.top + g.height / 2),
    left: Math.round(g.left), right: Math.round(g.right), top: Math.round(g.top) }; })()`);
const g0 = await evaluate(`(() => {
  localStorage.removeItem('wfsim-folds'); localStorage.removeItem('wfsim-jump');
  jump = { ax: null, y: null, edge: 'r', open: false }; renderJump();
  const g = document.getElementById('jump-grip').getBoundingClientRect();
  return { right: Math.round(g.right), top: Math.round(g.top), w: innerWidth,
    bar: Math.round(document.querySelector('.topbar').getBoundingClientRect().bottom) };
})()`);
check("by default it sits on the right edge, under the topbar",
  g0.right > g0.w - 40 && g0.top > g0.bar, JSON.stringify(g0));

// SHUT, THE GRIP DRAGS IT ANYWHERE, and a drag is not a click.
const s0 = await centre("jump-grip");
await drag([s0.x, s0.y], [s0.x - 260, s0.y + 150]);
const s1 = await centre("jump-grip");
check("the grip drags the menu", s1.x < s0.x - 200 && s1.y > s0.y + 100, `${JSON.stringify(s0)} -> ${JSON.stringify(s1)}`);
check("...a drag is not a click, so it stays shut", (await evaluate("jump.open")) === false);

// A CLICK IS STILL A CLICK, and a menu parked on the right half opens leftwards
// from where the grip was rather than being shoved over by the clamp.
await drag([s1.x, s1.y], [s1.x + 1, s1.y]);
const p1 = await evaluate(`(() => { const r = document.querySelector('.jump-panel').getBoundingClientRect();
  return { open: jump.open, edge: jump.edge, left: Math.round(r.left), right: Math.round(r.right), w: Math.round(r.width) }; })()`);
check("a click on the grip opens the menu", p1.open === true && p1.w > 0, JSON.stringify(p1));
check("...growing away from the side it is parked on",
  p1.edge === "r" ? Math.abs(p1.right - s1.right) <= 2 : Math.abs(p1.left - s1.left) <= 2,
  `${JSON.stringify(p1)} grip ${JSON.stringify(s1)}`);

// OPEN, THE HEADER IS THE HANDLE — and it cannot be dragged over the topbar or
// off the window, where a menu has no rows a reader can reach.
const h0 = await centre("jump-head");
await drag([h0.x - 40, h0.y], [-400, -400]);
const off = await evaluate(`(() => { const r = document.getElementById('jump').getBoundingClientRect();
  return { left: Math.round(r.left), top: Math.round(r.top), open: jump.open,
    bar: Math.round(document.querySelector('.topbar').getBoundingClientRect().bottom) }; })()`);
check("the header drags it, and it stays open", off.open === true && off.left < h0.left, JSON.stringify(off));
check("dragged off the corner, it stops at the edge and under the topbar",
  off.left >= 0 && off.top >= off.bar, JSON.stringify(off));

// IT KNOWS WHERE THE READER IS: the section on screen is the marked row and the
// name on the shut grip.
const here = await evaluate(`(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  jumpTo('sim-buffs'); await sleep(1500);
  const row = document.querySelector('.jump-row.here');
  const at = { row: row && row.dataset.jump };
  document.getElementById('jump-x').click(); await sleep(150);
  at.open = jump.open;
  at.now = document.getElementById('jump-now').textContent;
  at.want = foldTitle(document.querySelector('[data-fold="sim-buffs"]'));
  window.scrollTo(0, 0); await sleep(400);
  at.topNow = document.getElementById('jump-now').textContent;
  return at;
})()`);
check("the section on screen is the marked row", here.row === "sim-buffs", JSON.stringify(here));
check("...the close button shuts it", here.open === false);
check("...and the shut grip names it", here.now === here.want && here.topNow !== here.want, JSON.stringify(here));

// A CLICK ON THE HEADER IS A CLICK, the same as on the grip: it shuts the menu.
await evaluate("jump.open = true; renderJump(); 1");
await sleep(200);
const h1 = await centre("jump-head");
await drag([h1.left + 30, h1.y], [h1.left + 31, h1.y]);
check("a click on the header shuts the menu", (await evaluate("jump.open")) === false);

// ---- the optimizer: its two boxes ----------------------------------------

const opt = await evaluate(`(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  localStorage.removeItem('wfsim-folds');
  history.pushState({}, '', '/weapons/Torid/optimizer'); route(); await sleep(3500);
  const out = {};
  const hit = (sel) => { const el = document.querySelector(sel);
    if (el) el.click(); else out.missing = (out.missing || []).concat(sel);
    return !!el; };
  jump.open = true; renderJump(); await sleep(150);
  out.rows = [...document.querySelectorAll('.jump-row')].map(b => b.dataset.jump);
  // SHUTTING A BOX TAKES ITS CONTENTS WITH IT, and a jump brings them back.
  hit('[data-fold="opt-plan"] > .fold-h'); await sleep(200);
  out.boxGone = document.getElementById('opt-starts').offsetParent === null;
  hit('[data-jump="opt-plan"]'); await sleep(400);
  out.boxBack = document.getElementById('opt-starts').offsetParent !== null;
  // A CONTROL IN THE BOX'S BODY IS NOT A FOLD TOGGLE.
  document.getElementById('opt-cand-runs').click(); await sleep(120);
  out.controlKept = !document.querySelector('[data-fold="opt-plan"]').classList.contains('shut');
  // ...AND HERE TOO, where the Buffs grid has a twin. Defined again: the reload
  // above took the simulator half's page with it.
  const sectLeaks = (list) => list.flatMap(s => [...s.children]
    .filter(c => !c.classList.contains('fold-h') && getComputedStyle(c).display !== 'none')
    .map(c => s.dataset.fold + ' > ' + (c.id || c.className || c.tagName)));
  [...document.querySelectorAll('.fold.sect')].filter(s => !s.closest('[hidden]'))
    .forEach(s => s.classList.add('shut'));
  out.leaks = sectLeaks([...document.querySelectorAll('.config-page .fold.sect')]
    .filter(s => !s.closest('[hidden]') && s.closest('section.block')?.offsetParent));
  return out;
})()`);

check(`the optimizer's boxes are in the menu (${opt.rows.length} rows)`,
  opt.rows.includes("opt-plan") && opt.rows.includes("opt-fight"), opt.rows.join(","));
check("shutting a box takes its contents with it", opt.boxGone);
check("...and a jump to it brings them back", opt.boxBack);
check("a control in the box is not a fold toggle", opt.controlKept);
check("nothing inside a shut optimizer section is still drawn", opt.leaks.length === 0, JSON.stringify(opt.leaks));
check("...and every control the menu was asked for here was drawn too",
  !opt.missing, JSON.stringify(opt.missing));

await finish("every block and section folds, and the jump menu is read off them");
