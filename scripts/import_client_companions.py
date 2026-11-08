#!/usr/bin/env python3
"""Import companion bodies, modular parts and carrier scopes from a closed client batch."""

import argparse
import hashlib
import json
import re
import zipfile
from pathlib import Path

import yaml

import wiki_lua
from import_client_cadence import canonical

ROOT = Path(__file__).resolve().parents[1]
POLARITIES = {'AP_PRECEPT': 'penjaga', 'AP_ATTACK': 'madurai', 'AP_DEFENSE': 'vazarin',
              'AP_TACTIC': 'naramon', 'AP_POWER': 'zenurik', 'AP_UMBRA': 'umbra'}
BODY_FIELDS = ['MaxHealthOverride', 'MaxShieldOverride', 'ArmourRatingOverride',
               'ArtifactSlots', 'SpecialSlotItem', 'IncompatibleWeapons', 'LocalizeTag', 'LevelUpgrades']
PART_FIELDS = ['PartType', 'LocalizeTag', 'LocalizeDescTag', 'ModifyBase', 'OverridePolarity',
               'BaseHealth', 'BaseShields', 'BaseArmor', 'MaxHealthModifier',
               'MaxShieldModifier', 'MaxArmourRatingModifier', 'UngildedMaxHealthModifier',
               'UngildedMaxShieldModifier', 'UngildedMaxArmourModifier', 'Upgrades']
MOD_FIELDS = ['Upgrades', 'FusionLimit', 'ItemCompatibility', 'BaseDrain']


def slug(name):
    return re.sub(r'[^a-z0-9]+', '_', name.lower()).strip('_')


