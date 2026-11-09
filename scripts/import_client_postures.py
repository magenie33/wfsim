#!/usr/bin/env python3
"""Import beast posture callbacks from a verified closed client batch."""
import argparse
import json
from pathlib import Path
import struct

import yaml

from import_client_cadence import ROOT, fields, source, verify_closed_batch


def f32(value):
    return struct.unpack('<f', struct.pack('<f', value))[0]


def import_postures(exchange, check=False):
    batch = exchange / 'batches/20261108T111018Z-companion-category-source-review'
    manifest = verify_closed_batch(batch)
    names = ['raw-ancestry.json', 'review/postures.json', 'review/posture-formula-cases.json',
             'review/posture-callback-contexts.json', 'review/reviewed-script-slices.json',
             'review/upgrade-enum-evidence.json']
    payload, binding = source(batch, 'companion-evidence.zip', names)
    raw = {r['path']: r for r in json.loads(payload['raw-ancestry.json'])}
    postures = {r['name_en']: r for r in json.loads(payload['review/postures.json'])}
    cases = json.loads(payload['review/posture-formula-cases.json'])
    callbacks = json.loads(payload['review/posture-callback-contexts.json'])
    weapon = next(r for r in callbacks if r['owner'] == '/Lotus/Types/Friendly/Pets/BeastPetMeleeWeaponBase'
                  and r['top_level_field'] == 'DamageDoneScript')
    assert weapon['nearest_explicit_member_view']['_amountPerLevel'] == '1'
    scripts = [r for r in json.loads(payload['review/reviewed-script-slices.json'])
               if r['script'].endswith('/PostureMods.lua')]
    assert len(scripts) == 23 and len(cases) == 16
    for r in cases:
        p = next(p for p in postures.values() if p['path'] == r['path'])
        coefficient = p['runtime_apply_parameters']['amountPerLevel']
        sign = -1 if p['name_en'] == 'Elusive Posture' else 1
        value = f32(sign * coefficient * r['callback_level'])
        assert r['callback_level'] == r['rank'] + 1
        assert struct.pack('<f', coefficient).hex() == r['coefficient_bits_le']
        assert value == r['native_scalar_argument_float32']
        assert struct.pack('<f', value).hex() == r['native_scalar_bits_le']
    intents = {'Assassin Posture': 'eximus', 'Balanced Posture': 'free', 'Elusive Posture': 'avoid',
               'Frenzied Posture': 'different', 'Persistent Posture': 'same', 'Protector Posture': 'near_owner'}
    kinds = {'Assassin Posture': 'overguard_damage_bonus', 'Frenzied Posture': 'status_duration_bonus',
             'Persistent Posture': 'base_damage_added', 'Protector Posture': 'additional_forced_procs'}
    rows = []

    def output(path, value):
        text = json.dumps(value, indent=2) + '\n' if path.suffix == '.json' else yaml.safe_dump(value, sort_keys=False, allow_unicode=True)
        if check:
            assert path.read_text() == text, str(path.relative_to(ROOT)) + ' differs from verified client input'
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)

    for path in sorted((ROOT / 'data/mods/beast_claws').glob('*_posture.yaml')):
        card = yaml.safe_load(path.read_text())
        name = card['name']
        p = postures[name]
        assert card['internal_name'] == p['path'] and card['max_rank'] == 3
        own = fields(raw[p['path']]['ownText'])
        apply = fields(own['Script'].strip('{}\n'))
        coefficient = p['runtime_apply_parameters']['amountPerLevel']
        assert f32(float(apply['_amountPerLevel'])) == coefficient
        assert apply['Function'] == p['apply_function']
        target = {'kind': 'target_preference', 'preference': intents[name]}
        if name == 'Protector Posture':
            target['range_m'] = p['runtime_apply_parameters']['range']
        effects = [target]
        if name in kinds:
            values = ([float(i + 1) for i in range(4)] if name == 'Protector Posture' else
                      [f32(coefficient * (i + 1)) for i in range(4)])
            effect = {'kind': kinds[name], 'rank0': values[0], 'rankMax': values[-1], 'rank_values': values}
            if name in ('Persistent Posture', 'Protector Posture'):
                effect['element'] = 'impact' if name == 'Persistent Posture' else 'puncture'
            effects.append(effect)
        if name == 'Balanced Posture':
            effects.append({'kind': 'unmodelled', 'rank0': coefficient, 'rankMax': coefficient * 4,
                            'clause': 'Moving-only stagger within 0.5..2m is not simulated; companion movement is not run.'})
        if name == 'Elusive Posture':
            assert '_amountPerLevel=0.125' in own['Upgrades']
            effects.append({'kind': 'unmodelled', 'rank0': coefficient / 100, 'rankMax': coefficient / 25,
                            'clause': 'Incoming NPC bullet evasion is not simulated; enemies do not attack the companion.'})
        effects.append({'kind': 'unmodelled', 'clause': 'Native target scoring, other VIP classes, pathfinding, travel and retaliation are not simulated. Target intent uses a deterministic contact model.'})
        card['effects'] = effects
        output(path, card)
        rows.append({'id': card['id'], 'path': p['path'], 'raw_own_text': raw[p['path']]['ownText'],
                     'apply': p, 'effects': effects})
    fixture = {'client_build': manifest['client_build'], 'source': binding, 'cards': rows,
               'weapon_damage_callback': weapon, 'script_slices': scripts, 'scalar_cases': cases,
               'assumptions': ['Contact is assumed after approach; travel time is not added.',
                   'Target intent uses deterministic rank, nearest distance and body-index ties.',
                   'Protector uses the arena player position as the owner position.',
                   'Eximus preference recognizes the arena Eximus flag; other native VIP classes remain open.',
                   'Balanced moving stagger and Elusive incoming-fire evasion remain outside the static offensive fight.']}
    output(ROOT / 'tests/fixtures/client/postures-2026-10-28.json', fixture)
    return {'postures': len(rows), 'scalar_cases': len(cases), 'manifest_sha256': binding['manifest_sha256']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exchange', type=Path, required=True)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    print(json.dumps(import_postures(args.exchange, args.check)))


if __name__ == '__main__':
    main()
