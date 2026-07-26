#!/usr/bin/env python3
"""Set each mod's verbatim X-templated `description` and per-effect `rank0`/`max`
from the REAL per-rank data (warframestat levelStats), variant-matched to the
wiki's canonical MaxRank.

Why two sources: the wiki `Module:Mods/data` is authoritative for mechanical
fields and the canonical MaxRank, but only carries the max-rank text. The
per-RANK values (rank 0 … max) live in warframestat's `levelStats`. Picking the
warframestat variant whose `fusionLimit == wiki MaxRank` avoids the stale copies
(e.g. Seeker: wiki max 5 → +2.1, not the stale rank-10 +3.9).

Model (user 2026-07-26):
  - `description:` = the in-game text with the rank-VARYING number(s) shown as X
    (diff rank0 vs max text; only the differing token becomes X — fixed numbers
    like "x2"/"2.5"/"9s" stay). Faithful to what the game shows.
  - each effect carries the REAL `rank0:` and `max:` values (NOT a linear guess —
    Hemorrhage starts at 10%, not max/(rank+1)).

Effect kinds are NOT changed (hand-authored kinds are preserved); only rank0/max
are (re)written, matched to the effects IN ORDER. Files whose effect count does
not line up with the stat lines are left alone and flagged.

Usage: python scripts/reconcile_ranks.py --wfstat mods.json [--wiki mods.lua] [--write]
"""
import argparse
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import wiki_mods  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PISTOL_DIR = os.path.join(ROOT, "data", "mods", "pistol")


def strip_markup(s):
    return re.sub(r"<[^>]*>", "", s).strip()


def value_of(text):
    """Numeric value of a stat string as a fraction (or raw for non-%)."""
    t = strip_markup(text)
    mult = re.search(r"x\s*(\d+(?:\.\d+)?)", t.lower())
    if mult and "x2 when" not in t.lower():
        return round(float(mult.group(1)) - 1.0, 4)
    m = re.search(r"([+\-]?\d+(?:\.\d+)?)\s*%", t)
    if m:
        return round(float(m.group(1)) / 100.0, 4)
    m = re.search(r"([+\-]?\d+(?:\.\d+)?)", t)
    return round(float(m.group(1)), 4) if m else None


def x_template(a, b):
    """In-game text with the rank-varying number as X (diff rank0 vs max)."""
    a, b = strip_markup(a), strip_markup(b)
    ta, tb = a.split(), b.split()
    if len(ta) == len(tb):
        return " ".join(
            tb[i] if ta[i] == tb[i] else re.sub(r"[+\-]?\d+(?:\.\d+)?", "X", tb[i])
            for i in range(len(tb))
        )
    return b  # structure differs between ranks — keep max text as-is


def is_prefix(line):
    """A condition prefix like 'On Kill:' — not a stat effect."""
    s = strip_markup(line)
    return s.endswith(":") and not re.search(r"\d", s)


