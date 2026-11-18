# SPDX-License-Identifier: AGPL-3.0-or-later
"""ABBA comparison for two frozen simulation_cost executables on a popular-weapon matrix."""
import json
from pathlib import Path
import statistics
import subprocess
import sys

from engine_compare import first_difference, load_rows, read_json


def main(directory, before, after):
    calls = read_json(directory / 'fights.json')
    reference = None
    samples = {call['id']: {'A': [], 'B': []} for call in calls}
    for i, side in enumerate('ABBA'):
        dest = directory / f'native-{side}{i}.json'
        with dest.with_suffix('.log').open('w', encoding='utf-8') as log:
            subprocess.run([before if side == 'A' else after, directory / 'fights.json', dest, '2'], check=True, stderr=log)
        rows = load_rows(dest, calls)
        if reference is None:
            reference = [r['result'] for r in rows]
        for j, row in enumerate(rows):
            diff = first_difference(reference[j], row['result'])
            if diff is not None:
                raise ValueError((row['id'], diff))
            samples[row['id']][side].extend(row['milliseconds'])
        print(side, i, len(rows), flush=True)
    out = [{'id': c['id'], 'msA': statistics.median(samples[c['id']]['A']) / c['body']['count'],
            'msB': statistics.median(samples[c['id']]['B']) / c['body']['count'], 'times': samples[c['id']]} for c in calls]
    (directory / 'native-comparison.json').write_text(json.dumps(out), encoding='utf-8')


if __name__ == '__main__':
    main(*(Path(p).resolve() for p in sys.argv[1:]))
