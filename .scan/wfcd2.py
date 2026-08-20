# -*- coding: utf-8 -*-
"""WFCD cross-check, second pass — only the fields that are comparable.

Dropped from the first pass and why:
  - `totalDamage`: WFCD sums the direct hit AND its explosion, so every weapon
    with a `radial:` differs by design.
  - ARCH-GUN and COMPANION entries: two stat columns, and WFCD carries the
    ARCHWING one (data/README.md).
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

FIELDS = [('mastery_rank', 'masteryReq'), ('disposition', 'omegaAttenuation'),
          ('magazine', 'magazineSize'), ('reload_seconds', 'reloadTime')]
TOL = 5e-3

specs = {}
for f in sorted(glob.glob(os.path.join(ROOT, 'data/weapons/*/*.yaml'))):
    d = yaml.safe_load(io.open(f, encoding='utf-8'))
    d['_path'] = f.replace('\\', '/')
    specs[d['id']] = d


def near(a, b):
    return abs(float(a) - float(b)) <= TOL * max(1.0, abs(float(b)))


checked, findings = 0, []
for wid, d in sorted(specs.items()):
    if d.get('inherits'):
        continue
    if '/archgun/' in d['_path'] or '/sentinel/' in d['_path']:
        continue
    it = idx.get(d.get('internal_name'))
    if it is None:
        continue
    checked += 1
    for ours, theirs in FIELDS:
        if ours not in d or it.get(theirs) is None:
            continue
        if not near(d[ours], it[theirs]):
            findings.append('%-26s %-16s yaml %-8s WFCD %s'
                            % (wid, ours, d[ours], it[theirs]))

print('%d primary/secondary entries joined to WFCD, %d disagreement(s)'
      % (checked, len(findings)))
for f in findings:
    print('  ', f)
