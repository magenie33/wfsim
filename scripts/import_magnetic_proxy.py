#!/usr/bin/env python3
"""Import declared Magnetic source inputs from pinned native metadata."""
import argparse
import hashlib
import json
from pathlib import Path
import posixpath
import re
import zipfile

import yaml

from import_client_cadence import blocks, canonical, fields

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / 'tests/fixtures/client'
PINS = {
    '20261106T150853Z-client-metadata-offline': 'dbeb51209c791af1692f91f087826805b8f99198ee0958ec4606170ab2badea2',
    '20261112T165824Z-magnetic-pool-proxy': 'ccdba60c8d17b97e85010305b3f692640cfc0c9bdce8979f3ab8ae905435b3c6',
}


def verified(exchange, batch, filename):
    folder = exchange / 'batches' / batch
    raw = (folder / 'manifest.json').read_bytes()
    manifest = json.loads(raw)
    assert hashlib.sha256(raw).hexdigest() == PINS[batch]
    assert json.loads((folder / 'READY.json').read_bytes())['manifest_sha256'] == PINS[batch]
    entry = next(item for item in manifest['files'] if item['path'] == filename)
    path = folder / filename
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    assert path.stat().st_size == entry['bytes'] and digest.hexdigest() == entry['sha256']
    return path, manifest


def member(archive, manifest, filename, name):
    raw = archive.read(name)
    entry = next(item for item in manifest['archive_members'][filename] if item['path'] == name)
    assert len(raw) == entry['bytes'] and hashlib.sha256(raw).hexdigest() == entry['sha256']
    return raw


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exchange', type=Path, required=True)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    bindings = json.loads((FIXTURES / 'magnetic-proxy-bindings.json').read_text())
    path, manifest = verified(args.exchange, next(iter(PINS)), 'data.zip')
    records = {}
    consumed = {}
    with zipfile.ZipFile(path) as archive:
        for name in ['packages_records.jsonl', 'packages_owntext.jsonl']:
            expected = next(item for item in manifest['archive_members']['data.zip'] if item['path'] == name)
            digest = hashlib.sha256()
            size = 0
            with archive.open(name) as stream:
                for line in stream:
                    digest.update(line)
                    size += len(line)
                    obj = json.loads(line)
                    record = records.setdefault(obj['path'], {'parent': obj['parent'], 'fields': {}})
                    text = obj.get('ownText', '')
                    if any(key + '=' in text for key in ['WeaknessBonus', 'Weakness', 'DamageAtten', 'DamageControllerType', 'HitProxies']):
                        record['fields'] = fields(text)
            assert size == expected['bytes'] and digest.hexdigest() == expected['sha256']
            consumed[name] = expected['sha256']

    def lookup(owner, key):
        seen = set()
        while owner in records and owner not in seen:
            seen.add(owner)
            obj = records[owner]
            if key in obj['fields']:
                return owner, obj['fields'][key]
            owner = obj['parent']
        return None

    def reference(owner, token):
        token = token.strip('"')
        path = token if token.startswith('/') else posixpath.normpath(posixpath.join(posixpath.dirname(owner), token))
        assert path in records, (owner, token)
        return path

    def install(path, key, value, anchor):
        text = path.read_text()
        if args.check:
            assert yaml.safe_load(text).get(key) == value, path
            return
        if key == 'weakness_bonus':
            text = re.sub(r'^weakness_bonus:.*\n', '', text, flags=re.M)
            text, count = re.subn(r'(^internal_name:.*\n)', r'\g<1>weakness_bonus: ' + str(value) + '\n', text, count=1, flags=re.M)
            assert count == 1, path
        else:
            text = re.sub(r'^hit_proxies:\n(?:(?:[ \t].*|)\n)*', '', text, flags=re.M)
            body = 'hit_proxies:\n' + ''.join('  - ' + json.dumps(proxy) + '\n' for proxy in value)
            assert text.count(anchor) == 1
            text = text.replace(anchor, body + '\n' + anchor)
        path.write_text(text)

    weapons = []
    unresolved = []
    for path in sorted((ROOT / 'data/weapons').rglob('*.yaml')):
        doc = yaml.safe_load(path.read_text())
        if not isinstance(doc, dict) or not doc.get('internal_name'):
            continue
        found = lookup(doc['internal_name'], 'WeaknessBonus')
        if not found:
            unresolved.append(doc['id'])
            continue
        origin, token = found
        value = canonical(token)
        install(path, 'weakness_bonus', value, '')
        weapons.append(dict(id=doc['id'], owner=doc['internal_name'], origin=origin, token=token, value=value))

    enemies = []
    enemy_files = {yaml.safe_load(path.read_text())['id']: path for path in (ROOT / 'data/enemies').rglob('*.yaml')}
    for ident, owner in bindings['enemy_owners'].items():
        origin, token = lookup(owner, 'DamageControllerType')
        controller = reference(origin, token)
        proxy_owner, raw = lookup(controller, 'HitProxies')
        proxies = []
        ancestry = []
        for block in blocks(raw):
            path = reference(proxy_owner, fields(block)['Type'])
            weakness_origin, weakness_token = lookup(path, 'Weakness')
            damage = lookup(path, 'DamageAtten')
            proxy = dict(weakness=canonical(weakness_token))
            if damage:
                proxy['damage_multiplier'] = canonical(damage[1])
            proxies.append(proxy)
            ancestry.append(dict(owner=path, weakness_origin=weakness_origin, weakness_token=weakness_token,
                                 damage_origin=damage[0] if damage else None, damage_token=damage[1] if damage else None))
        install(enemy_files[ident], 'hit_proxies', proxies, 'body_parts:')
        enemies.append(dict(id=ident, owner=owner, controller=controller, proxy_owner=proxy_owner,
                            proxies=proxies, provenance=ancestry))

    batch = list(PINS)[1]
    path, manifest = verified(args.exchange, batch, 'client-evidence.zip')
    with zipfile.ZipFile(path) as archive:
        replay = json.loads(member(archive, manifest, 'client-evidence.zip', 'replay.json'))
    result = dict(client_build=bindings['client_build'], input_manifests=PINS, consumed_metadata=consumed,
                  weapons=weapons, unbound_weapon_defaults=unresolved, enemies=enemies,
                  source_cases=replay['source_cases'], tick_cases=replay['tick_cases'])
    output = FIXTURES / 'magnetic-proxy-inputs.json'
    rendered = json.dumps(result, indent=2) + '\n'
    if args.check:
        assert output.read_text() == rendered, 'Magnetic proxy fixture differs from native inputs'
    else:
        output.write_text(rendered)
    print(json.dumps(dict(weapons=len(weapons), unbound_defaults=len(unresolved), enemies=len(enemies), check=args.check)))


if __name__ == '__main__':
    main()
