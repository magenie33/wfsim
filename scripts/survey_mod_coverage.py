#!/usr/bin/env python3
"""Compare the Wiki's weapon, Warframe and companion mods with our catalogs.

    python scripts/survey_mod_coverage.py [--refresh] [--intake]

The Wiki identifies released cards; DE is joined only by InternalName for
client text and localization. Intake adds explicitly unmodelled cards and
never overwrites an existing entry. The committed survey is checked without
network access by the engine tests.
"""
import argparse
import re
import subprocess
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from urllib.parse import quote, unquote, urlparse

import yaml

import de_export
import wiki_lua
from catalogue import Dumper, slug
from de_i18n import card_text, clean_line, _already_said

ROOT = Path(__file__).resolve().parent.parent
CACHE = ROOT / 'vendor/wiki/mod_coverage'
WIKI = 'https://wiki.warframe.com/w/'
POOLS = {
    'Primary': 'primary', 'Rifle': 'rifle', 'Rifle (No AoE)': 'rifle',
    'Assault Rifle': 'assault_rifle', 'Shotgun': 'shotgun', 'Sniper': 'sniper',
    'Bow': 'bow', 'Pistol': 'pistol', 'Pistol (No AoE)': 'pistol',
    '{Pistol}': 'pistol', 'Thrown': 'pistol', 'Tome': 'tome',
    'Melee': 'melee', 'Thrown Melee': 'glaive', 'Archgun': 'archgun',
    'Dual Daggers': 'dual_daggers', 'Daggers': 'dagger', 'Polearms': 'polearm',
    'Whips': 'whip', 'Assault Saw': 'assault_saw', 'Blade And Whip': 'blade_and_whip',
    'Claws': 'claws', 'Dual Nikanas': 'dual_nikanas', 'Dual Swords': 'dual_swords',
    'Fists': 'fist', 'Glaives': 'glaive', 'Gunblade': 'gunblade',
    'Heavy Blade': 'heavy_blade', 'Heavy Scythe': 'heavy_scythe', 'Machetes': 'machete',
    'Nikanas': 'nikana', 'Nunchaku': 'nunchaku', 'Rapiers': 'rapier',
    'Scythes': 'scythe', 'Sparring': 'sparring', 'Staves': 'staff',
    'Sword And Shield': 'sword_and_shield', 'Swords': 'sword', 'Tonfas': 'tonfa',
    'Two-Handed Nikana': 'two_handed_nikana', 'Warfans': 'warfan', 'Hammers': 'hammer',
    'Bayonet': 'bayonet',
}
COMPANIONS = {
    'Companion', 'Robotic', 'Sentinel', 'MOA', 'Beast', 'Hound', 'Kubrow', 'Kavat',
    'Predasite', 'Vulpaphyla', 'Helminth Charger', 'Claws', 'Kubrow Claws', 'Kavat Claws',
    'Shade', 'Carrier', 'Diriga', 'Oxylus', 'Wyrm', 'Helios', 'Dethcube', 'Djinn',
    'Nautilus', 'Taxon', 'Vizier Predasite', 'Pharaoh Predasite', 'Medjay Predasite',
    'Adarza Kavat', 'Smeeta Kavat', 'Vasca Kavat', 'Crescent Vulpaphyla',
    'Panzer Vulpaphyla', 'Sly Vulpaphyla', 'Sahasa Kubrow', 'Raksa Kubrow',
    'Huras Kubrow', 'Chesa Kubrow', 'Sunika Kubrow',
}
# Wiki Type is an equippability class, not always its directory.
POOL_OVERRIDE = {'Covert Lethality': 'dagger', 'Adhesive Blast': 'primary'}
ADHESIVE = ['ogris', 'kuva_ogris', 'penta', 'secura_penta', 'carmine_penta',
            'tenet_ferrox', 'tenet_envoy', 'tonkor', 'kuva_tonkor', 'torid',
            'zarr', 'kuva_zarr', 'helstrum']
