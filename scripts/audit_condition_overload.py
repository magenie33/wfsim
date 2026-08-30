"""Reconcile every roster entry with the wiki's Condition Overload Attack Catalog.

WHY THIS EXISTS, and it is the whole lesson. A weapon yaml can open with
"NO row in the wiki's CO catalog" — and that check had been run against
**docs/CATALOGS.md**, our own transcription, which by construction carries only
"rows the roster already has". Asking it whether a NEW weapon has a row can only
ever answer no. On 2026-08-20 somebody read the WIKI PAGE instead and found
forty-four entries the catalog names and the roster contradicted, a third of them
weapons that had been here for months (the Lanka at 38%, both Laser Rifles, the
whole Cernos family). Condition Overload is on most builds, so each was a wrong
damage number rather than a wrong comment.

`the_only_condition_overload_anomalies_are_the_ones_the_catalog_names` is the
CI-side guard and it protects a different thing: that OUR data does not drift
from OUR list. Only this tool can see the wiki gaining a row.

MAPPING A ROW TO AN ENTRY. `co_behavior` is per ENTRY and the catalog is per
ATTACK, so the row's Attack Name column has to name a form. The catalog names an
attack the way that WEAPON's page does — "Projectile Impact", "Slug Impact",
"Reload From Empty Impact", "Lock-On Mode" — so ATTACK_FORM below is a
vocabulary rather than a rule, and a name it does not know is REPORTED rather
than skipped. That is the second lesson: the first pass used a short list and
under-reported nine rows in silence.

    python scripts/audit_condition_overload.py            # report
    python scripts/audit_condition_overload.py --fetch    # re-download first

Reads the wiki over the network, so it does not run in CI — a bench tool,
catalogued in docs/DATA_SOURCES.md.
"""
import glob
import io
import os
import re
import sys
import urllib.request

import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CACHE = os.path.join(ROOT, 'target', 'condition_overload.wikitext')
URL = ('https://wiki.warframe.com/index.php'
       '?title=Condition_Overload_(Mechanic)&action=raw')

# The catalog's Attack Name -> the FORM of ours it belongs to. A row about an
# AoE PART is not here: that is `takes_condition_overload` on the part, checked
# separately below.
ATTACK_FORM = {
    'normal attack': ('base',), 'main-fire': ('base',), 'primary fire': ('base',),
    'primary-fire': ('base',), 'projectile impact': ('base',), 'direct hit': ('base',),
    'uncharged direct hit': ('base',), 'slug impact': ('base',), 'slug': ('base',),
    'blob impact': ('base',), 'arrow direct hit': ('base',), 'rocket impact': ('base',),
    'partial reload impact': ('base',), 'reload from empty impact': ('base',),
    'uncharged attack': ('base',), 'uncharged shot': ('base',),
    'charged attack': ('charged',), 'charged shot': ('charged',),
    'perfect shot': ('charged',),
    'alt-fire': ('alt_fire', 'charged'), 'alt fire impact': ('alt_fire', 'charged'),
    'lock-on mode': ('alt_fire',), 'burst mode': ('alt_fire',), 'burst shot': ('alt_fire',),
    'throw': ('alt_fire',), 'semi-mode': ('semi_auto', 'alt_fire'),
    'incarnon mode': ('incarnon',), 'incarnon form': ('incarnon',),
}
# An AoE row is one of these words; the flag it maps to is on the part.
AOE_WORDS = ('aoe', 'radial', 'cloud', 'explosion', 'blast', 'bomblet', 'detonation',
             'pulse', 'singularity', 'orb', 'child bomb', 'tendrils', 'lightning strikes',
             'turret bullets', 'disk bonk', 'appendages', 'spore', 'cluster bombs')


def wikitext(fetch):
    if fetch or not os.path.exists(CACHE):
        req = urllib.request.Request(URL, headers={'User-Agent': 'wfsim/1.0'})
        text = urllib.request.urlopen(req, timeout=60).read().decode('utf-8')
        os.makedirs(os.path.dirname(CACHE), exist_ok=True)
        io.open(CACHE, 'w', encoding='utf-8').write(text)
        return text
    return io.open(CACHE, encoding='utf-8').read()


