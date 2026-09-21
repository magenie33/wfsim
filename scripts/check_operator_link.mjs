// SPDX-License-Identifier: AGPL-3.0-or-later
// THE OPERATOR IS LINKED THE WAY A WEAPON LINKS ITS WIELDER: a type control and
// a preset control though there is one Operator, the preset marked with who
// links it, and a deleted one landing on the read-only default — the blank, no
// Operator — instead of on some other preset.
//
//   node scripts/check_operator_link.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear(); sessionStorage.clear();
  history.pushState({}, '', '/warframes/Prototype'); route(); await sleep(4000);
  await loadWarframeCatalog();
  const school = WFCAT.focus[0].id;
  storePresetList(OPS, [
    { id: 'o1', name: 'preset 1', savedAt: 1, state: opBlank() },
    { id: 'o2', name: 'preset 2', savedAt: 1, state: { ...opBlank(), school } },
  ]);
  storePresetList(WF_BUILDS, [{ id: 'w1', name: 'preset 1', savedAt: 1, state: { ...wfBlank('valkyr'), operator: 'o2' } }], 'valkyr');
  localStorage.setItem(presetActiveKey(WF_BUILDS, 'valkyr'), 'preset 1');
  history.pushState({}, '', '/'); route(); await sleep(1500);
  history.pushState({}, '', '/warframes/Valkyr'); route(); await sleep(4000);
  const shown = () => ({ type: !!document.getElementById('dd-wf-operator-type'),
    typeText: (document.getElementById('dd-wf-operator-type') || {}).textContent,
    preset: (document.getElementById('dd-wf-operator') || {}).value,
    paid: !!operatorPickFor('o2') });
  const start = shown();

  history.pushState({}, '', '/operator'); route(); await sleep(3500);
  const by = (document.querySelector('#preset-bar-operators .pchip[data-name="preset 2"] .pby') || {}).title || '';
  document.querySelector('#preset-bar-operators .pchip[data-name="preset 2"]').click(); await sleep(600);
  const del = () => document.querySelector('#preset-bar-operators .pchip.sel .pop.del');
  del().click(); await sleep(400);
  const armed = del().textContent;
  del().click(); await sleep(1200);

  history.pushState({}, '', '/warframes/Valkyr'); route(); await sleep(4000);
  const after = shown();
  const menu = (() => { document.getElementById('dd-wf-operator').click(); return null; })();
  await sleep(400);
  const options = [...document.querySelectorAll('#dd-menu .opt')].map(o => o.dataset.v);
  return { start, by, armed, after: { ...after, paid: !!operatorPickFor('o2') }, options };
})()`);
console.log(JSON.stringify(r, null, 1));
check("the Operator has a type control and a preset control, like a wielder",
  r.start.type && r.start.preset === "o2", JSON.stringify(r.start));
check("...and its preset pays", r.start.paid === true);
check("the Operator's own page marks who links a preset", /Valkyr/.test(r.by), r.by);
check("deleting a linked one says so first", /1/.test(r.armed), r.armed);
check("its link lands on the default, and the default pays nothing",
  r.after.preset === "default" && r.after.paid === false, JSON.stringify(r.after));
check("...and the default is offered now that the link means it", r.options.includes("default"), JSON.stringify(r.options));
await app.finish("the operator is linked like a wielder");
