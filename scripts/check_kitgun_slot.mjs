// SPDX-License-Identifier: AGPL-3.0-or-later
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, finish } = app;

const slotNavigation = await evaluate(`(async () => {
  await nav("/weapons/Catchmoon");
  await new Promise(r => setTimeout(r, 4000));
  const out = [];
  for (const slot of ['secondary', 'primary']) {
    ddReg.get('dd-kitslot').onPick(slot);
    await loadWeaponBoard($('weapon').value);
    await new Promise(r => setTimeout(r, 3000));
    slots[0].mod = slot === 'secondary' ? 'hornet_strike' : 'point_blank';
    slots[0].rank = slot === 'secondary' ? 10 : 5;
    markPresetDirty();
    setAssemblyPart('loader', 'killstream');
    const before = snapshotState();
    const weapon = $('weapon').value;
    for (const module of ['simulator', 'optimizer', '']) {
      const link = [...document.querySelectorAll('#module-tabs a')]
        .find(a => a.getAttribute('href') === weaponPath(weapon) + (module ? '/' + module : ''));
      if (!link) { out.push({ slot, module, weapon, missingLink: true }); continue; }
      link.click();
      for (let i = 0; i < 50; i++) {
        await new Promise(r => setTimeout(r, 100));
        if (document.querySelector('#module-tabs .sel')?.getAttribute('href') === link.getAttribute('href')) break;
      }
      const p = buildPayload();
      const result = module === 'simulator'
        ? await api('/api/simulate', { ...p, ...theFight(), duration: 5, runs: 1, seed: 11 }) : null;
      out.push({ slot, module, weapon: p.weapon,
        kept: JSON.stringify(snapshotState()) === JSON.stringify(before),
        parts: p.assembly, simulated: module !== 'simulator' || (result?.ok === true && result?.dps > 0),
        error: result?.error });
    }
  }
  return out;
})()`);
check('Catchmoon keeps its slot, build and parts through simulator, optimizer and builder',
  slotNavigation.length === 6 && slotNavigation.every(x => !x.missingLink
    && x.weapon === 'catchmoon_' + x.slot && x.kept && x.parts.loader === 'killstream' && x.simulated),
  JSON.stringify(slotNavigation));

await finish('Kitgun module navigation keeps the selected slot and build');
