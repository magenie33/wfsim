#!/usr/bin/env python3
"""Bind the selected primary forms to immutable client records and supported inputs."""
import argparse
import hashlib
import json
from pathlib import Path
import re

from import_client_cadence import blocks, canonical, fields, source

ROOT = Path(__file__).resolve().parents[1]
ROSTER = '20261109T053558Z-weapon-roster-dossiers'
PINS = {ROSTER: '7b830c6730e20621029d67d9d26e9aa8a216c998c32de73e2e22fe187c979d91',
    '20261109T025857Z-spread-sampling': 'e5351a0d64a5b31f5c3b3267184b30bbbe0d47e6ce63b78fd3fb388cb28ab70e'}
GROUPS = ('torid', 'burston_prime', 'burston', 'felarx', 'boar_prime', 'boar', 'tenet_glaxion')
TYPES = {'DT_IMPACT': 'impact', 'DT_PUNCTURE': 'puncture', 'DT_SLASH': 'slash',
    'DT_FIRE': 'heat', 'DT_FREEZE': 'cold', 'DT_POISON': 'toxin',
    'DT_RADIATION': 'radiation', 'DT_CORROSIVE': 'corrosive'}


def inner(text):
    return fields(text.strip()[1:-1])


def numeric(text):
    return canonical(text)


def render(value):
    return format(value, '.15g')


def render_time(value):
    text = render(value)
    return text if '.' in text or 'e' in text else text + '.0'


