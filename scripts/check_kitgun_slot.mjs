// SPDX-License-Identifier: AGPL-3.0-or-later
import { openApp } from "./cdp.mjs";
import { isDeepStrictEqual } from "node:util";

const app = await openApp({ boot: 12000 });
const { evaluate, check, finish } = app;

const imageProbe = await app.send("Page.addScriptToEvaluateOnNewDocument", { source: `
  window.weaponArrivalImages = [];
  const imageSrc = Object.getOwnPropertyDescriptor(HTMLImageElement.prototype, 'src');
  Object.defineProperty(HTMLImageElement.prototype, 'src', { ...imageSrc, set(value) {
    if (this.id === 'w-img') window.weaponArrivalImages.push(value);
    imageSrc.set.call(this, value);
  } });
` });
for (const [path, weapon] of [["Furis", "furis"], ["catchmoon_secondary", "catchmoon_secondary"]]) {
  await app.load("/weapons/" + path + "/riven-analyst", 8000);
  const first = await evaluate(`(() => {
    const expected = IMG(weaponInfo(${JSON.stringify(weapon)}).image);
    return { weapon: $('weapon').value, images: window.weaponArrivalImages,
      correct: window.weaponArrivalImages.length > 0 && window.weaponArrivalImages.every(src => src === expected) };
  })()`);
  check(path + " cold-boots a hyphenated tab without painting another weapon first",
    first.weapon === weapon && first.correct, JSON.stringify(first));
}
await app.send("Page.removeScriptToEvaluateOnNewDocument", { identifier: imageProbe.result.identifier });
if (process.argv.includes("--arrival-only")) await app.finish("weapon arrivals paint the requested weapon first");

for (const [from, to] of [['catchmoon_primary', 'catchmoon_secondary'], ['arbucep', 'nikana_prime']]) {
  const arrival = await evaluate(`(async () => {
    nav(weaponPath(${JSON.stringify(from)}));
    await route();
    const load = loadWeaponBoard;
    let release;
    const waiting = new Promise(r => { release = r; });
    loadWeaponBoard = async (id, ...args) => {
      if (id === ${JSON.stringify(to)}) await waiting;
      return load(id, ...args);
    };
    try {
      history.pushState(null, '', weaponPath(${JSON.stringify(to)}) + '/optimizer');
      const pending = route();
      boardRowsLanded(${JSON.stringify(from)});
      const kept = routeWeaponId() === ${JSON.stringify(to)};
      release();
      await pending;
      return { kept, weapon: $('weapon').value, route: routeWeaponId() };
    } finally { release(); loadWeaponBoard = load; }
  })()`);
  check(from + ' board arriving during navigation preserves the destination',
    arrival.kept && arrival.weapon === to && arrival.route === to, JSON.stringify(arrival));
}

const chambers = await evaluate(`META.weapons.filter(w => w.assembly && w.slot === 'primary')
  .map(w => ({ id: w.id, chamber: w.assembly.chamber }))`);
check('the catalog has Kitgun chambers to exercise', chambers.length > 0);

