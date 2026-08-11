// A TARGET YOU MADE IS A TARGET LIKE ANY OTHER.
//
// The second CUSTOM (owner, 2026-08-11), and AGENTS.md named it before it
// existed: "custom enemies will become entries in the scenario's enemy list".
// That sentence is the whole test — if it is true, the simulator, the optimizer
// and the target card need no code of their own for it, because they all read
// the scenario's target list and the server reads one `EnemySpec` whether a
// wiki published it or a player typed it.
//
// What can go wrong here is specific, so this checks the specific things:
//   - a custom that never reaches the SERVER, so the fight silently runs against
//     the default unit and the number is somebody else's
//   - an editor that saves a shape the server rejects
//   - a vulnerability column that is shown and not applied (immunity is the
//     sharp case: 0 must mean nothing gets through)
//   - a delete that leaves the fight pointing at a target nobody answers to
//
//   node scripts/check_custom_enemies.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep } = app;

const r = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  localStorage.clear();
  const go = async (path) => { history.pushState({},'','/weapons/Torid'+path); route(); await sleep(700); };
  history.pushState({},'','/weapons/Torid'); route(); await sleep(3500);
  const out = {};

  // ---- the EDITOR ------------------------------------------------------
  await go('/enemies');
  const vis = (id) => { const e=document.getElementById(id); return !!e && e.offsetParent !== null; };
  out.tabShows = vis('enemy-block');
  out.emptyAtFirst = loadPresetList('enemies').length === 0;
  document.querySelector('#enemy-tools .cu-new').click(); await sleep(300);
  out.created = loadPresetList('enemies').map(p => p.name);
  out.formShown = !!document.querySelector('#enemy-form [data-en-k="stats.health"]');

  // Type a target: 5000 HP, no armour, a x4 head, and IMMUNE TO HEAT.
  const set = async (sel, v) => {
    const el = document.querySelector(sel);
    if (el.type === 'checkbox') el.checked = v; else el.value = String(v);
    el.dispatchEvent(new Event('change', {bubbles:true}));
    await sleep(150);
  };
  await set('[data-en-k="stats.health"]', 5000);
  await set('[data-en-k="stats.armor"]', 0);
  await set('[data-en-k="stats.shield"]', 0);
  await set('[data-en-k="stats.base_level"]', 1);
  await set('#en-own-col', true);
  await set('[data-en-dm="heat"]', 0);
  await set('[data-en-dm="void"]', 2);
  document.querySelectorAll('.en-part')[1].querySelector('[data-en-p="multiplier"]').value = '4';
  document.querySelectorAll('.en-part')[1].querySelector('[data-en-p="multiplier"]')
    .dispatchEvent(new Event('change', {bubbles:true}));
  await sleep(200);
  const doc = loadPresetList('enemies')[0].state;
  out.saved = { hp: doc.stats.health, heat: doc.damage_modifiers.heat, void: doc.damage_modifiers.void,
                head: doc.body_parts[1].multiplier };

  // ---- it is a TARGET, in every list that asks --------------------------
  const id = enemyId('target 1');
  out.inList = allEnemies().some(e => e.id === id);
  out.card = (enemyCard(id) || {}).type_modifiers;

  // ---- and it REACHES THE SERVER ---------------------------------------
  await go('/simulator');
  sim.enemy = id; sim.level = 1; sim.steel_path = false; sim.eximus = false;
  sim.runs = 20; sim.duration = 10;
  // Equipped with nothing: the Torid's own Toxin, so the immunity below is
  // about the target rather than about a mod.
  const body = { ...buildPayload(), ...fightPayload(), buffs: {} };
  out.sentCustom = (body.custom_enemies || []).map(e => e.id);
  const shot = await api('/api/simulate', body);
  out.ok = shot && shot.ok !== false;
  out.targetName = shot && shot.target && shot.target.name;
  out.err = shot && shot.error;

  // THE IMMUNITY IS THE SHARP ONE, and it is checked by MEASURING rather than
  // by reading the card back: a column that is shown and not applied looks
  // exactly like one that works. The Torid deals Toxin and nothing else, so a
  // Toxin-immune target must take nothing at all, and the same target at x1
  // must take something.
  const dmgVs = async (mult) => {
    const ps = loadPresetList('enemies');
    ps[0].state.damage_modifiers = { toxin: mult };
    storePresetList('enemies', ps);
    const res = await api('/api/simulate', { ...buildPayload(), ...fightPayload(), buffs: {} });
    return (res && res.dps) || 0;
  };
  out.immune = await dmgVs(0);
  out.normal = await dmgVs(1);

  // ---- and DELETING one does not leave the fight pointing at nothing -----
  await go('/enemies');
  document.querySelector('.en-row').click(); await sleep(300);
  document.querySelector('#enemy-tools .cu-del').click(); await sleep(300);
  out.afterDelete = sim.enemy;
  out.listEmpty = loadPresetList('enemies').length === 0;
  return out;
})()`);

check("the Enemies tab draws", r.tabShows === true);
check("...starting empty", r.emptyAtFirst === true);
check("...+ new target makes one", String(r.created) === "target 1", String(r.created));
check("...and opens its editor", r.formShown === true);
check("every field saves", JSON.stringify(r.saved) === JSON.stringify({ hp: 5000, heat: 0, void: 2, head: 4 }),
  JSON.stringify(r.saved));

check("it is in the target list", r.inList === true);
// Its own column, and it STARTED from the faction's rather than from fifteen
// ones — switching the toggle on copies what the target already was, so the
// Grineer entries survive beside the two that were typed.
check("...carrying its own vulnerability column", JSON.stringify(r.card) === JSON.stringify([
  { type: "impact", mult: 1.5 }, { type: "heat", mult: 0 },
  { type: "corrosive", mult: 1.5 }, { type: "void", mult: 2 },
]), JSON.stringify(r.card));

check("the fight carries it to the server", String(r.sentCustom) === "custom:target 1", String(r.sentCustom));
check("...which accepts it", r.ok === true, String(r.err));
check("...and fights the target that was typed", r.targetName === "target 1", String(r.targetName));

check("an immune column lets NOTHING through", r.immune === 0, String(r.immune));
check("...and the same target at x1 takes damage", r.normal > 0, String(r.normal));

check("deleting it repoints the fight at a real target",
  r.afterDelete && !String(r.afterDelete).startsWith("custom:"), String(r.afterDelete));
check("...and the list is empty again", r.listEmpty === true);

await app.finish("a target you made is a target like any other");
