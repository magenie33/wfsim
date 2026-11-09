#!/usr/bin/env python3
"""Bind Scourge inputs without treating unreviewed projectile branches as active."""
import argparse
import hashlib
import json
from pathlib import Path
import re

from import_client_cadence import blocks, fields, source
from import_client_primary import inner, numeric, replace_field, render

ROOT = Path(__file__).resolve().parents[1]
BATCH = '20261109T053558Z-weapon-roster-dossiers'
MANIFEST = '7b830c6730e20621029d67d9d26e9aa8a216c998c32de73e2e22fe187c979d91'
TYPES = {'DT_IMPACT': 'impact', 'DT_PUNCTURE': 'puncture', 'DT_SLASH': 'slash', 'DT_CORROSIVE': 'corrosive'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exchange', type=Path, required=True)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    batch = args.exchange / 'batches' / BATCH
    assert hashlib.sha256((batch / 'manifest.json').read_bytes()).hexdigest() == MANIFEST
    payload, binding = source(batch, 'weapon-roster-dossiers.zip',
        ['shared/objects.jsonl', 'weapons/scourge/dossier.json', 'weapons/scourge_prime/dossier.json'])
    objects = {o['path']: o for line in payload['shared/objects.jsonl'].splitlines() if (o := json.loads(line))}

    def members(path, key):
        merged = {}
        for ancestor in reversed(objects[path]['ancestry_leaf_first']):
            raw = objects[ancestor]['composed_fields'].get(key)
            if raw:
                merged.update(inner(raw.partition('=')[2]))
        return merged

    def projectile(token):
        matches = [p for p in objects if p.endswith('/' + token.strip('"'))]
        assert len(matches) == 1, matches
        path = matches[0]
        obj = objects[path]['composed_fields']
        return {'owner': path, 'attack': members(path, 'AttackData'),
            'explosion': members(path, 'ExplosiveAttack'),
            'fields': {k: v for k, v in obj.items() if k in (
                'CriticalChance', 'CriticalMultiplier', 'KinematicInitialSpeed', 'KinematicMaxSpeed',
                'KinematicGravity', 'KinematicVelocityAccel', 'DamageRadius', 'ExplosionFallOff',
                'ScriptInstance', 'MaxLife', 'SetLifeOnImpact', 'IgnoreFireIterations',
                'DealDamageThroughImpactBehavior', 'InitialWidth', 'InitialRadius')},
            'crit_chance': numeric(obj['CriticalChance'].partition('=')[2]),
            'crit_multiplier': numeric(obj['CriticalMultiplier'].partition('=')[2])}

    def damage(attack):
        amount = numeric(attack['Amount'])
        if attack.get('Type', 'DT_PHYSICAL') == 'DT_PHYSICAL':
            return {TYPES[k]: amount * numeric(v) for k, v in attack.items() if k in TYPES}
        return {TYPES[attack['Type']]: amount}

    fixture = {'source': binding, 'forms': [], 'attractor': {},
        'runtime_review': {'projectile_selector_closed': False, 'attractor_consumers_closed': False},
        'wiki': 'https://wiki.warframe.com/w/Scourge_Prime'}
    changes = {}
    for group in ('scourge', 'scourge_prime'):
        dossier = json.loads(payload[f'weapons/{group}/dossier.json'])
        owner = dossier['identity']['native_paths'][0]
        obj = objects[owner]['composed_fields']
        behaviors = list(blocks(obj['Behaviors'].partition('=')[2]))
        assert len(behaviors) == 2
        for index, entry in enumerate(dossier['entries']):
            assert entry['declared_form'] == ('base' if index == 0 else 'alt_fire')
            role = fields(behaviors[index])
            typed = {name: inner(role[next(k for k in role if k.startswith(name + ':') and not k.endswith(':Type'))])
                for name in ('fire', 'state')}
            fire, state = typed['fire'], typed['state']
            selected = projectile(fire['projectileType'])
            explosion = selected['explosion']
            row = {'id': entry['id'], 'owner': owner, 'behavior_index': index,
                'role_types': {k: v for k, v in role.items() if k.endswith(':Type')},
                'fire': {k: fire[k] for k in ('IgnoreFireIterations', 'fireIterations', 'ammoRequirement',
                    'AIMED_ACCURACY', 'HIP_FIRE_ACCURACY', 'SpreadRatioHorizontal', 'spreadUniform')},
                'state': {k: state[k] for k in ('fireRate', 'reloadTime')},
                'projectile_candidate': selected,
                'expected': {'damage': damage(selected['attack']), 'radial_damage': damage(explosion),
                    'crit_chance': selected['crit_chance'], 'crit_multiplier': selected['crit_multiplier'],
                    'status_chance': numeric(selected['attack']['ProcChance']),
                    'radial_status_chance': numeric(explosion['ProcChance']),
                    'radius_m': numeric(selected['fields']['DamageRadius'].partition('=')[2]),
                    'falloff_reduction': numeric(selected['fields']['ExplosionFallOff'].partition('=')[2]),
                    'fire_rate': numeric(state['fireRate']) / 60,
                    'disposition': numeric(obj['OmegaAttenuation'].partition('=')[2])}}
            path = ROOT / entry['yaml_file']
            text = original = path.read_text()
            if index == 0:
                text = replace_field(text, 'fire_rate', row['expected']['fire_rate'], 2)
                row['expected'].update(magazine=numeric(obj['AmmoClipSize'].partition('=')[2]),
                    ammo_max=numeric(obj['AmmoCapacity'].partition('=')[2]),
                    reload_seconds=numeric(state['reloadTime']),
                    projectile_speed_mps=numeric(selected['fields']['KinematicInitialSpeed'].partition('=')[2]))
                if group == 'scourge_prime' and 'reason: client_scourge_primary_co' not in text:
                    text = text.replace('\nunmodeled:\n', '\nunmodeled:\n  - reason: client_scourge_primary_co\n', 1)
                if 'reason: client_scourge_primary_collision' not in text:
                    text = text.replace('\nunmodeled:\n', '\nunmodeled:\n  - reason: client_scourge_primary_collision\n', 1)
            else:
                row['charged_candidate'] = projectile(fire['chargedProjectileType'])
                row['state'].update({k: state[k] for k in ('ReloadAfterEveryShot', 'DestroyActiveProjectiles', 'DetonateOnSwapToPrimary')})
                attractor_path = '/Lotus/Weapons/Tenno/LongGuns/TnPriestSpear/TnPriestSpearBulletAttractor'
                row['adopted_inputs'] = {'magazine': 1, 'reload_seconds': numeric(state['reloadTime']),
                    'attractor_seconds': numeric(objects[attractor_path]['composed_fields']['LifeSpan'].partition('=')[2]),
                    'projectile_speed_mps': numeric(selected['fields']['KinematicInitialSpeed'].partition('=')[2]),
                    'source_policy': 'parsed primary calibration; M42/M43 retain original observations'}
                for key in ('reload_seconds',):
                    if re.search(r'(?m)^' + key + ':', text):
                        text = replace_field(text, key, row['adopted_inputs'][key], 0)
                for key in ('attractor_seconds', 'projectile_speed_mps'):
                    text = replace_field(text, key, row['adopted_inputs'][key], 2)
                profile = {}
                for aim, key in [('aimed', 'AIMED_ACCURACY'), ('hip', 'HIP_FIRE_ACCURACY')]:
                    raw = objects[fire[key]]['composed_fields']['Spread']
                    shooting = inner(inner(raw.partition('=')[2])['SHOOTING'])
                    limits = [numeric(v) for v in shooting['range'].strip('{}').split(',')]
                    profile[aim] = {'owner': fire[key], 'raw': raw, 'min': limits[0], 'max': limits[1]}
                row['accuracy'] = profile
                sampling = {'horizontal_ratio': numeric(fire['SpreadRatioHorizontal']),
                    'uniform': fire['spreadUniform'] == '1',
                    'hip_min_deg': profile['hip']['min'], 'hip_max_deg': profile['hip']['max']}
                row['sampling'] = sampling
                text = replace_field(text, 'min_deg', profile['aimed']['min'], 4)
                text = replace_field(text, 'max_deg', profile['aimed']['max'], 4)
                text = re.sub(r'(?m)^    sampling:\n(?:      [^\n]+\n)+', '', text)
                block = '    sampling:\n' + ''.join('      ' + k + ': '
                    + (str(v).lower() if isinstance(v, bool) else render(v)) + '\n' for k, v in sampling.items())
                text = re.sub(r'(?m)^(    max_deg: [^\n]*\n)', lambda m: m[1] + block, text)
                text = re.sub(r'(?m)^  - "The Bullet Attractor\x27s value as an aiming aid[^\n]+\n', '', text)
                for reason in ('client_accuracy_runtime', 'client_scourge_field', 'client_scourge_throw'):
                    if 'reason: ' + reason not in text:
                        text = text.replace('\nunmodeled:\n', '\nunmodeled:\n  - reason: ' + reason + '\n', 1)
            if 'client_scourge_inputs' not in text:
                text += '\n# see notes: client_scourge_inputs\n'
            fixture['forms'].append(row)
            changes[path] = original, text
    attractor = '/Lotus/Weapons/Tenno/LongGuns/TnPriestSpear/TnPriestSpearBulletAttractor'
    fixture['attractor'] = {'owner': attractor, 'fields': objects[attractor]['composed_fields']}
    fixture_path = ROOT / 'tests/fixtures/client/scourge-2026-10-28.json'
    changes[fixture_path] = (fixture_path.read_text() if fixture_path.exists() else '', json.dumps(fixture, indent=2) + '\n')
    different = [str(p.relative_to(ROOT)) for p, (a, b) in changes.items() if a != b]
    if args.check:
        if different:
            raise SystemExit('Scourge inputs differ: ' + ', '.join(different))
    else:
        for p, (_, value) in changes.items():
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(value)
    print(json.dumps({'forms': len(fixture['forms']), 'changed_files': different, 'check': args.check}))


if __name__ == '__main__':
    main()
