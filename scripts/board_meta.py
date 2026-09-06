#!/usr/bin/env python3
"""`site/board.meta.json` — the small file that says WHICH board this is.

The board is the one thing this project serves that moves without a release
(docs/DISTRIBUTION.md §The data plane), so a client holding a copy cannot tell
whether it is current — and `board.json` is 4.3 MB, which is far too much to
fetch in order to find out. This is what it reads instead.

DERIVED, NEVER DECLARED. Everything here is a function of `site/board.json` and
`data/board_state.yaml`, so the two writers of the board — the scoring job,
every hour, and the site build keeping a local tree in step — produce
the same stamp without coordinating. A field either writer had to fill in by
itself is a field that is wrong for the other.

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
BOARD = ROOT / "site" / "board.json"
STATE = ROOT / "data" / "board_state.yaml"
OUT = ROOT / "site" / "board.meta.json"

# What a board's row in `board_state.yaml` publishes. Named rather than copied
# wholesale: the yaml is also where fields the page has no business reading
# would land, and a stamp that mirrors a file grows whatever that file grows.
STATE_FIELDS = ("scored_at_epoch_seconds", "submissions", "listed", "held")


def build() -> dict:
    body = BOARD.read_bytes()
    rows = json.loads(body)
    state = (yaml.safe_load(STATE.read_text(encoding="utf-8")) or {}).get("boards") or {}

    boards = {}
    for name, row in sorted(state.items()):
        boards[name] = {k: row[k] for k in STATE_FIELDS if row.get(k) is not None}

    scored = [b["scored_at_epoch_seconds"] for b in boards.values()
              if b.get("scored_at_epoch_seconds")]
    return {
        # THE PAYLOAD'S OWN DIGEST, which is the whole point of the file: a
        # client compares this against the copy it holds and fetches nothing
        # when they agree.
        "digest": hashlib.sha256(body).hexdigest(),
        "bytes": len(body),
        "weapons": len(rows),
        "rows": sum(len(v) for v in rows.values()),
        # The FRESHEST of the boards, for a page that shows one line. The
        # per-board times are below for anything that shows more.
        "scored_at": max(scored) if scored else None,
        "boards": boards,
    }


def main() -> None:
    meta = build()
    # Compact and key-sorted, the same rule `write_board` follows: a stamp that
    # reformats itself leaves the file dirty after every local build, and a
    # careless commit then ships it over a fresher one.
    OUT.write_text(json.dumps(meta, separators=(",", ":"), sort_keys=True),
                   encoding="utf-8", newline="\n")
    print(f"board: stamp {meta['digest'][:12]} — {meta['rows']} rows -> {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    if not BOARD.exists():
        sys.exit(f"{BOARD} is missing — nothing to stamp")
    main()
