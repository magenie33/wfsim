#!/usr/bin/env python3
"""Calibrate melee inputs and audit current declarations against pinned action clocks."""
import argparse
import csv
import copy
import hashlib
import io
import json
import math
from pathlib import Path
import re
import struct
import zipfile

import yaml

from import_client_cadence import blocks, canonical, fields, source
import import_melee_package as legacy

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests/fixtures/client"
ROSTER = "20261109T053558Z-weapon-roster-dossiers"
OFFLINE = "20261106T150853Z-client-metadata-offline"
TIMING = '20261106T160514Z-animation-incarnon-timing'
SLAM = '20261107T145535Z-magistar-slam-evidence'
PINS = {ROSTER: "7b830c6730e20621029d67d9d26e9aa8a216c998c32de73e2e22fe187c979d91", OFFLINE: "dbeb51209c791af1692f91f087826805b8f99198ee0958ec4606170ab2badea2"}
PINS.update({TIMING: '2d12018a10e8bd302c70bfc5ca09bfa04d8448d852fcbcada539a0faf7839e1e',
    SLAM: 'f5e38cef9fc3fc8de61d535696a4981480f2a3c82b7ffcf25907e9b2629348c7'})
TYPES = {"DT_IMPACT": "impact", "DT_PUNCTURE": "puncture", "DT_SLASH": "slash",
    "DT_FIRE": "heat", "DT_FREEZE": "cold", "DT_ELECTRICITY": "electricity",
    "DT_POISON": "toxin", "DT_EXPLOSION": "blast", "DT_RADIATION": "radiation",
    "DT_GAS": "gas", "DT_MAGNETIC": "magnetic", "DT_VIRAL": "viral",
    "DT_CORROSIVE": "corrosive", "DT_RADIANT": "void", "DT_SENTIENT": "tau", "DT_FINISHER": "true"}


def inner(raw):
    return fields(raw.strip()[1:-1])


def number(raw, default=0):
    return canonical(raw) if raw is not None else default


def pair(raw):
    return [number(v) for v in raw.strip('{}').split(',')]


def roles(raw):
    parsed = fields(raw)
    return ({kind: inner(next(v for k, v in parsed.items()
        if k.startswith(kind + ':') and not k.endswith(':Type')))
        for kind in ('fire', 'impact', 'state')},
        {k: v for k, v in parsed.items() if k.endswith(':Type')})


def members(objects, path, key):
    merged = {}
    for ancestor in reversed(objects[path]['ancestry_leaf_first']):
        raw = objects[ancestor]['composed_fields'].get(key)
        if raw:
            merged.update(inner(raw.partition('=')[2]))
    return merged


def damage(data, factor=1):
    if not data:
        return {}
    amount = number(data.get('Amount'))
    if data.get('UseNewFormat') == '1':
        vector = {TYPES[k]: number(v) for k, v in data.items() if k in TYPES}
    elif data.get('Type', 'DT_PHYSICAL') == 'DT_PHYSICAL':
        vector = {TYPES[k]: amount * number(v) for k, v in data.items() if k in TYPES}
    else:
        vector = {TYPES[data['Type']]: amount}
    return {k: float(format(v * factor, '.12g')) for k, v in vector.items() if v != 0}


def resolve(objects, token, owner_path, scope):
    token = token.strip('"')
    if not token:
        return None
    for path in (token, owner_path.rsplit('/', 1)[0] + '/' + token):
        if path in objects:
            return path
    matches = [p for p in scope if p.endswith('/' + token)]
    if not matches:
        matches = [p for p in objects if p.endswith('/' + token)]
    assert len(matches) == 1, (owner_path, token, matches)
    return matches[0]


