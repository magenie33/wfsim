# SPDX-License-Identifier: AGPL-3.0-or-later
"""Measure saved one_fight binaries in ABBA order after builds have finished."""

import argparse
from pathlib import Path
import statistics

from engine_compare import digest, execute, load_snapshot, write_json


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--rounds', type=int, default=2)
    parser.add_argument('--runs', type=int, default=1000)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    snapshots = {'A': args.baseline.resolve(), 'B': args.candidate.resolve()}
    report = {'complete': False, 'runs': args.runs, 'repeats': 3, 'duration': 180,
              'seed': 24301, 'order': [], 'observations': [], 'snapshots': {},
              'tool_sha256': digest(Path(__file__).read_bytes())}
    for side, folder in snapshots.items():
        report['snapshots'][side] = load_snapshot(folder)[0]
    write_json(output / 'report.json', report)
    for index, side in enumerate('ABBA' * args.rounds):
        manifest, _ = load_snapshot(snapshots[side])
        artifact = manifest['executables']['one_fight']
        binary = snapshots[side] / artifact['file']
        label = f'{index:03}-{side}'
        metrics = execute([str(binary), 'raw', f'runs={args.runs}', 'repeats=3',
                           'duration=180', 'seed=24301'], output, label)
        if digest(binary.read_bytes()) != artifact['sha256']:
            raise ValueError('changed benchmark binary')
        rows = {}
        for line in (output / f'{label}.stdout').read_text().splitlines():
            parts = line.split('\t')
            if parts[0] == 'RAW':
                rows[parts[1]] = {'milliseconds_per_run': float(parts[2]), 'ns_per_shot': float(parts[3])}
        if len(rows) != 4:
            raise ValueError('four complete workload shapes required')
        report['order'].append(side)
        report['observations'].append({'side': side, 'shapes': rows, **metrics})
        write_json(output / 'report.json', report)
        print(label, flush=True)
    summary = {}
    for weapon in report['observations'][0]['shapes']:
        times = {side: [row['shapes'][weapon]['milliseconds_per_run'] for row in report['observations'] if row['side'] == side] for side in snapshots}
        before, after = statistics.median(times['A']), statistics.median(times['B'])
        summary[weapon] = {'before_ms': before, 'after_ms': after,
                           'change_pct': (after / before - 1) * 100, 'samples': times}
    report['summary'] = summary
    report['complete'] = True
    write_json(output / 'report.json', report)
    print(summary)


if __name__ == '__main__':
    main()
