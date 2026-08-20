# -*- coding: utf-8 -*-
"""A comment that names a BLOCK the file does not contain reads as a checked fact.

The Mutalist Cernos said "the cloud is carried below as `lingering:`" and had no
such block. This looks for the rest of that class.
"""
import glob
import io
import re

BLOCKS = ['lingering', 'radial', 'beam', 'battery', 'gauge_form', 'sustained_fire_rate',
          'forced_procs', 'scope', 'burst', 'compression', 'ricochet', 'falloff',
          'pseudo_reload', 'valence', 'perks', 'headshot_multiplier', 'charge_seconds',
          'range_m', 'punch_through_m', 'projectile_speed_mps', 'transforms_from']

bad = 0
for f in sorted(glob.glob('data/weapons/*/*.yaml')):
    text = io.open(f, encoding='utf-8').read()
    lines = text.split('\n')
    # every key the file actually declares, at any depth
    keys = set()
    for ln in lines:
        m = re.match(r'\s*([a-z_]+):', ln)
        if m and not ln.lstrip().startswith('#'):
            keys.add(m.group(1))
    # every block a COMMENT names in backticks
    for i, ln in enumerate(lines):
        st = ln.strip()
        if not st.startswith('#'):
            continue
        for m in re.finditer(r'`([a-z_]+):?`', st):
            name = m.group(1)
            if name not in BLOCKS or name in keys:
                continue
            # "no `radial:`" / "carries no" / "cannot" — a comment may name a
            # block precisely to say it is ABSENT.
            low = st.lower()
            if any(w in low for w in ('no `', 'not ', 'never', 'cannot', "does not", 'without',
                                      'absent', 'omit', 'neither', 'rather than', 'instead of',
                                      'lacks', 'none', 'unread', 'left out', 'would')):
                continue
            print('%-52s line %-4d %s' % (f.replace('\\', '/'), i + 1, st[:96]))
            bad += 1
print('%d comment(s) name a block the file does not declare' % bad)
