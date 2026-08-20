# -*- coding: utf-8 -*-
"""Re-wrap the verbatim row inside the compression comments on word boundaries."""
import glob
import io
import textwrap

n = 0
for f in sorted(glob.glob('data/weapons/*/*.yaml')):
    s = io.open(f, encoding='utf-8').read()
    if 'PRIMARY COMPRESSION — the published table names this attack' not in s:
        continue
    lines = s.split('\n')
    out, i, changed = [], 0, False
    while i < len(lines):
        if lines[i].strip() == '# (docs/CATALOGS.md §2):':
            out.append(lines[i])
            out.append(lines[i + 1])          # the blank `  #`
            i += 2
            row = []
            while i < len(lines) and lines[i].startswith('  #   '):
                row.append(lines[i][6:])
                i += 1
            joined = ''.join(row)
            for w in textwrap.wrap(joined, width=70, break_long_words=False):
                out.append('  #   ' + w)
            changed = True
            continue
        out.append(lines[i])
        i += 1
    if changed:
        io.open(f, 'w', encoding='utf-8', newline='').write('\n'.join(out))
        n += 1
print('re-wrapped', n, 'entries')