def strip(c):
    c = re.sub(r'\{\{(?:Weapon|Resource|M|WF|D)\|([^}|]+)(\|[^}]*)?\}\}', r'\1', c)
    c = re.sub(r'<ref[^>]*>.*?</ref>', '', c, flags=re.S)
    c = re.sub(r'<[^>]+>', '', c)
    c = re.sub(r'\[\[([^\]|]+)(\|[^\]]*)?\]\]', r'\1', c)
    return c.replace("'''", '').replace("''", '').strip()


def rows_of(text):
    cat = text[text.index('==Attack Catalog=='):text.index('==Patch History==')]
    out = []
    for chunk in cat.split('|-'):
        if '{{Weapon|' not in chunk and '{{Resource|' not in chunk:
            continue
        cells = [c.strip() for c in chunk.strip().lstrip('|').strip().split('||')]
        if len(cells) < 7:
            continue
        out.append(dict(weapon=strip(cells[0].lstrip('|')), attack=strip(cells[1]),
                        rel=cells[5].strip(), kind=cells[6].split('\n')[0].strip()))
    return out


def pct(s):
    m = re.match(r'([\d.]+)\s*%', s.replace(',', ''))
    return float(m.group(1)) / 100.0 if m else None


def main(fetch):
    rows = rows_of(wikitext(fetch))
    specs = {}
    for f in sorted(glob.glob(os.path.join(ROOT, 'data/weapons/*/*.yaml'))):
        d = yaml.safe_load(io.open(f, encoding='utf-8'))
        specs[d['id']] = d

    def display(d):
        return (d.get('name')
                or specs.get(d.get('inherits'), {}).get('name', '')).split(' (')[0]

    fam = {}
    for wid, d in specs.items():
        fam.setdefault(display(d), []).append(wid)

    bad, unknown, aoe, checked = [], [], [], 0
    for r in rows:
        ids = fam.get(r['weapon'])
        if not ids:
            continue
        key = re.sub(r'\s*\(.*\)$', '', r['attack'].lower().strip())
        if any(w in r['attack'].lower() for w in AOE_WORDS):
            aoe.append(r)
            continue
        forms = ATTACK_FORM.get(key)
        if forms is None:
            unknown.append('%s | %s' % (r['weapon'], r['attack']))
            continue
        k = r['kind'].lower()
        want = ('independent' if k.startswith('multipl')
                else 'additive_with_base_damage' if k.startswith('add')
                else 'inert' if r['rel'].strip().startswith('0')
                else None)
        if want is None:
            continue
        frac = pct(r['rel'])
        for wid in sorted(ids):
            d = specs[wid]
            if d.get('form') not in forms:
                continue
            checked += 1
            have_k = d.get('co_behavior')
            have_f = float(d.get('co_base_fraction', 1.0))
            if have_k != want or (frac and want != 'inert' and abs(have_f - frac) > 5e-3):
                bad.append('%s: ours %s x%g, catalog %s x%s  (%s | %s)'
                           % (wid, have_k, have_f, want, frac, r['weapon'], r['attack']))

    print('%d entry/attack pairs reconciled, %d disagreement(s)' % (checked, len(bad)))
    for b in bad:
        print('  ', b)
    if unknown:
        print('\n%d catalog row(s) name a roster weapon and an ATTACK this tool cannot place '
              '— add it to ATTACK_FORM or confirm it is an AoE part:' % len(unknown))
        for u in unknown:
            print('  ', u)
    print('\n%d AoE-part rows (checked by `takes_condition_overload` on the part, '
          'not by `co_behavior`)' % len(aoe))
    return 1 if (bad or unknown) else 0


if __name__ == '__main__':
    raise SystemExit(main('--fetch' in sys.argv))
