# -*- coding: utf-8 -*-
"""Cross-check every weapon entry against WFCD's export — the SECOND source.

Every weapon yaml opens with "cross-checked against WFCD warframe-items — 0
disagreements". This is that sentence, executed. The join is
`internal_name` == `uniqueName`, never the display name.

WFCD's shape: weapon-level fields at the top, and per-attack numbers under
`attacks[]` with `shot_type`/`falloff`/`damage`. The weapon-level ones are what
both sources agree on unambiguously, so those are what this compares.
"""
import glob
import io
import json
import os

import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

idx = {}
for f in glob.glob(os.path.join(ROOT, 'vendor/warframe-items/data/json/*.json')):
    try:
        arr = json.load(io.open(f, encoding='utf-8'))
    except Exception:
        continue
    if isinstance(arr, list):
        for it in arr:
            if isinstance(it, dict) and it.get('uniqueName'):
                idx.setdefault(it['uniqueName'], it)

FIELDS = [
    ('mastery_rank', 'masteryReq'),
    ('disposition', 'omegaAttenuation'),
    ('magazine', 'magazineSize'),
    ('reload_seconds', 'reloadTime'),
    ('ammo_max', 'totalDamage'),          # placeholder, replaced below
]
FIELDS = [
    ('mastery_rank', 'masteryReq'),
    ('disposition', 'omegaAttenuation'),
    ('magazine', 'magazineSize'),
    ('reload_seconds', 'reloadTime'),
]
TOL = 2e-3

specs = {}
for f in sorted(glob.glob(os.path.join(ROOT, 'data/weapons/*/*.yaml'))):
    d = yaml.safe_load(io.open(f, encoding='utf-8'))
    specs[d['id']] = d


def near(a, b):
    return abs(float(a) - float(b)) <= TOL * max(1.0, abs(float(b)))


checked, unjoined, findings = 0, [], []
for wid, d in sorted(specs.items()):
    if d.get('inherits'):
        continue
    key = d.get('internal_name')
    it = idx.get(key) if key else None
    if it is None:
        unjoined.append(wid)
        continue
    checked += 1
    for ours, theirs in FIELDS:
        if ours not in d or it.get(theirs) is None:
            continue
        if not near(d[ours], it[theirs]):
            findings.append('%s.%s: yaml %s vs WFCD %s' % (wid, ours, d[ours], it[theirs]))
    # the total base damage of the entry's attack, against WFCD's own total
    total = sum((d.get('attack') or {}).get('damage', {}).values())
    wtot = it.get('totalDamage')
    if wtot:
        # WFCD's totalDamage is the ARSENAL figure, which is the default form's
        # attack — only comparable on a weapon whose entry IS that form.
        if d.get('default_form') and not near(total, wtot):
            findings.append('%s.damage total: yaml %g vs WFCD %g' % (wid, total, wtot))

print('%d weapon entries joined to WFCD, %d finding(s), %d unjoined'
      % (checked, len(findings), len(unjoined)))
for f in findings:
    print('  ', f)
if unjoined:
    print('  unjoined (no uniqueName match):', ', '.join(unjoined))
