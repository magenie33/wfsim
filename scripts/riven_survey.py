#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""A RIVEN SURVEY IN ONE COMMAND — docs/AGENT.md §"Riven survey".

    python scripts/riven_survey.py ocucor burston_prime   # these weapons
    python scripts/riven_survey.py --popular 10           # the 10 most tested not yet surveyed
    python scripts/riven_survey.py torid --bonus toxin,multishot,critical_chance --malus none,status_duration@1.1
    python scripts/riven_survey.py ... --dry              # say what it would open, touch nothing

Every shape of each weapon's riven (engine `riven_shapes`, which `--bonus` and
`--malus` narrow) is opened as a `survey` appraisal, and the official machines
freeze each on a clean page (`.github/workflows/survey.yml`), which hands it
to the factory. Running it again is safe: a weapon opens only the shapes it
lacks, and only unfrozen shapes are frozen.

The library is reached over ssh to its server, as `private/hetzner/ssh.env`
names it (SSH_TARGET, SSH_KEY, SSH_KNOWN_HOSTS).
"""
import argparse
import datetime
import hashlib
import json
import os
import subprocess
import sys
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SITE = os.environ.get("WFSIM_SITE", "https://wfsim.app")
CHANNEL, ASKER, RULER = "survey", "wfsim", "standard_single_target"
# 'S', three characters of the weapon, four of the shape: eight, inside the
# worker's 3-12 and apart from the six-character codes opened by hand.
CODE_CHARS = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"


def ssh_env():
    path = os.path.join(ROOT, "private", "hetzner", "ssh.env")
    env = {}
    with open(path, encoding="utf-8") as f:
        for line in f:
            if "=" in line and not line.lstrip().startswith("#"):
                k, v = line.strip().split("=", 1)
                env[k] = v
    return env


def psql(sql, stdin=None):
    """One psql call on the library's server; rows as lists of fields."""
    e = ssh_env()
    cmd = ["ssh", "-i", os.path.join(ROOT, e["SSH_KEY"]), "-o", f"UserKnownHostsFile={os.path.join(ROOT, e['SSH_KNOWN_HOSTS'])}",
           "-o", "BatchMode=yes", e["SSH_TARGET"],
           "sudo -u postgres psql -d wfsim -v ON_ERROR_STOP=1 -At -F '\t'" + ("" if stdin else f" -c {sh_quote(sql)}")]
    r = subprocess.run(cmd, input=stdin if stdin else "", capture_output=True, text=True, encoding="utf-8")
    if r.returncode:
        sys.exit(f"psql failed: {r.stderr.strip()}")
    return [line.split("\t") for line in r.stdout.splitlines() if line]


def sh_quote(s):
    return "'" + s.replace("'", "'\\''") + "'"


def sql_text(s):
    return "'" + s.replace("'", "''") + "'"


def base36(n, width):
    out = ""
    for _ in range(width):
        n, d = divmod(n, 36)
        out = CODE_CHARS[d] + out
    return out


def prefix(weapon):
    return "S" + base36(int(hashlib.sha1(weapon.encode()).hexdigest(), 16), 3)


def shapes(weapons, choice=()):
    """{weapon: line} for every weapon the engine can survey; the rest are said and skipped."""
    r = subprocess.run(["cargo", "run", "--quiet", "--release", "--bin", "riven_shapes", "--", *weapons, *choice],
                       cwd=ROOT, capture_output=True, text=True, encoding="utf-8")
    for line in r.stderr.splitlines():
        if ": " in line and not line.startswith(("warning", "error")):
            print(f"  skip {line}")
    out = {}
    for line in r.stdout.splitlines():
        d = json.loads(line)
        out[d["weapon"]] = d
    return out


def card_key(card):
    bonuses = sorted((b["id"], b["roll"]) for b in card["bonuses"])
    m = card.get("malus")
    return (tuple(bonuses), m and (m["id"], m["roll"]), card.get("rank"))


