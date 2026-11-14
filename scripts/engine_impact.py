# SPDX-License-Identifier: AGPL-3.0-or-later
"""Summarize strict comparison output, preserving work-only changes explicitly."""

import argparse
import json
from pathlib import Path
import statistics

from engine_compare import first_difference, read_json, write_json


def summarize(folder):
    report = read_json(folder / 'report.json')
    sides = {}
    for observation in report['observations']:
        side = observation['side']
        if side not in sides:
            sides[side] = read_json(folder / observation['result_file'])
    if set(sides) != {'A', 'B'}:
        raise ValueError('both versions must have retained output')
    classes = {'exact': [], 'work_only': [], 'answer': []}
    answers = {}
    for left, right in zip(sides['A'], sides['B']):
        if left['id'] != right['id']:
            raise ValueError('case identity changed')
        before, after = left['result'], right['result']
        strict = first_difference(before, after)
        a, b = before.copy(), after.copy()
        work = {'before': a.pop('work', None), 'after': b.pop('work', None)}
        answer = first_difference(a, b)
        category = 'exact' if strict is None else 'work_only' if answer is None else 'answer'
        classes[category].append(left['id'])
        if strict:
            answers[left['id']] = {'first_strict_difference': strict, 'first_answer_difference': answer, 'work': work}
            for field in ['dps', 'score', 'procs', 'kills', 'shots']:
                if field in before:
                    x, y = before[field], after[field]
                    answers[left['id']][field] = {'before': x, 'after': y,
                        'change_pct': (y / x - 1) * 100 if isinstance(x, (int, float)) and x else None}
    timing = {}
    memory = {}
    for side in ['A', 'B']:
        samples = {}
        rss, linear = [], []
        for observation in report['observations']:
            if observation['side'] != side:
                continue
            if observation.get('peak_rss_bytes') is not None:
                rss.append(observation['peak_rss_bytes'])
            for row in read_json(folder / observation['result_file']):
                samples.setdefault(row['id'], []).extend(row['milliseconds'])
                if row.get('memory_bytes') is not None:
                    linear.append(row['memory_bytes'])
        timing[side] = {key: {'median_ms': statistics.median(values), 'minimum_ms': min(values),
                             'maximum_ms': max(values), 'samples': len(values)} for key, values in samples.items()}
        memory[side] = {'peak_process_rss_bytes': max(rss) if rss else None,
                        'peak_wasm_linear_bytes': max(linear) if linear else None}
    return {'complete': report['complete'], 'strict_exact': report.get('exact'),
            'classification': {key: len(value) for key, value in classes.items()}, 'case_ids': classes,
            'classification_rule': 'Only top-level work is separated for the work_only class; the strict comparison retains every field.',
            'changes': answers, 'timing': timing, 'memory': memory}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folder', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    summary = summarize(args.folder)
    write_json(args.output, summary)
    print(json.dumps({'complete': summary['complete'], 'classification': summary['classification'],
                      'memory': summary['memory']}))


if __name__ == '__main__':
    main()