FAMILIES = {'amalgam_furax_body_count': 'combo_duration',
            'amalgam_ripkas_true_steel': 'true_steel',
            'sniper_ammo_mutation': 'sniper_ammo_mutation',
            'primed_sniper_ammo_mutation': 'sniper_ammo_mutation'}


def fetch(page, refresh=False):
    CACHE.mkdir(parents=True, exist_ok=True)
    path = CACHE / (page.replace('/', '_').replace(':', '_') + '.wiki')
    if refresh or not path.exists():
        r = subprocess.run(['curl', '-fsSL', '--max-time', '60', '-A',
                            'wfsim-dev/1.0 (mod coverage)',
                            WIKI + quote(page.replace(' ', '_'), safe='/:') + '?action=raw'],
                           capture_output=True, text=True, check=True)
        if not r.stdout.strip() or r.stdout.lstrip().startswith('<!DOCTYPE'):
            raise ValueError(f'{page}: no wikitext returned')
        path.write_text(r.stdout, encoding='utf-8')
    return path.read_text(encoding='utf-8')


def catalog():
    out = {}
    for pattern in ['data/mods/**/*.yaml', 'data/warframe_mods/*.yaml',
                    'data/auras/*.yaml', 'data/companion_mods/*.yaml']:
        for path in sorted(ROOT.glob(pattern)):
            row = yaml.safe_load(path.read_text(encoding='utf-8'))
            out[str(path.relative_to(ROOT))] = row
    return out


def title(url):
    return unquote(urlparse(url).path.removeprefix('/w/')).replace('_', ' ')


def scope(row, de, weapons, frames):
    kind = row.get('Type', '')
    u = row.get('InternalName', '')
    if kind == 'Warframe' or kind in frames or kind == 'Aura':
        return 'warframe'
    if kind in COMPANIONS and (kind != 'Claws' or '/MeleeTrees/' not in u):
        return 'companion'
    if kind in POOLS or kind in weapons or kind in {'Unique', 'Robotic Weapon', 'Rifle or Pistol'}:
        return 'weapon'
    if de.get(u, {}).get('type') in {'PRIMARY', 'SECONDARY', 'MELEE', 'ARCH-GUN'}:
        return 'weapon'
    return None


def excluded(row):
    if row.get('Conclave'):
        return 'Conclave-only card; the builder uses PvE pools'
    if row.get('IsFlawed') or row['key'].startswith('Flawed '):
        return 'flawed variant; the catalogs carry the regular card'
    if 'Riven Mod' in row['key']:
        return 'unrolled riven; the riven editor supplies the equipped card'
    if row.get('NotUpgradable'):
        return 'innate ability stance; not an equippable mod item'
    if row.get('_IgnoreEntry') or 'BaseDrain' not in row:
        return 'innate ability stance; not an equippable mod item'
    return None


def client_text(de, row):
    ranks = de.get('levelStats') or []
    body = card_text(ranks[-1].get('stats', [])) if ranks else clean_line(row.get('Description', ''))
    prefix = card_text(de.get('description') or [])
    return prefix + '\n' + body if prefix and not _already_said(prefix, body) else body


def rank_metadata(row):
    page = fetch(row.get('Link', row['Name']))
    stats = re.split(r'==\s*Stats\s*==', page, maxsplit=1, flags=re.I)
    if len(stats) == 1 or '{|' not in stats[1]:
        return abs(row['BaseDrain']), row['MaxRank']
    table = stats[1].split('{|', 1)[1].split('|}', 1)[0]
    costs = []
    for block in re.split(r'(?m)^\|-\s*$', table):
        rank = re.match(r'\s*\|\s*(\d+)\s*(?:\|\||\n\|)', block)
        cost = re.search(r'\|\|?\s*(-?\d+)\s*$', block)
        if rank and cost:
            costs.append((int(rank[1]), abs(int(cost[1]))))
    if not costs:
        return abs(row['BaseDrain']), row['MaxRank']
    if [r for r, _ in costs] != list(range(costs[-1][0] + 1)):
        raise ValueError(f"{row['Name']}: incomplete rank table {costs}")
    return costs[0][1], costs[-1][0]


