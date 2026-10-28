#!/usr/bin/env python3
"""Fill the local art cache: download every image referenced in
data/assets.yaml into web/cache/img/ (gitignored).

NOT optional any more. Both deployments serve art SAME-ORIGIN — the native
server from this cache, the static build from site/img/, which
`scripts/build_site_app.py` copies out of it and refuses to build without.
(The art is DE's and this repo makes no grant in it — see LICENSE-DATA.md §4.)

WHERE A FILE COMES FROM. A name in data/assets.yaml is OUR cache key; the file
is DE's own texture, found through the id's `internal_name` in DE's Public
Export (`scripts/de_export.py`). EXTRACTED_ART names official DE game artwork
whose extracted copies are hosted by browse.wf. A `wiki:` name, and the art
declared in a data file, come from the wiki.

Usage: python scripts/fetch_images.py
"""
import hashlib
import os
import re
import subprocess

import de_export

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CACHE = os.path.join(ROOT, "web", "cache", "img")
UA = "wfsim/0.1 (https://github.com/magenie33/wfsim; magenie33@gmail.com)"


def internal_names():
    """id -> internal_name, for every data file that states one."""
    out = {}
    for root, _dirs, files in os.walk(os.path.join(ROOT, "data")):
        for fn in files:
            if not fn.endswith(".yaml"):
                continue
            with open(os.path.join(root, fn), encoding="utf-8") as fh:
                text = fh.read()
            mid = re.search(r"^id:\s*(\S+)", text, re.M)
            iname = re.search(r"^internal_name:\s*(\S+)", text, re.M)
            if mid and iname:
                out.setdefault(mid.group(1), iname.group(1))
    return out


# Image-only joins: the riven has no data file; the other cards' gameplay
# records lack textures, so their art comes from an exported family member.
UNIQUE_NAMES = {
    "riven": "/Lotus/Upgrades/Mods/Randomized/LotusRifleRandomModRare",
    "equilibrium_pinnacle_pack": "/Lotus/Upgrades/Mods/Warframe/AvatarPickupBonusMod",
    "maglev": "/Lotus/Upgrades/Mods/Warframe/Beginner/AvatarSlideBoostModBeginner",
    "quick_thinking": "/Lotus/Upgrades/Mods/Warframe/Beginner/AvatarPowerToHealthOnDeathModBeginner",
}

# Official DE game artwork absent from Public Export. browse.wf hosts these
# extracted copies; it is a third-party download host, not a DE service.
EXTRACTED_ART = {
    "BerserkerHysteriaAbility.png": (
        "https://browse.wf/Lotus/Interface/Cards/Images/BerserkerHysteriaAbility.png",
        "7a7c5cc9431750a9a7d39697e1d942ade2c6db6b12fb1e73ad4b44045861256f",
    ),
    "StaffCombo1a.png": (
        "https://browse.wf/Lotus/Interface/Cards/Images/Stances/StaffCombo1a.png",
        "0f5a2d831c4da263e2ef0d6bd36cd72d3cc5ab8a7dd808bd7656f2c9387176c0",
    ),
}


def export_urls():
    """cache name -> DE's texture URL, through each id's `internal_name`.

    A FORM shares its weapon's name and has no `internal_name` of its own, so
    the name is resolved by whichever id wearing it is an item.
    """
    inames = {**internal_names(), **UNIQUE_NAMES}
    urls = {}
    with open(os.path.join(ROOT, "data", "assets.yaml"), encoding="utf-8") as fh:
        for line in fh:
            m = re.match(r"^\s+(\S+):\s*\"?([A-Za-z0-9_.-]+\.(?:png|jpg|jpeg))\"?\s*$", line)
            if not m or m.group(2) in urls:
                continue
            url = de_export.image_url(inames.get(m.group(1), ""))
            if url:
                urls[m.group(2)] = url
    return urls


