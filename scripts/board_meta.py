#!/usr/bin/env python3
"""`site/board.meta.json` — the small file that says WHICH board this is.

The board is the one thing this project serves that moves without a release
(docs/DISTRIBUTION.md §The data plane), so a client holding a copy cannot tell
whether it is current — and the board is 387 files and 4.8 MB, which is far too
much to fetch in order to find out. This is what it reads instead.

A DIGEST PER FILE, not one for the lot. The board is published a WEAPON at a
time, so the question a client actually has is "which of these have moved", and
an hourly rescore moves a handful. Answered per file, a client that already has
a board fetches only the weapons that changed; answered once, it fetches all of
them. `digest` is the digest OF THAT MANIFEST, which is what names the board as
a whole — a single value for "is this the board I have".

DERIVED, NEVER DECLARED. Everything here is a function of `site/board/` and
`data/board_state.yaml`, so the scoring job and the site build produce the same
stamp without coordinating. A field either writer had to fill in by itself is a
field that is wrong for the other.

That is also why there is no commit and no clock of this file's own: the only
honest timestamps are the scorer's, and they are already in the yaml.

    python scripts/board_meta.py
"""
import hashlib
import json
import pathlib
import sys

import yaml

ROOT = pathlib.Path(__file__).resolve().parent.parent
BOARD = ROOT / "site" / "board"
STATE = ROOT / "data" / "board_state.yaml"
OUT = ROOT / "site" / "board.meta.json"

# What a board's row in `board_state.yaml` publishes. Named rather than copied
# wholesale: the yaml is also where fields the page has no business reading
# would land, and a stamp that mirrors a file grows whatever that file grows.
STATE_FIELDS = ("scored_at_epoch_seconds", "submissions", "listed", "held")

# THE PUBLISHER'S OWN DERIVED FILE, under the same directory as the weapons and
# therefore not one of them. It has a digest here like any other file a client
# has to keep in step; it is excluded from `weapons` and `rows`, which are counts
# of the BOARD. Matches `INDEX_STEM` in `cli/src/bin/wfsim-board.rs`.
INDEX = "index"


def manifest() -> dict[str, str]:
    """Every published file's own digest, by stem."""
    return {
        f.stem: hashlib.sha256(f.read_bytes()).hexdigest()
        for f in sorted(BOARD.glob("*.json"))
    }


def digest_of(files: dict[str, str]) -> str:
    """The board's identity: one hash over the manifest, order fixed by sorting.

    A LINE PER FILE rather than a hash of the concatenated bytes, so the value
    cannot be reproduced by two different boards whose files happen to join into
    the same stream — and so a reader can check one file against it by name.
    """
    body = "".join(f"{name} {sha}\n" for name, sha in sorted(files.items()))
    return hashlib.sha256(body.encode("utf-8")).hexdigest()


def build() -> dict:
    files = manifest()
    state = (yaml.safe_load(STATE.read_text(encoding="utf-8")) or {}).get("boards") or {}

    boards = {}
    for name, row in sorted(state.items()):
        boards[name] = {k: row[k] for k in STATE_FIELDS if row.get(k) is not None}

    scored = [b["scored_at_epoch_seconds"] for b in boards.values()
              if b.get("scored_at_epoch_seconds")]
    weapons = [f for f in sorted(BOARD.glob("*.json")) if f.stem != INDEX]
    rows = 0
    for f in weapons:
        rows += len(json.loads(f.read_text(encoding="utf-8")))
    return {
        # WHICH BOARD THIS IS, in one value — the whole point of the file: a
        # client compares this against the copy it holds and fetches nothing
        # when they agree.
        "digest": digest_of(files),
        # …AND WHICH PART OF IT MOVED, so a client that disagrees fetches the
        # files that differ rather than the board.
        "files": files,
        "bytes": sum(f.stat().st_size for f in BOARD.glob("*.json")),
        "weapons": len(weapons),
        "rows": rows,
        # The FRESHEST of the boards, for a page that shows one line. The
        # per-board times are below for anything that shows more.
        "scored_at": max(scored) if scored else None,
        "boards": boards,
    }


def main() -> None:
    meta = build()
    # Compact and key-sorted, the same rule the publisher follows: a stamp that
    # reformats itself leaves the file dirty after every local build, and a
    # careless commit then ships it over a fresher one.
    OUT.write_text(json.dumps(meta, separators=(",", ":"), sort_keys=True),
                   encoding="utf-8", newline="\n")
    print(f"board: stamp {meta['digest'][:12]} — {meta['rows']} rows in "
          f"{meta['weapons']} file(s) -> {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    if not BOARD.is_dir():
        sys.exit(f"{BOARD} is missing — nothing to stamp")
    main()
