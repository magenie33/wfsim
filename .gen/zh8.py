# -*- coding: utf-8 -*-
"""Add the zh overlay lines batch 8 owes."""
import glob
import io

import yaml

BY_PREFIX = {
    "the orb HANGS IN PLACE":
        "那颗球会停在原地并伤害走进去的敌人，而主攻击可以打穿它获得额外伤害（维基）—— "
        "这是两个模式之间的两步联动，而本引擎是各自独立地发射每一个模式",
    "a direct hit forces a KNOCKDOWN and the explosion a RAGDOLL":
        "直接命中必定造成击倒、爆炸必定造成布娃娃（模块 ForcedProcs）—— "
        "两者都不是伤害类型，而这个战场里的敌人既不会被击倒也不会被抛飞",
    "the projectiles HOME on enemies":
        "弹丸会追踪敌人（维基）—— 这个战场里的子弹本来就打在瞄准的地方，所以追踪在这里买不到任何东西",
}

ui_path = 'data/i18n/zh/ui.yaml'
ui = yaml.safe_load(io.open(ui_path, encoding='utf-8'))['ui']

want = set()
for f in glob.glob('data/weapons/*/*.yaml'):
    d = yaml.safe_load(io.open(f, encoding='utf-8'))
    for u in d.get('unmodeled') or []:
        if isinstance(u, str) and u not in ui:
            want.add(u)

PAIRS, missed = {}, []
for text in sorted(want):
    hit = None
    for pre, zh in BY_PREFIX.items():
        if text.startswith(pre) and (hit is None or len(pre) > len(hit[0])):
            hit = (pre, zh)
    if hit:
        PAIRS[text] = hit[1]
    else:
        missed.append(text)
if missed:
    for m in missed:
        print('NO TRANSLATION:', m[:120])
    raise SystemExit(1)

lines = io.open(ui_path, encoding='utf-8').read().split('\n')
DQ = '"'
BS = chr(92)


def esc(s):
    return DQ + s.replace(BS, BS + BS).replace(DQ, BS + DQ) + DQ


def key_of(line):
    if not line.startswith('  ') or len(line) < 4:
        return None
    q = line[2]
    if q not in (DQ, "'"):
        return None
    i, buf = 3, []
    while i < len(line):
        c = line[i]
        if q == DQ and c == BS and i + 1 < len(line):
            buf.append(line[i + 1])
            i += 2
            continue
        if c == q:
            if q == "'" and i + 1 < len(line) and line[i + 1] == "'":
                buf.append("'")
                i += 2
                continue
            break
        buf.append(c)
        i += 1
    else:
        return None
    if i + 1 >= len(line) or line[i + 1] != ':':
        return None
    return ''.join(buf)


def rows():
    return [(i, k) for i, k in ((i, key_of(l)) for i, l in enumerate(lines)) if k is not None]


r = rows()
existing = {k for _, k in r}
added = 0
for k in sorted(PAIRS, reverse=True):
    if k in existing:
        continue
    at = next((i for i, ek in r if ek > k), r[-1][0] + 1)
    lines.insert(at, '  %s: %s' % (esc(k), esc(PAIRS[k])))
    r = rows()
    added += 1

io.open(ui_path, 'w', encoding='utf-8', newline='').write('\n'.join(lines))
print('added', added, 'of', len(PAIRS))