def value(row, key):
    return row['fields'].get(key, '').partition('=')[2]


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--exchange', type=Path, required=True)
    ap.add_argument('--wiki', type=Path, required=True, help='raw Module:Companions/data file')
    ap.add_argument('--export', type=Path, default=ROOT / 'vendor/public-export/en')
    ap.add_argument('--check', action='store_true')
    args = ap.parse_args()
    batch = args.exchange / 'batches/20261106T150853Z-client-metadata-offline'
    raw_manifest = (batch / 'manifest.json').read_bytes()
    manifest = json.loads(raw_manifest)
    ready = json.loads((batch / 'READY.json').read_bytes())
    assert hashlib.sha256(raw_manifest).hexdigest() == ready['manifest_sha256']
    archive = batch / 'data.zip'
    outer = next(x for x in manifest['files'] if x['path'] == 'data.zip')
    with archive.open('rb') as stream:
        assert hashlib.file_digest(stream, 'sha256').hexdigest() == outer['sha256']
    members = {x['path']: x for x in manifest['archive_members']['data.zip']}
    export_hosts = json.loads((args.export / 'ExportSentinels_en.json').read_bytes())['ExportSentinels']
    host_paths = {r['uniqueName'] for r in export_hosts}
    export_weapons = [r for r in json.loads((args.export / 'ExportWeapons_en.json').read_bytes())['ExportWeapons']
                      if r.get('productCategory') == 'SentinelWeapons']
    weapon_paths = {r['uniqueName'] for r in export_weapons}
    stat_mods = {}
    for path in (ROOT / 'data/companion_mods').glob('*.yaml'):
        card = yaml.safe_load(path.read_text())
        if any(e['kind'] == 'body_stat' for e in card['effects']):
            stat_mods[card['internal_name']] = (path, card)
    selected = {}
    with zipfile.ZipFile(archive) as z:
        dictionaries = {}
        for lang in ['en', 'zh']:
            key = f'dict.{lang}.json'
            data = z.read(key)
            assert hashlib.sha256(data).hexdigest() == members[key]['sha256']
            dictionaries[lang] = json.loads(data)
        digest = hashlib.sha256()
        for line in z.open('packages_effective.jsonl'):
            digest.update(line)
            row = json.loads(line)
            path = row['path']
            if path in host_paths or path in weapon_paths or path in stat_mods or path in [
                '/Lotus/Types/Friendly/Pets/MoaPets/MoaPetPowerSuit',
                '/Lotus/Types/Friendly/Pets/ZanukaPets/ZanukaPetPowerSuit'] or (
                path.startswith('/Lotus/Types/Friendly/Pets/') and any(
                    s in path for s in ['/MoaPetParts/', '/ZanukaPetParts/', '/CreaturePetParts/'])
            ):
                selected[path] = row
        assert digest.hexdigest() == members['packages_effective.jsonl']['sha256']
    wiki_raw = args.wiki.read_bytes()
    wiki = {}
    identity_corrections = []
    for row in wiki_lua.parse(wiki_raw.decode())['Companions'].values():
        if not (path := row.get('InternalName')):
            continue
        if row.get('Link') == 'Wyrm/Prime':
            # The module aliases Prime to the base suit; the export has distinct paths.
            corrected = '/Lotus/Types/Sentinels/SentinelPowersuits/PrimeWyrmPowerSuit'
            assert path == '/Lotus/Types/Sentinels/SentinelPowersuits/WyrmPowerSuit'
            assert corrected in host_paths
            identity_corrections.append({'page': row['Link'], 'reported': path, 'corrected': corrected})
            path = corrected
        assert path not in wiki, f'duplicate wiki identity: {path}'
        wiki[path] = row
    outputs, bodies, parts, translations = {}, [], [], {'companions': {}, 'companion_parts': {}, 'ui': {}}
    mod_inputs = []

    def put(path, obj):
        outputs[ROOT / path] = yaml.safe_dump(obj, sort_keys=False, allow_unicode=True, width=120)

    def label(row):
        tag = value(row, 'LocalizeTag')
        return dictionaries['en'].get(tag), dictionaries['zh'].get(tag)

    for path, (file, card) in stat_mods.items():
        upgrades = value(selected[path], 'Upgrades')
        token = re.search(r'^Value=([^\n]+)', upgrades, re.M)[1]
        assert 'OperationType=STACKING_MULTIPLY' in upgrades
        bonus = canonical(canonical(token) * (card['max_rank'] + 1))
        text = re.sub(r'(?m)^    bonus:.*$', f'    bonus: {bonus}', file.read_text())
        outputs[file] = text
        mod_inputs.append({'id': card['id'], 'source': path, 'rank_value': token,
                           'max_rank': card['max_rank'], 'max_bonus': bonus})

    for path in sorted(host_paths):
        row = selected[path]
        name, zh = label(row)
        assert name
        identity = slug(name)
        if '/SentinelPowersuits/' in path:
            kind, groups = 'sentinel', ['companion', 'robotic', 'sentinel', identity.removesuffix('_prime').removeprefix('prisma_')]
        elif '/KubrowPet/' in path:
            kind = 'helminth' if 'Charger' in path else 'kubrow'
            groups = ['companion', 'beast', 'kubrow', identity]
        elif '/CatbrowPet/' in path or '/Khora/Kavat/' in path:
            kind, groups = 'kavat', ['companion', 'beast', 'kavat', identity.removesuffix('_prime')]
        elif 'Predator' in path:
            kind, groups = 'predasite', ['companion', 'beast', 'kubrow', 'predasite', identity]
        else:
            kind, groups = 'vulpaphyla', ['companion', 'beast', 'kavat', 'vulpaphyla', identity]
        stats = {}
        for key, field, fallback in [('health', 'MaxHealthOverride', 'Health'), ('shield', 'MaxShieldOverride', 'Shield'),
                                     ('armor', 'ArmourRatingOverride', 'Armor')]:
            token = value(row, field)
            stats[key] = canonical(token) if token else wiki[path][fallback]
        raw_slots = value(row, 'ArtifactSlots')
        pols = [POLARITIES[p] for p in re.findall(r'AP_\w+', raw_slots) if p in POLARITIES] if raw_slots else [
            p.lower() for p in wiki[path]['Polarities']]
        body = {'id': identity, 'name': name, 'kind': kind, 'groups': sorted(set(groups)), 'stats': stats,
                'polarities': pols, 'internal_name': path,
                'source': {'url': 'https://wiki.warframe.com/w/' + name.replace(' ', '_')}}
        if kind in ['predasite', 'vulpaphyla']:
            body['assembly'] = kind
        bodies.append(body)
        if zh:
            translations['companions'][identity] = zh
    for kind in ['moa', 'hound']:
        path = '/Lotus/Types/Friendly/Pets/' + ('MoaPets/MoaPetPowerSuit' if kind == 'moa' else 'ZanukaPets/ZanukaPetPowerSuit')
        row = selected[path]
        name, zh = label(row)
        pols = [POLARITIES[p] for p in re.findall(r'AP_\w+', value(row, 'ArtifactSlots')) if p in POLARITIES]
        bodies.append({'id': kind, 'name': name, 'kind': kind, 'groups': ['companion', 'robotic', kind],
                       'stats': {'health': 350, 'shield': 350, 'armor': 350}, 'polarities': pols,
                       'assembly': kind, 'internal_name': path,
                       'source': {'url': 'https://wiki.warframe.com/w/' + ('MOA_(Companion)' if kind == 'moa' else 'Hound_(Companion)')}})
        if kind == 'hound':
            bodies[-1].pop('stats')
        else:
            bodies[-1]['precept_limits'] = {'moa': 2}
        if zh:
            translations['companions'][kind] = zh
    part_types = {
        'LWPT_MOA_ENGINE': ('moa', 'core'), 'LWPT_MOA_PAYLOAD': ('moa', 'gyro'),
        'LWPT_MOA_LEG': ('moa', 'bracket'), 'LWPT_MOA_HEAD': ('moa', 'model'),
        'LWPT_ZANUKA_BODY': ('hound', 'core'), 'LWPT_ZANUKA_HEAD': ('hound', 'model'),
        'LWPT_ZANUKA_LEG': ('hound', 'bracket'), 'LWPT_ZANUKA_TAIL': ('hound', 'stabilizer'),
        'LWPT_CATBROW_ANTIGEN': ('vulpaphyla', 'antigen'), 'LWPT_CATBROW_MUTAGEN': ('vulpaphyla', 'mutagen'),
        'LWPT_KUBROW_ANTIGEN': ('predasite', 'antigen'), 'LWPT_KUBROW_MUTAGEN': ('predasite', 'mutagen'),
    }
    for path, row in sorted(selected.items()):
        part_type = value(row, 'PartType')
        name, zh = label(row)
        if part_type not in part_types or not name:
            continue
        family, slot = part_types[part_type]
        part = {'id': slug(name), 'name': name, 'family': family, 'slot': slot, 'internal_name': path}
        if value(row, 'ModifyBase') == '1':
            part['modify_base'] = True
        for key, suffix in [('health', 'Health'), ('shield', 'Shields'), ('armor', 'Armor')]:
            if token := value(row, 'Base' + suffix):
                part.setdefault('base', {})[key] = canonical(token)
        for variant, prefix in [('bonus', 'Max'), ('ungilded_bonus', 'UngildedMax')]:
            for key, suffix in [('health', 'HealthModifier'), ('shield', 'ShieldModifier'), ('armor', 'ArmourRatingModifier')]:
                if variant == 'ungilded_bonus' and key == 'armor':
                    suffix = 'ArmourModifier'
                if token := value(row, prefix + suffix):
                    part.setdefault(variant, {})[key] = canonical(token)
        if (pol := value(row, 'OverridePolarity')) in POLARITIES:
            part['polarity'] = POLARITIES[pol]
        if slot == 'mutagen':
            part['unmodelled'] = ['Mutagen damage resistances and inherited combinations are not simulated.']
        parts.append(part)
        if zh:
            translations['companion_parts'][part['id']] = zh
    assert len([p for p in parts if p['family'] == 'moa']) == 21
    assert len([p for p in parts if p['family'] == 'hound']) == 12
    assert len([p for p in parts if p['family'] in ['vulpaphyla', 'predasite']]) == 16
    scopes = []
    sentinel_ids = [b['id'] for b in bodies if b['kind'] == 'sentinel']
    scopes.extend([
        {'id': 'sentinels', 'companions': sentinel_ids, 'weapons': [], 'default': 'sentinel'},
        {'id': 'moas', 'companions': ['moa'], 'weapons': [], 'default': 'prototype_moa'},
        {'id': 'hounds', 'companions': ['hound'], 'weapons': [], 'default': 'prototype_hound'},
        {'id': 'helios', 'companions': ['helios', 'helios_prime'], 'weapons': [], 'default': 'helios'},
        {'id': 'robotic_ranged', 'companions': sentinel_ids + ['moa'], 'weapons': [], 'default': 'sentinel'},
    ])
    scope_by_id = {s['id']: s for s in scopes}
    weapon_ids = {}
    for file in (ROOT / 'data/weapons/sentinel').glob('*.yaml'):
        spec = yaml.safe_load(file.read_text())
        path = spec['internal_name']
        assert path not in weapon_ids, f'duplicate weapon identity: {path}'
        weapon_ids[path] = spec['id']
    for weapon in export_weapons:
        path = weapon['uniqueName']
        group = 'helios' if 'GlaiveWeapon' in path else 'hounds' if 'ZanukaPetMeleeWeapon' in path else 'robotic_ranged'
        if group == 'hounds':
            identity = weapon_ids.get(path, slug(label(selected[path])[0]))
        else:
            identity = weapon_ids[path]
        scope_by_id[group]['weapons'].append(identity)
        if group == 'hounds':
            scope_by_id[group].setdefault('unmodelled_weapons', []).append({'id': identity, 'name': label(selected[path])[0]})
    for body in bodies:
        row = selected[body['internal_name']]
        binding = value(row, 'SpecialSlotItem')
        if '/BeastWeapons/' not in binding:
            continue
        path = re.search(r'ItemType=([^\n]+)', binding)[1]
        weapon_file = next(p for p in (ROOT / 'data/weapons/beast_claws').glob('*.yaml')
                           if yaml.safe_load(p.read_text())['internal_name'] == path)
        weapon = yaml.safe_load(weapon_file.read_text())
        scopes.append({'id': body['id'], 'companions': [body['id']], 'weapons': [weapon['id']], 'default': body['id']})
    proto_names = {'sentinel': 'Sentinel',
                   'prototype_moa': 'Prototype MOA', 'prototype_hound': 'Prototype Hound'}
    for scope in scopes:
        if scope['id'] == 'robotic_ranged':
            continue
        identity = scope['default']
        if identity not in proto_names:
            continue
        kinds = sorted({b['kind'] for b in bodies if b['id'] in scope['companions']})
        bodies.append({'id': identity, 'name': proto_names[identity], 'kind': kinds[0] if len(kinds) == 1 else 'robotic',
                       'prototype_scope': scope['id']})
    for body in bodies:
        put(f'data/companions/{body["id"]}.yaml', body)
    slot_tags = {'model': '/Lotus/Language/SolarisVenus/MoaPet_PartHead',
                 'core': '/Lotus/Language/SolarisVenus/MoaPet_PartEngine',
                 'gyro': '/Lotus/Language/SolarisVenus/MoaPet_PartPayload',
                 'bracket': '/Lotus/Language/SolarisVenus/ZanukaPet_PartLeg',
                 'stabilizer': '/Lotus/Language/SolarisVenus/ZanukaPet_PartTail',
                 'antigen': '/Lotus/Language/InfestedMicroplanet/KubrowAntigen',
                 'mutagen': '/Lotus/Language/InfestedMicroplanet/KubrowMutagen'}
    slot_labels = {slot: dictionaries['en'][tag] for slot, tag in slot_tags.items()}
    for tag in [*slot_tags.values(), '/Lotus/Language/OstronCrafting/Crafting_Gilded']:
        translations['ui'][dictionaries['en'][tag]] = dictionaries['zh'][tag]
    put('data/companion_parts.yaml', {'slot_labels': slot_labels, 'parts': parts})
    put('data/companion_scopes.yaml', {'scopes': scopes})
    put('data/i18n/zh/companions.yaml', translations)
    kept_fields = {path: {'parent': row['parent'], 'fields': {k: v for k, v in row['fields'].items()
                     if k in (MOD_FIELDS if path in stat_mods else PART_FIELDS if value(row, 'PartType') in part_types else BODY_FIELDS)}}
                   for path, row in sorted(selected.items())}
    fixture = {'source': {'batch_id': batch.name, 'manifest_sha256': ready['manifest_sha256'], 'archive': outer,
                          'members': [members[k] for k in ['packages_effective.jsonl', 'dict.en.json', 'dict.zh.json']]},
               'wiki': {'url': 'https://wiki.warframe.com/w/Module:Companions/data?action=raw',
                        'sha256': hashlib.sha256(wiki_raw).hexdigest(), 'identity_corrections': identity_corrections,
                        'armor_fallback': {b['id']: b['stats']['armor'] for b in bodies if 'stats' in b
                                           and not value(selected[b['internal_name']], 'ArmourRatingOverride')},
                        'polarity_fallback': {b['id']: b['polarities'] for b in bodies if b.get('internal_name') in wiki
                                              and not value(selected[b['internal_name']], 'ArtifactSlots')}},
               'mod_inputs': sorted(mod_inputs, key=lambda r: r['id']), 'records': kept_fields}
    outputs[ROOT / 'tests/fixtures/client/companions-2026-10-28.json'] = json.dumps(fixture, ensure_ascii=False, indent=2) + '\n'
    for path, text in outputs.items():
        if args.check:
            assert path.read_text() == text, f'{path}: differs from source'
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
    print(f'{len(bodies)} bodies, {len(parts)} parts, {len(scopes)} carrier scopes: ' + ('checked' if args.check else 'written'))


if __name__ == '__main__':
    main()
