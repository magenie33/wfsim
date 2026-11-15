#!/usr/bin/env python3
"""Verify client-only callback evidence and import its bounded native oracles."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

PIN = '681cc93ffc168138cc39c70742ab271178316ef54cddda3b97c39b8483b5d2db'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('batch', type=Path)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    raw = (args.batch / 'manifest.json').read_bytes()
    assert sha(raw) == PIN
    assert json.loads((args.batch / 'READY.json').read_text())['manifest_sha256'] == PIN
    manifest = json.loads(raw)
    for entry in manifest['files']:
        data = (args.batch / entry['path']).read_bytes()
        assert len(data) == entry['bytes'] and sha(data) == entry['sha256'], entry['path']
    for name, members in manifest['archive_members'].items():
        with zipfile.ZipFile(args.batch / name) as archive:
            for entry in members:
                data = archive.read(entry['path'])
                assert len(data) == entry['bytes'] and sha(data) == entry['sha256'], entry['path']
    with zipfile.ZipFile(args.batch / 'client-evidence.zip') as archive:
        algorithm = json.loads(archive.read('algorithm.json'))
        replay = json.loads(archive.read('native-replay.json'))
        script_name = 'script/B.Font--fa9f3da7532dd587fe91.annotated.json'
        script_raw = archive.read(script_name)
        script = json.loads(script_raw)
    assert len(script) == 4 and len(replay['counter_cases']) == 16 and len(replay['rng_cases']) == 24
    pcs = algorithm['known_script_pcs']
    callback = {line['pc']: line for line in script[pcs['callback_proto']]['lines']}
    assert 'Engine.DamageData()' in callback[pcs['new_packet']]['annotation']
    assert callback[pcs['set_proc']]['opcode'] == 'CALL'
    assert callback[pcs['apply_packet']]['opcode'] == 'CALL'
    fixture = dict(source_manifest_sha256=PIN, client_build=algorithm['client_build'],
                   script_member=script_name, script_member_sha256=sha(script_raw),
                   contract={name: algorithm[name] for name in ['native_path', 'threshold', 'pairs', 'condition',
                             'callback', 'duplicate_basic_elements', 'multiplicity', 'known_script_pcs', 'unresolved']},
                   counter_cases=replay['counter_cases'], rng_cases=replay['rng_cases'],
                   scope='Original native helper oracles and decoded callback contract; no Phantasma live cadence or full Luau VM execution')
    out = Path(__file__).resolve().parents[1] / 'tests/fixtures/client/primary-debilitate-contract-2026-10-28.json'
    text = json.dumps(fixture, indent=2) + '\n'
    if args.check:
        assert out.read_text() == text
    else:
        out.write_text(text)
    print('verified original batch files/archive members; 4 script prototypes, 16 counter and 24 RNG helper cases')


if __name__ == '__main__':
    main()
