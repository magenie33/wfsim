#!/usr/bin/env python3
"""Set each mod's `family` (mutual-exclusivity group, used by the engine) and
`incompatible_with` (documentation) from the wiki `Incompatible` lists.

The engine keys exclusivity off `family`: mods sharing a family string cannot be
equipped together. We derive families by UNION-FIND over the wiki's Incompatible
relationships, restricted to mods actually in our pool (Flawed variants etc. are
excluded). The family name is the shortest id in the component (the base mod —
Primed/Amalgam/Galvanized just add a prefix). Mods whose only incompatibility is
a Flawed variant get no family (nothing in-pool to exclude).

Usage: python scripts/reconcile_families.py --wiki mods.lua [--write]
"""
import argparse
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import wiki_mods  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PISTOL_DIR = os.path.join(ROOT, "data", "mods", "pistol")


def incompatible_names(text, name):
    """The Incompatible = { ... } list for one mod block, as names. Bounded to
    the mod's OWN block via brace-matching (a fixed window would bleed into the
    next mod and grab its Incompatible list)."""
    if " " in name:
        m = re.search(re.escape('["' + name + '"]') + r"\s*=\s*\{", text)
    else:
        m = re.search(r"(?m)^\s*" + re.escape(name) + r"\s*=\s*\{", text)
    if not m:
        return []
    i = m.end() - 1  # at the opening brace
    depth, j = 0, i
    while j < len(text):
        if text[j] == "{":
            depth += 1
        elif text[j] == "}":
            depth -= 1
            if depth == 0:
                break
        j += 1
    im = re.search(r"Incompatible\s*=\s*\{([^}]*)\}", text[i:j])
    if not im:
        return []
    return [x.strip().strip('"') for x in im.group(1).split(",") if x.strip()]


class UF:
    def __init__(self):
        self.p = {}
    def find(self, x):
        self.p.setdefault(x, x)
        while self.p[x] != x:
            self.p[x] = self.p[self.p[x]]
            x = self.p[x]
        return x
    def union(self, a, b):
        self.p[self.find(a)] = self.find(b)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--wiki")
    ap.add_argument("--write", action="store_true")
    args = ap.parse_args()

    text = open(args.wiki, encoding="utf-8").read() if args.wiki else wiki_mods.fetch_module()
    wiki = wiki_mods.parse_module(text)

    # pool: id -> name (from files), and name -> id
    files = {}
    name_to_id = {}
    for fn in sorted(os.listdir(PISTOL_DIR)):
        if not fn.endswith(".yaml"):
            continue
        name = next((re.match(r"name:\s*(.+)", l).group(1).strip()
                     for l in open(os.path.join(PISTOL_DIR, fn), encoding="utf-8")
                     if re.match(r"name:\s*", l)), None)
        if name:
            files[wiki_mods.slug(name)] = fn
            name_to_id[name] = wiki_mods.slug(name)

    uf = UF()
    incompat = {}  # id -> set of in-pool incompatible ids
    for name, mid in name_to_id.items():
        uf.find(mid)
        others = incompatible_names(text, name)
        incompat[mid] = set()
        for o in others:
            oid = name_to_id.get(o)  # only in-pool mods (skips Flawed/absent)
            if oid and oid != mid:
                uf.union(mid, oid)
                incompat[mid].add(oid)

    # components
    comp = {}
    for mid in name_to_id.values():
        comp.setdefault(uf.find(mid), []).append(mid)
    family = {}
    for members in comp.values():
        if len(members) > 1:
            fam = min(members, key=len)  # shortest id = base mod
            for m in members:
                family[m] = fam

    changed = 0
    for mid, fn in files.items():
        path = os.path.join(PISTOL_DIR, fn)
        lines = open(path, encoding="utf-8").readlines()
        # drop existing family / incompatible_with (+ continuations)
        out, skip = [], False
        for l in lines:
            s = l.strip()
            if skip:
                if s and not re.match(r"\w+:", s) and not s.startswith("source:") and l[0] in " ":
                    continue
                skip = False
            if s.startswith("family:") or s.startswith("incompatible_with:"):
                skip = s.startswith("incompatible_with:")  # its list may wrap
                continue
            out.append(l)
        # build the new block
        block = []
        if mid in family:
            block.append(f"family: {family[mid]}\n")
        if incompat.get(mid):
            ids = ", ".join(sorted(incompat[mid]))
            block.append(f"incompatible_with: [{ids}]\n")
        if block:
            idx = next((i for i, l in enumerate(out) if l.startswith("source:")), len(out))
            # ensure a blank line before source
            block.append("\n")
            out[idx:idx] = block
        if out != lines:
            changed += 1
            if args.write:
                open(path, "w", encoding="utf-8", newline="\n").writelines(out)

    fams = {}
    for m, f in family.items():
        fams.setdefault(f, []).append(m)
    print(f"{'wrote' if args.write else 'would change'} {changed} files; {len(fams)} families:")
    for f, ms in sorted(fams.items()):
        print(f"  {f}: {sorted(ms)}")


if __name__ == "__main__":
    main()
