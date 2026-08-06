#!/usr/bin/env python3
"""Fill in `data/assets.yaml` from the COMMITTED WFCD export.

The map is id -> image filename; the images themselves live on
`https://cdn.warframestat.us/img/<name>` and no binary enters this repo.

This reads `vendor/warframe-items/data/json/All.json`, which travels WITH the
repo, joined by `internal_name` == `uniqueName` — the same join rule the rest
of `data/` uses, and never by display name (WFCD has stale duplicates sharing
one). The previous generator lived in `private/` and fetched a live API, so it
could not be run by anyone else and its output could not be reproduced.

It only ADDS what is missing. Entries already present are left exactly as they
are, because several are deliberate overrides with comments explaining them —
an Incarnon form shows its base weapon's image, not the Genesis adapter icon.

AND IT REFUSES TO GUESS WRONG. WFCD's `imageName` is a SIBLING'S file for some
weapons: it gives MK1-Furis `Furis.png` and Ocucor `CrpSentExperimentPistol.png`
(which the CDN does not serve at all). Both are hand-set `wiki:` entries today,
and the one run of `--write` that saw them absent replaced them with the
export's answer — a page showing a Furis where an MK1-Furis belongs, which
nothing downstream can notice: the file exists, the fetcher caches it, and the
build's missing-art guard passes. So a proposed filename that another weapon
already wears is NOT written; it is reported for a hand-set override. Two forms
of ONE weapon sharing art is the legitimate case and is exempt.

    python scripts/gen_assets.py           # report what is missing
    python scripts/gen_assets.py --write   # add it
"""

import io
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "data" / "assets.yaml"
EXPORT = ROOT / "vendor" / "warframe-items" / "data" / "json" / "All.json"

# data directory -> the section of assets.yaml it belongs in
SECTIONS = [("mods", "mods"), ("weapons", "weapons"), ("arcanes", "arcanes")]


def data_entries(kind):
    """(id, internal_name) for every yaml under `data/<kind>/`."""
    out = []
    for path in sorted((ROOT / "data" / kind).rglob("*.yaml")):
        text = path.read_text(encoding="utf-8")
        mid = re.search(r"^id:\s*(\S+)", text, re.M)
        internal = re.search(r"^internal_name:\s*(\S+)", text, re.M)
        if mid:
            out.append((mid.group(1), internal.group(1) if internal else None))
    return out


def transform_groups():
    """id -> the transform group it belongs to, from `data/weapons/`.

    Two entries that are FORMS of one weapon may share a picture on purpose —
    an Incarnon form wears its base weapon's, a charged Larkspur wears the
    Larkspur's. That relation is declared (`transform_group:`), so it is read
    rather than guessed off the id's suffix: `_incarnon` and `_uncharged` are
    two of the suffixes in use, `_charged` is a third, and the next form to be
    registered would have been a false alarm here.
    """
    out = {}
    for path in sorted((ROOT / "data" / "weapons").rglob("*.yaml")):
        text = path.read_text(encoding="utf-8")
        mid = re.search(r"^id:\s*(\S+)", text, re.M)
        group = re.search(r"^transform_group:\s*(\S+)", text, re.M)
        if mid:
            # A weapon with one form is a group of one — its own id, which is
            # what `WeaponSpec::group()` returns for the same case.
            out[mid.group(1)] = group.group(1) if group else mid.group(1)
    return out


def main():
    write = "--write" in sys.argv
    export = json.loads(EXPORT.read_text(encoding="utf-8"))
    by_unique = {it["uniqueName"]: it for it in export if it.get("uniqueName")}

    text = ASSETS.read_text(encoding="utf-8")
    have = set(re.findall(r"^\s{2}(\S+):", text, re.M))

    # id -> image for entries ALREADY in the file, so a proposal can be tested
    # against them. Values keep their `wiki:` prefix off for the comparison —
    # `wiki:MK1-Furis.png` and `MK1-Furis.png` are the same picture.
    taken = {}
    for m in re.finditer(r"^  (\S+):\s*(\S+)\s*$", text, re.M):
        taken[m.group(1)] = m.group(2).removeprefix("wiki:")

    group = transform_groups()

    def stem(weapon_id):
        return group.get(weapon_id, weapon_id)

    missing, unresolved, collided = {}, [], []
    for kind, section in SECTIONS:
        if not (ROOT / "data" / kind).is_dir():
            continue
        for mid, internal in data_entries(kind):
            if mid in have:
                continue
            item = by_unique.get(internal) if internal else None
            image = (item or {}).get("imageName")
            if not image:
                unresolved.append((section, mid, internal))
                continue
            # Whose picture is this already? Only another FORM of the same
            # weapon may share it (`<x>` and `<x>_incarnon`), which is the same
            # exemption the Rust check makes off the transform group.
            owners = [
                other
                for other, img in taken.items()
                if img == image and stem(other) != stem(mid)
            ]
            already = [i for i, _ in missing.get(section, []) if stem(i) != stem(mid)]
            owners += [
                i for i, img in missing.get(section, []) if img == image and i in already
            ]
            if owners:
                collided.append((section, mid, image, sorted(set(owners))))
                continue
            missing.setdefault(section, []).append((mid, image))
            taken[mid] = image

    total = sum(len(v) for v in missing.values())
    for section, rows in missing.items():
        for mid, image in rows:
            print(f"  + {section:8s} {mid:26s} {image}")
    for section, mid, internal in unresolved:
        print(f"  ! {section:8s} {mid:26s} NO imageName for {internal}")
    for section, mid, image, owners in collided:
        print(f"  ! {section:8s} {mid:26s} {image} is already {', '.join(owners)}'s "
              f"picture — set this one by hand")

    if not total and not unresolved and not collided:
        print("data/assets.yaml is complete.")
        return 0
    if not write:
        print(f"\n{total} to add, {len(unresolved) + len(collided)} needing a hand-set "
              f"entry. Re-run with --write.")
        return 1

    # Append into each section, keeping it sorted. Only the tail of a section
    # is touched, so hand-written overrides and their comments survive.
    lines = text.splitlines(keepends=True)
    for section, rows in missing.items():
        start = next(i for i, l in enumerate(lines) if l.startswith(f"{section}:"))
        end = start + 1
        while end < len(lines) and (lines[end].startswith("  ") or not lines[end].strip()):
            end += 1
        block = [l for l in lines[start + 1 : end] if l.strip()]
        block += [f"  {mid}: {image}\n" for mid, image in rows]
        block.sort(key=lambda l: l.strip().split(":")[0] if not l.lstrip().startswith("#") else "")
        lines[start + 1 : end] = block + ["\n"]
    io.open(ASSETS, "w", encoding="utf-8", newline="\n").write("".join(lines))
    print(f"\nwrote {total} entries to {ASSETS.relative_to(ROOT)}")
    # NON-ZERO while anything still needs a human. A silent 0 is what made the
    # bad run look finished: it had written most of a section and left two
    # weapons wearing the wrong picture, and said only "wrote N entries".
    return 1 if (unresolved or collided) else 0


if __name__ == "__main__":
    raise SystemExit(main())
