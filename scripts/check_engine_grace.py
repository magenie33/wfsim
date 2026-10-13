# SPDX-License-Identifier: AGPL-3.0-or-later
"""WHICH OLDER ENGINE STAYS ACCEPTED, and on which weapons — build_site_app.py
`changed_weapons` and `accepted_engines` over the repository's own data."""
import json
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.argv = sys.argv[:1]
import build_site_app as b  # noqa: E402

bad = 0


def check(name, ok, detail=""):
    global bad
    print(f"  {'ok  ' if ok else 'FAIL'}  {name}" + ("" if ok else f"  — {detail}"))
    bad += 0 if ok else 1


w = lambda paths: b.changed_weapons(paths)
check("a weapon file moves that weapon", w(["data/weapons/archgun/arbucep.yaml"]) == {"arbucep"}, w(["data/weapons/archgun/arbucep.yaml"]))
check("...and every form and riven sibling linked to it", {"braton", "braton_prime", "braton_incarnon"} <= (w(["data/weapons/primary/braton.yaml"]) or set()))
check("a test is not in what the grace compares", not any("/tests/" in k for k in b.engine_files()))
check("a kitgun chamber moves the weapon built on it", "gaze_secondary" in (w(["data/kitguns/chambers/gaze_secondary.yaml"]) or set())
      and "catchmoon_primary" not in (w(["data/kitguns/chambers/gaze_secondary.yaml"]) or set()))
check("...a grip moves every kitgun", {"gaze_primary", "catchmoon_primary"} <= (w(["data/kitguns/grips_primary.yaml"]) or set()))
check("mechanics move everything", w(["engine/src/fight/run.rs", "data/weapons/archgun/arbucep.yaml"]) is None)
check("...and so does shared data", w(["data/mods/serration.yaml"]) is None)

files = b.engine_files()
with tempfile.TemporaryDirectory() as d:
    b.ENGINES_DIR = Path(d)
    def keep(engine, served_from, moved):
        json.dump({"engine": engine, "served_from": served_from, "files": {**files, **moved}}, open(Path(d) / f"{engine}.json", "w"))
    ago = lambda days: b.time.strftime("%Y-%m-%dT%H:%M:%S.000Z", b.time.gmtime(b.time.time() - days * 86400))
    keep("expired00000", ago(30), {})
    keep("expired00001", ago(29), {})
    keep("weaponold000", ago(3), {"data/weapons/archgun/arbucep.yaml": "x"})
    keep("mechanics000", ago(2), {"rest": "x"})
    out = {a["engine"]: a for a in b.accepted_engines("current00000")}
    check("an engine replaced by a weapon's measurement is accepted, that weapon apart",
          out.get("weaponold000", {}).get("changed") == ["arbucep"], out)
    check("one whose mechanics moved is accepted on every row, its answers only joining the served engine's",
          out.get("mechanics000", {}).get("join") is True and "changed" not in out.get("mechanics000", {}), out)
    check("one replaced more than the grace ago is not, and its list is dropped",
          "expired00000" not in out and not (Path(d) / "expired00000.json").exists(), out)
    check("...while one replaced within it stays, until the grace after", out.get("weaponold000", {}).get("until", "") > ago(2 - b.ENGINE_GRACE_DAYS + 0.1), out)

print(f"\n{bad} failed" if bad else "\nan older engine is accepted where its bits are the served one's, and joins it everywhere else")
sys.exit(1 if bad else 0)
