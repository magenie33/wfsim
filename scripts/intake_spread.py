#!/usr/bin/env python3
"""SPREAD, per attack, straight out of the wiki's own weapon module.

WHY SPREAD AND NOT ACCURACY (owner, 2026-08-15). The Arsenal's `Accuracy` is a
DERIVED, fuzzy number — the wiki's own page defines it as `100 / average spread
in degrees` and then prints a CATEGORY ("Very High") beside it. The thing the
game actually has is the cone: `Module:Weapons/data/<slot>` carries `MinSpread`
and `MaxSpread` on every ATTACK, in degrees from the reticle, which is both the
primary value and the one that is stated PER FORM. Deriving a cone back out of
the rounded scalar loses the min/max and cannot see a form at all.

  Torid        "Grenade Impact"  MinSpread 0    MaxSpread 0     -> pinpoint
  Torid        "Incarnon Form"   MinSpread 1    MaxSpread 1.5   -> 1.25 average

...which is exactly what the rendered page prints ("Spread: 1.25° (1.00° min,
1.50° max)") and what the Accuracy stat would have rounded into one number.

THE JOIN IS MULTI-FIELD AND IT REFUSES TO GUESS. Our entries carry no attack
index, so an attack is identified by MATCHING it: damage vector, fire rate,
crit chance, crit multiplier and status chance all have to agree, and the match
has to be UNIQUE. Anything ambiguous is skipped and left admitting, because a
spread copied onto the wrong form is precisely the failure AGENTS.md already
records (the Larkspur's alt-fire carrying its base form's accuracy).

    python scripts/intake_spread.py            # dry run: what would change
    python scripts/intake_spread.py --write    # write it into data/weapons/
"""
import argparse
import os
import re
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "private", "scripts"))
import wiki_weapons  # noqa: E402  (path is set above)

ROOT = os.path.join(os.path.dirname(__file__), "..")
SLOTS = {"primary": "primary", "secondary": "secondary", "archgun": "archwing",
         "sentinel": "companion"}


def yaml_entries():
    """(path, text) for every weapon entry, cheapest possible read."""
    for slot in sorted(os.listdir(os.path.join(ROOT, "data", "weapons"))):
        d = os.path.join(ROOT, "data", "weapons", slot)
        if not os.path.isdir(d):
            continue
        for f in sorted(os.listdir(d)):
            if f.endswith(".yaml"):
                p = os.path.join(d, f)
                with open(p, encoding="utf-8") as fh:
                    yield slot, p, fh.read()


def field(text, key, block=None):
    """A scalar field, optionally inside `block:` (one level of indent)."""
    if block:
        m = re.search(rf"^{block}:\s*$", text, re.M)
        if not m:
            return None
        text = text[m.end():]
        end = re.search(r"^\S", text, re.M)
        text = text[: end.start()] if end else text
        pat = rf"^\s+{key}:[ 	]*(.+)$"
    else:
        pat = rf"^{key}:[ 	]*(.+)$"
    m = re.search(pat, text, re.M)
    if not m:
        return None
    # A NAME HAS SPACES IN IT ("Boar Prime"), so the value is the rest of the
    # line with any trailing `# comment` cut off — not the first token, which
    # silently turned every Prime into its base weapon.
    return m.group(1).split("#")[0].strip() or None


def damage_total(text):
    """Sum of the attack's `damage: { .. }` inline map."""
    m = re.search(r"^  damage:\s*\{([^}]*)\}", text, re.M)
    if not m:
        return None
    return round(sum(float(v) for v in re.findall(r":\s*([0-9.]+)", m.group(1))), 3)


def close(a, b, tol=0.02):
    if a is None or b is None:
        return False
    return abs(float(a) - float(b)) <= tol * max(1.0, abs(float(b)))


def match_attack(entry, wiki):
    """The module attack this entry IS, or None when it is not unambiguous."""
    hits = []
    for at in wiki.get("Attacks", []) or []:
        if at.get("MinSpread") is None and at.get("MaxSpread") is None:
            continue
        ok = (close(entry["fire_rate"], at.get("FireRate"))
              and close(entry["crit_chance"], at.get("CritChance"))
              and close(entry["crit_mult"], at.get("CritMultiplier"))
              and close(entry["status_chance"], at.get("StatusChance")))
        if not ok:
            continue
        dmg = at.get("Damage") or {}
        if not close(entry["damage"], round(sum(dmg.values()), 3)):
            continue
        hits.append(at)
    return hits[0] if len(hits) == 1 else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true")
    args = ap.parse_args()

    mods = {s: wiki_weapons.load(s) for s in set(SLOTS.values())}
    by_id = {}
    for slot, path, text in yaml_entries():
        wid = field(text, "id")
        by_id[wid] = (slot, path, text)

    wrote = skipped = already = 0
    misses = []
    for wid, (slot, path, text) in sorted(by_id.items()):
        if re.search(r"^\s+spread:\s*$", text, re.M):
            already += 1
            continue
        # The module is keyed by the WEAPON's display name; a form entry is an
        # attack inside its parent's record.
        parent = field(text, "transform_group") or wid
        pname = field(by_id.get(parent, (None, None, text))[2] or text, "name")
        if not pname:
            pname = field(text, "name")
        rec = mods.get(SLOTS.get(slot), {}).get(pname)
        if rec is None:
            misses.append((wid, f"no module record for {pname!r}"))
            skipped += 1
            continue
        entry = {
            "fire_rate": field(text, "fire_rate", "attack"),
            "crit_chance": field(text, "crit_chance", "attack"),
            "crit_mult": field(text, "crit_multiplier", "attack"),
            "status_chance": field(text, "status_chance", "attack"),
            "damage": damage_total(text),
        }
        at = match_attack(entry, rec)
        if at is None:
            misses.append((wid, "no unique attack match"))
            skipped += 1
            continue
        lo, hi = at.get("MinSpread") or 0.0, at.get("MaxSpread") or 0.0
        block = (
            "  # Cone half-angle from the reticle, degrees — wiki\n"
            "  # Module:Weapons/data (MinSpread/MaxSpread on this attack).\n"
            "  spread:\n"
            f"    min_deg: {lo}\n"
            f"    max_deg: {hi}\n"
        )
        if args.write:
            m = re.search(r"^attack:\s*$", text, re.M)
            text = text[: m.end() + 1] + block + text[m.end() + 1:]
            with open(path, "w", encoding="utf-8", newline="") as fh:
                fh.write(text)
        wrote += 1

    print(f"{wrote} entries matched, {already} already had one, {skipped} skipped")
    for wid, why in misses[:25]:
        print(f"  skip {wid}: {why}")
    if len(misses) > 25:
        print(f"  ... and {len(misses) - 25} more")
    if not args.write:
        print("\n(dry run — pass --write to apply)")


if __name__ == "__main__":
    main()
