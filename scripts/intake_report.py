#!/usr/bin/env python3
"""What is IN the roster, and what about it is not finished.

Written for the per-gun calibration pass: the bulk intake put every Incarnon
weapon in the library in one day, and "in the library" is not "verified". This
prints, per weapon, everything that would make a number wrong or a page
incomplete — so a calibration session can be aimed rather than exhaustive.

    python scripts/intake_report.py            # one line per weapon
    python scripts/intake_report.py --full     # and every detail under it
    python scripts/intake_report.py --csv      # for a spreadsheet

Columns, and what each means:

  BULK   this entry came from the bulk intake (its stats are DE's export and
         its evolutions are the wiki's table read by a rule engine); a blank
         means it was written by hand off the weapon's own page.
  EVO    how many evolutions the weapon has. 9 for an adapter, 13 for a
         natural Incarnon. Anything else is a transcription that stopped.
  INERT  evolution effects the engine loads but does NOT apply — the perk is
         on the card, it is marked "not modelled yet" on its tile, and it adds
         nothing. This is the number that decides whether a build is
         trustworthy.
  GAPS   `unmodeled:` lines on the weapon's own entries — a part of the attack
         this entry does not carry, a mechanic the arena does not run.
  ZH     is the weapon named in Chinese, and how many of its perks are.
"""

import csv
import io
import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
W = ROOT / "data" / "weapons"
E = ROOT / "data" / "evolutions"


def weapons():
    out = {}
    for slot in ("primary", "secondary"):
        for p in sorted((W / slot).glob("*.yaml")):
            t = p.read_text(encoding="utf-8")
            out[p.stem] = {
                "slot": slot,
                "text": t,
                "name": (re.search(r"^name: (.+)$", t, re.M) or [None, "?"])[1].strip()
                if re.search(r"^name: (.+)$", t, re.M) else "?",
                "group": (re.search(r"^transform_group: (\S+)$", t, re.M) or [None, p.stem])[1]
                if re.search(r"^transform_group: (\S+)$", t, re.M) else p.stem,
                "bulk": "# BULK INTAKE" in t,
                "unmodeled": re.findall(r'^  - "(.+)"$', t, re.M),
                "co": (re.search(r"^co_behavior: (\S+)$", t, re.M) or [None, "-"])[1]
                if re.search(r"^co_behavior: (\S+)$", t, re.M) else "-",
                "co_frac": (re.search(r"^co_base_fraction: (\S+)$", t, re.M) or [None, ""])[1]
                if re.search(r"^co_base_fraction: (\S+)$", t, re.M) else "",
            }
    return out


def evolutions():
    """{weapon_id: [(perk_id, name, tier, inert_kinds)]}"""
    out = {}
    for p in sorted(E.glob("*.yaml")):
        t = p.read_text(encoding="utf-8")
        w = re.search(r"^weapon: (\S+)$", t, re.M)
        if not w:
            continue
        tier = re.search(r"^tier: (\d+)$", t, re.M)
        # An effect the engine has no arm for is written as `kind:
        # unmodelled_<the clause's own words>` by the intake, or as a kind the
        # loader files as Inert. Both print "not modelled yet" on the tile.
        #
        # A QUALIFIER IS NOT A GAP. "Stacks up to 4x" caps the bonus above it,
        # and in every perk carrying one that bonus is itself inert — so
        # counting the cap said "partly modelled" twice for one thing and put a
        # fifth of the total on a fragment of a sentence. The engine draws the
        # same line (`EvoEffect::Qualifier`); this keeps the report's number
        # and the engine's the same number.
        inert = [k for k in re.findall(r"^\s*- kind: (unmodeled_\w+|unmodelled_\w+)", t, re.M)
                 if not k.startswith("unmodelled_stacks_up_to")]
        out.setdefault(w.group(1), []).append(
            (p.stem, tier.group(1) if tier else "?", inert))
    return out


def zh_names():
    """{family: {id: name}} across the locale's files.

    EVOLUTIONS LIVE IN THEIR OWN FILE. They are not items, so they appear in
    neither source the rest of the app localizes from — the header of
    `data/i18n/zh/evolutions.yaml` says why — and reading only names.yaml
    reports every perk as unnamed when a third of them are named.
    """
    out = {}
    for fn, default in (("names.yaml", None), ("evolutions.yaml", "evolutions")):
        p = ROOT / "data" / "i18n" / "zh" / fn
        if not p.exists():
            continue
        section = default
        for line in p.read_text(encoding="utf-8").split("\n"):
            m = re.match(r"^(\w+):\s*$", line)
            if m:
                section = m.group(1)
                continue
            m = re.match(r"^  ([\w.-]+): (.+)$", line)
            if m and section:
                out.setdefault(section, {})[m.group(1)] = m.group(2).strip()
    return out


