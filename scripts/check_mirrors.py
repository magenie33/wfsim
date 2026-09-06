#!/usr/bin/env python3
"""Do the mirrors agree, and with the repository?

ALIGNMENT IS AN INVARIANT NOBODY CAN SEE. A mirror serving a release from
yesterday serves a page that works: it opens, it computes, it answers — with
the previous engine. The only symptom is two readers comparing numbers, which
is how this was found the first time and is not a monitoring strategy.

So it is asked out loud, on a schedule and on demand. READ-ONLY AND
CREDENTIAL-FREE, which is what lets it run anywhere: it fetches the same public
files a client fetches and compares them with the tree it is run from.

    python scripts/check_mirrors.py            # exits non-zero on a mismatch
    python scripts/check_mirrors.py --warn     # reports, always exits 0

WHAT IT DOES NOT DO is fetch a payload. The pointer and the stamp are a few
hundred bytes each and they name everything else, which is the whole reason
they exist — docs/DISTRIBUTION.md.
"""
import argparse
import json
import pathlib
import sys
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
SITE = ROOT / "site"

# The channel's own host first, the site second — the order a client tries.
# `channel.json` lives only on the first today; the second is asked for the
# board, which is served from the site and mirrored nowhere.
MIRRORS = [
    ("channel", "https://wfsim-1388973035.cos.ap-shanghai.myqcloud.com"),
    ("site", "https://wfsim.app"),
]

TIMEOUT = 60


# NAMED, BECAUSE THE DEFAULT IS REFUSED. Cloudflare answers `Python-urllib/3.x`
# with a 403, which this reported as an unreachable mirror — a check that cries
# outage over its own user agent is worse than no check.
UA = "wfsim-mirror-check/1 (+https://wfsim.app)"


def get(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
        return r.read()


def local_release() -> str:
    f = SITE / "release.json"
    if not f.exists():
        return ""
    return json.loads(f.read_text(encoding="utf-8")).get("release", "")


def local_board() -> str:
    f = SITE / "board.meta.json"
    if not f.exists():
        return ""
    return json.loads(f.read_text(encoding="utf-8")).get("digest", "")


def channel_release(base: str) -> tuple[str, str]:
    """What the signed pointer says this channel is serving.

    THE SIGNATURE IS NOT CHECKED HERE and that is deliberate: this asks WHICH
    release a mirror points at, and the client is the thing that has to refuse
    an unsigned one. A check that also verified would be a second, weaker copy
    of `update.rs` — and the two would disagree the day one of them was fixed.
    """
    doc = json.loads(get(f"{base}/channel.json"))
    p = json.loads(doc["signed"])
    return p.get("release", ""), p.get("manifest", "")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--warn", action="store_true",
                    help="report and exit 0 — for a scheduled run that must not fail the tree")
    args = ap.parse_args()

    want_release = local_release()
    want_board = local_board()
    print(f"repository   release {want_release or '(none)'}   board {want_board[:12] or '(none)'}")

    bad = []
    for name, base in MIRRORS:
        # THE RELEASE, from whichever of the two things this mirror publishes.
        # A mirror that carries neither is not serving this product and says so
        # rather than being silently green.
        got = None
        try:
            got, _ = channel_release(base)
            where = "channel.json"
        except (urllib.error.URLError, KeyError, ValueError, OSError):
            try:
                got = json.loads(get(f"{base}/release.json")).get("release", "")
                where = "release.json"
            except (urllib.error.URLError, ValueError, OSError) as e:
                print(f"{name:<12} UNREACHABLE — {e}")
                bad.append(name)
                continue
        mark = "ok " if got == want_release else "OLD"
        if got != want_release:
            bad.append(name)
        print(f"{name:<12} {mark} release {got or '(none)'}   [{where}]")

    # THE BOARD IS ITS OWN PLANE and is checked separately: it moves three times
    # an hour, so it disagreeing with the tree is ordinary, and only the site is
    # expected to carry it at all.
    try:
        live = json.loads(get("https://wfsim.app/board.meta.json"))
        same = live.get("digest", "") == want_board
        print(f"board        {'same as tree' if same else 'ahead of tree'} — "
              f"{live.get('digest', '')[:12]}, {live.get('rows', 0)} rows")
    except (urllib.error.URLError, ValueError, OSError) as e:
        print(f"board        UNREACHABLE — {e}")

    if bad and not args.warn:
        sys.exit(f"\nmirrors behind or unreachable: {', '.join(bad)}")
    print("\nall mirrors serve the release this tree holds" if not bad else "")


if __name__ == "__main__":
    main()
