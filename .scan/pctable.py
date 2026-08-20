# -*- coding: utf-8 -*-
"""Dump the published Primary Compression table as rows we can transcribe."""
import io
import os
import re

txt = io.open(os.path.expanduser('~/sc/pc.txt'), encoding='utf-8').read()
tbl = txt[txt.index('==Primary Compression Table=='):]

# Rows are separated by `|-`; each row's cells start with `|` or `||`.
rows = []
cur = []
for line in tbl.split('\n'):
    st = line.strip()
    if st.startswith('|-'):
        if cur:
            rows.append(cur)
        cur = []
        continue
    if st.startswith('!') or st.startswith('{|') or st.startswith('|}'):
        continue
    if st.startswith('|'):
        body = st[1:]
        # `a || b || c` is three cells on one line
        parts = re.split(r'\|\|', body)
        cur.extend(p.strip() for p in parts)
if cur:
    rows.append(cur)


def clean(c):
    c = re.sub(r'\{\{Weapon\|([^}|]+)(\|[^}]*)?\}\}', r'\1', c)
    c = re.sub(r'\{\{Resource\|([^}|]+)(\|[^}]*)?\}\}', r'\1', c)
    c = re.sub(r'\{\{[A-Za-z ]+\|([^}|]+)(\|[^}]*)?\}\}', r'\1', c)
    c = re.sub(r'<ref[^>]*>.*?</ref>', '', c, flags=re.S)
    c = re.sub(r'<[^>]+>', '', c)
    c = re.sub(r'\[\[([^\]|]+)(\|[^\]]*)?\]\]', r'\1', c)
    return c.replace("'''", '').strip()


print('%d rows' % len(rows))
for r in rows:
    cells = [clean(c) for c in r]
    if not cells or not cells[0] or cells[0].isdigit():
        continue
    print(' | '.join(cells))
