# SPDX-License-Identifier: AGPL-3.0-or-later
"""Source-accounting exceptions must not hide a changed combat answer."""

import copy
import math
import json
from pathlib import Path
import subprocess
import sys
import tempfile

from engine_answer_equivalence import answer_difference
from engine_compare import digest, read_json, write_json

base = {'ok': True, 'dps': 100.0, 'work': 10,
        'events': [{'id': 1, 'damage': 12.5}, {'id': 2, 'damage': 3.0}],
        'native_trace': {'steps': [{'output_bits': 1234}]}}
accounting = copy.deepcopy(base)
accounting['work'] = 5
assert answer_difference(base, accounting, '/api/simulate')[0] is not None
assert answer_difference(base, accounting, '/api/simulate', True) == (None, ['/work'])
for mutate in [
    lambda row: row.update(dps=math.nextafter(row['dps'], math.inf)),
    lambda row: row['events'][0].update(damage=12.6),
    lambda row: row['events'].reverse(),
    lambda row: row['native_trace']['steps'][0].update(output_bits=1235),
    lambda row: row.pop('work'),
    lambda row: row.update(work='5'),
    lambda row: row.update(work=False),
    lambda row: row.update(work=-1),
    lambda row: row.update(new_native_field=1),
]:
    changed = copy.deepcopy(accounting)
    mutate(changed)
    assert answer_difference(base, changed, '/api/simulate', True)[0] is not None
assert answer_difference(base, accounting, '/api/log', True)[0] is not None
board = {'ok': True, 'shards': [{'gas': 50, 'score': 7.0, 'run': [0, 12]}]}
changed = copy.deepcopy(board)
changed['shards'][0]['gas'] = 20
assert answer_difference(board, changed, '/api/board/runs', True) == (None, ['/shards/0/gas'])
for mutate in [lambda row: row['shards'][0].update(score=7.1),
               lambda row: row['shards'][0]['run'].reverse(),
               lambda row: row['shards'].append({'gas': 0, 'score': 0, 'run': [0, 1]})]:
    bad = copy.deepcopy(changed)
    mutate(bad)
    assert answer_difference(board, bad, '/api/board/runs', True)[0] is not None
print('Paired source accounting is explicit; one-bit values, damage, event order, native trace bits and shape mutations fail.')

if len(sys.argv) > 1:
    folder = Path(sys.argv[1])
    report = read_json(folder/'report.json')
    calls = read_json(folder/'calls.json')
    observation = next(row for row in report['observations'] if row['side'] == 'A')
    original = read_json(folder/observation['result_file'])[0]
    assert calls[0]['path'] == '/api/board/runs'
    with tempfile.TemporaryDirectory(prefix='wfsim-combat-equivalence-') as temporary:
        root = Path(temporary)
        write_json(root/'calls.json', [calls[0]])
        write_json(root/'A.json', [original])
        changed = copy.deepcopy(original)
        changed['result']['shards'][0]['gas'] += 1
        write_json(root/'B.json', [changed])

        def run():
            observations = [{'side': side, 'result_file': side+'.json',
                'result_sha256': digest((root/(side+'.json')).read_bytes())} for side in ['A', 'B']]
            write_json(root/'report.json', {'complete': True,
                'calls_sha256': digest((root/'calls.json').read_bytes()), 'observations': observations})
            process = subprocess.run([sys.executable, str(Path(__file__).with_name('engine_answer_equivalence.py')),
                str(root), '--allow-source-accounting', '--output', str(root/'equivalence.json')],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            return process.returncode, read_json(root/'equivalence.json')

        assert run()[0] == 0
        value = changed['result']['shards'][0]['by_body'][0]
        changed['result']['shards'][0]['by_body'][0] = math.nextafter(value, math.inf)
        write_json(root/'B.json', [changed])
        code, evidence = run()
        assert code == 1 and not evidence['combat_exact']
        assert evidence['differences'][0]['pointer'] == '/shards/0/by_body/0'
        print(json.dumps({'real_response_control': 'one-ULP body damage rejected',
            'pointer': evidence['differences'][0]['pointer'], 'before': value,
            'after': changed['result']['shards'][0]['by_body'][0]}))
