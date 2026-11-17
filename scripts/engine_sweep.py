# SPDX-License-Identifier: AGPL-3.0-or-later
"""Freeze unmodded catalog mode requests from a captured baseline /api/meta."""

import argparse
import hashlib
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--meta', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    raw = args.meta.read_bytes()
    meta = json.loads(raw)
    calls = []
    stances = {mod['id']: mod for pool in meta['mod_pools'].values() for mod in pool if mod['stance']}
    for weapon in meta['weapons']:
        evolutions = [tier['options'][0]['id'] for tier in weapon['evolutions'] if tier['options']]
        for mode in weapon['modes']:
            body = {'weapon': weapon['id'], 'mode': mode, 'mods': [], 'arcane': ['none'],
                    'evolutions': evolutions, 'buffs': {}, 'enemy': 'corrupted_heavy_gunner',
                    'level': 100, 'steel_path': True, 'eximus': False,
                    'duration': 8, 'runs': 2, 'seed': 24301, 'headshot_pct': 0,
                    'infinite_ammo': True, 'player_at': [0, 0], 'target_at': [0, 10]}
            available = [mod for mod in weapon['mods'] if mod in stances]
            combo = 'neutral' if mode == 'base' else mode
            stance = next((mod for mod in available if combo in stances[mod]['stance_combos']),
                          next(iter(available), None))
            if stance and not weapon['fixed_stance']:
                body['mods'] = [stance]
            calls.append({'id': f'{weapon["id"]}:{mode}', 'path': '/api/simulate', 'body': body})
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open('x') as output:
        json.dump(calls, output, indent=2)
        output.write('\n')
    print(json.dumps({'weapons': len(meta['weapons']), 'calls': len(calls),
                      'meta_sha256': hashlib.sha256(raw).hexdigest(),
                      'calls_sha256': hashlib.sha256(args.output.read_bytes()).hexdigest()}))


if __name__ == '__main__':
    main()
