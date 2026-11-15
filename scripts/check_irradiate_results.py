#!/usr/bin/env python3
"""Check the bounded Irradiate migration against retained complete API results."""
import argparse
from pathlib import Path

from engine_answer_equivalence import answer_difference
from engine_compare import digest, load_rows, read_json, write_json


def check(calls_path, before_path, after_path):
    calls = read_json(calls_path)
    before = {row['id']: row['result'] for row in load_rows(before_path, calls)}
    after = {row['id']: row['result'] for row in load_rows(after_path, calls)}
    prefixes = ['cortege_base_one', 'cortege_base_two',
                'dual_toxocyst_incarnon_one', 'tenet_cycron_base_one']
    expected = {f'{name}_{endpoint}' for name in prefixes for endpoint in ['simulate', 'log']}
    changed = set()
    deltas = []
    echoes = []
    for call in calls:
        name, endpoint = call['id'], call['path']
        a, b = before[name], after[name]
        difference, _ = answer_difference(a, b, endpoint, allow_source_accounting=True)
        if difference is not None:
            changed.add(name)
        if name in expected and endpoint == '/api/simulate':
            assert b['dps'] > a['dps'], f'{name}: expected additional echo damage'
            deltas.append(dict(id=name, before_dps=a['dps'], after_dps=b['dps'],
                               change_pct=(b['dps'] / a['dps'] - 1) * 100))
        if endpoint == '/api/log':
            old = [e for e in a['events'] if e.get('origin') == 'echo' and e.get('body') == 0]
            new = [e for e in b['events'] if e.get('origin') == 'echo' and e.get('body') == 0]
            if name in expected:
                assert not old and new, f'{name}: other victims must echo back to the aimed body'
                echoes.append(dict(id=name, old_aimed_echoes=len(old), new_aimed_echoes=len(new)))
            if '_none_' in name:
                assert not any(e.get('origin') == 'echo' for e in b['events']), name
    assert changed == expected, f'unexpected changed response set: {sorted(changed ^ expected)}'
    return dict(complete=True, calls_sha256=digest(calls_path.read_bytes()),
                before_sha256=digest(before_path.read_bytes()), after_sha256=digest(after_path.read_bytes()),
                cases=len(calls), preserved_answers=len(calls) - len(changed),
                expected_changed=sorted(changed), dps=deltas, aimed_echoes=echoes,
                scope='Scene regression comparison; not an in-game damage oracle or native scheduler certification')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--calls', type=Path, default=Path('tests/fixtures/engine-compare/irradiate-calls.json'))
    parser.add_argument('--before', type=Path, required=True)
    parser.add_argument('--after', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = check(args.calls, args.before, args.after)
    write_json(args.output, result)
    print(f"{result['cases']} cases: {result['preserved_answers']} preserved, {len(result['expected_changed'])} expected changes")


if __name__ == '__main__':
    main()
