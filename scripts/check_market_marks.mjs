// THE ONE NUMBER THIS APP DOES NOT COMPUTE — what a card costs to buy — is a
// MARK, and a mark is only useful where the reader is looking at the card.
//
// So it is asserted as a PROPERTY of every surface that draws one, not of the
// list it was written into first: the mod picker, an equipped mod, the arcane
// picker, an equipped arcane, the weapon's own title. A mark that reaches the
// picker and not the slot is the shape this was in — the reader decides they
// want the card while it is already seated, which is the one place it was
// missing.
//
// …AND THE READER'S SWITCH GOVERNS ALL OF THEM. `marketPrefs.on` is off in the
// topbar's overflow, and the page must then ship NO anchor at all rather than
// hide a row of dead ones.
//
//   node scripts/check_market_marks.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
await app.load("/weapons/Torid", 12000);

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  await sleep(2500);
  const out = {};
  const mark = (el) => {
    const a = el && el.querySelector('a.wm');
    return a ? a.getAttribute('href') : '';
  };
  const slots = () => [...document.querySelectorAll('#mod-slots .slot')];
  const empty = () => slots().find((s) => s.classList.contains('empty'));

  // THE PICKER, and the row that is about to be seated. A card that does not
  // TRADE carries no mark and is not the case under test, so the first one
  // that does is what gets seated.
  const marked = (sel) => [...document.querySelectorAll(sel)].find((o) => o.querySelector('a.wm'));
  empty().click(); await sleep(800);
  const row = marked('#mod-popover .opt');
  out.pickerRow = (row.querySelector('.mn') || {}).textContent || '';
  out.pickerMark = mark(row);
  row.click(); await sleep(900);

  // …AND THE SAME CARD ONCE IT IS SEATED. Same mod, so the two marks are the
  // same link or one of them is wrong.
  const filled = slots().find((s) => s.classList.contains('filled'));
  out.slotName = (filled.querySelector('.mn') || {}).textContent || '';
  out.slotMark = mark(filled);

  // THE ARCANE, which has had the mark on both surfaces all along and is what
  // the mod's slot is being held against.
  const arc = document.querySelector('#arcane-slots .slot');
  if (arc) {
    if (arc.classList.contains('empty')) { arc.click(); await sleep(800); }
    const arow = marked('#arcane-popover .opt');
    out.arcPickerMark = arow ? mark(arow) : '';
    if (arow) { arow.click(); await sleep(900); }
    const af = document.querySelector('#arcane-slots .slot.filled');
    out.arcSlotMark = af ? mark(af) : '';
  }

  // THE WEAPON'S OWN, in the title — and this weapon does not TRADE, so the
  // honest answer here is no mark at all. The one that does is asked below.
  out.weaponSlug = (weaponInfo(document.getElementById('weapon').value) || {}).market_slug || '';
  out.weaponMark = mark(document.getElementById('w-name'));

  // THE SWITCH, thrown the way a reader throws it. Counted ON SCREEN: a picker
  // that is shut still holds the rows it was last drawn with, and the reader is
  // not being shown those.
  const onScreen = () => [...document.querySelectorAll('a.wm')].filter((a) => a.offsetParent !== null).length;
  const box = document.getElementById('market-on');
  box.click(); await sleep(900);
  out.offAnywhere = onScreen();
  out.offStored = JSON.parse(localStorage.getItem('wfsim-market') || '{}').on;
  box.click(); await sleep(900);
  out.backOn = onScreen();
  return out;
})()`);

const market = (u) => typeof u === "string" && u.startsWith("https://warframe.market/");

check("the picker's mod row carries the market mark", market(r.pickerMark),
  `${JSON.stringify(r.pickerRow.trim().slice(0, 40))} → ${JSON.stringify(r.pickerMark)}`);
check("...and so does the same mod once it is EQUIPPED", market(r.slotMark),
  `${JSON.stringify(r.slotName.trim().slice(0, 40))} → ${JSON.stringify(r.slotMark)}`);
check("...to the same item page", r.slotMark === r.pickerMark,
  `${r.pickerMark} vs ${r.slotMark}`);
check("an arcane carries it in the picker and in its slot alike",
  market(r.arcPickerMark) && market(r.arcSlotMark),
  `picker ${JSON.stringify(r.arcPickerMark)}, slot ${JSON.stringify(r.arcSlotMark)}`);
check("a weapon that does not trade carries no mark in its title",
  r.weaponSlug === "" && r.weaponMark === "", `slug ${JSON.stringify(r.weaponSlug)}, mark ${JSON.stringify(r.weaponMark)}`);
check("the reader's switch takes EVERY mark off the page, not just the open list",
  r.offAnywhere === 0 && r.offStored === false, `${r.offAnywhere} left, stored ${r.offStored}`);
check("...and they all come back", r.backOn > 0, `${r.backOn} marks`);

// …AND ONE THAT DOES TRADE, so the rule above is not asserted on absence alone.
await app.load("/weapons/Ballistica_Prime", 12000);
const t = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  await sleep(2500);
  const a = document.querySelector('#w-name a.wm');
  return { slug: (weaponInfo(document.getElementById('weapon').value) || {}).market_slug || '',
           mark: a ? a.getAttribute('href') : '' };
})()`);
check("a weapon that does carries one in its title", !!t.slug && market(t.mark),
  `slug ${JSON.stringify(t.slug)}, mark ${JSON.stringify(t.mark)}`);

process.exit(0);
