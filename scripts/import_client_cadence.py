#!/usr/bin/env python3
"""Import audited refire profiles and beast claw inputs from immutable client batches."""
import argparse
import csv
import hashlib
import io
import json
import math
from pathlib import Path, PurePosixPath
import re
import struct
import zipfile

import yaml

class Dumper(yaml.SafeDumper):
    pass


def string_value(dumper, value):
    return dumper.represent_scalar('tag:yaml.org,2002:str', value,
                                   style='"' if '\n' in value else None)


Dumper.add_representer(str, string_value)


def dump(doc):
    return yaml.dump(doc, Dumper=Dumper, sort_keys=False, width=10000)

ROOT = Path(__file__).resolve().parents[1]
CHECK = False
TYPES = {'DT_IMPACT': 'impact', 'DT_PUNCTURE': 'puncture', 'DT_SLASH': 'slash',
         'DT_FIRE': 'heat', 'DT_FREEZE': 'cold', 'DT_ELECTRICITY': 'electricity',
         'DT_POISON': 'toxin', 'DT_EXPLOSION': 'blast', 'DT_RADIATION': 'radiation',
         'DT_GAS': 'gas', 'DT_MAGNETIC': 'magnetic', 'DT_VIRAL': 'viral',
         'DT_CORROSIVE': 'corrosive', 'DT_RADIANT': 'void'}


def fields(text):
    lines, out, i = text.splitlines(), {}, 0
    while i < len(lines):
        s = lines[i].strip()
        i += 1
        m = re.match(r'([\w:]+)=(.*)', s)
        if not m:
            continue
        depth = s.count('{') - s.count('}')
        while depth > 0:
            line = lines[i]
            i += 1
            s += '\n' + line.strip()
            depth += line.count('{') - line.count('}')
        out[m[1]] = s.partition('=')[2]
    return out


def blocks(s):
    depth = 0
    for i, c in enumerate(s):
        if c == '{':
            depth += 1
            if depth == 2:
                start = i + 1
        elif c == '}':
            if depth == 2:
                yield s[start:i]
            depth -= 1


def canonical(token):
    raw = struct.pack('<f', float(token))
    value = struct.unpack('<f', raw)[0]
    for digits in range(1, 10):
        text = format(value, f'.{digits}g')
        if struct.pack('<f', float(text)) == raw:
            return float(text)
    raise ValueError(token)


def output(path, text):
    if CHECK:
        assert path.read_text() == text, f'{path}: differs from parsed inputs'
    else:
        path.write_text(text)


def effect_phrase(effect):
    if effect.get('element'):
        return effect['element']
    return {'base_damage_bonus': 'melee damage', 'crit_chance_bonus': 'critical chance',
            'crit_damage_bonus': 'critical damage', 'status_chance_bonus': 'status chance',
            'status_duration_bonus': 'status duration', 'status_damage_bonus': 'status damage',
            'condition_overload': 'per status type', 'noncrit_damage_bonus': 'non-critical'}.get(effect['kind'], '')


def source(batch, archive, names):
    raw = (batch / 'manifest.json').read_bytes()
    manifest = json.loads(raw)
    ready = json.loads((batch / 'READY.json').read_bytes())
    assert ready['batch_id'] == batch.name
    assert hashlib.sha256(raw).hexdigest() == ready['manifest_sha256']
    expected = next(f for f in manifest['files'] if f['path'] == archive)
    path = batch / archive
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for data in iter(lambda: stream.read(4 * 1024 * 1024), b''):
            digest.update(data)
    assert path.stat().st_size == expected['bytes'] and digest.hexdigest() == expected['sha256']
    result = {}
    with zipfile.ZipFile(path) as z:
        declared = {r['path']: r for r in manifest['archive_members'][archive]}
        for name in names:
            data = z.read(name)
            r = declared[name]
            assert len(data) == r['bytes'] and hashlib.sha256(data).hexdigest() == r['sha256']
            result[name] = data
    return result, {'batch_id': batch.name, 'manifest_sha256': ready['manifest_sha256'],
                    'archive': expected, 'members': [declared[n] for n in names]}


