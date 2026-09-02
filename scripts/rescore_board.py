#!/usr/bin/env python3
"""IS THE BOARD STILL WHAT THIS ENGINE SCORES? — check, and optionally fix.

The board's numbers are produced by `wfsim-board` in CI (`.github/workflows/
board.yml`), hourly and on every push that touches `engine/`, `data/` or
`webapi/`. That job is the only thing keeping published ranks in step with the
engine, and on 2026-08-06 it failed three pushes running with

    "The job was not acquired by Runner of type hosted even after
     multiple attempts"

— GitHub never allocated a runner. Nothing ran, nothing was wrong with the
code, and nothing said so; two Furis rows sat 22% and 13% low until somebody
happened to look at them. The hourly schedule would have healed it within the
hour, but "within the hour, if you notice" is not a guarantee, and a board that
disagrees with the engine is the one thing this project cannot ship.

So this is the manual path, as one command:

    python scripts/rescore_board.py            # report drift, change nothing
    python scripts/rescore_board.py --write    # rescore in place

WHAT IT CAN AND CANNOT SEE. It re-scores the builds already ON the board,
which it reads from the committed yaml. Submissions sitting BELOW the cut live
in Cloudflare KV and need credentials this does not have — only the workflow
covers those. So a clean report here means "every published row is current",
not "every submission has been considered".

Exits non-zero when any score has drifted, so it can be used as a check.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent
BOARDS = ROOT / "boards"
SCORER = ROOT / "target" / "release" / ("wfsim-board.exe" if sys.platform == "win32" else "wfsim-board")

# The fields a SUBMISSION carries. A submission never carries a score — that is
# the whole point of the board (`wfsim-board`'s own header: "nobody's number is
# trusted because nobody's number is asked for"), so rebuilding submissions
# from published rows means dropping exactly the field we are re-deriving.
SUBMISSION_FIELDS = ("weapon", "mode", "mods", "arcanes", "evolutions", "exilus",
                     "arcane_rank", "rivens")


def build_scorer() -> None:
    """ALWAYS, not only when the binary is missing.

    This tool exists to answer "is the board still what this engine scores",
    and skipping the build whenever `wfsim-board.exe` already exists is wrong —
    so it answered with whatever engine was compiled last time. On 2026-08-07 it
    reported a clean board immediately after a change that moved every row,
    because the binary predated the change. A tool that can be stale about
    staleness is worse than no tool: it produces a confident "no drift".

    `cargo build` is a no-op when nothing changed, so this costs a second.
    """
    print("building the scorer…", flush=True)
    subprocess.run(
        ["cargo", "build", "--release", "--bin", "wfsim-board"],
        cwd=ROOT, check=True,
    )


def key(entry: dict) -> tuple:
    """Build identity, for pairing a row with its re-scored self.

    Mods are SORTED here and only here: two rows differing solely in mod order
    are the same submission for the purpose of this diff, while the scorer
    itself is order-sensitive (elements pair in listed order). This is a report
    key, never a scoring input.
    """
    return (
        entry["weapon"],
        # HOW it was played, part of the entrant rather than of the fight — two
        # modes of one build are two rows and must not pair with each other.
        entry.get("mode") or "",
        tuple(sorted(entry.get("mods") or [])),
        tuple(entry.get("evolutions") or []),
        tuple(entry.get("arcanes") or []),
    )


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true", help="rescore in place instead of only reporting")
    args = ap.parse_args()

    build_scorer()

    drifted_total = 0
    for path in sorted(BOARDS.glob("*.yaml")):
        board = yaml.safe_load(path.read_text(encoding="utf-8")) or {}
        entries = board.get("entries") or []
        bench = board.get("benchmark") or path.stem
        if not entries:
            print(f"{bench}: no rows")
            continue

        subs = []
        for e in entries:
            s = {k: v for k, v in e.items() if k in SUBMISSION_FIELDS}
            s["benchmark"] = bench
            subs.append(s)

        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False, encoding="utf-8") as f:
            json.dump(subs, f)
            subs_path = f.name
        out = subprocess.run(
            [str(SCORER), bench],
            stdin=open(subs_path, encoding="utf-8"),
            # encoding PINNED: the scorer's yaml header carries em-dashes and
            # typographic quotes, and Windows would decode this pipe as GBK.
            capture_output=True, text=True, encoding="utf-8", cwd=ROOT,
        )
        if out.returncode != 0:
            print(f"{bench}: scorer failed\n{out.stderr}", file=sys.stderr)
            return 2
        fresh = yaml.safe_load(out.stdout) or {}

        published = {key(e): e["score"] for e in entries}
        now = {key(e): e["score"] for e in (fresh.get("entries") or [])}
        drifted = [(k, published[k], now[k]) for k in published if k in now and abs(published[k] - now[k]) > 1e-9]
        gone = [k for k in published if k not in now]

        print(f"{bench}: {len(entries)} rows, {len(drifted)} drifted"
              + (f", {len(gone)} unpairable" if gone else ""))
        for k, before, after in sorted(drifted, key=lambda x: -abs(x[2] - x[1]) / max(abs(x[1]), 1e-9)):
            pct = (after - before) / before * 100 if before else float("inf")
            print(f"    {k[0]:<16} {before:>12.4f} -> {after:>12.4f}  ({pct:+.1f}%)")
        drifted_total += len(drifted)

        if args.write and (drifted or gone):
            # Written by the SCORER, both outputs, exactly as the workflow does
            # it — `site/board.json` carries a `shown` field the scorer formats
            # and the local site build does not, so regenerating through
            # `build_site_app.py` instead would quietly downgrade the file.
            subprocess.run(
                [str(SCORER), bench, "site/board.json"],
                stdin=open(subs_path, encoding="utf-8"),
                stdout=path.open("w", encoding="utf-8"),
                check=True, cwd=ROOT,
            )
            print(f"    rewrote {path.relative_to(ROOT)} and site/board.json")

    if drifted_total and not args.write:
        print("\nthe board disagrees with this engine — rerun with --write, or let "
              "the board workflow do it")
        return 1
    print("\nthe board is what this engine scores" if not drifted_total else "\nrescored")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
