#!/usr/bin/env python3
"""Fetch/refresh vendored reference datasets into vendor/ (gitignored).

Currently: WFCD/warframe-items (MIT) — DE's official export as JSON, the
machine arm of the i18n dual verification and the planned importer's seed.
Shallow-cloned; rerun to update.

A REFRESH IS A RESET, NOT A PULL. A shallow clone's history is a moving window:
the upstream repo rebuilds and force-pushes several times a day, so the commit
our boundary sits on stops being an ancestor of theirs and `git pull` refuses —
"You have divergent branches" — which left this script unable to do the one
thing its own docstring promised (found 2026-08-27, a month after the last
refresh). Nothing here is ever edited, so there is no divergence to reconcile:
what we want is THEIR tree, and `fetch` + `reset --hard` says exactly that.
"""

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REPOS = {
    "warframe-items": "https://github.com/WFCD/warframe-items.git",
}


def main() -> int:
    vendor = ROOT / "vendor"
    vendor.mkdir(exist_ok=True)
    for name, url in REPOS.items():
        dest = vendor / name
        if dest.exists():
            print(f"[{name}] updating ...")
            subprocess.run(
                ["git", "-C", str(dest), "fetch", "--depth", "1", "origin"], check=True
            )
            subprocess.run(
                ["git", "-C", str(dest), "reset", "--hard", "FETCH_HEAD"], check=True
            )
            # WHICH BUILD WE ARE NOW READING. A vendored dataset with no version
            # on screen is one nobody can tell apart from the one it replaced.
            v = subprocess.run(
                ["git", "-C", str(dest), "describe", "--tags"],
                capture_output=True, text=True,
            ).stdout.strip()
            print(f"[{name}] at {v or 'untagged'}")
        else:
            print(f"[{name}] cloning (shallow) ...")
            subprocess.run(["git", "clone", "--depth", "1", url, str(dest)], check=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
