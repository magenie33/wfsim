#!/usr/bin/env python3
"""Bind Kitgun shot counts and beam-loader status budgets to closed client inputs."""
import argparse
import hashlib
import json
from pathlib import Path

import yaml

from import_primary_calibration import (FIXTURES, OFFLINE, PINS, ROOT, ROSTER, blocks,
    inner, roles, source, supplement)

CLIPS = '/Lotus/Weapons/SolarisUnited/Secondary/SUModularSecondarySet1/Clip/'
INFESTED = '/Lotus/Weapons/Infested/Pistols/InfKitGun/Clips/'
LOADERS = {
    'arcroid': INFESTED + 'InfSmallClip/InfModularClipSmallCritStatPart',
    'bashrack': CLIPS + 'SUModularCritICapIClipPart',
    'bellows': CLIPS + 'SUModularCapIIClipPart',
    'deepbreath': CLIPS + 'SUModularCapIClipPart',
    'flutterfire': CLIPS + 'SUModularStatIIReloadIClipPart',
    'killstream': CLIPS + 'SUModularCritIIReloadIClipPart',
    'macro_arcroid': INFESTED + 'InfClipBig/InfModularClipBigPart',
    'macro_thymoid': INFESTED + 'InfClipBig/InfModularClipBigCritStatPart',
    'ramflare': CLIPS + 'SUModularStatIICapIClipPart',
    'slap': CLIPS + 'SUModularReloadIClipPart',
    'slapneedle': CLIPS + 'SUModularCritIReloadIClipPart',
    'sparkfire': CLIPS + 'SUModularStatICapIClipPart',
    'splat': CLIPS + 'SUModularCritIICapIClipPart',
    'stitch': CLIPS + 'SUModularCritICapIIClipPart',
    'swiftfire': CLIPS + 'SUModularStatIReloadIClipPart',
    'thunderdrum': CLIPS + 'SUModularStatICapIIClipPart',
    'thymoid': INFESTED + 'InfSmallClip/InfModularClipSmallPart',
    'zip': CLIPS + 'SUModularReloadIIClipPart',
    'zipfire': CLIPS + 'SUModularStatIReloadIIClipPart',
    'zipneedle': CLIPS + 'SUModularCritIReloadIIClipPart',
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exchange', type=Path, required=True)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    batches = args.exchange / 'batches'
    assert hashlib.sha256((batches / ROSTER / 'manifest.json').read_bytes()).hexdigest() == PINS[ROSTER]
    payload, binding = source(batches / ROSTER, 'weapon-roster-dossiers.zip',
        ['shared/objects.jsonl'])
    objects = {r['path']: r for line in payload['shared/objects.jsonl'].splitlines()
        if (r := json.loads(line))}
    parts, receipt = supplement(batches / OFFLINE)
    parts = {r['path']: r for r in parts}
    barrels = []
    for path in sorted((ROOT / 'data/kitguns/chambers').glob('*.yaml')):
        chamber = yaml.safe_load(path.read_text())
        weapon = yaml.safe_load((ROOT / 'data/weapons' / chamber['slot'] /
            (chamber['id'] + '.yaml')).read_text())
        owner = weapon['internal_name']
        key = 'PrimaryBehaviorOverrides' if chamber['slot'] == 'primary' else 'BehaviorOverrides'
        typed, _ = roles(objects[owner]['composed_fields'][key].partition('=')[2].strip()[1:-1])
        barrels.append({'id': chamber['id'], 'owner': owner, 'override': key,
            'fire_iterations': typed['fire']['fireIterations'],
            'gun_type': parts[owner]['fields'].get('GunType', '').partition('=')[2]})
    loaders = []
    for identity, path in LOADERS.items():
        overrides = parts[path]['fields']['Overrides'].partition('=')[2]
        rows = [inner('{' + raw + '}') for raw in blocks(overrides)]
        beam = [row for row in rows if row['GunType'] == 'GT_BEAM']
        assert len(beam) == 1, path
        loaders.append({'id': identity, 'path': path, 'beam': beam[0]})
    expected = {r['id'] for r in yaml.safe_load((ROOT / 'data/kitguns/loaders.yaml').read_text())['loaders']}
    assert LOADERS.keys() == expected
    fixture = {'source': binding, 'parts_source': receipt, 'barrels': barrels, 'loaders': loaders}
    text = json.dumps(fixture, indent=2) + '\n'
    path = FIXTURES / 'kitgun-status-2026-10-28.json'
    if args.check:
        assert path.read_text() == text, 'Kitgun status fixture differs from parsed inputs'
    else:
        path.write_text(text)
    print(json.dumps({'chambers': len(barrels), 'loaders': len(loaders), 'check': args.check}))


if __name__ == '__main__':
    main()
