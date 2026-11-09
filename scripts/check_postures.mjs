// SPDX-License-Identifier: AGPL-3.0-or-later
// Postures use the stance slot and expose combat effects with partial limits.
import { openApp } from './cdp.mjs';

const app = await openApp({ boot: 12000, lang: 'en' });
const result = await app.evaluate(`(async () => {
  history.pushState({}, '', weaponPath('adarza_claws') + '/builder');
  await route();
  for (let i = 0; i < 100 && document.getElementById('weapon').value !== 'adarza_claws'; i++) {
    await new Promise(r => setTimeout(r, 100));
  }
  const cards = weaponAxes('adarza_claws').mods.filter(m => m.id.endsWith('_posture'));
  const panel = mods => api('/api/panel', { weapon: 'adarza_claws', mods });
  const panels = await Promise.all(['assassin_posture', 'assassin_posture@0',
    'protector_posture', 'protector_posture@0', 'persistent_posture',
    'frenzied_posture', 'balanced_posture', 'elusive_posture'].map(id => panel([id])));
  const seated = await window.wfsim.do('builder.mod.set', {slot: 'stance', mod: 'protector_posture', rank: 0});
  const payload = buildPayload();
  const sim = await api('/api/simulate', {weapon: 'adarza_claws', mods: ['elusive_posture'], duration: 1, runs: 1});
  const two = await panel(['assassin_posture', 'protector_posture']);
  return { cards, panels, payload, seated, sim: {ok: sim.ok, shots: sim.shots, dps: sim.dps}, two,
    label: document.getElementById('stance-label')?.textContent,
    postureHint: document.getElementById('posture-hint')?.hidden === false,
    comboHint: document.getElementById('stance-combo-hint')?.hidden === true };
})()`);
const rows = p => (p.forms || []).flatMap(f => [...(f.stats || []), ...(f.parts || []).flatMap(part => part.stats || [])]);
const row = (i, key) => rows(result.panels[i]).find(r => r.key === key);
const number = v => Number(String(v).replace(/[^0-9.]/g, ''));
app.check('all six Postures are usable stance cards with disclosed partial clauses',
  result.cards.length === 6 && result.cards.every(c => c.stance && !c.not_modeled && c.unmodeled_effects.length > 0));
app.check('all Posture descriptions resolve their rank placeholders',
  result.cards.every(c => c.desc_ranks.length === 4 && c.desc_ranks.every(s => !/X[%m ]/.test(s))));
app.check('Assassin max and minimum ranks apply their Overguard bonus',
  number(row(0, 'overguard_damage')?.final) === 4 && number(row(1, 'overguard_damage')?.final) === 1.75);
app.check('Protector max and minimum ranks show four and one forced statuses',
  row(2, 'additional_forced_procs')?.final === '4' && row(3, 'additional_forced_procs')?.final === '1');
app.check('forced status sources use Puncture counts',
  row(2, 'additional_forced_procs')?.sources[0]?.value === '+4 Puncture');
app.check('Persistent adds Impact to claws with no innate Impact',
  result.panels[4].forms?.[0]?.parts?.[0]?.damage?.some(d => d.type === 'Impact' && Number(d.amount) === 36));
app.check('Frenzied increases status duration', number(row(5, 'status_duration')?.final) === 1.8);
app.check('every Posture exposes its target intent',
  result.panels.every(p => rows(p).some(r => r.key === 'target_preference')));
app.check('seating a lower-rank Posture sends its stance-slot value to the API',
  result.payload.mods?.includes('protector_posture@0'), JSON.stringify(result.seated));
app.check('the builder explains Postures without promising a melee combo',
  result.label === 'Posture' && result.postureHint && result.comboHint);
app.check('Elusive suppresses ordinary attacks in the real simulator',
  result.sim.ok && result.sim.shots === 0 && result.sim.dps === 0);
app.check('a build cannot combine two Postures', result.two.ok === false);
await app.finish('Postures reach the builder and simulator');