def wfstat_variant(entries, max_rank):
    """The warframestat entry whose fusionLimit matches the wiki MaxRank."""
    exact = [e for e in entries if e.get("fusionLimit") == max_rank]
    if exact:
        return exact[0]
    # fallback: highest fusionLimit <= max_rank, else the highest
    le = sorted((e for e in entries if (e.get("fusionLimit") or 0) <= max_rank),
                key=lambda e: e.get("fusionLimit") or 0)
    return le[-1] if le else max(entries, key=lambda e: e.get("fusionLimit") or 0)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--wfstat", required=True, help="warframestat mods JSON dump")
    ap.add_argument("--wiki", help="saved Module:Mods/data (for canonical MaxRank)")
    ap.add_argument("--write", action="store_true")
    args = ap.parse_args()

    wiki = wiki_mods.load(args.wiki)
    wf = json.load(open(args.wfstat, encoding="utf-8"))
    by_name = {}
    for e in wf:
        by_name.setdefault(e["name"], []).append(e)

    changed, flagged = 0, []
    for fn in sorted(os.listdir(PISTOL_DIR)):
        if not fn.endswith(".yaml"):
            continue
        path = os.path.join(PISTOL_DIR, fn)
        lines = open(path, encoding="utf-8").readlines()
        name = next((re.match(r"name:\s*(.+)", l).group(1).strip()
                     for l in lines if re.match(r"name:\s*", l)), None)
        w = wiki.get(name)
        ents = by_name.get(name)
        if not w or not ents:
            flagged.append(f"{fn}: no wiki/warframestat match")
            continue
        v = wfstat_variant(ents, w.get("MaxRank"))
        ls = v.get("levelStats") or []
        if len(ls) < 2:
            continue
        s0 = [s for s in ls[0].get("stats", [])]
        sN = [s for s in ls[-1].get("stats", [])]
        # Description = the WIKI text (AUTHORITATIVE and fuller than warframestat's
        # terse levelStats), with the rank-VARYING number(s) shown as X.
        # warframestat (a FAST source only; wiki is the authority — user
        # 2026-07-26) is used just to learn WHICH number varies and its values.
        desc = strip_markup(w.get("Description") or "")
        desc = desc.replace("\\r\\n", "\n").replace("\r\n", "\n").replace("\r", "")
        for i in range(min(len(s0), len(sN))):
            if is_prefix(sN[i]) or value_of(s0[i]) == value_of(sN[i]):
                continue
            mnum = re.search(r"[+\-]?(\d+(?:\.\d+)?)", strip_markup(sN[i]))
            if mnum:
                tok = mnum.group(1)
                desc = re.sub(r"(?<![\d.])" + re.escape(tok) + r"(?![\d.])", "X", desc)
        desc = desc.replace("\n", "\\n").replace('"', '\\"')
        # rank0/max per non-prefix stat line, matched to effects in order.
        stat_idx = [i for i in range(len(sN)) if not is_prefix(sN[i])]
        vals = [(value_of(s0[i]), value_of(sN[i])) for i in stat_idx]

        # Block-style effects carrying a `rankMax:`. Matched to the stat lines
        # IN ORDER, and ONLY when the counts line up exactly; otherwise the file
        # is FLAGGED and its (hand-authored) values are left untouched — this is
        # how multi-effect mods with hidden stats (e.g. Amalgam Barrel Diffusion:
        # 3 effects vs 2 tooltip lines) stay correct. Kind-based matching is the
        # robust upgrade; until then, mismatches are hand-verified.
        eff_maxlines = [i for i, l in enumerate(lines)
                        if re.match(r"\s+rankMax:\s*-?\d", l)]
        out = None
        if eff_maxlines and len(eff_maxlines) == len(vals):
            out = []
            k = 0
            for l in lines:
                if re.match(r"\s+(per_rank|rank0):", l):
                    continue  # drop old values (idempotent re-run)
                mm = re.match(r"(\s+)rankMax:\s*-?\d", l)
                if mm and k < len(vals):
                    r0, mx = vals[k]
                    ind = mm.group(1)
                    if r0 is not None:
                        out.append(f"{ind}rank0: {r0}\n")
                    out.append(f"{ind}rankMax: {mx}\n")
                    k += 1
                    continue
                out.append(l)
        else:
            flagged.append(f"{fn}: {len(eff_maxlines)} rankMax-effects vs {len(vals)} stat lines — values left as-is")
            out = [l for l in lines]
        # replace / insert description
        did_desc = False
        for i, l in enumerate(out):
            if l.lstrip().startswith("description:"):
                out[i] = f'description: "{desc}"\n'
                did_desc = True
                break
        if not did_desc:
            idx = next((i for i, l in enumerate(out) if l.lstrip().startswith("internal_name:")), None)
            if idx is not None:
                out.insert(idx + 1, f'description: "{desc}"\n')

        if out != lines:
            changed += 1
            if args.write:
                open(path, "w", encoding="utf-8", newline="\n").writelines(out)

    print(f"{'wrote' if args.write else 'would change'} {changed} files")
    if flagged:
        print(f"\n!! {len(flagged)} need review:")
        for f in flagged:
            print("   - " + f)


if __name__ == "__main__":
    main()
