// SPDX-License-Identifier: AGPL-3.0-or-later
// A PICK IN THE OPTIMIZER'S LIMITS DOES NOT MOVE THE READER. Every click
// redraws the box, and each list in it scrolls on its own — a redraw that
// forgot where a list was threw the reader back to its first row.
//
//   node scripts/check_opt_limits_scroll.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear(); sessionStorage.clear();
  history.pushState({}, '', '/weapons/Braton/optimizer'); route(); await sleep(3000);
  copyActivePreset(optBarCfg()); await sleep(300);
  const fold = document.querySelector('[data-fold="opt-limits"]');
  if (fold && fold.classList.contains('shut')) fold.querySelector('.fold-h').click();
  await sleep(400);
  const list = () => document.getElementById('opt-limit-mods');
  const l = list();
  if (!l) return { error: 'no mod list' };
  l.scrollTop = Math.min(600, l.scrollHeight - l.clientHeight);
  await sleep(100);
  const before = l.scrollTop;
  // A ROW THE READER CAN SEE, which is the one they would click.
  const top = l.getBoundingClientRect().top;
  const row = [...l.querySelectorAll('.opt[data-axis]')].find((o) => o.getBoundingClientRect().top > top + 20);
  const pageBefore = window.scrollY;
  row.click(); await sleep(300);
  const afterPick = list().scrollTop;
  const excluded = list().querySelector('.opt[data-id="' + row.dataset.id + '"]').classList.contains('opt-out');
  list().querySelector('.opt[data-id="' + row.dataset.id + '"]').click(); await sleep(300);
  return { before, afterPick, afterUndo: list().scrollTop, excluded, pageBefore, pageAfter: window.scrollY,
           id: row.dataset.id };
})()`);
console.log(JSON.stringify(r));
check("the mod list was scrolled away from its top", r.before > 100, JSON.stringify(r));
check("the pick took", r.excluded, JSON.stringify(r));
check("a pick leaves the list where it was", Math.abs(r.afterPick - r.before) <= 1, JSON.stringify(r));
check("...and so does taking it back", Math.abs(r.afterUndo - r.before) <= 1, JSON.stringify(r));
check("the page itself does not move", Math.abs(r.pageAfter - r.pageBefore) <= 1, JSON.stringify(r));
await app.finish("a pick in the limits keeps the reader's place");