def popular(n, surveyed):
    req = urllib.request.Request(f"{SITE}/api/popularity", headers={"user-agent": "wfsim-riven-survey"})
    with urllib.request.urlopen(req, timeout=30) as f:
        scores = json.load(f).get("scores") or {}
    ranked = sorted((w for w in scores if w not in surveyed), key=lambda w: (-scores[w], w))
    # Ask the engine for more than n: some of the most tested are melee or have no riven.
    found = shapes(ranked[: n * 3])
    return [w for w in ranked if w in found][:n], found


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("weapons", nargs="*")
    ap.add_argument("--popular", type=int, default=0)
    ap.add_argument("--dry", action="store_true")
    # WHICH STATS: passed to the engine as they are (engine `riven_shapes`).
    ap.add_argument("--bonus", help="only these bonuses, comma-separated ids")
    ap.add_argument("--malus", help="each combination's maluses: none, harmless, <id> or <id>@<roll>")
    a = ap.parse_args()
    choice = [f"--{k}={v}" for k, v in (("bonus", a.bonus), ("malus", a.malus)) if v]

    counts = {w: int(n) for w, n in psql(f"SELECT weapon, count(*) FROM appraisals WHERE channel = '{CHANNEL}' GROUP BY weapon")}
    if a.popular:
        weapons, found = popular(a.popular, counts)
    else:
        weapons, found = a.weapons, shapes(a.weapons, choice)
    if not weapons:
        sys.exit("no weapon to survey")

    now = datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")
    for w in weapons:
        d = found.get(w)
        if not d:
            continue
        p = prefix(w)
        clash = psql(f"SELECT DISTINCT weapon FROM appraisals WHERE code LIKE '{p}%' AND weapon <> {sql_text(w)}")
        if clash:
            sys.exit(f"{w}: code prefix {p} is {clash[0][0]}'s")
        # A WEAPON SURVEYED ALREADY IS ASKED ONLY THE SHAPES IT LACKS, numbered
        # after its last code. A shape is its card IN ANY BONUS ORDER: cards opened
        # earlier list the same bonuses in another order.
        held = psql(f"SELECT code, riven FROM appraisals WHERE channel = '{CHANNEL}' AND weapon = {sql_text(w)}")
        have = {card_key(json.loads(r)) for _, r in held}
        start = 1 + max((int(c[len(p):], 36) for c, _ in held if c.startswith(p)), default=-1)
        new = [json.dumps(s, separators=(",", ":")) for s in d["shapes"] if card_key(s) not in have]
        print(f"{w}: {d['class']}, {len(d['stats'])} stats, malus {d['malus']}, {len(d['shapes'])} shapes, "
              f"{len(new)} new, codes {p}xxxx")
        if a.dry or not new:
            continue
        rows = ",\n".join(
            f"({sql_text(p + base36(start + i, 4))}, '{CHANNEL}', '{{}}', '{ASKER}', '', {sql_text(w)}, '{RULER}', "
            f"{sql_text(t)}, '{now}')"
            for i, t in enumerate(new))
        psql(None, stdin="INSERT INTO appraisals (code, channel, chat, asker, room, weapon, ruler, riven, at) VALUES\n"
             + rows + "\nON CONFLICT (code) DO NOTHING;\n")

    names = ", ".join(sql_text(w) for w in weapons)
    codes = [r[0] for r in psql(f"SELECT code FROM appraisals WHERE channel = '{CHANNEL}' AND weapon IN ({names}) "
                                "AND request IS NULL ORDER BY code")]
    print(f"{len(codes)} shapes to freeze")
    if a.dry or not codes:
        return
    # FROZEN ON THE OFFICIAL MACHINES (.github/workflows/survey.yml), never on
    # the machine that opened them; its hourly run would take them anyway.
    r = subprocess.run(["gh", "workflow", "run", "survey.yml"], cwd=ROOT)
    if r.returncode:
        sys.exit("could not start survey.yml; its hourly run will take them")
    print("survey.yml started: gh run list --workflow survey.yml")


if __name__ == "__main__":
    main()
