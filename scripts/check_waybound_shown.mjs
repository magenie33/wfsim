// SPDX-License-Identifier: AGPL-3.0-or-later
// THE TEN WAYBOUNDS ARE SHOWN AND NOT OFFERED. Two a school, unbound from it
// and permanent, so an account that has played far enough to pick a school has
// them all at max rank — the panel draws them with no control beside them, and
// they stay drawn whichever school is active, including none.
//
//   node scripts/check_waybound_shown.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear(); sessionStorage.clear();
  history.pushState({}, '', '/operator'); route(); await sleep(4500);
  const rows = () => [...document.querySelectorAll('#op-nodes .op-node.wb')].map((el) => ({
    text: el.textContent.replace(/\\s+/g, ' ').trim(),
    controls: el.querySelectorAll('input, button, select').length,
  }));
  // WITH NO SCHOOL PICKED they are still every one of them there.
  const none = rows();
  // …AND PICKING ONE CHANGES NOTHING ABOUT THEM: they are unbound from it.
  const btn = document.querySelector('#op-schools [data-school]');
  btn.click(); await sleep(1200);
  const picked = rows();
  const served = (WFCAT.focus || []).map((s) => [s.id, (s.waybound || []).length]);
  return { none: none.length, picked: picked.map((x) => x.text),
           controls: picked.reduce((n, x) => n + x.controls, 0), served,
           same: JSON.stringify(none.map((x) => x.text)) === JSON.stringify(picked.map((x) => x.text)) };
})()`);
console.log(JSON.stringify(r, null, 1));
check("every school serves exactly two Waybounds",
  r.served.length === 5 && r.served.every(([, n]) => n === 2), JSON.stringify(r.served));
check("the panel draws all ten, with no school picked",
  r.none === 10, String(r.none));
check("...and picking a school changes none of them — they are unbound from it",
  r.same && r.picked.length === 10, JSON.stringify(r.picked.length));
check("...and not one of them offers a control: unlocking cannot be undone",
  r.controls === 0, String(r.controls));
await app.finish("the Waybounds are shown and not offered");