def set_path(text, path, value):
    """Replace one YAML field without rewriting surrounding source commentary."""
    lines = text.splitlines(keepends=True)
    start, end = 0, len(lines)
    for level, key in enumerate(path):
        indent = '  ' * level
        matches = [i for i in range(start, end) if lines[i].startswith(indent + key + ':')]
        assert len(matches) <= 1, path
        if not matches:
            assert level == len(path) - 1, path
            index = end
            old_end = end
        else:
            index = matches[0]
            tail = lines[index].partition(':')[2].strip()
            if level < len(path) - 1 and tail.startswith('{'):
                mapping = yaml.safe_load(tail)
                replacement = [indent + key + ':\n'] + [indent + '  ' + line + '\n'
                    for line in yaml.safe_dump(mapping, sort_keys=False).rstrip().splitlines()]
                lines[index:index + 1] = replacement
                end += len(replacement) - 1
            old_end = next((i for i in range(index + 1, end) if lines[i].strip()
                and not lines[i].lstrip().startswith('#')
                and len(lines[i]) - len(lines[i].lstrip()) <= level * 2), end)
        if level < len(path) - 1:
            assert matches, path
            start, end = index + 1, old_end
            continue
        # Keep preceding/trailing comments outside the replaced value.
        value_end = index + 1
        if matches and not lines[index].partition(':')[2].strip().split('#')[0].strip():
            while value_end < old_end and (not lines[value_end].strip()
                    or len(lines[value_end]) - len(lines[value_end].lstrip()) > level * 2):
                value_end += 1
            while value_end > index + 1 and (not lines[value_end - 1].strip()
                    or lines[value_end - 1].lstrip().startswith('#')):
                value_end -= 1
        if isinstance(value, (dict, list)):
            rendered = indent + key + ':\n' + ''.join(indent + '  ' + line + '\n'
                for line in yaml.safe_dump(value, sort_keys=False, width=10000).rstrip().splitlines())
        else:
            scalar = json.dumps(value) if isinstance(value, (str, bool)) else format(value, '.15g')
            if isinstance(value, float) and math.isinf(value):
                scalar = '.inf' if value > 0 else '-.inf'
            rendered = indent + key + ': ' + scalar + '\n'
        lines[index:value_end if matches else index] = [rendered]
    return ''.join(lines)


def add_reason(text, reason):
    if 'reason: ' + reason in text:
        return text
    if '\nunmodeled:\n' in text:
        tail = text.split('\nunmodeled:\n', 1)[1]
        item = re.search(r'(?m)^( *)- ', tail)
        indent = item[1] if item else '  '
        return text.replace('\nunmodeled:\n', '\nunmodeled:\n' + indent + '- reason: ' + reason + '\n', 1)
    if '\nunmodeled: []\n' in text:
        return text.replace('\nunmodeled: []\n', '\nunmodeled:\n  - reason: ' + reason + '\n', 1)
    return text.rstrip() + '\n\nunmodeled:\n  - reason: ' + reason + '\n'


def supplement(batch, required):
    manifest_raw = (batch / 'manifest.json').read_bytes()
    manifest = json.loads(manifest_raw)
    assert hashlib.sha256(manifest_raw).hexdigest() == json.loads((batch / 'READY.json').read_bytes())['manifest_sha256'] == PINS[OFFLINE]
    archive = next(v for v in manifest['files'] if v['path'] == 'data.zip')
    path = batch / 'data.zip'
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(4 * 1024 * 1024), b''):
            digest.update(chunk)
    assert path.stat().st_size == archive['bytes'] and digest.hexdigest() == archive['sha256']
    member = next(v for v in manifest['archive_members']['data.zip'] if v['path'] == 'packages_effective.jsonl')
    selected, parents, digest, size = {}, {}, hashlib.sha256(), 0
    with zipfile.ZipFile(path) as archive_file, archive_file.open(member['path']) as stream:
        for line in stream:
            digest.update(line)
            size += len(line)
            row = json.loads(line)
            parents[row['path']] = row['parent']
            if row['path'] in required or 'Melee' in row['path'] or 'Stance' in row['path'] or 'Attacks' in row['fields']:
                selected[row['path']] = row
    assert size == member['bytes'] and digest.hexdigest() == member['sha256']
    missing = set()
    for row in list(selected.values()):
        parent, seen = row['parent'], set()
        while parent and parent not in seen:
            seen.add(parent)
            if parent not in selected:
                missing.add(parent)
            parent = parents.get(parent)
    if missing:
        with zipfile.ZipFile(path) as archive_file, archive_file.open(member['path']) as stream:
            for line in stream:
                row = json.loads(line)
                if row['path'] in missing:
                    selected[row['path']] = row
    return list(selected.values()), {'batch_id': OFFLINE, 'manifest_sha256': PINS[OFFLINE],
        'archive': archive, 'member': member}


