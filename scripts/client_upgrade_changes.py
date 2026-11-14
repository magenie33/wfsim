# SPDX-License-Identifier: AGPL-3.0-or-later
"""Compare verified client exports and locate affected native inputs and consumers."""

import argparse
import hashlib
import json
import struct
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read_export(folder, pin):
    raw = (folder / 'manifest.json').read_bytes()
    assert digest(raw) == pin == json.loads((folder / 'READY.json').read_bytes())['manifest_sha256']
    manifest = json.loads(raw)
    name = 'evidence.zip'
    ref = next(row for row in manifest['files'] if row['path'] == name)
    data = (folder / name).read_bytes()
    assert len(data) == ref['bytes'] and digest(data) == ref['sha256']
    expected = {row['path']: row for row in manifest['archive_members'][name]}
    captured = {}
    with zipfile.ZipFile(folder / name) as archive:
        for member in ['data/entries.json', 'data/objects.json', 'enums/enum-evidence.json']:
            data = archive.read(member)
            ref = expected[member]
            assert len(data) == ref['bytes'] and digest(data) == ref['sha256']
            captured[member] = json.loads(data)
    return {'pin': pin, 'manifest': manifest,
            'entries': captured['data/entries.json'], 'objects': captured['data/objects.json'],
            'enums': captured['enums/enum-evidence.json'],
            'members': {path: row['sha256'] for path, row in expected.items()},
            'native_spans': {path: row['sha256'] for path, row in expected.items()
                             if path.startswith('native/') and path.endswith(('.asm.txt', '.hex.txt'))}}


def differences(before, after, path=''):
    if type(before) is not type(after):
        return [{'field': path, 'before': before, 'after': after}]
    if isinstance(before, dict):
        result = []
        for key in sorted(before.keys() | after.keys()):
            escaped = str(key).replace('~', '~0').replace('/', '~1')
            child = f'{path}/{escaped}'
            if key not in before:
                result.append({'field': child, 'change': 'added', 'after': after[key]})
            elif key not in after:
                result.append({'field': child, 'change': 'removed', 'before': before[key]})
            else:
                result.extend(differences(before[key], after[key], child))
        return result
    if isinstance(before, list):
        result = []
        for index in range(max(len(before), len(after))):
            child = f'{path}/{index}'
            if index >= len(before):
                result.append({'field': child, 'change': 'added', 'after': after[index]})
            elif index >= len(after):
                result.append({'field': child, 'change': 'removed', 'before': before[index]})
            else:
                result.extend(differences(before[index], after[index], child))
        return result
    same = struct.pack('>d', before) == struct.pack('>d', after) if isinstance(before, float) else before == after
    return [] if same else [{'field': path, 'before': before, 'after': after}]


def index_unique(rows, key):
    result = {}
    for row in rows:
        identity = key(row)
        assert identity not in result, identity
        result[identity] = row
    return result


def compare(before, after, bindings, attributes=()):
    previous = index_unique(before['objects'], lambda row: row['path'])
    current = index_unique(after['objects'], lambda row: row['path'])
    dependents = {}
    for binding in bindings:
        owner = binding['source']['owner']
        ancestry = (set(previous.get(owner, {}).get('ancestry_leaf_first', [owner]))
                    | set(current.get(owner, {}).get('ancestry_leaf_first', [owner])) | {owner})
        for path in ancestry:
            dependents.setdefault(path, []).append({'id': binding['id'], 'path': binding['path'],
                                                    'effect_index': binding['effect_index']})
    entries_before = index_unique(before['entries'], lambda row: (row['owner'], row['array'], row['index']))
    entries_after = index_unique(after['entries'], lambda row: (row['owner'], row['array'], row['index']))
    rule_consumers = {}
    for attribute in attributes:
        for key in (attribute['native_name'], str(attribute['UpgradeType'])):
            rule_consumers[key] = attribute
    changes = []
    for key in sorted(entries_before.keys() | entries_after.keys()):
        delta = differences(entries_before.get(key), entries_after.get(key))
        if delta:
            linked = {}
            for row in (entries_before.get(key, {}), entries_after.get(key, {})):
                kind = row.get('fields', {}).get('UpgradeType')
                rule = rule_consumers.get(str(kind))
                if rule:
                    linked[rule['UpgradeType']] = rule
            changes.append({'owner': key[0], 'array': key[1], 'index': key[2], 'changes': delta,
                            'engine_consumers': dependents.get(key[0], []),
                            'rule_consumers': [linked[kind] for kind in sorted(linked)]})
    object_changes = []
    for path in sorted(previous.keys() | current.keys()):
        delta = differences(previous.get(path), current.get(path))
        if delta:
            object_changes.append({'owner': path, 'changes': delta, 'engine_consumers': dependents.get(path, [])})
    return {'entry_changes': changes, 'object_changes': object_changes,
            'enum_changes': differences(before['enums'], after['enums']),
            'artifact_member_changes': differences(before.get('members', {}), after.get('members', {})),
            'native_span_changes': differences(before['native_spans'], after['native_spans']),
            'client_identity_changes': differences(
                {key: before['manifest'].get(key) for key in ['client_build', 'inputs']},
                {key: after['manifest'].get(key) for key in ['client_build', 'inputs']}),
            'scope': 'Every entry/object/enum field and native span hash participates. Consumer links cover the migrated binding registry; an unlinked change is not declared irrelevant. Native span changes require rule rebinding.'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for side in ['before', 'after']:
        parser.add_argument('--' + side, type=Path, required=True)
        parser.add_argument('--' + side + '-sha256', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    assert not args.output.exists(), str(args.output)
    before = read_export(args.before, args.before_sha256)
    after = read_export(args.after, args.after_sha256)
    fixture = ROOT / 'tests/fixtures/client/upgrade-ranks-2026-10-28.json'
    bindings = json.loads(fixture.read_text())['mod_inputs']
    consumer_path = ROOT / 'tests/fixtures/client/native-consumers.json'
    consumers = json.loads(consumer_path.read_text())['attributes']
    for consumer in consumers:
        assert all((ROOT / path).is_file() for path in consumer['consumers'] + [consumer['fixture']]), consumer
    result = {'rule_registry_sha256': digest(consumer_path.read_bytes()), 'before_sha256': before['pin'], 'after_sha256': after['pin'],
              'binding_registry_sha256': digest(fixture.read_bytes()), **compare(before, after, bindings, consumers)}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({key: len(value) for key, value in result.items() if key.endswith('_changes')}))


if __name__ == '__main__':
    main()
