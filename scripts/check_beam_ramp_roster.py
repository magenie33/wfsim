#!/usr/bin/env python3
"""Verify the held-fire attenuation receipt against the pinned client roster."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import zipfile

from import_primary_calibration import blocks, roles

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / 'tests/fixtures/client'
BATCH = '20261109T053558Z-weapon-roster-dossiers'
PIN = '7b830c6730e20621029d67d9d26e9aa8a216c998c32de73e2e22fe187c979d91'
ARCHIVE = 'weapon-roster-dossiers.zip'
MEMBER = 'shared/objects.jsonl'


def verify(exchange):
    held = {p.stem: p.read_text() for p in (ROOT / 'data/weapons').rglob('*.yaml')
            if re.search(r'^\s*trigger:\s*held\b', p.read_text(), re.M)}
    forms = {v['id']: v for group in ('primary', 'secondary')
             for v in json.loads((FIXTURES / f'{group}-calibration-2026-10-28.json').read_text())['forms']
             if v['id'] in held}
    for identity in held.keys() - forms.keys():
        path = re.search(r'^internal_name:\s*(\S+)', held[identity], re.M)[1]
        forms[identity] = {'owner': path, 'behavior_index': 0}
    owners = {v['owner'] for v in forms.values()}
    batch = exchange / 'batches' / BATCH
    manifest_bytes = (batch / 'manifest.json').read_bytes()
    assert hashlib.sha256(manifest_bytes).hexdigest() == PIN
    assert json.loads((batch / 'READY.json').read_text())['manifest_sha256'] == PIN
    manifest = json.loads(manifest_bytes)
    archive = batch / ARCHIVE
    declaration = next(x for x in manifest['files'] if x['path'] == ARCHIVE)
    assert archive.stat().st_size == declaration['bytes']
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == declaration['sha256']
    member = next(x for x in manifest['archive_members'][ARCHIVE] if x['path'] == MEMBER)
    digest, length, objects = hashlib.sha256(), 0, {}
    tokens = [f'"{owner}"'.encode() for owner in owners]
    with zipfile.ZipFile(archive) as z, z.open(MEMBER) as stream:
        for line in stream:
            digest.update(line)
            length += len(line)
            if any(token in line for token in tokens):
                obj = json.loads(line)
                if obj['path'] in owners:
                    objects[obj['path']] = obj
    assert (digest.hexdigest(), length) == (member['sha256'], member['bytes'])
    rows = []
    for identity in sorted(held):
        form = forms[identity]
        owner, index = form['owner'], form.get('behavior_index')
        outer = objects[owner]['composed_fields']
        if index is None:
            key = 'PrimaryBehaviorOverrides' if identity.endswith('_primary') else 'BehaviorOverrides'
            raw = outer[key].partition('=')[2].strip()[1:-1]
        else:
            raw = list(blocks(outer['Behaviors'].partition('=')[2]))[index]
        typed, types = roles(raw)
        values = {k: v for k, v in typed['impact'].items() if 'ContinuousDamage' in k}
        assert values, identity
        rows.append(dict(id=identity, owner=owner, index=index,
                         impact_type=types['impact:Type'], fields=values, error=None))
    expected = json.loads((FIXTURES / 'beam-ramp-roster-2026-10-28.json').read_text())
    assert rows == expected, 'attenuation receipt drift'
    print(f'verified {len(rows)} held-mode declarations')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exchange', type=Path, required=True)
    verify(parser.parse_args().exchange)