def inherited(doc, catalog):
    text = (ROOT / 'engine/src/data/weapons/catalog.rs').read_text()
    keys = []
    for name in ('INHERITED', 'INHERITED_BLOCKS'):
        keys.extend(re.findall(r'"([^"]+)"', re.search(r'const ' + name + r': .*?= \[(.*?)\];', text, re.S)[1]))
    return {**{k: v for k, v in catalog.get(doc.get('inherits'), {}).items() if k in keys}, **doc}


def equivalent(a, b, tolerance=1e-9):
    if isinstance(a, dict) and isinstance(b, dict):
        return a.keys() == b.keys() and all(equivalent(a[k], b[k], tolerance) for k in a)
    if isinstance(a, list) and isinstance(b, list):
        return len(a) == len(b) and all(equivalent(x, y, tolerance) for x, y in zip(a, b))
    if isinstance(a, (int, float)) and isinstance(b, (int, float)):
        return a == b or abs(a - b) <= tolerance or struct.pack('f', a) == struct.pack('f', b)
    return a == b


def at_path(doc, path):
    value = doc
    for key in path.split('.'):
        value = value.get(key) if isinstance(value, dict) else None
    return value


def adopt(row, doc, key, value, tolerance=1e-9):
    old = at_path(doc, key)
    row['applied'][key] = old if equivalent(old, value, tolerance) else value


def retained(row, doc, key, candidate, reason):
    row['retained'][key] = {'declared': at_path(doc, key), 'candidate': candidate, 'reason': reason}
    row['limits'].append(reason)


def native_behaviors(obj):
    return [roles(raw) for raw in blocks(obj['composed_fields'].get('Behaviors', '').partition('=')[2])]


def action_properties(data):
    out = {key: number(data[native]) for native, key in (
        ('BaseDamageAtten', 'atten'), ('ComboHitsToAdd', 'comboHits'), ('ProcChanceAtten', 'procAtten')) if native in data}
    if 'ExtraProcs' in data:
        out['extraProcs'] = data['ExtraProcs'].strip('{}\n ').replace(',', '').split()
    if 'DamagePctAtten' in data:
        out['damagePct'] = {k: number(v) for k, v in inner(data['DamagePctAtten']).items() if number(v) != 1}
    return out