def replace_field(text, key, value, indent):
    pattern = rf'(?m)^{{indent}}{re.escape(key)}: [^\n]*'.replace('{indent}', ' ' * indent)
    rendered = render_time(value) if key.startswith('transmute_') else render(value)
    text, count = re.subn(pattern, ' ' * indent + key + ': ' + rendered, text)
    if count != 1:
        raise ValueError(f'{key}: expected one field, found {count}')
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exchange', type=Path, required=True)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    batches = args.exchange / 'batches'
    for batch, expected in PINS.items():
        assert hashlib.sha256((batches / batch / 'manifest.json').read_bytes()).hexdigest() == expected
    names = ['shared/objects.jsonl'] + [f'weapons/{group}/dossier.json' for group in GROUPS]
    payload, binding = source(batches / ROSTER, 'weapon-roster-dossiers.zip', names)
    objects = {o['path']: o for line in payload['shared/objects.jsonl'].splitlines() if (o := json.loads(line))}
    vectors, sampler_binding = source(batches / '20261109T025857Z-spread-sampling',
        'spread-sampling-evidence.zip', ['native/vectors.json', 'native/evidence.json'])
    fixture = {'source': binding, 'sampler_source': sampler_binding, 'forms': [], 'sampler_vectors': [], 'accuracy_upgrades': []}
    for v in json.loads(vectors['native/vectors.json']):
        if (v['kind'] == 'generic-sampler' and v['incoming_spread'] in (7, 30)
                and v['seed_before'] in ('0x1', '0x3039', '0xdeadbeef') and v['samples'] is not None):
            fixture['sampler_vectors'].append(v)
    assert fixture['sampler_vectors']
    changes = {}
    for group in GROUPS:
        dossier = json.loads(payload[f'weapons/{group}/dossier.json'])
        owner = dossier['identity']['native_paths'][0]
        obj = objects[owner]
        for evo in dossier['evolution_joins']:
            if evo['id'] not in ('boar_practiced_grip', 'boar_prime_practiced_grip', 'felarx_attuned_accuracy'):
                continue
            for ref in evo['upgrade_references']:
                original = objects[ref['target']]['composed_fields']['Upgrades']
                for block in blocks(original.partition('=')[2]):
                    effect = fields(block)
                    if effect['UpgradeType'] == 'WEAPON_SPREAD':
                        assert effect['OperationType'] == 'STACKING_MULTIPLY'
                        fixture['accuracy_upgrades'].append({'id': evo['id'], 'owner': ref['target'],
                            'raw': original, 'value': numeric(effect['Value']),
                            'aiming': 'PM_AIM' in effect['ValidModifiers']})
        behaviors = list(blocks(obj['composed_fields']['Behaviors'].partition('=')[2]))
        entries = {e['declared_form']: e for e in dossier['entries']}
        for index, raw in enumerate(behaviors):
            role = fields(raw)
            typed = {r: inner(role[next(k for k in role if k.startswith(r + ':') and not k.endswith(':Type'))])
                     for r in ('fire', 'impact', 'state')}
            form = 'base' if index == 0 else 'incarnon'
            entry = entries[form]
            fire, impact, state = (typed[r] for r in ('fire', 'impact', 'state'))
            row = {'id': entry['id'], 'owner': owner, 'behavior_index': index,
                'role_types': {r: role[r + ':Type'] for r in typed},
                'state': {k: state[k] for k in ('fireRate', 'reloadTime', 'ModeSwitchTime')},
                'fire': {k: fire[k] for k in ('fireIterations', 'ammoRequirement', 'AIMED_ACCURACY',
                    'HIP_FIRE_ACCURACY', 'SpreadRatioHorizontal', 'spreadUniform')},
                'impact': {k: v for k, v in impact.items() if k in ('AttackData', 'criticalHitChance',
                    'criticalHitDamageMultiplier', 'ContinuousDamageAttenuationTime',
                    'ContinuousDamageAttenuationCooldownTime', 'ContinuousDamageAttenuationCooldownDelay')}}
            row['economy'] = {k: v for k,v in obj['composed_fields'].items() if k in
                ('AmmoClipSize', 'AmmoCapacity', 'MaxCharges', 'AddCharges', 'ConsumeChargeConditions')}
            path = ROOT / entry['yaml_file']
            text = path.read_text()
            original = text
            damage = inner(impact['AttackData'])
            crit = numeric(impact['criticalHitChance'])
            multiplier = numeric(impact['criticalHitDamageMultiplier'])
            if 'projectileType' in fire and fire['projectileType'] != '""':
                token = fire['projectileType'].strip('"')
                candidates = [p for p in objects if p.endswith('/' + token)]
                assert len(candidates) == 1, (entry['id'], token, candidates)
                projectile = objects[candidates[0]]
                row['projectile'] = {'owner': candidates[0], 'fields': {k:v for k,v in
                    projectile['composed_fields'].items() if k in ('AttackData', 'CriticalChance',
                    'CriticalMultiplier', 'DamageFallOff', 'KinematicInitialSpeed', 'KinematicMaxSpeed')}}
                # Member-level inheritance keeps the base projectile's damage type.
                for ancestor in reversed(projectile['ancestry_leaf_first']):
                    parent = objects[ancestor]['composed_fields']
                    if 'AttackData' in parent:
                        damage.update(inner(parent['AttackData'].partition('=')[2]))
                crit = numeric(projectile['composed_fields']['CriticalChance'].partition('=')[2])
                multiplier = numeric(projectile['composed_fields']['CriticalMultiplier'].partition('=')[2])
            amount = numeric(damage['Amount'])
            if damage.get('UseNewFormat') == '1':
                vector = {TYPES[k]: numeric(v) for k,v in damage.items() if k in TYPES}
            elif damage.get('Type', 'DT_PHYSICAL') == 'DT_PHYSICAL':
                vector = {TYPES[k]: amount * numeric(v) for k,v in damage.items() if k in TYPES}
            else:
                vector = {TYPES[damage['Type']]: amount}
            row['expected'] = {'damage': vector, 'crit_chance': crit, 'crit_multiplier': multiplier,
                'status_chance': numeric(damage['ProcChance']) / numeric(fire['fireIterations']),
                'fire_rate': numeric(state['fireRate']) / 60, 'multishot': numeric(fire['fireIterations'])}
            rate = numeric(state['fireRate']) / 60
            if 'trigger: burst' not in text:
                text = replace_field(text, 'fire_rate', rate, 2)
            if form == 'incarnon':
                entry_time = numeric(inner(fields(behaviors[0])[
                    next(k for k in fields(behaviors[0]) if k.startswith('state:') and not k.endswith(':Type'))])['ModeSwitchTime'])
                exit_time = numeric(state['ModeSwitchTime'])
                text = replace_field(text, 'transmute_in_seconds', entry_time, 2)
                text = re.sub(r'(?m)^  transmute_out_seconds: [^\n]*\n', '', text)
                text = re.sub(r'(?m)^(  transmute_in_seconds: [^\n]*\n)',
                    lambda m: m[1] + '  transmute_out_seconds: ' + render_time(exit_time) + '\n', text)
                row['transition'] = {'entry_seconds': entry_time, 'exit_seconds': exit_time}
                if 'pseudo_reload:' in text and re.search(r'(?m)^  reload_seconds:', text):
                    text = replace_field(text, 'reload_seconds', entry_time + exit_time, 2)
            if 'ContinuousDamageAttenuationTime' in impact:
                row['beam_expected'] = {'range_m': numeric(fire['traceDistance']),
                    'damage_radius_m': numeric(fire['Radius']), 'chain_hops': int(impact['NumChains']),
                    'chain_range_m': numeric(impact['ChainDistance']),
                    'chain_damage_per_hop': numeric(impact['ChainDamageMultiplier']),
                    'chain_compounds': impact['SequentialChains'] == '1',
                    'per_target': impact['AttenuateContinuousDamagePerTarget'] == '1'}
                ramp = {'rise_seconds': numeric(impact['ContinuousDamageAttenuationTime']),
                    'cooldown_delay_seconds': numeric(impact['ContinuousDamageAttenuationCooldownDelay']),
                    'cooldown_divisor': numeric(impact['ContinuousDamageAttenuationCooldownTime'])}
                row['beam_ramp'] = ramp
                block = '    ramp:\n' + ''.join('      ' + k + ': ' + render(v) + '\n' for k,v in ramp.items())
                text = re.sub(r'(?m)^    ramp:\n(?:      [^\n]+\n)+', '', text)
                text, count = re.subn(r'(?m)^  beam:\n', '  beam:\n' + block, text)
                assert count == 1
            profile = objects.get(fire['AIMED_ACCURACY'])
            if profile:
                row['accuracy'] = {'owner': profile['path'], 'declared': profile['composed_fields']}
            # Special projectile/beam emission and live envelope timing remain disclosed.
            if (group in ('burston_prime', 'burston', 'felarx')
                    or group in ('boar_prime', 'boar') and form == 'base'
                    or group == 'torid' and form == 'incarnon'):
                sampling = {'horizontal_ratio': numeric(fire['SpreadRatioHorizontal']),
                    'uniform': fire['spreadUniform'] == '1'}
                hip = objects[fire['HIP_FIRE_ACCURACY']]['composed_fields']['Spread'].partition('=')[2]
                shooting = inner(inner(hip)['SHOOTING'])
                limits = [numeric(x) for x in shooting['range'].strip('{}').split(',')]
                sampling.update(hip_min_deg=limits[0], hip_max_deg=limits[1])
                row['sampling'] = sampling
                text = re.sub(r'(?m)^    sampling:\n(?:      [^\n]+\n)+', '', text)
                text, count = re.subn(r'(?m)^(    max_deg: [^\n]*\n)', lambda m: m[1]
                    + '    sampling:\n      horizontal_ratio: ' + render(sampling['horizontal_ratio'])
                    + '\n      uniform: ' + str(sampling['uniform']).lower()
                    + '\n      hip_min_deg: ' + render(limits[0]) + '\n      hip_max_deg: ' + render(limits[1]) + '\n', text)
                assert count == 1
            if 'client_primary_inputs' not in text:
                text += '\n# see notes: client_primary_inputs\n'
            gap = 'client_accuracy_runtime'
            prose = 'Sustained-fire spread growth, recovery and smoothing are not simulated; shots use the resting spread for the selected aiming state. Camera recoil is compensated by holding the reticle steady.'
            text = text.replace('  - ' + json.dumps(prose) + '\n', '')
            if 'reason: ' + gap not in text:
                if '\nunmodeled:\n' in text:
                    text = text.replace('\nunmodeled:\n', '\nunmodeled:\n  - reason: ' + gap + '\n', 1)
                else:
                    text += '\nunmodeled:\n  - reason: ' + gap + '\n'
            if 'beam_ramp' in row:
                gap = 'client_beam_target_ramp'
                prose = 'Beam ramp progress is shared across targets; per-target ramp initialization and reacquisition are not simulated.'
                text = text.replace('  - ' + json.dumps(prose) + '\n', '')
                if 'reason: ' + gap not in text:
                    text = text.replace('\nunmodeled:\n', '\nunmodeled:\n  - reason: ' + gap + '\n', 1)
            changes[path] = (original, text)
            fixture['forms'].append(row)
    fixture_path = ROOT / 'tests/fixtures/client/primary-five-2026-10-28.json'
    changes[fixture_path] = (fixture_path.read_text() if fixture_path.exists() else '', json.dumps(fixture, indent=2) + '\n')
    different = [str(p.relative_to(ROOT)) for p,(old,new) in changes.items() if old != new]
    if args.check:
        if different:
            raise SystemExit('client primary inputs differ: ' + ', '.join(different))
    else:
        for p,(_,text) in changes.items():
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text)
    print(json.dumps({'forms': len(fixture['forms']), 'native_sampler_vectors': len(fixture['sampler_vectors']),
        'changed_files': different, 'check': args.check}))


if __name__ == '__main__':
    main()
