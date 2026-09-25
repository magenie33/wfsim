#!/usr/bin/env python3
"""DE's Public Export — the first-party item data, cached under vendor/ (gitignored).

LAST IN THE ORDER: measurement > wiki > this (docs/DATA_SOURCES.md). It fills
what the wiki does not state — `uniqueName` (every data file's
`internal_name`, the join key), `baseDrain` / `fusionLimit`, polarity, rarity,
compatibility tags, texture paths, DE's own card text in every language — and
loses every conflict with the wiki. It is never the source of a mechanical
number: `levelStats` is the card as DE DISPLAYS it, rounded (Argon Scope reads
2 / 3 / 5 / 6 / 8 / 9 s where the wiki's table is 1.5 s a rank).

Usage:
  python scripts/de_export.py fetch [--lang en --lang zh]
      Download (or refresh) every Export*.json for each language and the
      manifest. Files are content-addressed by DE's hash, so a refresh only
      downloads what changed.
  python scripts/de_export.py show <uniqueName> [--lang zh]
      Print one entry as DE exports it.

In a script:
  import de_export
  items = de_export.items("en")        # uniqueName -> entry, every category
  de_export.image_url("/Lotus/...")    # the texture DE serves for it, or None
"""

import argparse
import json
import lzma
import struct
import sys
import urllib.request
from functools import cache
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CACHE = ROOT / "vendor" / "public-export"
ORIGIN = "https://origin.warframe.com/PublicExport/index_{lang}.txt.lzma"
CONTENT = "https://content.warframe.com/PublicExport/Manifest/{file}"
# Textures are served from the content host, keyed by the manifest's path.
TEXTURE = "https://content.warframe.com/PublicExport{path}"
UA = {"User-Agent": "wfsim-data/1.0"}


def _get(url: str) -> bytes:
    with urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=120) as r:
        return r.read()


def _unlzma(data: bytes) -> bytes:
    # DE's index is LZMA "alone" with a size field the stdlib refuses as
    # corrupt; the raw LZMA1 stream after the 13-byte header decodes fine.
    props = data[0]
    lc, lp, pb = props % 9, (props // 9) % 5, props // 45
    dict_size = struct.unpack("<I", data[1:5])[0]
    dec = lzma.LZMADecompressor(
        format=lzma.FORMAT_RAW,
        filters=[{"id": lzma.FILTER_LZMA1, "lc": lc, "lp": lp, "pb": pb, "dict_size": dict_size}],
    )
    return dec.decompress(data[13:])


def fetch(lang: str) -> None:
    index = _unlzma(_get(ORIGIN.format(lang=lang))).decode("utf-8")
    out = CACHE / lang
    out.mkdir(parents=True, exist_ok=True)
    kept = set()
    for line in index.split():
        name = line.split("!")[0]
        kept.add(name)
        stamp = out / (name + ".hash")
        if stamp.exists() and stamp.read_text() == line and (out / name).exists():
            continue
        print(f"[{lang}] {name}", file=sys.stderr)
        (out / name).write_bytes(_get(CONTENT.format(file=line)))
        stamp.write_text(line)
    for stale in out.glob("*.json"):
        if stale.name not in kept:
            stale.unlink()


def _load(path: Path):
    # DE's files carry raw control characters inside strings.
    return json.loads(path.read_text(encoding="utf-8"), strict=False)


def _files(lang: str) -> list[Path]:
    files = sorted((CACHE / lang).glob("Export*.json"))
    if not files:
        sys.exit(f"no Public Export under {CACHE / lang} — run: python scripts/de_export.py fetch --lang {lang}")
    return files


@cache
def categories(lang: str = "en") -> dict[str, list[dict]]:
    """Every top-level list in every export file, by its key (`ExportUpgrades`,
    `ExportWeapons`, `ExportModSet`, ...)."""
    out: dict[str, list[dict]] = {}
    for f in _files(lang):
        if f.name.startswith("ExportManifest"):
            continue
        for key, rows in _load(f).items():
            if isinstance(rows, list):
                out.setdefault(key, []).extend(r for r in rows if isinstance(r, dict))
    return out


@cache
def items(lang: str = "en") -> dict[str, dict]:
    """uniqueName -> entry, across every category. Each entry gains a
    `_category` naming the list it came from."""
    out: dict[str, dict] = {}
    for key, rows in categories(lang).items():
        for r in rows:
            if "uniqueName" in r:
                out.setdefault(r["uniqueName"], {**r, "_category": key})
            elif "abilityUniqueName" in r:
                out.setdefault(r["abilityUniqueName"], {**r, "_category": key})
            # An ability is nested inside its Warframe, keyed by its own path.
            for ab in r.get("abilities") or []:
                if "abilityUniqueName" in ab:
                    out.setdefault(ab["abilityUniqueName"], {**ab, "_category": "abilities"})
    return out


@cache
def _textures() -> dict[str, str]:
    for lang in ("en", "zh"):
        m = CACHE / lang / "ExportManifest.json"
        if m.exists():
            return {
                r["uniqueName"]: r["textureLocation"]
                for r in _load(m).get("Manifest", [])
                if r.get("textureLocation")
            }
    sys.exit("no ExportManifest.json — run: python scripts/de_export.py fetch")


def texture(unique_name: str) -> str | None:
    """DE's texture path for an item (`/Lotus/Interface/Icons/...png!00_hash`)."""
    return _textures().get(unique_name)


def image_url(unique_name: str) -> str | None:
    t = texture(unique_name)
    return TEXTURE.format(path=t) if t else None


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    f = sub.add_parser("fetch")
    f.add_argument("--lang", action="append")
    s = sub.add_parser("show")
    s.add_argument("unique_name")
    s.add_argument("--lang", default="en")
    a = ap.parse_args()
    if a.cmd == "fetch":
        for lang in a.lang or ["en", "zh"]:
            fetch(lang)
        return 0
    entry = items(a.lang).get(a.unique_name)
    if entry is None:
        print("not in the export", file=sys.stderr)
        return 1
    sys.stdout.reconfigure(encoding="utf-8")
    print(json.dumps(entry, ensure_ascii=False, indent=2))
    tex = texture(a.unique_name)
    if tex:
        print("texture:", tex)
    return 0


if __name__ == "__main__":
    sys.exit(main())