for (const chamber of chambers) {
  const slotNavigation = await evaluate(`(async () => {
    nav(weaponPath(${JSON.stringify(chamber.id)}));
    await new Promise(r => setTimeout(r, 4000));
    const out = [];
    for (const slot of ['secondary', 'primary']) {
      jump.open = true;
      renderJump();
      nav(weaponPath(${JSON.stringify(chamber.chamber)} + '_' + slot));
      for (let i = 0; i < 100 && weaponInfo($('weapon').value).slot !== slot; i++) {
        await new Promise(r => setTimeout(r, 100));
      }
      await loadWeaponBoard($('weapon').value);
      await new Promise(r => setTimeout(r, 3000));
      const mod = slot === 'secondary' ? 'hornet_strike'
        : weaponInfo($('weapon').value).mod_pools.includes('shotgun') ? 'point_blank' : 'serration';
      slots[0].mod = mod;
      slots[0].rank = mod === 'point_blank' ? 5 : 10;
      markPresetDirty();
      setAssemblyPart('loader', 'killstream');
      const before = snapshotState();
      const weapon = $('weapon').value;
      flushPresetSaves();
      const addressMatches = routeWeaponId() === weapon;
      await route();
      const rerouted = $('weapon').value === weapon && sameState(snapshotState(), before);
      if (!rerouted) whileApplying(() => restoreState(before, weapon));
      const jumpLink = document.querySelector('#jump-body a[href$="/simulator"]');
      jumpLink.click();
      await new Promise(r => setTimeout(r, 1500));
      out.push({ kind: 'entry', slot, addressMatches, rerouted,
        fixedSlot: !document.querySelector('#assembly-row #dd-kitslot')
          && document.querySelector('#w-name').textContent.includes(tr(ARC_POOL_LABEL[slot])),
        jumped: buildPayload().weapon === weapon && document.body.classList.contains('on-simulator') });
      nav(weaponPath(weapon));
      await new Promise(r => setTimeout(r, 1000));
      if (!sameState(snapshotState(), before)) whileApplying(() => restoreState(before, weapon));
      activeScenario = null;
      sim.duration = 5;
      localStorage.setItem(SIM_RUNS_KEY, '1');
      for (const module of ['simulator', 'optimizer', '']) {
        const link = [...document.querySelectorAll('#module-tabs a')]
          .find(a => a.getAttribute('href') === weaponPath(weapon) + (module ? '/' + module : ''));
        if (!link) { out.push({ kind: 'module', slot, module, weapon, missingLink: true }); continue; }
        link.click();
        for (let i = 0; i < 50; i++) {
          await new Promise(r => setTimeout(r, 100));
          if (document.querySelector('#module-tabs .sel')?.getAttribute('href') === link.getAttribute('href')) break;
        }
        const p = buildPayload();
        let result = null, sent = null;
        if (module === 'simulator') {
          const simulate = simulateFleet;
          const previous = shownResult;
          simulateFleet = (body, ...args) => { sent = body.weapon; return simulate(body, ...args); };
          try {
            $('run-sim').click();
            for (let i = 0; i < 300 && $('run-sim').disabled; i++) await new Promise(r => setTimeout(r, 100));
            result = shownResult !== previous ? shownResult?.r : null;
          } finally { simulateFleet = simulate; }
        }
        out.push({ kind: 'module', slot, module, weapon: p.weapon,
          kept: JSON.stringify(snapshotState()) === JSON.stringify(before),
          parts: p.assembly, simulated: module !== 'simulator'
            || (sent === weapon && !$('run-sim').disabled && result?.ok === true && result?.dps > 0),
          error: result?.error });
      }
    }
    return out;
  })()`);
  const entries = slotNavigation.filter(x => x.kind === 'entry');
  const modules = slotNavigation.filter(x => x.kind === 'module');
  check(chamber.chamber + ' keeps the selected slot through routing and the open jump menu',
    entries.length === 2 && entries.every(x => x.addressMatches && x.rerouted && x.jumped && x.fixedSlot),
    JSON.stringify(entries));
  check(chamber.chamber + ' keeps its slot, build and parts through simulator, optimizer and builder',
    modules.length === 6 && modules.every(x => !x.missingLink
      && x.weapon === chamber.chamber + '_' + x.slot && x.kept && x.parts.loader === 'killstream' && x.simulated),
    JSON.stringify(slotNavigation));

  if (chamber.chamber === 'catchmoon') {
    for (const slot of ['secondary', 'primary']) {
      const saved = await evaluate(`(async () => {
        nav(weaponPath(${JSON.stringify(chamber.chamber)} + '_' + ${JSON.stringify(slot)}));
        for (let i = 0; i < 100 && weaponInfo($('weapon').value).slot !== ${JSON.stringify(slot)}; i++) {
          await new Promise(r => setTimeout(r, 100));
        }
        flushPresetSaves();
        return { path: location.pathname, state: snapshotState() };
      })()`);
      await app.load(saved.path, 8000);
      const opened = await evaluate(`({ weapon: buildPayload().weapon, state: snapshotState() })`);
      check('Catchmoon ' + slot + ' survives a cold page reload with its build',
        opened.weapon === 'catchmoon_' + slot && isDeepStrictEqual(opened.state, saved.state),
        JSON.stringify({ path: saved.path, opened }));
    }
    const shared = await evaluate(`(async () => {
      const pause = ms => new Promise(r => setTimeout(r, ms));
      nav(weaponPath('catchmoon_secondary')); await pause(1500);
      flushPresetSaves();
      const before = seatPayload(snapshotState());
      const code = await shareCode();
      nav(weaponPath('catchmoon_primary')); await pause(1500);
      jump.open = true; renderJump();
      const imported = await importShare(code); await pause(1500);
      return { imported, weapon: buildPayload().weapon, route: routeWeaponId(),
        kept: sameState(before, seatPayload(snapshotState())),
        jump: document.querySelector('#jump-body a[href$="/simulator"]')?.getAttribute('href') };
    })()`);
    check('a secondary Catchmoon share restores its route and jump menu from the primary entry',
      shared.imported && shared.weapon === 'catchmoon_secondary' && shared.route === shared.weapon
        && shared.kept && shared.jump === '/weapons/catchmoon_secondary/simulator',
      JSON.stringify(shared));
    const history = await evaluate(`(async () => {
      const pause = ms => new Promise(r => setTimeout(r, ms));
      const arrived = async id => {
        for (let i = 0; i < 100 && $('weapon').value !== id; i++) await pause(100);
      };
      nav(weaponPath('catchmoon_primary')); await arrived('catchmoon_primary');
      const primary = snapshotState(); flushPresetSaves();
      nav(weaponPath('catchmoon_secondary')); await arrived('catchmoon_secondary');
      const secondary = snapshotState(); flushPresetSaves();
      window.history.back(); await arrived('catchmoon_primary');
      const back = sameState(primary, snapshotState());
      window.history.forward(); await arrived('catchmoon_secondary');
      return { back, forward: sameState(secondary, snapshotState()), weapon: buildPayload().weapon };
    })()`);
    check('browser Back and Forward restore each Catchmoon slot and its build',
      history.back && history.forward && history.weapon === 'catchmoon_secondary', JSON.stringify(history));
    const stashed = await evaluate(`(() => {
      const state = snapshotState();
      sessionStorage.setItem('wfsim-lang-stash', JSON.stringify(state));
      return state;
    })()`);
    await app.load('/weapons/Catchmoon', 8000);
    const restored = await evaluate(`({ weapon: buildPayload().weapon, route: routeWeaponId(), state: snapshotState() })`);
    check('a reload stash from the old chamber URL restores the secondary route and build',
      restored.weapon === 'catchmoon_secondary' && restored.route === restored.weapon
        && isDeepStrictEqual(restored.state, stashed), JSON.stringify(restored));
  }
}

await finish('Kitgun module navigation keeps the selected slot and build');