def main():
    full = "--full" in sys.argv
    as_csv = "--csv" in sys.argv
    ws, evos, zh = weapons(), evolutions(), zh_names()
    zw, ze = zh.get("weapons", {}), zh.get("evolutions", {})

    rows = []
    # One row per transform GROUP, which is what a player picks.
    groups = {}
    for wid, w in ws.items():
        groups.setdefault(w["group"], []).append(wid)
    for g, ids in sorted(groups.items()):
        base = next((i for i in ids if "default_form: true" in ws[i]["text"]), ids[0])
        ev = evos.get(g, []) or evos.get(base, [])
        inert = sum(len(x[2]) for x in ev)
        gaps = [line for i in ids for line in ws[i]["unmodeled"]]
        named = sum(1 for x in ev if x[0] in ze)
        rows.append({
            "weapon": g,
            "name": ws[base]["name"],
            "forms": len(ids),
            "bulk": "bulk" if ws[base]["bulk"] else "hand",
            "evo": len(ev),
            "inert": inert,
            "gaps": len(gaps),
            "zh_weapon": "y" if base in zw else "NO",
            "zh_perks": "%d/%d" % (named, len(ev)),
            "co": ws[base]["co"] + (" x" + ws[base]["co_frac"] if ws[base]["co_frac"] else ""),
            "gap_lines": gaps,
            "inert_perks": [(x[0], x[2]) for x in ev if x[2]],
        })

    if as_csv:
        w = csv.DictWriter(sys.stdout, [k for k in rows[0] if not k.startswith(("gap_", "inert_"))],
                           extrasaction="ignore", lineterminator="\n")
        w.writeheader()
        w.writerows(rows)
        return 0

    print("%-24s %-5s %-5s %-4s %-6s %-5s %-6s %-8s %s"
          % ("weapon", "forms", "src", "evo", "inert", "gaps", "zh", "perks-zh", "CO"))
    print("-" * 92)
    for r in rows:
        flag = "  <-- " if (r["evo"] not in (0, 9, 13) or r["zh_weapon"] == "NO") else ""
        print("%-24s %-5d %-5s %-4d %-6d %-5d %-6s %-8s %s%s"
              % (r["weapon"], r["forms"], r["bulk"], r["evo"], r["inert"], r["gaps"],
                 r["zh_weapon"], r["zh_perks"], r["co"], flag))
        if full:
            for line in r["gap_lines"]:
                print("      gap:   %s" % line)
            for pid, kinds in r["inert_perks"]:
                print("      inert: %-34s %s" % (pid, ", ".join(kinds)))

    tot = len(rows)
    print()
    print("%d weapons, %d entries, %d evolutions"
          % (tot, len(ws), sum(r["evo"] for r in rows)))
    print("  %d bulk-intake, %d hand-written"
          % (sum(1 for r in rows if r["bulk"] == "bulk"),
             sum(1 for r in rows if r["bulk"] == "hand")))
    # THE RATCHET. This number is derived, so it is honest without anyone
    # maintaining it — and honest is not the same as improving. The engine's
    # `the_number_of_unmodelled_evolution_effects_only_goes_down` is what
    # enforces it; this prints it so a session can see where it stands.
    #
    # It counts what the YAML declares. The ENGINE's count is higher, because
    # an effect can also go inert with a kind the loader knows and a shape it
    # cannot use — 39 of those today, and they are the cheap ones to fix.
    print("  %d evolution effects load INERT by declaration (the perk shows, and adds nothing)"
          % sum(r["inert"] for r in rows))
    print("     …plus the ones the loader drops for a shape it cannot use — see"
          " `cargo test -p wfsim-engine the_number_of_unmodelled`, ceiling 254")
    print("  %d weapons carry at least one `unmodeled:` line"
          % sum(1 for r in rows if r["gaps"]))
    named = sum(int(r["zh_perks"].split("/")[0]) for r in rows)
    total = sum(int(r["zh_perks"].split("/")[1]) for r in rows)
    print("  %d of %d perks are named in Chinese" % (named, total))
    bad = [r["weapon"] for r in rows if r["evo"] not in (0, 9, 13)]
    if bad:
        print("  ! unexpected evolution counts: %s" % ", ".join(bad))
    return 0


raise SystemExit(main())
