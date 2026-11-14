# SPDX-License-Identifier: AGPL-3.0-or-later
"""Import pinned ordinary continuous-rate and FIRE/refire replay vectors."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[1]
BATCH = '20261111T063217Z-coda-bubonico-spool-client-only'
PIN = '285234fac1d5c352dff6a479a34d91ba92d075bf3e4bf4f0a23f62f9e82ab27d'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exchange', type=Path, required=True)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    folder = args.exchange / 'batches' / BATCH
    raw = (folder / 'manifest.json').read_bytes()
    assert hashlib.sha256(raw).hexdigest() == PIN == json.loads((folder / 'READY.json').read_bytes())['manifest_sha256']
    manifest = json.loads(raw)
    bindings = []
    for name in ('client-evidence.zip', 'source.zip'):
        raw = (folder / name).read_bytes()
        ref = next(row for row in manifest['files'] if row['path'] == name)
        assert len(raw) == ref['bytes'] and hashlib.sha256(raw).hexdigest() == ref['sha256']
        with zipfile.ZipFile(folder / name) as archive:
            for ref in manifest['archive_members'][name]:
                raw = archive.read(ref['path'])
                assert len(raw) == ref['bytes'] and hashlib.sha256(raw).hexdigest() == ref['sha256']
                bindings.append({'archive': name, **ref})
            if name == 'client-evidence.zip':
                vectors = json.loads(archive.read('native-replay.json'))
            else:
                report = json.loads(archive.read('reports/coda-bubonico-spool-2026-11-11.json'))
    fixture = {'schema': 1, 'batch': BATCH, 'manifest_sha256': PIN, 'client_build': manifest['client_build'],
               'source_members': bindings, 'declared_state_fields': report['declared_state_fields'],
               'scope': 'Ordinary zero-duration automatic FIRE entry and sampled refire rate; emission/frame scheduling and full battery/holster lifecycle remain separate',
               **{key: vectors[key] for key in ['successive_fire_entries', 'rate_vectors', 'increment_vectors', 'fixed_fire_iteration_vectors']}}
    output = json.dumps(fixture, indent=2) + '\n'
    path = ROOT / 'tests/fixtures/client/continuous-rate-2026-10-28.json'
    if args.check:
        assert path.read_text() == output, str(path)
    else:
        path.write_text(output)
    print(f'Verified {len(bindings)} source members, 84 rate and 8 coupled FIRE/refire vectors')


if __name__ == '__main__':
    main()
