#!/usr/bin/env python3
"""DE'S OWN RIVEN FAMILY NAMES, from the weekly trade dump.

    https://www-static.warframe.com/repos/weeklyRivensPC.json

DE publishes a week of riven trades, and each row carries `compatibility` —
documented on the wiki as *"Name of item/family of items it can be equipped on
in all capital case"*. That field IS the riven family, named by DE, and nothing
else in any source we read says what a family is called.

WHY IT MATTERS. `riven_family` decides three things: which weapons share a pool
(`rivens_data::derived_for`), which weapons an `exceptions.yaml` entry covers,
and — because the market is queried by family — whether a family can be
surveyed at all. A name nobody else uses is not a small typo: it silently makes
the family a singleton and silently makes the survey come back empty.

    python scripts/survey_riven_families.py            # report
    python scripts/survey_riven_families.py --write    # write the yaml

THE NAMING RULE IS NOT "STRIP THE PREFIX". That is right for a Prime, a Vandal,
a Wraith, a Prisma, a Rakta or a Telos — DE's own list holds `Boltor` and no
`Boltor Prime`, `Ballistica` and no `Rakta Ballistica`, `Lex` and no
`Lex Prime`. It is WRONG for a weapon with no ordinary counterpart, where the
prefix is part of the name: `Kuva Ayanga`, `Gotva Prime`, `Vadarya Prime`,
`Coda Bassocyst`, `Dual Coda Torxica`. Six of the roster's families had been
stripped that way, and one of them is why `data/rivens/pools.yaml` records
"Gotva: NOT SURVEYED (the API refused)" — the API did not refuse, it was asked
about a weapon that does not exist (2026-08-21).

IT IS A WEEK, NOT A CATALOGUE. The dump lists only families TRADED that week
(415 of them), so it can CONFIRM a name and can never refute one: a family that
is absent is a family nobody traded. The ratchet reads it that way — a declared
family that appears must match exactly, and one that does not appear is
reported as uncovered rather than failed.
"""

import io
import json
import os
import re
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "data" / "rivens" / "de_families.yaml"
URL = "https://www-static.warframe.com/repos/weeklyRivensPC.json"
SQ, BS = chr(39), chr(92)


def fetch():
    """The dump, parsed.

    It is a JavaScript object literal rather than JSON — unquoted keys and
    single-quoted strings — despite the `.json` extension, so `json.load` on it
    fails at line 3 every time.
    """
    req = urllib.request.Request(URL, headers={"User-Agent": "wfsim-dev/1.0"})
    with urllib.request.urlopen(req, timeout=120) as r:
        t = r.read().decode("utf-8")
    t = re.sub(r"([{,]\s*)([A-Za-z_][A-Za-z0-9_]*)(\s*:)", r'\1"\2"\3', t)
    pat = SQ + "((?:[^" + SQ + BS + BS + "]|" + BS + BS + ".)*)" + SQ
    t = re.sub(pat, lambda m: json.dumps(m.group(1).replace(BS + SQ, SQ)), t)
    return json.loads(t)


def ours():
    """Every `riven_family` the roster declares, and who declares it."""
    out = {}
    for p in sorted((ROOT / "data" / "weapons").rglob("*.yaml")):
        m = re.search(r"^riven_family: ([^#\n]+)", p.read_text(encoding="utf-8"), re.M)
        if m:
            out.setdefault(m.group(1).strip(), []).append(p.stem)
    return out


def main():
    rows = fetch()
    de = sorted({r["compatibility"] for r in rows if r.get("compatibility")})
    mine = ours()
    # A NAME DE KNOWS AND WE SPELL DIFFERENTLY. Case-folded, because the only
    # thing that could differ innocently is capitalisation and it does not:
    # DE's own dump is title case despite the schema saying otherwise.
    de_fold = {x.casefold(): x for x in de}
    wrong = sorted(
        (f, de_fold[f.casefold()]) for f in mine
        if f not in de and f.casefold() in de_fold
    )
    absent = sorted(f for f in mine if f.casefold() not in de_fold)
    print(f"{len(rows)} rows, {len(de)} families traded this week; "
          f"the roster declares {len(mine)}")
    print(f"  confirmed by DE : {len(mine) - len(wrong) - len(absent)}")
    print(f"  MISSPELLED      : {len(wrong)}")
    for f, right in wrong:
        print(f"      {f!r} -> {right!r}   {mine[f][:3]}")
    print(f"  not traded this week (cannot be checked) : {len(absent)}")
    if "-v" in sys.argv:
        for f in absent:
            print(f"      {f}")
    if "--write" in sys.argv:
        OUT.parent.mkdir(parents=True, exist_ok=True)
        body = [
            "# DE's OWN riven family names — the `compatibility` field of the weekly",
            "# trade dump (https://www-static.warframe.com/repos/weeklyRivensPC.json).",
            "# Written by scripts/survey_riven_families.py; do not hand-edit.",
            "#",
            "# It is ONE WEEK of trades, so it CONFIRMS a name and can never refute",
            "# one: a family that is absent is a family nobody traded. The test that",
            "# reads it treats it that way.",
            f"surveyed: \"{rows and 'weeklyRivensPC'}\"",
            "families:",
        ]
        body += [f"  - {json.dumps(x, ensure_ascii=False)}" for x in de]
        OUT.write_text("\n".join(body) + "\n", encoding="utf-8", newline="\n")
        print(f"wrote {OUT.relative_to(ROOT)} ({len(de)} names)")
    return 1 if wrong else 0


if __name__ == "__main__":
    sys.exit(main())
