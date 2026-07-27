#!/usr/bin/env python3
"""Pre-warm the local art cache: download every image referenced in
data/assets.yaml from the WFCD CDN into web/cache/img/ (gitignored).

The web server serves art from that cache via /img/<name>, falling back to the
CDN on a miss — so this script is optional (it just makes the first load fast
and enables offline use). DE art stays out of the repo (cache is gitignored).

Usage: python scripts/fetch_images.py
"""
import os
import re
import subprocess

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CACHE = os.path.join(ROOT, "web", "cache", "img")
CDN = "https://cdn.warframestat.us/img/"
UA = "wfsim/0.1 (https://github.com/magenie33/wfsim; magenie33@gmail.com)"


def image_names():
    names = set()
    with open(os.path.join(ROOT, "data", "assets.yaml"), encoding="utf-8") as fh:
        for line in fh:
            m = re.search(r":\s*([A-Za-z0-9_.-]+\.(?:png|jpg|jpeg))\s*$", line)
            if m:
                names.add(m.group(1))
    return sorted(names)


def wiki_icon_names():
    """Evolution icons: `icon:` fields in data/evolutions/*.yaml — hosted on
    the wiki (Special:FilePath), not the WFCD CDN."""
    names = set()
    evdir = os.path.join(ROOT, "data", "evolutions")
    for fn in os.listdir(evdir):
        if not fn.endswith(".yaml"):
            continue
        with open(os.path.join(evdir, fn), encoding="utf-8") as fh:
            for line in fh:
                m = re.match(r"icon:\s*(\S+\.(?:png|jpg))", line)
                if m:
                    names.add(m.group(1))
    return sorted(names)


def main():
    os.makedirs(CACHE, exist_ok=True)
    # (name, base url) pairs: CDN art + wiki-hosted evolution icons.
    jobs = [(n, CDN + n) for n in image_names()] + [
        (n, "https://wiki.warframe.com/w/Special:FilePath/" + n) for n in wiki_icon_names()
    ]
    have = fetched = 0
    for n, url in jobs:
        dst = os.path.join(CACHE, n)
        if os.path.exists(dst) and os.path.getsize(dst) > 0:
            have += 1
            continue
        subprocess.run(["curl", "-sL", "--max-time", "25", "-A", UA, url, "-o", dst],
                       capture_output=True)
        if os.path.exists(dst) and os.path.getsize(dst) > 0:
            fetched += 1
        else:
            print(f"  MISS {n}")
    print(f"cache: {have} already present, {fetched} downloaded, {len(jobs)} total -> {CACHE}")


if __name__ == "__main__":
    main()
