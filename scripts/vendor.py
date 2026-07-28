#!/usr/bin/env python3
"""Fetch/refresh vendored reference datasets into vendor/ (gitignored).

Currently: WFCD/warframe-items (MIT) — DE's official export as JSON, the
machine arm of the i18n dual verification and the planned importer's seed.
Shallow-cloned; rerun to update.
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
            subprocess.run(["git", "-C", str(dest), "pull", "--depth", "1"], check=True)
        else:
            print(f"[{name}] cloning (shallow) ...")
            subprocess.run(["git", "clone", "--depth", "1", url, str(dest)], check=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
