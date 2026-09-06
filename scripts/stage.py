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

    release = json.loads((SITE / "release.json").read_text(encoding="utf-8"))["release"]
    body = payload_manifest.build()
    manifest = json.loads(body)
    digest = hashlib.sha256(body).hexdigest()
    files = manifest["files"]
    print(f"release {release} ({manifest['version']}): {len(files)} files, "
          f"{sum(f['n'] for f in files) / 1e6:.1f} MB\nmanifest digest {digest}")

    if not cos.configured():
        print("\nno COS credentials — nothing staged")
        return

    c = cos.creds()
    print(f"bucket  {cos.host(c)}")

    # THE GATE IS THE RELEASE, NOT THE MANIFEST. A manifest names the commit it
    # was built from, so its digest moves on every push whether or not a single
    # served byte did; the release digest moves when the CODE does. No history
    # to inspect, and a re-run costs one request.
    if cos.head(c, f"release/{release}.json") == 200:
        print(f"\nrelease {release} is already staged — nothing to do")
        return

    # ONE LIST RATHER THAN A HEAD PER FILE: 868 round trips is minutes of a
    # publish that uploads a megabyte and a half. LISTED EVEN ON A DRY RUN,
    # because it is read-only and a rehearsal reporting 868 uploads where three
    # are due is a rehearsal of the wrong thing.
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
        # WRITTEN LAST: it is what the gate above reads and what `promote.py`
        # resolves a release through. A run that dies before this leaves blobs
        # nothing names, which the next run finds already present and reuses.
        cos.put(c, f"release/{release}.json",
                json.dumps({"manifest": digest, "version": manifest["version"]},
                           separators=(",", ":"), sort_keys=True).encode("utf-8"),
                "application/json; charset=utf-8")
    print(f"manifest staged at manifest/{digest}.json"
          + ("  [DRY RUN — nothing uploaded]" if args.dry_run else ""))
    print("\nto publish it:  python scripts/promote.py")


if __name__ == "__main__":
    main()