def image_names():
    """(export_names, wiki_names) from data/assets.yaml.

    A `wiki:` prefix says DE's export does not carry that file (the wiki does).
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
    """Art declared in a data file rather than in assets.yaml, because DE's
    export does not carry it — the wiki does (Special:FilePath):

    - evolution icons: `icon:` in data/evolutions/*.yaml
    - enemy portraits: `image:` in data/enemies/**.yaml (enemies are not items)
    - Warframe ability icons: `icon:` in data/warframe_abilities/*.yaml
    """
    names = set()
    for rel, field in (("evolutions", "icon"), ("enemies", "image"), ("warframe_abilities", "icon")):
        for root, _dirs, files in os.walk(os.path.join(ROOT, "data", rel)):
            for fn in files:
                if not fn.endswith(".yaml"):
                    continue
                with open(os.path.join(root, fn), encoding="utf-8") as fh:
                    for line in fh:
                        m = re.match(rf"{field}:\s*(\S+\.(?:png|jpg))", line)
                        if m:
                            names.add(m.group(1))
    return sorted(names)


# A downloaded file is not an image because it is non-empty. `Special:FilePath`
# answers a name that does not exist with 200 and an HTML error page, so a
# mistyped `icon:` cached as a 31 KB ".png" and shipped — the site build's own
# gate only asked whether the file EXISTED (Boar Prime's Incarnon form, fixed
# 2026-08-03). Magic bytes are the cheapest question that has the right answer.
MAGIC = tuple(bytes.fromhex(h) for h in (
    "89504e470d0a1a0a",   # PNG
    "ffd8ff",             # JPEG
    "474946383761",       # GIF87a
    "474946383961",       # GIF89a
))


def is_image(path):
    """Image signature, plus the pinned digest for extracted game artwork."""
    try:
        with open(path, "rb") as fh:
            head = fh.read(12)
    except OSError:
        return False
    if not (head.startswith(MAGIC) or (head[:4] == b"RIFF" and head[8:12] == b"WEBP")):
        return False
    extracted = EXTRACTED_ART.get(os.path.basename(path))
    if extracted:
        with open(path, "rb") as fh:
            return hashlib.sha256(fh.read()).hexdigest() == extracted[1]
    return True


def main():
    os.makedirs(CACHE, exist_ok=True)
    # DE's exported and extracted textures, plus wiki-hosted non-mod art.
    export_names, wiki_names = image_names()
    wiki_names = sorted(set(wiki_names) | set(wiki_icon_names()))
    urls = export_urls()
    urls.update({name: source[0] for name, source in EXTRACTED_ART.items()})
    jobs = [(n, urls.get(n)) for n in export_names] + [
        (n, "https://wiki.warframe.com/w/Special:FilePath/" + n) for n in wiki_names
    ]
    have = fetched = 0
    bad = []
    for n, url in jobs:
        dst = os.path.join(CACHE, n)
        # Re-validate what is already cached, not just what is downloaded: a
        # poisoned entry is the one case that would otherwise live forever,
        # since every later run takes the "already present" branch.
        if os.path.exists(dst):
            if is_image(dst):
                have += 1
                continue
            os.remove(dst)          # HTML error page, truncated file, …
            print(f"  BAD  {n} — cached image failed validation, refetching")
        if url is None:
            bad.append(n)
            print(f"  MISS {n} — no id wearing it resolves to a texture in DE's export")
            continue
        subprocess.run(["curl", "-sL", "--max-time", "25", "-A", UA, url, "-o", dst],
                       capture_output=True)
        if is_image(dst):
            fetched += 1
        else:
            if os.path.exists(dst):
                os.remove(dst)      # leave nothing that a later run counts as a hit
            bad.append(n)
            print(f"  MISS {n} — {url}")
    print(f"cache: {have} already present, {fetched} downloaded, {len(jobs)} total -> {CACHE}")
    if bad:
        # Loud, and non-zero: the site build refuses these anyway, and finding
        # out here names the URL that answered wrong.
        print("")
        print(f"{len(bad)} images did not arrive as images: {', '.join(bad)}")
        raise SystemExit(1)


if __name__ == "__main__":
    main()
