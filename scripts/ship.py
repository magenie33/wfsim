#!/usr/bin/env python3
"""Ship a release: `site/` and the desktop channel, from one tree, in one act.

THE DESKTOP CLIENT IS THE SITE, and the promise is exact — same files, same
bugs. Two publish paths made that a promise nobody could keep: pushing `main`
deploys `site/` by itself while the channel waits for a command run by hand, so
a week of pushes reached the web and none of them reached an installed client.

So there is ONE command. It builds `site/`, rebuilds the payload that declares
what the client gets, commits, pushes, publishes the channel — and then
VERIFIES, by fetching the manifest it just published and hashing every file it
names against `site/`. A mirror that is only promised drifts; this one is read
back.

    python scripts/ship.py              # build, commit, push, publish, verify
    python scripts/ship.py --dry-run    # every build, neither publish
    python scripts/ship.py --verify     # only: do the two agree right now?
    python scripts/ship.py --no-push    # publish the channel, leave git alone

A DIRTY TREE IS REFUSED, because what ships would then be in no commit and no
later checkout could reproduce it. `--dirty` says so out loud.
"""
import argparse
import hashlib
import json
import pathlib
import subprocess
import sys
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
BUILT_MANIFEST = ROOT / "desktop" / "target" / "payload-manifest.json"
# The first source in `release_desktop.SOURCES`, which is where a client looks
# first and therefore what "published" means.
CHANNEL = "https://wfsim-1388973035.cos.ap-shanghai.myqcloud.com"


def cargo() -> str:
    exe = pathlib.Path.home() / ".cargo" / "bin" / "cargo.exe"
    return str(exe) if exe.exists() else "cargo"


def run(*cmd: str) -> None:
    print(f"\n$ {' '.join(cmd)}", flush=True)
    r = subprocess.run(cmd, cwd=ROOT)
    if r.returncode != 0:
        sys.exit(f"ship: `{cmd[0]}` failed ({r.returncode}) — nothing after this ran")


def git(*a: str) -> str:
    return subprocess.run(
        ("git", *a), cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout.strip()


def verify() -> int:
    """Hash every file the LIVE manifest names against `site/`.

    The manifest is fetched rather than read off disk: what a client sees is
    the bucket's copy, and a local file that was never uploaded looks identical
    to one that was.
    """
    with urllib.request.urlopen(f"{CHANNEL}/manifest.json", timeout=60) as r:
        live = json.loads(r.read())
    bad = []
    for e in live["files"]:
        f = SITE / e["p"]
        if not f.exists() or hashlib.sha256(f.read_bytes()).hexdigest() != e["h"]:
            bad.append(e["p"])
    head = git("rev-parse", "--short=8", "HEAD")
    print(f"\nchannel {live['version']}: {len(live['files'])} files   HEAD {head}")
    if bad:
        print(f"MIRROR BROKEN — {len(bad)} file(s) differ from site/:")
        for p in bad[:10]:
            print(f"  {p}")
        return 1
    print("mirror ok — every file the channel names is the file site/ holds")
    return 0


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--verify", action="store_true")
    ap.add_argument("--no-push", action="store_true")
    ap.add_argument("--dirty", action="store_true")
    args = ap.parse_args()

    if args.verify:
        sys.exit(verify())

    # `site/` is a build product and is expected to differ; anything else means
    # the tree holds work that no commit records.
    #
    # NOT `git()`, which strips: a porcelain line BEGINS with its two status
    # columns, so stripping the block eats the first line's leading space and
    # shifts exactly one path by one character — which is enough to make
    # `site/app.js` look like a file outside `site/`.
    porcelain = subprocess.run(
        ("git", "status", "--porcelain"), cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout
    dirty = [ln[2:].strip() for ln in porcelain.splitlines() if ln.strip()]
    unrecorded = [p for p in dirty if not p.startswith("site/")]
    if unrecorded and not args.dirty:
        print("ship: the tree carries changes outside site/:")
        for p in unrecorded[:20]:
            print(f"  {p}")
        sys.exit(
            "\nCommit them first — what ships has to be a tree somebody can check "
            "out again.\nPass --dirty to publish anyway."
        )

    run(sys.executable, str(ROOT / "scripts" / "build_site_app.py"))
    # The payload list is `desktop/build.rs`'s, and it is rebuilt HERE so the
    # channel cannot describe a `site/` older than the one just built.
    run(cargo(), "build", "--manifest-path", str(ROOT / "desktop" / "Cargo.toml"))
    if not BUILT_MANIFEST.exists():
        sys.exit(f"ship: {BUILT_MANIFEST} was not written — the shell did not build")

    release = [sys.executable, str(ROOT / "scripts" / "release_desktop.py")]
    if args.dry_run:
        run(*release, "--dry-run")
        print("\n[DRY RUN] site/ built, payload rebuilt, nothing published or committed")
        return

    # GIT GOES FIRST, and the publish only follows a push that landed. A
    # parallel push rejects this one, and a channel published before that is a
    # channel ahead of the repository — the divergence this script exists to
    # end, arrived at from the other side.
    if not args.no_push:
        if git("status", "--porcelain", "--", "site"):
            run("git", "add", "site")
            run("git", "commit", "-m", "site: regenerate, and the desktop channel with it")
        else:
            print("\nsite/ is unchanged — nothing to commit")
        run("git", "push")

    run(*release)
    sys.exit(verify())


if __name__ == "__main__":
    main()
