"""Read a wiki `Module:*/data` page (`?action=raw`) into Python values.

The data modules are one `return { ... }` of Lua table constructors: strings,
numbers, booleans, nested tables, keys bare or `["quoted"]`. Anything after
the returned table (a loop that builds an index) is not data and is skipped.
A table with only positional entries becomes a list, otherwise a dict.
"""

import re
import urllib.request
from pathlib import Path

WIKI = "https://wiki.warframe.com/w/"


def fetch(page: str, cache: Path | None = None, refresh: bool = False) -> str:
    """The raw text of a wiki page, kept in `cache` when one is given."""
    path = cache / (page.replace("/", "_").replace(":", "_") + ".lua") if cache else None
    if path and path.exists() and not refresh:
        return path.read_text(encoding="utf-8")
    req = urllib.request.Request(WIKI + page + "?action=raw", headers={"User-Agent": "wfsim-data"})
    with urllib.request.urlopen(req, timeout=120) as r:
        text = r.read().decode("utf-8")
    if path:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
    return text


TOKEN = re.compile(r"""
    (?P<ws>\s+|--\[\[.*?\]\]|--[^\n]*)
  | (?P<str>"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')
  | (?P<num>-?\d+(?:\.\d+)?(?:[eE][-+]?\d+)?)
  | (?P<name>[A-Za-z_][A-Za-z0-9_]*)
  | (?P<op>[{}\[\]=,;])
""", re.X | re.S)

ESCAPES = {"r": "", "n": "\n", "t": "\t", "\\": "\\", '"': '"', "'": "'"}


def _tokens(text: str):
    pos = 0
    while pos < len(text):
        m = TOKEN.match(text, pos)
        if not m:
            raise ValueError(f"unexpected {text[pos:pos + 40]!r} at {pos}")
        pos = m.end()
        kind = m.lastgroup
        if kind == "ws":
            continue
        val = m.group()
        if kind == "str":
            val = re.sub(r"\\(.)", lambda e: ESCAPES.get(e.group(1), e.group(1)), val[1:-1])
        elif kind == "num":
            val = float(val) if re.search(r"[.eE]", val) else int(val)
        yield kind, val


def parse(text: str):
    """The value the module returns."""
    start = text.index("return")
    toks = list(_tokens(text[start + len("return"):].split("\nfor ")[0]))
    value, _ = _value(toks, 0)
    return value


def parse_local(text: str, name: str):
    """The table a module assigns to `local <name> = {...}`."""
    m = re.search(rf"local\s+{name}\s*=", text)
    toks = list(_tokens(text[m.end():].split("\nfor ")[0]))
    value, _ = _value(toks, 0)
    return value


def _value(toks, i):
    kind, val = toks[i]
    if kind in ("str", "num"):
        return val, i + 1
    if kind == "name":
        return {"true": True, "false": False, "nil": None}.get(val, val), i + 1
    if val == "{":
        return _table(toks, i + 1)
    raise ValueError(f"unexpected token {val!r}")


def _table(toks, i):
    items, keyed = [], {}
    while toks[i] != ("op", "}"):
        if toks[i] == ("op", "["):
            key, i = _value(toks, i + 1)
            i += 2  # "]" "="
            keyed[key], i = _value(toks, i)
        elif toks[i][0] == "name" and toks[i + 1] == ("op", "="):
            key = toks[i][1]
            keyed[key], i = _value(toks, i + 2)
        else:
            v, i = _value(toks, i)
            items.append(v)
        if toks[i][0] == "op" and toks[i][1] in ",;":
            i += 1
    if keyed:
        keyed.update({n + 1: v for n, v in enumerate(items)})
        return keyed, i + 1
    return items, i + 1
