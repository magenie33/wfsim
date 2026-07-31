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


def main():
    write = "--write" in sys.argv
    export = json.loads(EXPORT.read_text(encoding="utf-8"))
    by_unique = {it["uniqueName"]: it for it in export if it.get("uniqueName")}

    text = ASSETS.read_text(encoding="utf-8")
    have = set(re.findall(r"^\s{2}(\S+):", text, re.M))

    missing, unresolved = {}, []
    for kind, section in SECTIONS:
        if not (ROOT / "data" / kind).is_dir():
            continue
        for mid, internal in data_entries(kind):
            if mid in have:
                continue
            item = by_unique.get(internal) if internal else None
            image = (item or {}).get("imageName")
            if image:
                missing.setdefault(section, []).append((mid, image))
            else:
                unresolved.append((section, mid, internal))

    total = sum(len(v) for v in missing.values())
    for section, rows in missing.items():
        for mid, image in rows:
            print(f"  + {section:8s} {mid:26s} {image}")
    for section, mid, internal in unresolved:
        print(f"  ! {section:8s} {mid:26s} NO imageName for {internal}")

    if not total and not unresolved:
        print("data/assets.yaml is complete.")
        return 0
    if not write:
        print(f"\n{total} to add, {len(unresolved)} unresolved. Re-run with --write.")
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
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
