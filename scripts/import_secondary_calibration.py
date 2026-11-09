#!/usr/bin/env python3
"""Calibrate secondary forms and reviewed components against pinned client inputs."""
import argparse
import hashlib
import json
from pathlib import Path
import re

import yaml

from import_primary_calibration import (FIXTURES, OFFLINE, PINS, ROOT, ROSTER,
    add_reason, adopt, blocks, calibrate, damage, members, number, pair, resolve,
    roles, set_path, source, supplement)

CAT = '20261108T060529Z-catabolyst-reload-grenades'
STUG = '20261109T051454Z-weapon-dossier-stug'
TOME = '20261109T051501Z-weapon-dossier-grimoire'
SPECIAL = {
    CAT: ('fada24224458c64e02f6e7a4896442c95000aea2b52dcafac7631bcf1bb8ac0b',
        'evidence.zip', ['projectile-profiles.json', 'attachment-callbacks.json',
            'reload-markers.json', 'fan-verification.json']),
    STUG: ('ad7b6651c0344f6b98cb99bf659a8085d983e63f4482dbb5a05ad238c0aad43a',
        'weapon-dossier.zip', ['mechanism-ledger.json']),
    TOME: ('7b7d2991f1da2afc25515436c72b8e6f0d165cea192a60c39cf7532cf320094a',
        'weapon-dossier.zip', ['mechanism-ledger.json']),
}


def inherited(doc, catalog):
    # The engine owns which fields a form inherits; attacks and batteries never do.
    text = (ROOT / 'engine/src/data/weapons/catalog.rs').read_text()
    keys = []
    for name in ('INHERITED', 'INHERITED_BLOCKS'):
        block = re.search(r'const ' + name + r': .*?= \[(.*?)\];', text, re.S)[1]
        keys.extend(re.findall(r'"([^"]+)"', block))
    parent = catalog.get(doc.get('inherits'), {})
    return {**{k: v for k, v in parent.items() if k in keys}, **doc}


def grenades(row, doc, binding, payload):
    profiles = {r['path']: r for r in json.loads(payload['projectile-profiles.json'])}
    callbacks = [r for r in json.loads(payload['attachment-callbacks.json'])
        if r['owner'] == binding['grenade_attachment']]
    row['reload_grenades'] = []
    for mode, function, field in [('partial', 'OnQuickReload', '_quickReloadProjectileType'),
            ('from_empty', 'OnEmptyReload', '_emptyReloadProjectileType')]:
        callback = next(r for r in callbacks if r['script_parameters']['Function'] == function)
        parameters = callback['script_parameters']
        token = parameters[field]
        matches = [p for p in profiles if p.endswith('/' + token)]
        assert len(matches) == 1, token
        packet = profiles[matches[0]]
        row['reload_grenades'].append({'mode': mode, 'callback': callback, 'packet': packet})
        prefix = ('attack', 'reload_grenade')
        count = int(number(parameters['_numProjectiles']))
        adopt(row['applied'], doc, prefix + ('count',), count)
        adopt(row['applied'], doc, prefix + ('fan_deg',),
            number(parameters['_totalProjectileSpread']) if count > 1 else 0)
        stats = {'crit_chance': number(str(packet['critical_chance_float32'])),
            'crit_multiplier': number(str(packet['critical_multiplier_float32'])),
            'status_chance': number(packet['contact']['ProcChance']),
            'damage': damage(packet['contact'])}
        for key, value in stats.items():
            adopt(row['applied'], doc, prefix + (mode, key), value)
        radial = {'damage': damage(packet['explosion']),
            'radius_m': packet['radius_float32'],
            'falloff_reduction': number(packet['source_fields']['ExplosionFallOff'].partition('=')[2]),
            'takes_blast_radius_mods': packet['source_fields']['DisableExplosionRadiusUpgrades'].endswith('=0'),
            'crit_chance': stats['crit_chance'], 'crit_multiplier': stats['crit_multiplier'],
            'status_chance': number(packet['explosion']['ProcChance'])}
        for key, value in radial.items():
            adopt(row['applied'], doc, prefix + (mode, 'radial', key), value)