def verify_closed_batch(batch):
    raw = (batch / 'manifest.json').read_bytes()
    ready = json.loads((batch / 'READY.json').read_bytes())
    assert ready['batch_id'] == batch.name
    assert hashlib.sha256(raw).hexdigest() == ready['manifest_sha256']
    manifest = json.loads(raw)
    for item in manifest['files']:
        relative = PurePosixPath(item['path'])
        assert not relative.is_absolute() and '..' not in relative.parts
        payload = (batch / item['path']).read_bytes()
        assert len(payload) == item['bytes'] and hashlib.sha256(payload).hexdigest() == item['sha256']
    for archive, members in manifest['archive_members'].items():
        with zipfile.ZipFile(batch / archive) as z:
            assert len(z.namelist()) == len(members)
            assert set(z.namelist()) == {m['path'] for m in members}
            for item in members:
                payload = z.read(item['path'])
                assert len(payload) == item['bytes'] and hashlib.sha256(payload).hexdigest() == item['sha256']
    return manifest


def ordinary_interval_proxies(batches, prior_metadata, prior_manifest):
    batch = batches / '20261108T052700Z-beast-attack-interval'
    manifest = verify_closed_batch(batch)
    data, binding = source(batch, 'evidence.zip', ['evidence.json', 'verification.json',
        'prior-evidence/representative-beast-attacks.json', 'prior-evidence/beast-logic-graphs.json',
        'prior-evidence/metadata-composed.json'])
    evidence, verification = (json.loads(data[n]) for n in ['evidence.json', 'verification.json'])
    assert evidence['dependency']['batch_id'] == '20261107T231054Z-companion-cadence-aiming'
    assert len(evidence['functions']) == 18
    assert evidence['dependency']['manifest_sha256'] == prior_manifest
    assert data['prior-evidence/metadata-composed.json'] == prior_metadata
    with zipfile.ZipFile(batch / 'evidence.zip') as z:
        for function in evidence['functions']:
            for fragment in function['chained_fragments']:
                code = bytes.fromhex(z.read(f'native/{fragment["stem"]}.hex.txt').decode())
                assert len(code) == fragment['bytes'] and hashlib.sha256(code).hexdigest() == fragment['sha256']
    assert all(v is None for v in evidence['runtime_interval_seconds'].values())
    assert verification['passed'] == 16 and verification['failed'] == 0
    for v in verification['vectors']:
        if v['case'] == 'simple_stop_delay':
            assert struct.pack('<f', v['reevaluation_delay_float32']).hex() == v['bits_le'] == 'cdcccc3d'
        else:
            assert v['returned_status'] == (0 if v['melee_started_phase'] and not v['weapon_active_predicate'] else 2)
    graphs = json.loads(data['prior-evidence/beast-logic-graphs.json'])
    cat = next(n for g in graphs if g['path'].endswith('/CatbrowLogicGraph')
               for n in g['nodes'] if n['id'] == 19)
    endpoints = re.search(r'TimeBetweenAttacksRange=\{([^}]+)\}', cat['raw'])[1].split(',')
    assert endpoints == ['0.1', '0.1']
    attacks = json.loads(data['prior-evidence/representative-beast-attacks.json'])
    proxies = {}
    for family, clip, delay in [('catbrow', '/Lotus/Animations/Creatures/Catbrow/Melee01_anim.fbx', float(endpoints[0])),
                                ('kubrow', '/Lotus/Animations/Creatures/KubrowFeral/Melee_anim.fbx', 0.0)]:
        row = next(r for r in attacks if r['direct_reference']['path'] == clip)
        base = row['base_clip_rows'][0]
        assert canonical(1 / base['sequence_rate_float32']) == canonical(base['base_clip_seconds_float32'])
        proxies[family] = {'reference': row, 'post_action_delay_seconds': canonical(delay),
            'assumed_seconds': canonical(base['base_clip_seconds_float32'] + struct.unpack('<f', struct.pack('<f', delay))[0])}
    return {'source': binding, 'family_proxies': proxies, 'weapon_proxies': [],
        'verification': verification, 'runtime_interval_seconds': evidence['runtime_interval_seconds'],
        'limitations': evidence['limitations'], 'scope': 'Ordinary-action proxies selected by inherited pet weapon family; variant AI and runtime completion remain uncalibrated.',
        'verified_native_functions': len(evidence['functions']), 'verified_outer_files': len(manifest['files']),
        'verified_archive_members': sum(len(v) for v in manifest['archive_members'].values())}


