"""Shared access to the Warframe wiki's authoritative ARCANE data module.

`Module:Arcane/data` on wiki.warframe.com is the SOURCE OF TRUTH for arcane
mechanical fields — one canonical entry per arcane with Name / Type (the slot:
Secondary / Primary / Warframe / …) / Rarity / MaxRank / InternalName /
Description (max-rank text). Same fetch rules as scripts/wiki_mods.py: a
DESCRIPTIVE User-Agent (Weird Gloop policy) — a generic UA gets 403.

Per-RANK values are NOT here — they come from warframestat's `levelStats`
(items/search per name), the FAST source, same split as the mod pipeline.
"""
import re
import subprocess
import sys

WIKI_URL = "https://wiki.warframe.com/index.php?title=Module:Arcane/data&action=raw"
UA = "wfsim/0.1 (https://github.com/magenie33/wfsim; magenie33@gmail.com) mod-data"


def fetch_module() -> str:
    """Pull the raw Lua module (curl — proxy-aware, like wiki_mods.py)."""
    out = subprocess.run(
        ["curl", "-s", "--max-time", "45", "-A", UA, WIKI_URL],
        capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=60,
    ).stdout
    if not out or "InternalName" not in out:
        sys.exit("ERROR: could not fetch the wiki arcane module (UA blocked / "
                 "offline). Pass a saved copy instead.")
    return out


def parse_module(text: str) -> dict:
    """name -> {field: value} for every arcane block (brace-matched).

    Arcane names are multi-word, so the module keys them quoted
    (`["Secondary Merciless"] = {`). Blocks are recognized by their
    InternalName field (nested tables carry none)."""
    arcanes = {}
    for match in re.finditer(r'\["([^"]+)"\]\s*=\s*\{', text):
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
        if "InternalName" not in fields:
            continue  # nested table / non-arcane key
        name = fields.get("Name") or match.group(1)
        if name not in arcanes:
            arcanes[name] = fields
    return arcanes


def load(cache: str | None) -> dict:
    """Parsed module, from `cache` file if given, else fetched fresh."""
    if cache:
        with open(cache, encoding="utf-8") as fh:
            return parse_module(fh.read())
    return parse_module(fetch_module())


def slug(name: str) -> str:
    """Arcane display name -> our file-id convention (snake_case)."""
    s = name.lower().replace("&", " and ").replace("'", "")
    return re.sub(r"[^a-z0-9]+", "_", s).strip("_")
