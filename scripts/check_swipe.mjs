// SPDX-License-Identifier: AGPL-3.0-or-later
// Swipe belongs to eligible claw weapons and changes range/target limits.
import { openApp } from './cdp.mjs';

const app = await openApp({ boot: 12000 });
const result = await app.evaluate(`(async () => {
  history.pushState({}, '', weaponPath('adarza_claws') + '/builder');
  await route();
  for (let i = 0; i < 100 && document.getElementById('weapon').value !== 'adarza_claws'; i++) {
    await new Promise(r => setTimeout(r, 100));
  }
  const eligible = META.weapons.filter(w => w.mod_class === 'beast_claws')
    .filter(w => withPoolOf(w, () => weaponAxes(w.id).mods.some(m => m.id === 'swipe'))).map(w => w.id).sort();
  const panel = await api('/api/panel', { weapon: 'adarza_claws', mods: ['swipe'] });
  const low = await api('/api/panel', { weapon: 'adarza_claws', mods: ['swipe@0'] });
  const rows = p => (p.forms || []).flatMap(f => f.stats || []);
  return { eligible, rows: rows(panel), low: rows(low),
    description: weaponAxes('adarza_claws').mods.find(m => m.id === 'swipe')?.description };
})()`);
const expected = ['adarza_claws', 'crescent_claws', 'panzer_claws', 'sly_claws',
  'smeeta_claws', 'vasca_claws', 'venari_claws', 'venari_prime_claws'];
app.check('Swipe is offered on exactly its eight native-compatible claws',
  JSON.stringify(result.eligible) === JSON.stringify(expected), JSON.stringify(result.eligible));
const row = (rows, key) => rows.find(r => r.key === key);
app.check('Swipe max rank shows a three-metre radius and five-target limit',
  row(result.rows, 'melee_radius')?.final === '3.0 m' && row(result.rows, 'melee_targets')?.final === '5',
  JSON.stringify(result.rows));
app.check('Swipe rank zero shows its smaller radius and target limit',
  row(result.low, 'melee_radius')?.final === '1.5 m' && row(result.low, 'melee_targets')?.final === '2',
  JSON.stringify(result.low));
app.check('Swipe sources use metres and counts rather than percentages',
  row(result.rows, 'melee_radius')?.sources[0]?.value === '+2.0 m'
  && row(result.rows, 'melee_targets')?.sources[0]?.value === '+4');
await app.finish('Swipe pool and rank-resolved geometry are exposed');