def audit_actions(objects, package):
    rows, usable = [], {}
    event_names = {e['event'] for a in package['attacks'].values() for e in a['events']}
    for path, old in sorted(package['attacks'].items()):
        outer = objects[path]['composed_fields']
        attack = members(objects, path, 'Attack')
        properties = members(objects, path, 'AttackProperties')
        row = {'path': path, 'attack': attack, 'properties': properties, 'swings': [],
            'clock_source': str(legacy.SOURCE.relative_to(ROOT))}
        for field, value in action_properties(properties).items():
            assert equivalent(value, old[field], 1e-5), (path, field, value, old[field])
        if attack.get('Animation'):
            assert attack['Animation'].strip('"') == (old['anim'] or ''), (path, 'animation')
        animation = objects.get(old['anim'], {}).get('composed_fields', {})
        if 'Events' in animation:
            raw_events = animation['Events'].partition('=')[2]
            current = []
            for block in blocks(raw_events):
                event = fields(block)
                if event.get('Event') not in event_names:
                    continue
                value = {'event': event['Event'], 't': number(event['Time'])}
                if event['Event'].startswith(('StartMeleeSweep', 'StartMeleeThrust')):
                    value['param'] = event.get('Param', '0')
                current.append(value)
            normalize = lambda events: [dict(event=e['event'], t=e['t'], param=str(e.get('param', '0'))) for e in events]
            assert equivalent(normalize(current), normalize(old['events']), 1e-6), (path, 'event markers')
            row['current_events'] = raw_events
        raw = outer.get('PerSwingOverrides', '').partition('=')[2]
        for index, block in enumerate(blocks(raw)):
            data = fields(block)
            if data.get('overrideAttackProperties') != '1':
                row['swings'].append(None)
                continue
            data = inner(data.get('attackProperties', '{}'))
            row['swings'].append(data)
            assert index < len(old['swings']), (path, 'new swing', index)
            prior = old['swings'][index] or old
            for field, value in action_properties(data).items():
                assert equivalent(value, prior[field], 1e-5), (path, index, field, value, prior[field])
        try:
            expected, _ = legacy.action(path, package, [])
            row['expected'] = expected
            usable[path] = expected
        except ValueError as error:
            row['unavailable'] = str(error)
        rows.append(row)
    return rows, usable


def audit_trees(objects, package, usable):
    rows, expected = [], {}
    contexts = set(legacy.CONTEXTS.values())
    for path, old in sorted(package['trees'].items()):
        sets = members(objects, path, 'EquippedAttackSets')
        current, records = {}, {}
        for context, token in sets.items():
            if context not in contexts or token == '""':
                continue
            owner = resolve(objects, token, path, objects)
            raw = objects[owner]['composed_fields'].get('Attacks', '').partition('=')[2]
            current[context] = [resolve(objects, v.strip(), owner, objects)
                for v in raw.strip('{}\n ').split(',') if v.strip()]
            records[context] = {'owner': owner, 'Attacks': raw}
        prior = {k: v for k, v in old['contexts'].items() if k in contexts and v}
        assert current == prior, (path, 'changed context', current, prior)
        live = {k: v for k, v in current.items() if all(p in usable for p in v)}
        if live:
            expected[path] = live
        rows.append({'path': path, 'sets': records, 'expected': live,
            'follow_through': objects[path]['composed_fields'].get('AvatarDamageFalloff')})
    return rows, expected


def packet_calibration(row, doc, objects, owner, behavior, key):
    typed, _ = native_behaviors(objects[owner])[behavior]
    path = resolve(objects, typed['fire'][key], owner, objects)
    native = objects[path]['composed_fields']
    packet = members(objects, path, 'AttackData')
    row['projectile'] = {'path': path, 'key': key, 'attack': packet,
        'fields': {k: v for k, v in native.items() if k in ('CriticalChance', 'CriticalMultiplier',
            'KinematicInitialSpeed', 'KinematicVelocityAccel', 'DamageFallOff', 'DamageFallOffMinDamage',
            'DamageRadius', 'ExplosionFallOff', 'HeavyRecallDamageAtten', 'HeavyRecallProcs')}}
    vector = damage(packet)
    if vector:
        adopt(row, doc, 'attack.damage', vector, 0.001)
    for native_key, key in [('CriticalChance', 'crit_chance'), ('CriticalMultiplier', 'crit_multiplier'),
            ('KinematicInitialSpeed', 'projectile_speed_mps')]:
        if native_key in native:
            adopt(row, doc, 'attack.' + key, number(native[native_key].partition('=')[2]))
    if 'ProcChance' in packet:
        adopt(row, doc, 'attack.status_chance', number(packet['ProcChance']))
    explosion = members(objects, path, 'ExplosiveAttack')
    if explosion:
        row['projectile']['explosion'] = dict(explosion)
    if row['component'] == 'throw':
        forced = [legacy.PROCS[p] for p in packet.get('ForcedProcs', '{}').strip('{}\n ').replace(',', '').split()
            if p in legacy.PROCS and legacy.PROCS[p] not in ('lifted', 'knockdown')]
        adopt(row, doc, 'attack.forced_procs', forced)
        for prefix, factor in [('attack.radial', number(native.get('HeavyRecallDamageAtten', '').partition('=')[2] or None, 1)),
                ('attack.thrown_bounce.explosion', 1)]:
            target = at_path(doc, prefix)
            if not target:
                continue
            # The selected throw packet supplies the component type when its child omits it.
            component = dict(explosion)
            if 'Type' not in component:
                kind = next(iter(target['damage']))
                component['Type'] = next(k for k, v in TYPES.items() if v == kind)
                row['projectile'].setdefault('declared_type_fallback', {})[prefix] = kind
            vector = damage(component, factor)
            if vector:
                adopt(row, doc, prefix + '.damage', vector, 0.001)
            geometry = [('DamageRadius', 'radius_m')]
            if prefix.endswith('.explosion'):
                geometry.append(('ExplosionFallOff', 'falloff_reduction'))
            elif 'ExplosionFallOff' in native:
                retained(row, doc, prefix + '.falloff_reduction',
                    number(native['ExplosionFallOff'].partition('=')[2]), 'client_melee_special')
            for source_key, key in geometry:
                if source_key in native:
                    adopt(row, doc, prefix + '.' + key, number(native[source_key].partition('=')[2]))
            if 'ProcChance' in explosion:
                adopt(row, doc, prefix + '.status_chance', number(explosion['ProcChance']))
    return row


