// THE TOPBAR SEARCH IS ONE BOX (10-weapon-search.js `initWeaponSearch`): no
// filter row above the list, and a weapon's kind is a word in the box like its
// name — in the page's language and in English. An empty box opens nothing,
// typing draws the list once it pauses, and on a phone the list has a way out.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, lang: "zh" });
const { evaluate, check } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((ok) => setTimeout(ok, ms));
  const input = document.getElementById('wsearch-input');
  const type = async (q) => {
    input.focus(); input.value = q; input.dispatchEvent(new Event('input', { bubbles: true })); await sleep(250);
    return [...document.querySelectorAll('#wsearch-list .opt')].map((o) => o.dataset.id);
  };
  const out = {};
  const panel = document.getElementById('wsearch-panel');
  out.emptyShut = (await type('')).length === 0 && panel.hidden;
  // A BURST OF KEYSTROKES IS ONE DRAW.
  let draws = 0;
  const mo = new MutationObserver(() => draws++);
  mo.observe(document.getElementById('wsearch-list'), { childList: true });
  input.focus();
  for (const q of ['b', 'br', 'bra', 'brat']) { input.value = q; input.dispatchEvent(new Event('input', { bubbles: true })); await sleep(15); }
  await sleep(400);
  mo.disconnect();
  out.draws = draws;
  const all = await type('a');
  out.rows = all.length;
  out.noTools = !document.querySelector('#wsearch-panel .pchip, #wsearch-panel .dd');
  const shotguns = META.weapons.filter((w) => w.mod_class === 'shotgun').map((w) => w.id);
  const word = tr('Shotgun');
  out.shotgunWord = word;
  const sg = await type(word);
  out.byKindZh = word !== 'Shotgun' && shotguns.length > 3 && shotguns.every((id) => sg.includes(id));
  const sgEn = await type('shotgun');
  out.byKindEn = shotguns.every((id) => sgEn.includes(id));
  out.byName = (await type('托里德')).includes('torid');
  const kits = await type('Gaze');
  out.kitgunEntries = ['gaze_primary', 'gaze_secondary'].every(id => kits.includes(id));
  out.kitgunLabels = [...document.querySelectorAll('#wsearch-list .opt')]
    .filter(el => kits.includes(el.dataset.id) && weaponInfo(el.dataset.id)?.assembly)
    .every(el => el.querySelector('.mn').textContent.includes(tr(ARC_POOL_LABEL[weaponInfo(el.dataset.id).slot])));
  return out;
})()`);
check("an empty box opens nothing", r.emptyShut === true, JSON.stringify(r));
check("...a burst of typing draws the list once, when it pauses", r.draws === 1, JSON.stringify(r));
check("the panel is one list, with no filter row or sort above it", r.noTools === true && r.rows > 50, JSON.stringify(r));
check(`a weapon's kind is found by its word in the page's language (${r.shotgunWord})`, r.byKindZh === true, JSON.stringify(r));
check("...and by its English one", r.byKindEn === true, JSON.stringify(r));
check("...and a name as before", r.byName === true);
check("Kitgun search offers both weapon slots with distinct names", r.kitgunEntries && r.kitgunLabels, JSON.stringify(r));

// ON A PHONE the results take the screen's width under the bar, what floats over
// the screen steps aside while they are open, and every row's kind is in the
// page's language.
await app.send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 2, mobile: true });
await app.send("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
await app.send("Page.navigate", { url: app.BASE });
await app.sleep(11000);
const m = await evaluate(`(async () => {
  const i = document.getElementById('wsearch-input');
  i.focus(); i.value = 'torid'; i.dispatchEvent(new Event('input', { bubbles: true }));
  await new Promise((ok) => setTimeout(ok, 500));
  const p = document.getElementById('wsearch-panel').getBoundingClientRect();
  const fab = document.querySelector('.nona-fab');
  i.value = '霰弹'; i.dispatchEvent(new Event('input', { bubbles: true }));
  await new Promise((ok) => setTimeout(ok, 400));
  const rows = [...document.querySelectorAll('#wsearch-list .opt')];
  const kinds = rows.map((o) => [o.querySelector('.me').textContent.trim(), tr((META.weapons.find((w) => w.id === o.dataset.id) || {}).subtype || '')]);
  const untranslated = [...new Set(META.weapons.map((w) => w.subtype).filter((k) => k && tr(k) === k))];
  const fabWhileOpen = !!fab && getComputedStyle(fab).display !== 'none';
  // THE WAY OUT: shown over the list, and it closes, empties and lets go of the box.
  const close = document.getElementById('wsearch-close');
  const closeShown = !!close && close.getBoundingClientRect().height > 0;
  if (close) close.click();
  await new Promise((ok) => setTimeout(ok, 100));
  const closed = document.getElementById('wsearch-panel').hidden && i.value === '' && document.activeElement !== i;
  return { closeShown, closed, width: p.width, vw: innerWidth, top: p.top, fabShown: fabWhileOpen,
    kinds: kinds.length > 3 && kinds.every(([shown, want]) => shown === want), untranslated };
})()`);
check("on a phone the results take the screen's width", m.width >= m.vw * 0.9, JSON.stringify(m));
check("...and nothing floats over them", m.fabShown === false, JSON.stringify(m));
check("...and a row's kind is shown in the page's language wherever it has one", m.kinds === true, JSON.stringify(m));
check("...and a close button over them shuts the list, empties the box and lets go of it", m.closeShown && m.closed, JSON.stringify(m));
console.log(`  (kinds with no Chinese yet: ${m.untranslated.join(", ")})`);

await app.finish("the topbar search is one box, and a weapon's kind is a word in it");
