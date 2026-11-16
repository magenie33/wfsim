#!/usr/bin/env python3
"""Bind weapon WeaknessBonus to the closed native/package ancestry inventory."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import zipfile

import yaml

from import_client_cadence import canonical, fields

ROOT = Path(__file__).resolve().parents[1]
BATCH = '20261116T025053Z-weapon-innate-weakness'
PIN = '3a564b0e1b4a42655e7f1a25501b7a98d4057c1daaef857395bf1dbdf5c5746f'
FIXTURE = ROOT / 'tests/fixtures/client/weapon-weakness-2026-10-28.json'


def verified(folder):
    raw = (folder / 'manifest.json').read_bytes()
    assert hashlib.sha256(raw).hexdigest() == PIN
    assert json.loads((folder / 'READY.json').read_text())['manifest_sha256'] == PIN
    manifest = json.loads(raw)

    def check(stream, entry):
        digest = hashlib.sha256()
        size = 0
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
            size += len(chunk)
        assert size == entry['bytes'] and digest.hexdigest() == entry['sha256'], entry['path']

    for entry in manifest['files']:
        with (folder / entry['path']).open('rb') as stream:
            check(stream, entry)
    for archive_name, entries in manifest['archive_members'].items():
        with zipfile.ZipFile(folder / archive_name) as archive:
            assert sorted(e['path'] for e in entries) == sorted(
                e.filename for e in archive.infolist() if not e.is_dir())
            for entry in entries:
                with archive.open(entry['path']) as stream:
                    check(stream, entry)
    return manifest


def inventory(folder):
    verified(folder)
    document = json.loads((folder / 'weapon-weakness.json').read_text())
    objects = document['objects']
    with zipfile.ZipFile(folder / 'client-evidence.zip') as archive:
        ancestry = [json.loads(line) for line in archive.read('raw-ancestry.jsonl').splitlines()]
        native = json.loads(archive.read('native-evidence.json'))
        types = json.loads(archive.read('native-types.json'))['types']
    assert len(ancestry) == len({r['path'] for r in ancestry})
    raw = {r['path']: r for r in ancestry}
    for row in types:
        assert raw[row['path']]['native_type'] == row, row['path']
    parents, declarations = {}, {}
    for path, row in raw.items():
        assert len(row['records']) <= 1 and len(row['own_text_records']) <= 1, path
        package_parent = row['records'][0]['parent'] if row['records'] else ''
        native_parent = (row['native_type'] or {}).get('parent', '')
        assert not package_parent or not native_parent or package_parent == native_parent, path
        parents[path] = package_parent or native_parent
        if row['own_text_records']:
            token = fields(row['own_text_records'][0]['ownText']).get('WeaknessBonus')
            if token is not None:
                declarations[path] = token
    for path, row in objects.items():
        chain, current = [], path
        while current:
            assert current not in chain and current in raw, current
            chain.append(current)
            current = parents[current]
        assert chain == row['ancestry_leaf_first'], path
        origin = next((p for p in chain if p in declarations), None)
        if origin:
            token, state = declarations[origin], 'declared'
        elif '/Lotus/Types/Game/LotusWeapon' in chain:
            token, origin, state = '0', '0xf1a863', 'native-constructor-default'
        else:
            assert row['weakness_bonus_f32'] is None, path
            continue
        bits = struct.pack('<f', float(token)).hex()
        assert (token, origin, state, bits) == (
            row['weakness_bonus_token'], row['declared_by_or_initializer_rva'],
            row['value_state'], row['weakness_bonus_f32_le_hex']), path
    return document, native, types


def weapon_files():
    result = {}
    for path in sorted((ROOT / 'data/weapons').rglob('*.yaml')):
        doc = yaml.safe_load(path.read_text())
        if isinstance(doc, dict) and doc.get('id'):
            assert doc['id'] not in result, doc['id']
            result[doc['id']] = (path, doc)
    return result


def metadata(weapons, ident, key, seen=()):
    assert ident not in seen, ident
    doc = weapons[ident][1]
    if key in doc:
        return doc[key]
    parent = doc.get('inherits')
    return metadata(weapons, parent, key, (*seen, ident)) if parent in weapons else None


def weapon_owner(weapons, ident, seen=()):
    assert ident not in seen, ident
    owner = metadata(weapons, ident, 'internal_name')
    if owner:
        return owner
    parent = weapons[ident][1].get('transforms_from')
    return weapon_owner(weapons, parent, (*seen, ident)) if parent in weapons else None


def resolved(weapons, ident):
    value = metadata(weapons, ident, 'weakness_bonus')
    if value is None:
        parent = weapons[ident][1].get('transforms_from')
        if parent in weapons:
            value = metadata(weapons, parent, 'weakness_bonus')
    return value


def render(folder, install):
    source, native, types = inventory(folder)
    objects, weapons = source['objects'], weapon_files()
    forms, owners, unbound, changed = [], {}, [], []
    for ident, (path, doc) in weapons.items():
        owner = weapon_owner(weapons, ident)
        row = objects.get(owner)
        if row is None or row['weakness_bonus_f32'] is None:
            unbound.append(dict(id=ident, owner=owner, reason='source-owner-not-in-inventory'))
            continue
        assert row['scope_class'] == 'weapon-or-gear', ident
        value = canonical(row['weakness_bonus_token'])
        forms.append(dict(id=ident, owner=owner))
        owners[owner] = {k: row[k] for k in ['value_state', 'weakness_bonus_token',
            'weakness_bonus_f32_le_hex', 'declared_by_or_initializer_rva',
            'ancestry_leaf_first', 'parent_edges', 'ignore_headshot_bonus_declarations']}
        owners[owner]['value'] = value
        parent = doc.get('inherits') or doc.get('transforms_from')
        parent_owner = weapon_owner(weapons, parent) if parent in weapons else None
        parent_row = objects.get(parent_owner)
        inherited = parent_row is not None and parent_row['weakness_bonus_f32_le_hex'] == row['weakness_bonus_f32_le_hex']
        if install and not inherited and resolved(weapons, ident) != value:
            text = path.read_text()
            text = re.sub(r'^weakness_bonus:.*\n', '', text, flags=re.M)
            text, count = re.subn(r'(^internal_name:.*\n)',
                lambda m: m[1] + 'weakness_bonus: ' + str(value) + '\n', text, count=1, flags=re.M)
            assert count == 1, ident
            path.write_text(text)
            doc['weakness_bonus'] = value
            changed.append(ident)
    fixture = dict(client_build=source['client_build'], source=dict(batch=BATCH, manifest_sha256=PIN),
        field_offset=native['field_offset'], native_types=types,
        getter_cases=native['getter_cases'], initializer_cases=native['initializer_cases'],
        owners=dict(sorted(owners.items())), forms=forms, unbound_forms=unbound,
        unbound_native_classes=sorted(p for p, r in objects.items() if r['weakness_bonus_f32'] is None),
        scope='Weapon scalar inputs only; existing form adapters retained. Impact/radial/script gates are not inferred from this inventory.')
    return fixture, weapons, changed


def check_data(fixture, weapons):
    for form in fixture['forms']:
        value = resolved(weapons, form['id'])
        expected = fixture['owners'][form['owner']]['value']
        assert value == expected, (form['id'], value, expected)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exchange', type=Path, required=True)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--fixture-only', action='store_true')
    args = parser.parse_args()
    fixture, weapons, changed = render(args.exchange / 'batches' / BATCH,
        install=not args.check and not args.fixture_only)
    rendered = json.dumps(fixture, indent=2) + '\n'
    if args.check:
        assert FIXTURE.read_text() == rendered, 'weapon weakness fixture differs from source'
        check_data(fixture, weapons)
    else:
        FIXTURE.write_text(rendered)
        if not args.fixture_only:
            check_data(fixture, weapons)
    print(json.dumps(dict(owners=len(fixture['owners']), forms=len(fixture['forms']),
        unbound=len(fixture['unbound_forms']), changed=changed, check=args.check)))


if __name__ == '__main__':
    main()