def calibrate(binding, group, objects, doc):
    row = {'id': binding['id'], 'form': binding['form'], 'group': group['group'],
        'component': binding['component'], 'applied': {}, 'retained': {}, 'limits': []}
    native_owner = binding.get('native_owner', group['owner'])
    row['native_owner'] = native_owner
    outer = objects[native_owner]['composed_fields']
    if binding['form'] == 'slide':
        # The spin component bypasses the ordinary tree's damage attenuation.
        adopt(row, doc, 'attack.follow_through', 1.0)
    if binding.get('metadata'):
        for source_key, key in [('OmegaAttenuation', 'disposition'), ('RequiredLevel', 'mastery_rank')]:
            if source_key in outer:
                value = number(outer[source_key].partition('=')[2])
                adopt(row, doc, key, int(value) if key == 'mastery_rank' else value)
    if group['scope'] == 'companion':
        row['limits'].append('client_melee_companion')
        return row
    if group['scope'] == 'assembly':
        row['limits'].extend(['client_melee_assembly', 'client_melee_timing', 'client_melee_geometry'])
        row['part'] = {'field': group['hand_field'], 'declaration': outer[group['hand_field']]}
        return row
    if row['component'] == 'ability':
        row['limits'].append('client_melee_ability')
        return row
    if group['scope'] == 'fixed_ability':
        row['limits'].append('client_melee_ability')
    typed, types = native_behaviors(objects[native_owner])[0]
    impact, state = typed['impact'], typed['state']
    packet = inner(impact.get('AttackData', '{}'))
    row['body_behavior'] = 0
    special = row['component'] == 'special'
    if not special and row['component'] != 'throw':
        if row['component'] != 'slam':
            vector = damage(packet)
            if group['scope'] == 'fixed_ability':
                retained(row, doc, 'attack.damage', vector, 'client_melee_ability')
                retained(row, doc, 'attack.status_chance', number(packet.get('ProcChance')), 'client_melee_ability')
            else:
                if vector:
                    adopt(row, doc, 'attack.damage', vector, 0.001)
                if 'ProcChance' in packet:
                    adopt(row, doc, 'attack.status_chance', number(packet['ProcChance']))
        for key, value in [('crit_chance', number(impact.get('criticalHitChance'))),
                ('crit_multiplier', number(impact.get('criticalHitDamageMultiplier'), 1))]:
            adopt(row, doc, 'attack.' + key, value)
    if row['component'] != 'throw':
        rate = number(state.get('fireRate')) / 60
        if rate > 0:
            adopt(row, doc, 'attack.fire_rate', rate)
    if 'TimeBetweenHits' in impact:
        duration = math.inf if impact.get('InfiniteTimeBetweenHits') == '1' else number(impact['TimeBetweenHits'])
        adopt(row, doc, 'combo_duration_seconds', duration)
    counter = {k: impact[k] for k in ('InitialHitCounter', 'InitialHitCounterRecoveryRate', 'BaseHitCount',
        'BaseHitMultipler', 'HitCounterCap', 'HitReqNextTierOperator', 'HitReqNextTierOperationType') if k in impact}
    if counter.get('InitialHitCounter', '0') != '0' or counter.get('BaseHitCount', '20') != '20' or counter.get('HitCounterCap', '220') != '220':
        row['limits'].append('client_melee_counter')
    if row['component'] == 'body' and binding['form'] != 'slide' and doc['attack'].get('follow_through') is not None:
        tree_token = outer.get('MeleeTreeType', '').partition('=')[2]
        tree = resolve(objects, tree_token, native_owner, objects)
        if tree and 'AvatarDamageFalloff' in objects[tree]['composed_fields']:
            adopt(row, doc, 'attack.follow_through', number(objects[tree]['composed_fields']['AvatarDamageFalloff'].partition('=')[2]))
    combo = inner(state.get('ComboData', '{}'))
    if doc['attack'].get('heavy') and 'HeavyMeleeChargeTime' in combo:
        duration = number(combo['HeavyMeleeChargeTime'])
        if binding.get('retain_charge'):
            retained(row, doc, 'attack.heavy.windup_seconds', duration, 'client_melee_special')
        else:
            adopt(row, doc, 'attack.heavy.windup_seconds', duration)
        if binding.get('legacy_windup') and not binding.get('retain_charge'):
            script = copy.deepcopy(doc['attack']['combo_script'])
            for hit in script:
                if 'windup_seconds' in hit:
                    hit['windup_seconds'] = duration
            adopt(row, doc, 'attack.combo_script', script)
    if row['component'] == 'throw':
        packet_calibration(row, doc, objects, native_owner, binding['projectile_behavior'], binding['projectile_key'])
        if 'ChargeDuration' in state:
            adopt(row, doc, 'attack.throw_charge_seconds', number(state['ChargeDuration']))
    if binding.get('projectile_behavior') is not None and row['component'] == 'special':
        packet_calibration(row, doc, objects, native_owner, binding['projectile_behavior'], binding['projectile_key'])
    if row['component'] == 'slam' or doc['attack'].get('slam'):
        row['limits'].append('client_melee_slam')
    if special:
        row['limits'].append('client_melee_special')
    if isinstance(doc['attack'].get('combo_script'), dict):
        row['limits'].append('client_melee_timing')
    elif row['component'] != 'slam':
        row['limits'].append('client_melee_legacy')
    if row['component'] not in ('slam', 'throw'):
        row['limits'].append('client_melee_geometry')
    return row


