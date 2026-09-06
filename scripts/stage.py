#!/usr/bin/env python3
"""Stage a release: put the bytes where clients can reach them, point at nothing.

THE HALF OF PUBLISHING THAT NEEDS NO KEY. Uploading blobs and the manifest that
names them changes nothing any client reads — the pointer does that, and the
pointer is signed by a person (`scripts/promote.py`). Splitting the two is what
lets the expensive half run automatically without putting a signing key in CI:
CI can stage anything and reach nobody.

The failure mode of a stage nobody promotes is that readers stay on the last
release. LATE, NOT DIVERGENT — docs/DISTRIBUTION.md §The release plane.

    python scripts/stage.py              # upload, print the digest to promote
    python scripts/stage.py --dry-run    # say what would go, upload nothing

Credentials are `private/cos.json`, or COS_SECRET_ID / COS_SECRET_KEY /
COS_BUCKET / COS_REGION in the environment. UNCONFIGURED IS SILENT AND GREEN:
a runner without them has nothing to say, and failing there would redden every
push for a job nobody asked to be able to run.
"""
import argparse
import hashlib
import json
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import cos  # noqa: E402
import payload_manifest  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
SITE = ROOT / "site"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    body = payload_manifest.build()
    manifest = json.loads(body)
    digest = hashlib.sha256(body).hexdigest()
    files = manifest["files"]
    print(f"release {manifest['version']}: {len(files)} files, "
          f"{sum(f['n'] for f in files) / 1e6:.1f} MB\nmanifest digest {digest}")

    if not cos.configured():
        print("\nno COS credentials — nothing staged")
        return

    c = cos.creds()
    print(f"bucket  {cos.host(c)}")

    # ALREADY STAGED IS A NO-OP, and that is what gates this job: a push that
    # moved the board and nothing else produces the manifest that is already up
    # there. No history to inspect, and a re-run costs one request.
    if cos.head(c, f"manifest/{digest}.json") == 200:
        print(f"\nmanifest/{digest}.json is already staged — nothing to do")
        return

    # ONE LIST RATHER THAN A HEAD PER FILE: 868 round trips is minutes of a
    # publish that uploads a megabyte and a half.
    # LISTED EVEN ON A DRY RUN. It is read-only, and a dry run that reports 868
    # uploads where three are due is a rehearsal of the wrong thing.
    have = cos.list_keys(c, "blob/")
    new = sent = 0
    for f in files:
        key = f"blob/{f['h']}"
        if key in have:
            continue
        blob = (SITE / f["p"]).read_bytes()
        if hashlib.sha256(blob).hexdigest() != f["h"]:
            sys.exit(f"{f['p']} changed under this run — stage a tree that is holding still")
        if not args.dry_run:
            cos.put(c, key, blob, cos.MIME.get("." + f["p"].rsplit(".", 1)[-1]))
        new += 1
        sent += len(blob)
    print(f"blobs   {new} uploaded ({sent / 1e6:.2f} MB), {len(files) - new} already present")

    # THE MANIFEST, ADDRESSED BY ITS OWN DIGEST — immutable, so a client that
    # verified a digest can check that what came back is what it verified. Still
    # not the pointer: no client is looking at this yet.
    if not args.dry_run:
        cos.put(c, f"manifest/{digest}.json", body, "application/json; charset=utf-8")
    print(f"manifest staged at manifest/{digest}.json"
          + ("  [DRY RUN — nothing uploaded]" if args.dry_run else ""))
    print(f"\nto publish it:  python scripts/promote.py {digest}")


if __name__ == "__main__":
    main()
