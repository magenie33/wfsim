# -*- coding: utf-8 -*-
"""Pull the published Base Radius / Max Damage Bonus for the rows we transcribed."""
import io
import os
import re

txt = io.open(os.path.expanduser('~/sc/pc.txt'), encoding='utf-8').read()
tbl = txt[txt.index('==Primary Compression Table=='):]

rows, cur = [], []
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
        cur.extend(p.strip() for p in re.split(r'\|\|', st[1:]))
if cur:
    rows.append(cur)


def clean(c):
    c = re.sub(r'\{\{Weapon\|([^}|]+)(\|[^}]*)?\}\}', r'\1', c)
    c = re.sub(r'\{\{Resource\|([^}|]+)(\|[^}]*)?\}\}', r'\1', c)
    c = re.sub(r'<ref[^>]*>.*?</ref>', '', c, flags=re.S)
    c = re.sub(r'<[^>]+>', '', c)
    c = re.sub(r'\[\[([^\]|]+)(\|[^\]]*)?\]\]', r'\1', c)
    c = re.sub(r'data-sort="[^"]*"', '', c)
    return c.replace("'''", '').strip()


for r in rows:
    c = [clean(x) for x in r]
    if len(c) < 7 or not c[0] or c[0].startswith('+') or c[0].isdigit():
        continue
    print('%-40s | %-24s | %-8s | %-14s | %-18s | %s'
          % (c[0][:40], c[1][:24], c[2][:8], c[3][:14], c[5][:18], c[6][:22]))
