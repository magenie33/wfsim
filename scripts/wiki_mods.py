"""Shared access to the Warframe wiki's authoritative mod data module.

`Module:Mods/data` on wiki.warframe.com (a Weird Gloop MediaWiki) is the SOURCE
OF TRUTH for mod mechanical fields — one canonical entry per mod (no stale
duplicates, unlike warframestat) with BaseDrain / MaxRank / Polarity / Rarity /
InternalName / Type / IsExilus / Conclave / Incompatible / Description.

Both scripts/gen_pistol_mods.py (import) and scripts/verify_pistol_mods.py
(cross-check) use THIS module, so there is a single fetch + parse path.

Requires a DESCRIPTIVE User-Agent (Weird Gloop policy) — a generic UA gets 403.
"""
import re
import subprocess
import sys

# Fetched via index.php?action=raw (api.php also works with the same UA).
WIKI_URL = "https://wiki.warframe.com/index.php?title=Module:Mods/data&action=raw"
UA = "wfsim/0.1 (https://github.com/magenie33/wfsim; magenie33@gmail.com) mod-data"


def fetch_module() -> str:
    """Pull the raw Lua module (curl — proxy-aware, like gen_assets.py)."""
    out = subprocess.run(
        ["curl", "-s", "--max-time", "45", "-A", UA, WIKI_URL],
        capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=60,
    ).stdout
    if not out or "BaseDrain" not in out:
        sys.exit("ERROR: could not fetch the wiki module (UA blocked / offline). "
                 "Pass a saved copy instead.")
    return out


def parse_module(text: str) -> dict:
    """name -> {field: value} for every mod block (brace-matched).

    The module keys mods TWO ways: quoted for multi-word names
    (`["Sharpened Bullets"] = {`) and BARE Lua identifiers for single-word
    names (`Convulsion = {`). Match both; key each block by its inner `Name`
    field. Nested tables (Incompatible = {...}) match too but carry no
    BaseDrain, so they are filtered out.
    """
    mods = {}
    for match in re.finditer(r'(?:\["([^"]+)"\]|([A-Za-z_]\w*))\s*=\s*\{', text):
        i = match.end() - 1  # at the opening brace
        depth, j = 0, i
        while j < len(text):
            if text[j] == "{":
                depth += 1
            elif text[j] == "}":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        body = text[i + 1:j]
        fields = {}
        for fm in re.finditer(r'(\w+)\s*=\s*("([^"]*)"|true|false|-?\d+)', body):
            key, raw, strval = fm.group(1), fm.group(2), fm.group(3)
            if raw in ("true", "false"):
                fields[key] = raw == "true"
            elif strval is not None and raw.startswith('"'):
                fields[key] = strval
            else:
                fields[key] = int(raw)
        if "BaseDrain" not in fields:
            continue  # nested table / non-mod key
        name = fields.get("Name") or match.group(1) or match.group(2)
        if name not in mods:  # keep the first (outermost) occurrence
            mods[name] = fields
    return mods


def load(cache: str | None) -> dict:
    """Parsed module, from `cache` file if given, else fetched fresh."""
    if cache:
        with open(cache, encoding="utf-8") as fh:
            return parse_module(fh.read())
    return parse_module(fetch_module())


def slug(name: str) -> str:
    """Mod display name -> our file-id convention (snake_case)."""
    s = name.lower().replace("&", " and ").replace("'", "")
    return re.sub(r"[^a-z0-9]+", "_", s).strip("_")
