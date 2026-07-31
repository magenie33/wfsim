#!/usr/bin/env python3
"""Fill the local art cache: download every image referenced in
data/assets.yaml into web/cache/img/ (gitignored).

NOT optional any more. Both deployments serve art SAME-ORIGIN — the native
server from this cache, the static build from site/img/, which
`scripts/build_site_app.py` copies out of it and refuses to build without.
The static build used to hotlink the CDN instead, and that CDN 301s to
raw.githubusercontent.com: unreliable to blocked from mainland China, where
the players are. (DE permits hosting the art; their Content Policy asks only
that the use be non-commercial.)

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
    """(cdn_names, wiki_names) from data/assets.yaml.

    A `wiki:` prefix says the CDN does not carry that file (the wiki does).
    It is quoted in the yaml, which the old end-of-line regex could not match
    at all — so those entries were silently never cached, and the page fell
    back to hotlinking the wiki. With art served same-origin that is no longer
    a fallback but a hole, so the prefix is parsed here.
    """
    cdn, wiki = set(), set()
    with open(os.path.join(ROOT, "data", "assets.yaml"), encoding="utf-8") as fh:
        for line in fh:
            m = re.search(r":\s*\"?(wiki:)?([A-Za-z0-9_.-]+\.(?:png|jpg|jpeg))\"?\s*$", line)
            if m:
                (wiki if m.group(1) else cdn).add(m.group(2))
    return sorted(cdn), sorted(wiki)


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
    cdn_names, wiki_names = image_names()
    wiki_names = sorted(set(wiki_names) | set(wiki_icon_names()))
    jobs = [(n, CDN + n) for n in cdn_names] + [
        (n, "https://wiki.warframe.com/w/Special:FilePath/" + n) for n in wiki_names
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
