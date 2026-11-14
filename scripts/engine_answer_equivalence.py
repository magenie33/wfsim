# SPDX-License-Identifier: AGPL-3.0-or-later
"""Verify retained combat responses while naming source-accounting changes explicitly."""

import argparse
import copy
from pathlib import Path

from engine_compare import digest, first_difference, load_rows, read_json, write_json


def answer_difference(before, after, endpoint, allow_source_accounting=False):
    left, right = copy.deepcopy(before), copy.deepcopy(after)
    accounting = []

    def remove_pair(a, b, key, pointer):
        if key in a and key in b and type(a[key]) is int and type(b[key]) is int and a[key] >= 0 and b[key] >= 0:
            if first_difference(a[key], b[key]) is not None:
                accounting.append(pointer)
            del a[key]
            del b[key]

    if allow_source_accounting:
        if endpoint == '/api/simulate':
            remove_pair(left, right, 'work', '/work')
        elif endpoint == '/api/board/runs':
            for index, (a, b) in enumerate(zip(left.get('shards', []), right.get('shards', []))):
                remove_pair(a, b, 'gas', f'/shards/{index}/gas')
    return first_difference(left, right), accounting


def verify(folder, calls_path, allow_source_accounting=False):
    report = read_json(folder/'report.json')
    if not report['complete']:
        raise ValueError('incomplete comparison')
    calls = read_json(calls_path)
    if digest(calls_path.read_bytes()) != report['calls_sha256']:
        raise ValueError('comparison input hash changed')
    paths = {row['id']: row['path'] for row in calls}
    if len(paths) != len(calls):
        raise ValueError('duplicate case identity')
    observations = report['observations']
    if {row['side'] for row in observations} != {'A', 'B'}:
        raise ValueError('both variants must have retained responses')
    first = next(row for row in observations if row['side'] == 'A')

    def rows(observation):
        path = folder/observation['result_file']
        if digest(path.read_bytes()) != observation['result_sha256']:
            raise ValueError('response hash changed')
        return {row['id']: row['result'] for row in load_rows(path, calls)}

    reference = rows(first)
    differences, accounting_changes = [], []
    checked, strict_changes = 0, 0
    for observation in observations:
        current = rows(observation)
        for identity, before in reference.items():
            after = current[identity]
            strict_changes += first_difference(before, after) is not None
            difference, accounting = answer_difference(before, after, paths[identity], allow_source_accounting)
            checked += 1
            if difference is not None:
                differences.append({'case': identity, 'result_file': observation['result_file'], **difference})
            if accounting:
                accounting_changes.append({'case': identity, 'result_file': observation['result_file'], 'paths': accounting})
    return {'complete': True, 'combat_exact': not differences, 'responses_verified': checked,
            'strict_response_changes': strict_changes,
            'allow_source_accounting': allow_source_accounting,
            'policy': 'Only paired nonnegative integer /api/simulate /work and /api/board/runs /shards/<index>/gas values may differ; missing fields, types and all other data remain strict.',
            'differences': differences, 'accounting_changes': accounting_changes}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('comparison', type=Path)
    parser.add_argument('--calls', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--allow-source-accounting', action='store_true')
    args = parser.parse_args()
    result = verify(args.comparison, args.calls or args.comparison/'calls.json', args.allow_source_accounting)
    write_json(args.output, result)
    print({key: result[key] for key in ['combat_exact', 'responses_verified', 'allow_source_accounting']})
    return 0 if result['combat_exact'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
