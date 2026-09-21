// SPDX-License-Identifier: AGPL-3.0-or-later
// A FRAMED PAGE CARRIES NONE OF THE OUTER PAGE'S FURNITURE: the Wielder block
// frames the Warframe page, and the Operator page inside it, and each of those
// is the whole app. Only the outer page has a Nona launcher, a jump menu, a top
// bar and a footer, or the screen holds one of each per frame.
//
//   node scripts/check_embed_chrome.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear(); sessionStorage.clear();
  history.pushState({}, '', '/weapons/Praedos'); route(); await sleep(3500);
  document.querySelector('#wielder-block > .bh').click(); await sleep(9000);
  const shown = (doc, sel) => [...doc.querySelectorAll(sel)].filter((e) => {
    const s = doc.defaultView.getComputedStyle(e);
    return s.display !== 'none' && s.visibility !== 'hidden' && !e.hidden;
  }).length;
  const furniture = '.nona-fab, .topbar, .footer, .jump';
  const outer = shown(document, furniture);
  const wf = document.querySelector('#wielder-detail iframe');
  const inner = wf ? shown(wf.contentDocument, furniture) : -1;
  const op = wf && wf.contentDocument.querySelector('#wf-operator iframe');
  const nested = op ? shown(op.contentDocument, furniture) : -1;
  const fabs = document.querySelectorAll('.nona-fab').length
    + (wf ? wf.contentDocument.querySelectorAll('.nona-fab').length : 0)
    + (op ? op.contentDocument.querySelectorAll('.nona-fab').length : 0);
  return { outer, inner, nested, fabs, frames: [!!wf, !!op] };
})()`);
console.log(JSON.stringify(r));
check("the outer page has its furniture", r.outer >= 2, JSON.stringify(r));
check("the Warframe page framed in it shows none", r.frames[0] && r.inner === 0, String(r.inner));
check("...nor the Operator page framed in that", r.frames[1] && r.nested === 0, String(r.nested));
check("there is one Nona launcher on the whole screen", r.fabs === 1, String(r.fabs));
await app.finish("a framed page has no furniture");
