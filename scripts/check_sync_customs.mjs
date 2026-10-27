/// A CUSTOM TRAVELS WITH WHAT NAMES IT, AND IS HELD WHERE IT IS ABSENT
/// (docs/UI.md §"Build sync").
///
/// The sync server is faked in the page, as `check_sync_client` fakes it, and a
/// second browser is this one with its storage swapped. What it holds: a synced
/// build brings the riven it names, which reaches the account first; past the
/// customs allowance the riven stays and is named; a browser without the riven
/// shows its slot as absent, saves the build WITH it, and seats it when it
/// arrives; taking a riven off says who uses it; a fight whose custom target is
/// absent stands a unit in for it and saves the target, not the stand-in.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, base: process.env.WFSIM_BASE });
const { evaluate, check } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const L = 'wfsim-presets-builder-builds', RV = 'wfsim-customs-rivens', EN = 'wfsim-customs-enemies', SC = 'wfsim-presets-simulator-scenarios';
  const st = (mod) => buildState('torid', { evoSel: {}, arcane: ['none'], arcaneRank: [null],
    slots: [{ mod, rank: mod ? 8 : null }, { mod: 'serration', rank: 10 }], mode: null, valence: null, assembly: null, wielder: null });
  const riven = (id, name) => ({ id, name, scope: rivenScope('torid'), savedAt: 1, state: blankRiven() });
  const srv = new Map();
  const order = [];
  const attempts = [];
  let clock = 1, allowance = null;
  const realFetch = window.fetch;
  window.fetch = async (url, o = {}) => {
    const path = String(url);
    const reply = (j) => new Response(JSON.stringify(j), { status: 200, headers: { 'content-type': 'application/json' } });
    if (path === '/api/account') return reply({ ok: true, providers: ['email'],
      account: { id: 'acc1', created_at: '', identities: [{ provider: 'email', label: 'a@x' }] } });
    if (path !== '/api/cloud/sync') return realFetch(url, o);
    const b = JSON.parse(o.body || '{}');
    const versions = {}, refused = [], conflicts = [];
    for (const c of b.changes || []) {
      attempts.push(c.id);
      const had = srv.get(c.id);
      if (c.base !== undefined && had && c.base !== had.version) {
        conflicts.push({ id: c.id, list: had.list, version: had.version, body: had.body });
        continue;
      }
      // THE REAL SERVER'S ALLOWANCE: a new live item past it is refused.
      const live = (pool) => [...srv.values()].filter((e) => e.body && e.body.cloud_sync !== false && e.list.startsWith('wfsim-' + pool + '-')).length;
      const pool = c.list.startsWith('wfsim-customs-') ? 'customs' : 'presets';
      if (allowance && !srv.has(c.id) && !c.deleted && live(pool) >= allowance[pool]) { refused.push(c.id); continue; }
      order.push(c.id);
      const v = ((srv.get(c.id) || {}).version || 0) + 1;
      srv.set(c.id, { list: c.list, body: c.deleted ? null : JSON.parse(JSON.stringify(c.body)), updated_at: c.updated_at, synced_at: clock++, version: v });
      versions[c.id] = v;
    }
    const extra = { conflicts, ...(allowance ? { allowance, refused } : {}) };
    if (b.pull === false) return reply({ ok: true, full: false, versions, ...extra });
    const since = b.after ? b.after.t : (b.since || 0);
    const rows = [...srv.entries()].filter(([, e]) => e.synced_at > since).sort((x, y) => x[1].synced_at - y[1].synced_at);
    const last = rows[rows.length - 1];
    return reply({ ok: true, full: false, versions, ...extra, entries: rows.map(([id, e]) => ({ id, ...e })),
      next: null, cursor: last ? last[1].synced_at : (b.since || 0) });
  };
  const quiet = async () => {
    clearTimeout(syncTimer); syncTimer = null;
    while (syncRunning || syncQueued) await (syncQueued || syncRunning);
    for (const k of [...pendingSaves.keys()]) dropSave(k);
  };
  const keep = async () => { await quiet(); return Object.fromEntries(Object.keys(localStorage).filter((k) => k.startsWith('wfsim-')).map((k) => [k, localStorage.getItem(k)])); };
  const read = (k) => JSON.parse(localStorage.getItem(k) || '[]');
  const note = () => (document.getElementById('page-note') || {}).textContent || '';
  const clearNote = () => { const n = document.getElementById('page-note'); if (n) n.remove(); };
  const out = {};

  // BROWSER A: a riven kept on this browser, and a synced build that wears it.
  localStorage.clear();
  localStorage.setItem(RV, JSON.stringify([{ ...riven('rv1', 'riven one'), cloud_sync: false }]));
  localStorage.setItem(L, JSON.stringify([{ id: 'b1', scope: 'torid', name: 'with riven', savedAt: 5, state: st('riven:rv1') }]));
  await loadAccount(); await quiet(); await syncNow();
  out.brought = [srv.has('rv1') && !!srv.get('rv1').body.state, read(RV)[0].cloud_sync === undefined];
  out.order = order.indexOf('rv1') >= 0 && order.indexOf('rv1') < order.indexOf('b1');

  // PAST THE CUSTOMS ALLOWANCE: the build goes, its riven stays and is named.
  allowance = { presets: 99, customs: 1 };
  clearNote();
  localStorage.setItem(RV, JSON.stringify(read(RV).concat([riven('rv2', 'riven two')])));
  localStorage.setItem(L, JSON.stringify(read(L).concat([{ id: 'b2', scope: 'torid', name: 'short', savedAt: 6, state: st('riven:rv2') }])));
  await syncNow();
  out.short = [srv.has('b2'), srv.has('rv2'), note().includes('riven two')];
  allowance = null;
  await keep();

  // BROWSER B: the account's builds, and only the riven the account holds.
  localStorage.clear();
  await syncNow();
  out.bHas = [read(L).map((p) => p.id).sort().join(), read(RV).map((p) => p.id).join()];
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(3000);
  clearNote();
  pickPreset(buildBarCfg(), 'b2');
  await sleep(500);
  const slot0 = document.querySelector('#mod-slots .slot');
  out.absent = [slots[0].mod, !!slot0 && slot0.classList.contains('absent'), note().length > 0];
  // AN EDIT ON B SAVES THE BUILD — WITH the riven it could not seat.
  slots[1].rank = 9; markPresetDirty(); flushPresetSaves();
  const saved = (read(L).find((p) => p.id === 'b2') || {}).state || {};
  out.kept = [saved.slots && saved.slots[0].mod, saved.slots && saved.slots[1].rank];
  // ITS FORMA IS NOT PLANNED without it.
  clearNote();
  out.noForma = (await autoForma()) === null && note().length > 0;
  // THE RIVEN ARRIVES — another browser syncs it: it is seated where the build held it.
  await quiet(); await syncNow();
  out.beforeArrive = slots[0].mod;
  srv.set('rv2', { list: RV, body: riven('rv2', 'riven two'), updated_at: Date.now(), synced_at: clock++, version: 1 });
  await syncNow(); await sleep(300);
  out.seated = [slots[0].mod, (read(L).find((p) => p.id === 'b2') || {}).state.slots[0].mod];

  // TAKING A RIVEN OFF says how many synced items use it.
  clearNote();
  setCloudSync(RV, 'rv1', false);
  out.offNote = /1/.test(note());

  // A FIGHT WHOSE CUSTOM TARGET IS ABSENT: a unit stands in, the target is saved.
  localStorage.setItem(SC, JSON.stringify([{ id: 's1', name: 'vs mine', savedAt: 7, state: { enemy: 'custom:e1', level: 100 } }]));
  history.pushState({}, '', '/weapons/Torid/simulator'); route(); await sleep(2500);
  clearNote();
  pickPreset(scenarioBarCfg(), 's1'); await sleep(300);
  out.fightStand = [sim.enemy !== 'custom:e1' && !!enemyCard(sim.enemy), note().length > 0];
  sim.level = 120; markScenarioDirty(); flushPresetSaves();
  const sc = (read(SC).find((p) => p.id === 's1') || {}).state || {};
  out.fightKept = [sc.enemy, sc.level];
  // …AND IT IS SEATED WHEN IT ARRIVES.
  localStorage.setItem(EN, JSON.stringify([{ id: 'e1', name: 'my target', savedAt: 8, state: blankEnemy() }]));
  syncShow({ lists: new Set([EN]), ids: new Set(['e1']), removed: new Set() });
  await sleep(300);
  out.fightSeated = sim.enemy;

  // The former board-derived owned identity is already deleted on the account.
  await quiet();
  localStorage.clear(); srv.clear(); order.length = 0; attempts.length = 0;
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(3000);
  const weapon = $('weapon').value;
  const rows = (BOARD[weapon] || []).filter(x => x.benchmark === 'standard_single_target');
  if (!rows.length) throw new Error('no board row for read-only sync fixture');
  const best = rows.reduce((a, b) => a.score > b.score ? a : b);
  const shape = { bonuses: ['critical_damage', 'damage', 'multishot'], malus: 'recoil', rolls: [1.1, 1.1, 1.1, 1.1] };
  BOARD[weapon] = [{ ...best, score: best.score * 1.5,
    mods: best.mods.map((m, i) => i === 0 ? 'riven' : m), riven: shape }, ...rows];
  const legacy = boardRivenName(shape);
  srv.set(legacy, { list: RV, body: null, updated_at: 1, synced_at: clock++, version: 2 });
  await syncNow(); await quiet();
  const entry = builtinBuilds().find(p => p.board && p.board.riven);
  if (!entry) throw new Error('no published riven build for sync fixture');
  const beforeStorage = localStorage.getItem(RV);
  const cfg = buildBarCfg(); cfg.setActive(''); pickPreset(cfg, entry.builtin);
  await quiet(); await syncNow(); await quiet();
  out.readonlySync = [read(RV).length === 0, localStorage.getItem(RV) === beforeStorage,
    !attempts.includes(legacy), srv.get(legacy).body === null, srv.get(legacy).version === 2];
  const copyId = copyActivePreset(cfg);
  await quiet(); await syncNow(); await quiet();
  const copy = read(L).find(p => p.id === copyId);
  const ref = copy?.state.slots.find(s => isRivenId(s.mod))?.mod;
  const customId = ref?.slice(RIVEN_PREFIX.length);
  out.readonlyCopy = [!!customId && customId !== legacy && !customId.startsWith('readonly:'),
    !!srv.get(customId)?.body, !!srv.get(copyId)?.body,
    order.indexOf(customId) >= 0 && order.indexOf(customId) < order.indexOf(copyId),
    srv.get(copyId)?.body.state.slots.some(s => s.mod === ref) === true,
    srv.get(legacy).body === null && srv.get(legacy).version === 2];
  window.fetch = realFetch;
  return out;
})()`);

const ok = (x) => JSON.stringify(x);
check("a synced build brings the riven it names, even one kept on this browser", ok(r.brought) === ok([true, true]), ok(r.brought));
check("...and the riven reaches the account before the build", r.order === true);
check("past the customs allowance the build syncs, its riven stays here, and is named", ok(r.short) === ok([true, false, true]), ok(r.short));
check("another browser takes the builds and only the riven the account holds", ok(r.bHas) === ok(["b1,b2", "rv1"]), ok(r.bHas));
check("a build whose riven is not on this browser shows the slot as absent, and says so", ok(r.absent) === ok([null, true, true]), ok(r.absent));
check("...an edit there saves the build WITH its riven", ok(r.kept) === ok(["riven:rv2", 9]), ok(r.kept));
check("...its Forma is not planned without it, out loud", r.noForma === true);
check("...and the riven is seated when it arrives", r.beforeArrive === null && ok(r.seated) === ok(["riven:rv2", "riven:rv2"]), ok([r.beforeArrive, r.seated]));
check("taking a riven off the account says how many synced items use it", r.offNote === true);
check("a fight whose custom target is absent stands a unit in, and says so", ok(r.fightStand) === ok([true, true]), ok(r.fightStand));
check("...and saves the target, not the stand-in", ok(r.fightKept) === ok(["custom:e1", 120]), ok(r.fightKept));
check("...and fights it once it arrives", r.fightSeated === "custom:e1", r.fightSeated);
check("opening a published riven neither writes owned storage nor resurrects its old cloud tombstone",
  r.readonlySync?.every(Boolean), ok(r.readonlySync));
check("copying syncs a fresh owned riven before the build that references it and preserves the tombstone",
  r.readonlyCopy?.every(Boolean), ok(r.readonlyCopy));

await app.finish("a custom travels with what names it, and is held where it is absent");
