# SPDX-License-Identifier: AGPL-3.0-or-later
"""Run the actual fold source and prove original-instruction vectors reject a mutation."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    source = ROOT / 'engine/src/rules/upgrades'
    dependencies = ROOT / 'target/release/deps'
    extern = []
    for name in ['serde', 'serde_json']:
        library = max(dependencies.glob(f'lib{name}-*.rlib'), key=lambda path: path.stat().st_mtime)
        extern += ['--extern', f'{name}={library}']
    receipt = {'complete': False, 'instrumentation': 'Only metering attributes removed in isolated copies',
               'source_sha256': {name: hashlib.sha256((source / name).read_bytes()).hexdigest()
                                  for name in ['mod.rs', 'ids.rs', 'tests.rs']}, 'runs': []}
    for mutated in [False, True]:
        folder = args.output / ('mutant' if mutated else 'original')
        folder.mkdir()
        for name in ['mod.rs', 'ids.rs', 'tests.rs']:
            text = (source / name).read_text().replace('#[wfsim_gas_macro::metered]\n', '')
            text = text.replace('../../../../tests/fixtures/client/upgrade-operations-2026-10-28.json',
                                str(ROOT / 'tests/fixtures/client/upgrade-operations-2026-10-28.json'))
            if mutated and name == 'mod.rs':
                expected = 'let added = scaled + self.add;'
                assert text.count(expected) == 1
                text = text.replace(expected, 'let added = scaled - self.add;')
            (folder / name).write_text(text)
        binary = folder / 'probe'
        command = ['rustc', '--test', '--edition=2021', '-O', '-L', f'dependency={dependencies}',
                   *extern, str(folder / 'mod.rs'), '-o', str(binary)]
        build = subprocess.run(command, capture_output=True)
        (folder / 'build.stderr').write_bytes(build.stderr)
        assert build.returncode == 0, f'see {folder}/build.stderr'
        result = subprocess.run([str(binary)], capture_output=True)
        (folder / 'test.stdout').write_bytes(result.stdout)
        (folder / 'test.stderr').write_bytes(result.stderr)
        assert (result.returncode != 0) == mutated
        if mutated:
            assert b'native vector ' in result.stdout
        receipt['runs'].append({'mutated': mutated, 'command': command, 'exit': result.returncode,
                                'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest()})
    receipt['complete'] = True
    (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print('Original fold passes; changed ADD arithmetic fails the original-instruction vectors.')


if __name__ == '__main__':
    main()