def json_ready(value):
    if isinstance(value, float) and math.isinf(value):
        return '.inf' if value > 0 else '-.inf'
    if isinstance(value, dict):
        return {k: json_ready(v) for k, v in value.items()}
    if isinstance(value, list):
        return [json_ready(v) for v in value]
    return value


def clock_evidence(batches, package):
    _, timing = source(batches / TIMING, 'timing.zip', ['summary.json'])
    payload, slam = source(batches / SLAM, 'slam-evidence.zip', ['clip-ledger.csv',
        'baseline-and-timing-boundaries.json', 'radius-arithmetic-verification.json'])
    def normalized(path):
        for suffix in ('_anim.fbx', '.fbx'):
            if path.endswith(suffix):
                path = path[:-len(suffix)]
        return path
    clips = {}
    for row in csv.DictReader(io.StringIO(payload['clip-ledger.csv'].decode('utf-8-sig'))):
        clips.setdefault(normalized(row['path']), []).append(row)
    candidates = []
    for path, action in sorted(package['attacks'].items()):
        if not action['anim'] or normalized(action['anim']) not in clips:
            continue
        rows = clips[normalized(action['anim'])]
        values = {float(row['base_clip_seconds_float32']) for row in rows}
        if len(values) == 1 and action['duration'] is not None:
            assert round(next(iter(values)), 6) == round(action['duration'], 6), (path, 'changed base clip')
        candidates.append({'action': path, 'legacy_duration': action['duration'], 'current_clip_rows': rows,
            'status': 'equivalent at the earlier export precision' if len(values) == 1 else 'skeleton selection unverified'})
    return {'timing_source': timing, 'slam_source': slam, 'current_clip_candidates': candidates,
        'physical_cycle': json.loads(payload['baseline-and-timing-boundaries.json']),
        'radius_arithmetic': json.loads(payload['radius-arithmetic-verification.json'])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exchange', type=Path, required=True)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    groups = json.loads((FIXTURES / 'melee-bindings.json').read_text())['groups']
    batches = args.exchange / 'batches'
    for batch, pin in PINS.items():
        assert hashlib.sha256((batches / batch / 'manifest.json').read_bytes()).hexdigest() == pin
    payload, receipt = source(batches / ROSTER, 'weapon-roster-dossiers.zip',
        ['shared/objects.jsonl'] + [f'weapons/{g["group"]}/dossier.json' for g in groups])
    objects = {o['path']: o for line in payload['shared/objects.jsonl'].splitlines() if (o := json.loads(line))}
    package_raw = legacy.SOURCE.read_bytes()
    package = json.loads(package_raw)
    extra, extra_receipt = supplement(batches / OFFLINE, set(package['attacks']) | set(package['trees']) | set(package['weapons']))
    for obj in extra:
        objects.setdefault(obj['path'], {'path': obj['path'], 'parent': obj['parent'], 'composed_fields': obj['fields']})
    for obj in objects.values():
        path, chain = obj['path'], []
        while path in objects and path not in chain:
            chain.append(path)
            path = objects[path].get('parent')
        obj['ancestry_leaf_first'] = chain
    actions, usable = audit_actions(objects, package)
    trees, expected_trees = audit_trees(objects, package, usable)
    referenced = {p for contexts in expected_trees.values() for paths in contexts.values() for p in paths}
    fixture = {'source': receipt, 'supplement_source': extra_receipt,
        'legacy_package': {'path': str(legacy.SOURCE.relative_to(ROOT)), 'sha256': hashlib.sha256(package_raw).hexdigest(), 'build': package['buildLabel']},
        'actions': actions, 'shared_actions': {p: a for p, a in usable.items() if p in referenced},
        'trees': trees, 'shared_trees': expected_trees, 'clock_evidence': clock_evidence(batches, package),
        'weapons': [], 'forms': []}
    catalog = {}
    for path in (ROOT / 'data/weapons').rglob('*.yaml'):
        doc = yaml.safe_load(path.read_text())
        if isinstance(doc, dict) and 'id' in doc:
            catalog[doc['id']] = doc
    changes = {}
    for group in groups:
        dossier = json.loads(payload[f'weapons/{group["group"]}/dossier.json'])
        assert dossier['identity']['native_paths'] == [group['owner']]
        entries = {e['id']: e for e in dossier['entries']}
        assert entries.keys() == {f['id'] for f in group['forms']}
        owners = sorted({f.get('native_owner', group['owner']) for f in group['forms']})
        weapon = {'group': group['group'], 'scope': group['scope'], 'sources': []}
        for owner in owners:
            obj = objects[owner]
            source_row = {'owner': owner, 'parent': obj.get('parent'),
                'fields': {k: v for k, v in obj['composed_fields'].items() if k in ('MeleeTreeType', 'Slams',
                    'FullSlamRadiusHeight', 'SlamRadiusPercentageClamps', 'HorizontalDisplacementAttenuation',
                    'OmegaAttenuation', 'RequiredLevel', 'OneHanded', 'TwoHanded', 'BaseCritChance', 'AttackData')},
                'behaviors': []}
            for index, (typed, types) in enumerate(native_behaviors(obj)):
                source_row['behaviors'].append({'index': index, 'types': types,
                    'impact': {k: v for k, v in typed['impact'].items() if k in ('AttackData', 'criticalHitChance',
                        'criticalHitDamageMultiplier', 'InitialHitCounter', 'InitialHitCounterRecoveryRate', 'BaseHitCount',
                        'BaseHitMultipler', 'HitCounterCap', 'HitReqNextTierOperator', 'HitReqNextTierOperationType',
                        'InfiniteTimeBetweenHits', 'TimeBetweenHits')},
                    'state': {k: v for k, v in typed['state'].items() if k in ('fireRate', 'ComboData', 'ChargeDuration', 'MeleeDisableTime')},
                    'fire': {k: v for k, v in typed['fire'].items() if k in ('projectileType', 'chargedProjectileType',
                        'TurboProjectileType', 'fireIterations', 'SweepRadius', 'SweepInfo', 'traceDistance')}})
            weapon['sources'].append(source_row)
        fixture['weapons'].append(weapon)
        for form in group['forms']:
            entry = entries[form['id']]
            assert entry['declared_form'] == form['form']
            doc = inherited(catalog[form['id']], catalog)
            row = calibrate(form, group, objects, doc)
            row['limits'] = sorted(set(row['limits']))
            path = ROOT / entry['yaml_file']
            text = original = path.read_text()
            for key, value in row['applied'].items():
                if not equivalent(at_path(yaml.safe_load(text), key), value, 0):
                    text = set_path(text, key.split('.'), value)
            for reason in row['limits']:
                text = add_reason(text, reason)
            if form.get('legacy_windup'):
                # The wind-up is native; the remaining legacy motion is still disclosed.
                text = re.sub(r'(?m)^  - reason: heavy_charge_share_assumed\n(?:    .*\n)*', '', text)
            if 'client_melee_calibration' not in text:
                text = text.rstrip() + '\n\n# see notes: client_melee_calibration\n'
            changes[path] = original, text
            fixture['forms'].append(row)
    assert len(groups) == 270 and len(fixture['forms']) == 1730
    # Weapon-level clocks remain on the parent when their values are identical.
    by_id = {yaml.safe_load(text)['id']: yaml.safe_load(text) for _, text in changes.values()}
    for path, (old, text) in changes.items():
        doc = yaml.safe_load(text)
        parent = by_id.get(doc.get('inherits'), {})
        if not parent.get('inherits') and equivalent(doc.get('combo_duration_seconds'), parent.get('combo_duration_seconds')) and 'combo_duration_seconds' in parent:
            text = re.sub(r'(?m)^combo_duration_seconds: [^\n]*\n', '', text)
        changes[path] = old, text
    fixture_path = FIXTURES / 'melee-calibration-2026-10-28.json'
    changes[fixture_path] = (fixture_path.read_text() if fixture_path.exists() else '', json.dumps(json_ready(fixture), indent=2) + '\n')
    different = [str(p.relative_to(ROOT)) for p, (old, new) in changes.items() if old != new]
    if args.check and different:
        raise SystemExit('melee calibration differs: ' + ', '.join(different))
    if not args.check:
        for path, (_, text) in changes.items():
            path.write_text(text)
    print(json.dumps({'groups': len(groups), 'forms': len(fixture['forms']), 'shared_actions': len(fixture['shared_actions']),
        'audited_actions': len(actions), 'shared_trees': len(expected_trees),
        'adopted_fields': sum(len(r['applied']) for r in fixture['forms']), 'changed_files': len(different), 'check': args.check}))


if __name__ == '__main__':
    main()