def sync_aoe_tags():
    tagged = set()
    for slot in ['primary', 'secondary']:
        data = wiki_lua.parse(fetch('Module:Weapons/data/' + slot).replace('math.huge', '1e999'))
        tagged.update(r['InternalName'] for r in data.values()
                      if r.get('InternalName') and 'AOE' in (r.get('CompatibilityTags') or []))
    changed = 0
    for path in ROOT.glob('data/weapons/**/*.yaml'):
        text = path.read_text(encoding='utf-8')
        row = yaml.safe_load(text)
        tags = row.get('compatibility_tags') or []
        if row.get('internal_name') not in tagged or 'aoe' in tags:
            continue
        tags.append('aoe')
        line = 'compatibility_tags: [' + ', '.join(tags) + ']'
        if re.search(r'^compatibility_tags:', text, re.M):
            text = re.sub(r'^compatibility_tags:.*$', line, text, flags=re.M)
        else:
            text = re.sub(r'^(mod_pools:.*)$', r'\1\n' + line, text, count=1, flags=re.M)
        path.write_text(text, encoding='utf-8')
        changed += 1
    print('Weapons declaring their AoE compatibility tag:', changed)


def add_card(row, category, de, weapons, frames, abilities):
    name, kind = row['Name'], row['Type']
    internal = row['InternalName']
    exported = de.get(internal, {})
    text = client_text(exported, row)
    if not text:
        raise ValueError(f'{name}: no client card text')
    card = {'id': slug(name), 'name': name, 'rarity': row['Rarity'].lower(),
            'polarity': row['Polarity'].lower(), 'base_drain': abs(row['BaseDrain']),
            'max_rank': row['MaxRank'], 'internal_name': internal}
    card['base_drain'], card['max_rank'] = rank_metadata(row)
    if row.get('IsExilus'):
        card['exilus'] = True
    if category == 'weapon':
        pool = POOL_OVERRIDE.get(name, POOLS.get(kind))
        if pool is None:
            pool = {'PRIMARY': 'rifle', 'SECONDARY': 'pistol', 'MELEE': 'melee',
                    'ARCH-GUN': 'archgun'}[exported['type']]
        if kind in weapons:
            card['exclusive_to'] = sorted(weapons[kind])
        if name == 'Adhesive Blast':
            card['exclusive_to'] = ADHESIVE
        if kind == 'Thrown':
            card['exclusive_to'] = sorted({i for family in ['Despair', 'Fusilai', 'Hikou', 'Kunai', 'Spira', 'Aegrit', 'Cantare']
                                           for i in weapons.get(family, [])})
        if kind.endswith('(No AoE)'):
            card['excludes_weapon'] = ['aoe']
        if '/DualSource/' in internal:
            card['excludes_weapon'] = ['sentinel_weapon']
        if '/Sets/' in internal:
            card['set'] = slug(row['Set'].removesuffix(' Set'))
        if card['id'] in FAMILIES:
            card['family'] = FAMILIES[card['id']]
        card['lower_ranks_unmodelled'] = True
        path = ROOT / 'data/mods' / pool / (card['id'] + '.yaml')
        card['description'] = text
        card['effects'] = [{'kind': 'unmodelled'}]
    elif category == 'warframe':
        path = ROOT / 'data/warframe_mods' / (card['id'] + '.yaml')
        if kind in frames:
            card['exclusive_to'] = sorted(frames[kind])
        for ability_name, ability_id in abilities.items():
            if text.startswith(ability_name + ' Augment:'):
                card['augments'] = ability_id
                card.pop('exclusive_to', None)
                break
        card['description'] = text
        card['effects'] = [{'kind': 'unmodelled', 'text': text}]
    else:
        path = ROOT / 'data/companion_mods' / (card['id'] + '.yaml')
        card['compat'] = slug(kind)
        if exported.get('type') == 'MELEE' or kind == 'Claws':
            card['compat'] = {'Claws': 'beast_claws', 'Kubrow Claws': 'kubrow_claws',
                              'Kavat Claws': 'kavat_claws'}.get(kind, 'helminth_charger_claws')
        card['precept'] = card['polarity'] == 'penjaga'
        card['description'] = text
        card['effects'] = [{'kind': 'unmodelled', 'text': text}]
    card['source'] = {'url': WIKI + quote(row.get('Link', name).replace(' ', '_'), safe='/:')}
    if path.exists():
        raise ValueError(f'{name}: refusing to overwrite {path}')
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(yaml.dump(card, Dumper=Dumper, allow_unicode=True, sort_keys=False, width=100000), encoding='utf-8')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--refresh', action='store_true')
    parser.add_argument('--intake', action='store_true')
    args = parser.parse_args()
    de = de_export.items()
    wiki = wiki_lua.parse_local(fetch('Module:Mods/data', args.refresh), 'modData')['Mods']
    weapons, frames, abilities = {}, {}, {}
    for path in ROOT.glob('data/weapons/**/*.yaml'):
        w = yaml.safe_load(path.read_text())
        weapons.setdefault(w['name'], set()).add(w['id'])
        if w.get('riven_family'):
            weapons.setdefault(w['riven_family'], set()).add(w['id'])
    for path in ROOT.glob('data/warframes/*.yaml'):
        w = yaml.safe_load(path.read_text())
        frames.setdefault(w['name'].removesuffix(' Prime'), set()).add(w['id'])
    for path in ROOT.glob('data/warframe_abilities/*.yaml'):
        w = yaml.safe_load(path.read_text())
        abilities[w['name']] = w['id']
    current = catalog()
    by_internal = {r.get('internal_name'): p for p, r in current.items() if r.get('internal_name')}
    by_page = {title(r.get('source', {}).get('url', '')): p for p, r in current.items()}
    rows, gaps = [], []
    for key, mod in sorted(wiki.items()):
        row = dict(mod, key=key)
        category = scope(row, de, weapons, frames)
        if not category:
            continue
        entry = {'name': key, 'category': category}
        reason = excluded(row)
        if reason:
            entry['reason'] = reason
        else:
            u = row['InternalName']
            carried = by_internal.get(u) or by_page.get(row.get('Link', row['Name']))
            if carried:
                entry['path'] = carried.removeprefix('data/')
            else:
                entry['internal_name'] = u
            if not carried:
                gaps.append((row, category))
        rows.append(entry)
    if args.intake:
        if any(r.get('Type', '').endswith('(No AoE)') for r, _ in gaps):
            sync_aoe_tags()
        with ThreadPoolExecutor(max_workers=4) as workers:
            list(workers.map(lambda pair: fetch(pair[0].get('Link', pair[0]['Name'])), gaps))
        for row, category in gaps:
            add_card(row, category, de, weapons, frames, abilities)
        current = catalog()
        by_internal = {r.get('internal_name'): p for p, r in current.items() if r.get('internal_name')}
        for entry in rows:
            if 'internal_name' in entry and entry['internal_name'] in by_internal:
                entry['path'] = by_internal[entry.pop('internal_name')].removeprefix('data/')
    survey = {'source': {'url': WIKI + 'Module:Mods/data'}, 'mods': rows}
    out = ROOT / 'data/surveys/wiki_mods.yaml'
    out.write_text('# Generated by scripts/survey_mod_coverage.py; exclusions state their scope.\n'
                   + yaml.dump(survey, Dumper=Dumper, allow_unicode=True, sort_keys=False, width=100000), encoding='utf-8')
    print('Scoped:', dict(Counter(r['category'] for r in rows)))
    print('Excluded:', dict(Counter(r.get('reason') for r in rows if 'reason' in r)))
    print('Added' if args.intake else 'Missing', dict(Counter(c for _, c in gaps)))
    for row, category in gaps:
        print(category, row['Name'])
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
