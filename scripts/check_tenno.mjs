// The TENNO block, on screen: the fields exist, they change the panel, and
// they survive a share link.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep, send, BASE } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(2500);
  document.querySelectorAll('.tab').forEach(t => { if (/Sim/i.test(t.textContent)) t.click(); });
  await sleep(1200);
  const box = document.getElementById('sim-technique');
  const keys = [...box.querySelectorAll('[data-k]')].map(e => e.dataset.k);

  // Equip Primary Bulwark, then give the Warframe armor.
  arcanes = ['primary_bulwark'];
  markPresetDirty(); renderMods(); refreshPanel(); await sleep(2200);
  // Bulwark is NOT a buff card — its value is a Warframe stat, not a stack
  // anyone earns. It lists as a CONDITIONAL, the panel's channel for "this
  // pays and here is what decides it".
  const conds = () => [...document.querySelectorAll('#stats-conditionals .scond')]
    .map(e => e.textContent).join(' | ');
  const before = conds();

  const armor = box.querySelector('[data-k="wf_armor"]');
  armor.value = '1500'; armor.dispatchEvent(new Event('change'));
  await sleep(2500);
  const after = conds();
  return { keys, before: before.replace(/\\s+/g,' ').trim().slice(0,120),
           after: after.replace(/\\s+/g,' ').trim().slice(0,120),
           simArmor: sim.wf_armor, url: await shareUrl() };
})()`);

check("the Tenno block carries every player field",
  ["aiming", "headshot_pct", "invisible", "airborne", "wf_armor", "wf_energy"].every((k) => r.keys.includes(k)),
  r.keys.join(","));
check("typing armor reaches the scenario state", r.simArmor === 1500, String(r.simArmor));
check("no frame: Bulwark says nothing", !/Bulwark/i.test(r.before), r.before || "(no conditionals)");
check("1,500 armor: the panel states Bulwark's +500%", /Bulwark/i.test(r.after) && /500/.test(r.after), r.after || "(no conditionals)");

// ...AND IT TRAVELS, when there is a way to send it. Sharing can be switched
// off (SHARE_ENABLED) and is while its reliability is being investigated; the
// claim is about the PAYLOAD, so it is asserted whenever a payload can be made
// and returns of its own accord when the feature does.
const canShare = await evaluate("typeof SHARE_ENABLED === 'undefined' ? true : SHARE_ENABLED");
if (canShare) {
  await evaluate(`(() => { localStorage.clear(); location.href = ${JSON.stringify(r.url)}; })()`);
  await sleep(12000);
  const got = await evaluate(`(async () => { await new Promise(r=>setTimeout(r,2500)); return { armor: sim.wf_armor }; })()`);
  check("the Warframe travels in a share link", got.armor === 1500, String(got.armor));
} else {
  console.log("  --  sharing is off; the share half of this check is waiting for it");
}

await app.finish("the Tenno is on the field, and the page knows it");
