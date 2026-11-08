// Restored ranks and roll corners use the intake's canonical parent identity.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const binary = resolve(process.env.WFSIM_INVENTORY_BOARD_BIN || 'target/debug/wfsim-board');
const dir = mkdtempSync(join(tmpdir(), 'default-parent-'));
const record = {
  weapon: 'torid', arcanes: ['primary_deadhead'], exilus: 'vigilante_supplies',
  evolutions: ['torid_evo1_incarnon_form', 'torid_final_fusillade', 'torid_swift_deliverance', 'torid_survivors_edge'],
  mods: ['primed_cryo_rounds', 'malignant_force', 'hellfire', 'primary_acuity', 'galvanized_scope', 'vital_sense', 'hunter_track@0', 'riven'],
  riven_neg: 'status_duration', riven_pos: ['critical_chance', 'critical_damage', 'damage'],
  riven_rolls: [1.1, 1.1, 1.1, 0.9],
};
const parent = { ...record, mods: record.mods.map(mod => mod.split('@')[0]) };
const lead = { ...parent, arcanes: ['primary_merciless'] };
const rolled = { ...parent, riven_rolls: [0.9, 1.1, 1.1, 0.9] };
const parse = text => text.trim().split('\n').filter(Boolean).map(line => JSON.parse(line));
const canonical = records => parse(execFileSync(binary, ['--inventory-builds'], {
  input: JSON.stringify(records), encoding: 'utf8', timeout: 30000,
}));
const [child, full, leader, roll] = canonical([record, parent, lead, rolled]);
assert.equal(child.id, '6b96ab9988535a7badff7904ccba032c', 'saved child identity stays frozen');
assert.equal(full.id, '73e98d44d3ef0b61a2b5e33bb10ea262', 'intake parent identity stays frozen');

function missing(name, records, facts = [], workers) {
  const source = join(dir, name), queue = source + '.queue', measured = source + '.facts';
  writeFileSync(queue, '');
  writeFileSync(measured, facts.map(row => JSON.stringify({ ruler: 'standard_single_target', mode: 'base',
    finished_at: '2026-11-01T00:00:00.000Z', ...row })).join('\n'));
  execFileSync(binary, ['--each-ruler', '--dry-run', '--gate', '--facts-in', measured,
    '--queue-in', queue, '--queue-missing', source + '.{id}.missing',
    ...(workers === undefined ? [] : ['--inventory-workers', String(workers)])], {
    input: JSON.stringify(records), encoding: 'utf8', timeout: 30000, stdio: ['pipe', 'pipe', 'pipe'],
  });
  const roster = execFileSync(binary, ['--rulers'], { encoding: 'utf8', timeout: 30000 }).trim().split(/\s+/);
  return roster.flatMap(ruler => parse(readFileSync(source + `.${ruler}.missing`, 'utf8')));
}

try {
  const waiting = missing('waiting', [child.record, full.record]);
  for (const workers of [1, 2]) assert.deepEqual(missing(`workers-${workers}`, [child.record, full.record], [], workers), waiting,
    'bounded parallel rulers preserve every ordered missing row');
  for (const workers of [['0'], ['9'], ['broken'], []]) assert.throws(() => execFileSync(binary,
    ['--each-ruler', '--dry-run', '--inventory-workers', ...workers], { input: '[]', stdio: 'pipe', timeout: 30000 }),
    error => error.status === 2 && error.stderr.toString().includes('--inventory-workers requires'));
  assert.equal(waiting.filter(row => row.build_id === child.id).length, 0,
    'a lower rank waits for its existing canonical parent on every ruler and mode');
  assert(waiting.some(row => row.build_id === full.id), 'the parent still receives its first fight');
  const below = missing('below', [child.record, full.record, leader.record], [
    { identity: full.id, score: 1 }, { identity: leader.id, score: 100 },
  ]);
  assert.equal(below.filter(row => row.build_id === child.id).length, 0, 'a parent below the entry line cannot release its variant');
  const earned = missing('earned', [child.record, full.record], [{ identity: full.id, score: 100 }]);
  assert.deepEqual(earned.filter(row => row.build_id === child.id && row.ruler === 'standard_single_target')
    .map(row => row.mode).sort(), ['base', 'cycle'], 'a measured qualifying parent releases both playable modes');
  const legacy = missing('legacy', [child.record]);
  assert(legacy.some(row => row.build_id === child.id), 'a legacy variant whose parent is absent remains eligible');
  const rollWaiting = missing('roll-waiting', [roll.record, full.record]);
  assert.equal(rollWaiting.filter(row => row.build_id === roll.id).length, 0, 'roll-only alternatives still wait');
  console.log('Canonical rank parents gate all rulers and modes; earned, legacy and roll-only cases remain intact.');
} finally { rmSync(dir, { recursive: true, force: true }); }