def main():
    global CHECK
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--exchange', type=Path, required=True)
    ap.add_argument('--check', action='store_true')
    args = ap.parse_args()
    CHECK = args.check
    export_path = ROOT / 'vendor/public-export/en/ExportUpgrades_en.json'
    export = {r['uniqueName']: r for r in json.loads(export_path.read_bytes())['ExportUpgrades']}
    batches = args.exchange / 'batches'
    data, companion_source = source(batches / '20261107T231054Z-companion-cadence-aiming',
        'companion-evidence.zip', ['metadata-composed.json', 'weapon-state-profiles.json',
                                   'native/cadence-verification.json'])
    names, name_source = source(batches / '20261106T150853Z-client-metadata-offline',
        'data.zip', ['dict.en.json', 'dict.zh.json'])
    unified, mod_source = source(batches / '20261107T040140Z-unified-precision',
        'evidence.zip', ['data/entries.json', 'data/canonical-rank-values.csv'])
    metadata = {r['path']: r for r in json.loads(data['metadata-composed.json'])}
    profiles = json.loads(data['weapon-state-profiles.json'])
    english, chinese = (json.loads(names[f'dict.{lang}.json']) for lang in ['en', 'zh'])
    entries = json.loads(unified['data/entries.json'])
    ranks = list(csv.DictReader(io.StringIO(unified['data/canonical-rank-values.csv'].decode('utf-8-sig'))))
    fixture = {'sources': [companion_source, name_source, mod_source],
               'display_source_sha256': hashlib.sha256(export_path.read_bytes()).hexdigest(),
               'cadence_vectors': json.loads(data['native/cadence-verification.json']),
               'refire_profiles': [], 'claws': [], 'mod_inputs': []}
    fixture['assumed_intervals'] = ordinary_interval_proxies(batches, data['metadata-composed.json'],
                                                          companion_source['manifest_sha256'])
    docs = {d['id']: (p, d) for p in (ROOT / 'data/weapons').rglob('*.yaml')
            if (d := yaml.safe_load(p.read_text())).get('id')}
    grouped = {}
    for r in profiles:
        grouped.setdefault(r['path'], []).append(r)
    for identity, (p, doc) in sorted(docs.items()):
        parent = docs.get(doc.get('transform_group'), (None, {}))[1]
        path = doc.get('internal_name', parent.get('internal_name'))
        candidates = grouped.get(path, [])
        if 'incarnon' in doc.get('form', '') or not candidates:
            continue
        if doc.get('default_form'):
            candidates = [r for r in candidates if r['profile_index'] == 0]
        else:
            rate = doc['attack']['fire_rate']
            candidates = [r for r in candidates if math.isclose(float(
                r['state_fields'].get('fireRate', '=0').partition('=')[2]) / 60,
                rate, rel_tol=0.005)]
        if len(candidates) != 1:
            continue
        r = candidates[0]
        if r['state_type'] not in ['/EE/Types/Game/WeaponSemiAutomaticStateBehavior',
                                   '/Lotus/Types/Weapon/LotusSemiAutomaticStateBehavior']:
            continue
        if doc['attack'].get('charge_seconds') is not None or doc['attack'].get('burst'):
            continue
        rule = {'kind': 'semi_automatic', 'skip_wait_on_empty':
                r['state_fields'].get('SkipRefireWaitTimerOnEmptyClip') == 'SkipRefireWaitTimerOnEmptyClip=1'}
        text = p.read_text()
        text = re.sub(r'(?m)^  refire:.*\n', '', text)
        output(p, re.sub(r'(?m)^(  fire_rate:.*)$',
            lambda m: m[1] + '\n  refire: ' + json.dumps(rule), text, count=1))
        snapshot = {k: r[k] for k in ['path', 'profile_index', 'state_type']}
        snapshot['state_fields'] = {k: v for k, v in r['state_fields'].items() if k in
            ['fireRate', 'IsAutomatic', 'SkipRefireWaitTimerOnEmptyClip', 'SkipRefireOnEmpty']}
        fixture['refire_profiles'].append({'weapon': identity, 'rule': rule, 'source': snapshot})

    fixture['rate_inputs'] = []
    for identity, (p, doc) in sorted(docs.items()):
        path = doc.get('internal_name', '')
        candidates = grouped.get(path, [])
        if len(candidates) != 1 or not path.startswith('/Lotus/Types/Sentinels/SentinelWeapons/'):
            continue
        r = candidates[0]
        if r['state_type'] != '/EE/Types/Game/WeaponGrenadeStateBehavior':
            continue
        rpm = float(r['state_fields']['fireRate'].partition('=')[2])
        factor = struct.unpack('<f', struct.pack('<f', 1 / 60))[0]
        rate = canonical(rpm * factor)
        text = re.sub(r'(?m)^  refire:.*\n', '', p.read_text())
        note = 'The active-projectile cap, recall and return availability are not simulated; the fire rate is the nominal refire timer.'
        if note not in text:
            text = text.replace('unmodeled:\n', 'unmodeled:\n  - ' + json.dumps(note) + '\n', 1)
        output(p, re.sub(r'(?m)^  fire_rate:.*$', f'  fire_rate: {rate}\n  refire: {{kind: native_rate}}', text, count=1))
        fixture['rate_inputs'].append({'weapon': identity, 'path': path, 'rpm': rpm, 'rate': rate})

    weapon_dir = ROOT / 'data/weapons/beast_claws'
    weapon_dir.mkdir(exist_ok=True)
    for path, r in metadata.items():
        if not path.startswith('/Lotus/Types/Friendly/Pets/BeastWeapons/') or not path.endswith('PetWeapon'):
            continue
        f = r['fields']
        label = f['LocalizeTag'].partition('=')[2]
        name = english[label]
        identity = re.sub(r'[^a-z0-9]+', '_', name.lower()).strip('_')
        b = fields(next(blocks(f['Behaviors'])))
        impact = fields(next(v for k, v in b.items() if k.startswith('impact:') and k != 'impact:Type').strip('{}\n'))
        damage = fields(impact['AttackData'].strip('{}\n'))
        if damage['UseNewFormat'] == '1':
            vector = {v: canonical(damage[k]) for k, v in TYPES.items() if float(damage.get(k, '0'))}
        elif damage['Type'] == 'DT_PHYSICAL':
            vector = {TYPES[k]: canonical(float(damage['Amount']) * float(damage[k]))
                      for k in ['DT_IMPACT', 'DT_PUNCTURE', 'DT_SLASH'] if float(damage[k])}
        else:
            vector = {TYPES[damage['Type']]: canonical(damage['Amount'])}
        lineage, parent = [], r['parent']
        while parent not in ['/Lotus/Types/Friendly/Pets/CatbrowPetMeleeWeapon',
                             '/Lotus/Types/Friendly/Pets/KubrowPetMeleeWeapon']:
            lineage.append(parent)
            parent = metadata[parent]['parent']
        lineage.append(parent)
        family = 'catbrow' if parent.endswith('/CatbrowPetMeleeWeapon') else 'kubrow'
        estimate = fixture['assumed_intervals']['family_proxies'][family]['assumed_seconds']
        fixture['assumed_intervals']['weapon_proxies'].append({'weapon': identity, 'source': path,
            'parent_chain': lineage, 'proxy_family': family})
        attack = {'trigger': 'melee', 'shot_type': 'hit_scan', 'fire_rate': 1.0,
                  'refire': {'kind': 'assumed_interval', 'assumed_seconds': estimate}, 'multishot': 1.0,
                  'ammo_cost': 0, 'no_magazine': True,
                  'crit_chance': canonical(impact['criticalHitChance']),
                  'crit_multiplier': canonical(impact['criticalHitDamageMultiplier']),
                  'status_chance': canonical(damage['ProcChance']), 'punch_through_m': 0.0,
                  'combo_multiplies_damage': False, 'damage': vector}
        doc = {'id': identity, 'name': name, 'slot': 'sentinel', 'weapon_type': 'melee',
               'wielder_modelled': True, 'no_riven': True,
               'class': 'beast_claws', 'form': 'base', 'default_form': True,
               'mod_pools': ['beast_claws'], 'mastery_rank': 0, 'max_rank': 30,
               'polarities': [], 'internal_name': path, 'co_behavior': 'additive_with_base_damage',
               'attack': attack, 'unmodeled': [
                   'The complete attack cycle is not calibrated. Simulation estimates the interval from a representative ordinary action for this weapon family.',
                   'Attacks are assumed to connect. Companion movement, contact geometry, attack selection, precepts and posture target selection are not simulated.'],
               'source': {'url': 'https://wiki.warframe.com/w/' + name.replace(' ', '_')}}
        output(weapon_dir / f'{identity}.yaml', dump(doc))
        fixture['claws'].append({'weapon': identity, 'name': name, 'name_zh': chinese.get(label, ''),
                                 'source': path, 'icon': f['Icon'], 'attack_data': damage, 'impact': {k: impact[k] for k in
                                 ['criticalHitChance', 'criticalHitDamageMultiplier']}, 'attack': attack})

    mod_dir = ROOT / 'data/mods/beast_claws'
    mod_dir.mkdir(exist_ok=True)
    scalar_map = {'WEAPON_MELEE_DAMAGE': 'base_damage_bonus', 'WEAPON_CRIT_CHANCE': 'crit_chance_bonus',
                  'WEAPON_CRIT_DAMAGE': 'crit_damage_bonus', 'WEAPON_PROC_CHANCE': 'status_chance_bonus',
                  'WEAPON_PROC_TIME': 'status_duration_bonus', 'WEAPON_PROC_DAMAGE': 'status_damage_bonus',
                  'WEAPON_DAMAGE_IF_VICTIM_PROC_ACTIVE': 'condition_overload'}
    for p in sorted(list((ROOT / 'data/companion_mods').glob('*.yaml')) + list(mod_dir.glob('*.yaml'))):
        doc = yaml.safe_load(p.read_text())
        if doc['compat'] != 'beast_claws':
            continue
        card = export[doc['internal_name']]
        clean = lambda r: re.sub(r'<[^>]+>', '', '\n'.join(r['stats']))
        source_description = clean(card['levelStats'][-1])
        source_entries = [r for r in entries if r['owner'] == doc['internal_name']]
        effects, supported = [], []
        for r in source_entries:
            typ, f = r['upgrade_type'], r['fields']
            if r['array'] not in ['Upgrades', 'ExtraUpgrades'] or f.get('ValidModifiers', '{}') != '{}':
                continue
            effect = None
            if typ == 'WEAPON_NOCRIT_DAMAGE' and r['operation'] == 'ADD':
                assert any(e['upgrade_type'] == 'WEAPON_NOCRIT_CHANCE' and
                           e['base']['float32_value'] == 1.0 for e in source_entries)
                effect = {'kind': 'noncrit_damage_bonus'}
            elif typ == 'WEAPON_NOCRIT_CHANCE' and r['base']['float32_value'] == 1.0:
                supported.append(r)
                continue
            elif typ in scalar_map and r['array'] == 'Upgrades' and r['operation'] == 'STACKING_MULTIPLY':
                effect = {'kind': scalar_map[typ]}
            elif typ in ['WEAPON_PERCENT_BASE_DAMAGE_ADDED', 'WEAPON_PERCENT_BASE_DAMAGE_ADDED_POST_COALESCE']:
                if f.get('DamageType') in TYPES:
                    t = TYPES[f['DamageType']]
                    effect = {'kind': 'post_coalesce_element' if typ.endswith('POST_COALESCE') else 'elemental_damage_bonus', 'element': t}
            elif typ in ['WEAPON_DAMAGE_CONVERSION', 'WEAPON_DAMAGE_CONVERSION_POST_COALESCE']:
                if f.get('SymbolFilter') in TYPES:
                    effect = {'kind': 'damage_conversion', 'from': 'physical' if typ == 'WEAPON_DAMAGE_CONVERSION' else 'elemental',
                              'element': TYPES[f['SymbolFilter']]}
                    if any(all(e.get(k) == v for k, v in effect.items()) for e in effects):
                        supported.append(r)
                        continue
            if effect is None:
                continue
            values = [x for x in ranks if x['path'] == r['owner'] and x['array'] == r['array']
                      and x['entry_index'] == str(r['index'])]
            if effect['kind'] != 'damage_conversion':
                values = {int(x['rank']): float(x['canonical_decimal']) for x in values}
                effect.update(rank0=values[0], rankMax=values[doc['max_rank']])
            effect['text'] = source_description
            effects.append(effect)
            supported.append(r)
        if effects:
            if len(supported) < len(source_entries):
                effects.append({'kind': 'unmodelled', 'text': 'Conditional effects and companion behavior are not simulated.'})
            doc['effects'] = effects
        else:
            doc['effects'] = [{'kind': 'unmodelled', 'text': source_description}]
        if 'conditioning' in doc['id']:
            doc['family'] = 'beast_conditioning'
        if doc['id'] in ['burning_claws', 'chilling_claws', 'shocking_claws', 'sepsis_claws']:
            doc['family'] = 'beast_element_conversion'
        if doc['id'].endswith('_posture'):
            doc['family'] = 'beast_posture'
            doc['combos'] = {}
        if '/Sets/Hunter/' in doc['internal_name']:
            doc['set'] = 'hunter'
        low, high = clean(card['levelStats'][0]), clean(card['levelStats'][-1])
        numbers = list(re.finditer(r'\d+(?:\.\d+)?', high))
        lower = list(re.finditer(r'\d+(?:\.\d+)?', low))
        assert len(numbers) == len(lower), doc['id']
        ordered, other = [], list(doc['effects'])
        for at, lo in reversed(list(zip(numbers, lower))):
            if float(at[0]) == float(lo[0]):
                continue
            unit = 100 if high[at.end():].startswith('%') else 1
            line = high[high.rfind('\n', 0, at.start()) + 1:high.find('\n', at.end())
                        if '\n' in high[at.end():] else len(high)].lower()
            match = next((e for e in other if e.get('rankMax') is not None and
                          abs(e['rankMax'] * unit - float(at[0])) < 0.001 and
                          abs(e['rank0'] * unit - float(lo[0])) < 0.001 and
                          effect_phrase(e) in line), None)
            if match is not None:
                other.remove(match)
            else:
                match = {'kind': 'unmodelled', 'text': source_description,
                         'rank0': float(lo[0]) / unit, 'rankMax': float(at[0]) / unit}
            ordered.insert(0, match)
            high = high[:at.start()] + 'X' + high[at.end():]
        doc['description'] = high
        doc['effects'] = ordered + other
        output(mod_dir / p.name, dump(doc))
        if p.parent != mod_dir and not CHECK:
            p.unlink()
        fixture['mod_inputs'].append({'mod': doc['id'], 'source': doc['internal_name'],
            'entries': [{k: r[k] for k in ['array', 'index', 'upgrade_type', 'operation', 'fields', 'base',
                                          'unlock_rank', 'rank_transform'] if k in r} for r in source_entries]})
    assert len(fixture['claws']) == 17
    out = ROOT / 'tests/fixtures/client/cadence-claws-2026-10-28.json'
    out.parent.mkdir(parents=True, exist_ok=True)
    output(out, json.dumps(fixture, indent=2) + '\n')
    print(json.dumps({'refire_profiles': len(fixture['refire_profiles']), 'claws': len(fixture['claws']),
                      'claw_mods': len(fixture['mod_inputs']), 'fixture': str(out)}))


if __name__ == '__main__':
    main()