def components(row, objects, binding, dossier, doc, special):
    updates = row['applied']
    if binding.get('weapon_metadata'):
        native = objects[row['owner']]['composed_fields']
        row['weapon_metadata'] = {k: native[k] for k in ('OmegaAttenuation', 'RequiredLevel') if k in native}
        for source_key, key in [('OmegaAttenuation', 'disposition'), ('RequiredLevel', 'mastery_rank')]:
            if source_key in native:
                value = number(native[source_key].partition('=')[2])
                adopt(updates, doc, (key,), int(value) if key == 'mastery_rank' else value)
    if updates.get('attack.charge_seconds') == 0:
        # A zero partial draw is the ordinary fire interval, not a charge phase.
        del updates['attack.charge_seconds']
        row['instant_release'] = True
    kind = binding.get('component_model')
    if kind == 'reload_grenade':
        grenades(row, doc, binding, special[CAT])
    if kind in ('blob', 'orb'):
        path = row['projectile']['path']
        native = objects[path]['composed_fields']
        ledger = json.loads(special[STUG if kind == 'blob' else TOME]['mechanism-ledger.json'])
        row['component_declarations'] = next(r for r in ledger if r['owner'] == path)
        if kind == 'blob':
            prefix = ('attack', 'blob')
            adopt(updates, doc, prefix + ('lifespan_seconds',), pair(native['EmbedTime'].partition('=')[2])[0])
            adopt(updates, doc, prefix + ('embed_radius_m',), number(native['EmbedDamageRadius'].partition('=')[2]))
            adopt(updates, doc, prefix + ('radius_at_cap_m',), pair(native['ExplosionRange'].partition('=')[2])[1])
            if 'MaxProjectileStack' in native:
                adopt(updates, doc, prefix + ('cap',), int(number(native['MaxProjectileStack'].partition('=')[2])))
            explosion = members(objects, path, 'EmbedDeathAttack')
            row['projectile']['embed_death_attack'] = explosion
            adopt(updates, doc, prefix + ('explosion', 'damage'), damage(explosion))
            for source_key, key in [('DamageRadius', 'radius_m'), ('ExplosionFallOff', 'falloff_reduction')]:
                adopt(updates, doc, prefix + ('explosion', key), number(native[source_key].partition('=')[2]))
        else:
            for source_key, key in [('MaxLife', 'fuse_seconds'), ('TimeBetweenAttacks', 'strike_interval_seconds'),
                    ('KinematicInitialSpeed', 'speed_mps'), ('DamageRadius', 'strike_radius_m')]:
                adopt(updates, doc, ('attack', 'orb', key), number(native[source_key].partition('=')[2]))
    if 'reload_behavior' in binding:
        owner = objects[row['owner']]
        raw = list(blocks(owner['composed_fields']['Behaviors'].partition('=')[2]))[binding['reload_behavior']]
        typed, _ = roles(raw)
        row['reload_source'] = {'behavior_index': binding['reload_behavior'], 'state': typed['state']}
        adopt(updates, doc, ('reload_seconds',), number(typed['state']['reloadTime']))
    for category in ('unmodelled_behaviors', 'non_attack_behaviors'):
        for index in binding.get(category, []):
            raw = list(blocks(objects[row['owner']]['composed_fields']['Behaviors'].partition('=')[2]))[index]
            typed, types = roles(raw)
            record = {'behavior_index': index, 'roles': typed, 'types': types}
            for key in ('projectileType', 'chargedProjectileType'):
                path = resolve(objects, typed['fire'].get(key, '""'), row['owner'], dossier['source_closure'])
                if path:
                    record.setdefault('projectiles', {})[key] = {'path': path,
                        'fields': objects[path]['composed_fields']}
            row.setdefault(category, []).append(record)
    row['retained'] = {}
    for key, reason in binding.get('retain', {}).items():
        candidate = updates.pop(key, None)
        declared = doc
        for part in key.split('.'):
            declared = declared.get(part) if isinstance(declared, dict) else None
        row['retained'][key] = {'declared': declared, 'packet_candidate': candidate, 'reason': reason}
    row['limits'] = sorted(set(row['limits'] + binding.get('limits', [])))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exchange', type=Path, required=True)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    groups = json.loads((FIXTURES / 'secondary-bindings.json').read_text())['groups']
    batches = args.exchange / 'batches'
    for batch, expected in {**PINS, **{k: v[0] for k, v in SPECIAL.items()}}.items():
        assert hashlib.sha256((batches / batch / 'manifest.json').read_bytes()).hexdigest() == expected
    payload, receipt = source(batches / ROSTER, 'weapon-roster-dossiers.zip',
        ['shared/objects.jsonl'] + [f'weapons/{g["group"]}/dossier.json' for g in groups])
    objects = {o['path']: o for line in payload['shared/objects.jsonl'].splitlines() if (o := json.loads(line))}
    extra, extra_receipt = supplement(batches / OFFLINE, (b'1999EntHybridPistol',))
    for obj in extra:
        objects.setdefault(obj['path'], {'path': obj['path'], 'parent': obj['parent'],
            'composed_fields': obj['fields'], 'ancestry_leaf_first': [obj['path']]})
    for obj in extra:
        target = objects[obj['path']]
        if len(target['ancestry_leaf_first']) == 1 and obj['parent'] in objects:
            target['ancestry_leaf_first'] += objects[obj['parent']]['ancestry_leaf_first']
    special, special_receipts = {}, {}
    for name, (_, archive, names) in SPECIAL.items():
        special[name], special_receipts[name] = source(batches / name, archive, names)
    catalog = {}
    for path in (ROOT / 'data/weapons').rglob('*.yaml'):
        doc = yaml.safe_load(path.read_text())
        if isinstance(doc, dict) and 'id' in doc:
            catalog[doc['id']] = doc
    fixture = {'source': receipt, 'supplement_source': extra_receipt,
        'component_sources': special_receipts, 'forms': []}
    changes = {}
    for group in groups:
        dossier = json.loads(payload[f'weapons/{group["group"]}/dossier.json'])
        assert dossier['identity']['native_paths'] == [group['owner']]
        entries = {e['id']: e for e in dossier['entries']}
        assert entries.keys() == {f['id'] for f in group['forms']}
        classified = {f['behavior'] for f in group['forms']}
        for form in group['forms']:
            classified.update(form.get('unmodelled_behaviors', []))
            classified.update(form.get('non_attack_behaviors', []))
        native = objects[group['owner']]['composed_fields'].get('Behaviors', '').partition('=')[2]
        for index, raw in enumerate(blocks(native)):
            _, types = roles(raw)
            assert index in classified or 'MeleeRadius' in types['state:Type'], (group['group'], index)
        for form in group['forms']:
            entry = entries[form['id']]
            assert entry['yaml_file'].startswith('data/weapons/secondary/')
            assert entry['declared_form'] == form['form']
            doc = inherited(catalog[form['id']], catalog)
            row = calibrate(objects, form, dossier, doc)
            components(row, objects, form, dossier, doc, special)
            path = ROOT / entry['yaml_file']
            original = text = path.read_text()
            if row.get('instant_release'):
                text = re.sub(r'(?m)^  charge_seconds: [^\n]*\n', '', text)
            for key, value in row['applied'].items():
                actual = yaml.safe_load(text)
                for part in key.split('.'):
                    actual = actual.get(part) if isinstance(actual, dict) else None
                if actual != value:
                    text = set_path(text, key.split('.'), value)
            for reason in row['limits']:
                text = add_reason(text, reason)
            if 'client_secondary_calibration' not in text:
                text = text.rstrip() + '\n\n# see notes: client_secondary_calibration\n'
            changes[path] = (original, text)
            fixture['forms'].append(row)
    assert len(groups) == 154 and len(fixture['forms']) == 211
    by_id = {yaml.safe_load(text)['id']: yaml.safe_load(text) for _, text in changes.values()}
    for path, (old, text) in changes.items():
        doc = yaml.safe_load(text)
        if doc.get('inherits') in by_id:
            for key in ('reload_seconds', 'magazine', 'ammo_max', 'ammo_pickup'):
                if doc.get(key) == by_id[doc['inherits']].get(key):
                    text = re.sub(r'(?m)^' + key + r': [^\n]*\n', '', text)
        changes[path] = old, text
    path = FIXTURES / 'secondary-calibration-2026-10-28.json'
    changes[path] = (path.read_text() if path.exists() else '', json.dumps(fixture, indent=2) + '\n')
    different = [str(p.relative_to(ROOT)) for p, (old, new) in changes.items() if old != new]
    if args.check and different:
        raise SystemExit('secondary calibration differs: ' + ', '.join(different))
    if not args.check:
        for path, (_, text) in changes.items():
            path.write_text(text)
    print(json.dumps({'groups': len(groups), 'forms': len(fixture['forms']),
        'adopted_fields': sum(len(r['applied']) for r in fixture['forms']),
        'changed_files': len(different), 'check': args.check}))


if __name__ == '__main__':
    main()
