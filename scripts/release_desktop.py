#!/usr/bin/env python3
"""Publish a desktop update: manifest, signature, blobs, source archive.

WHAT A PUBLISH IS. The client compares a signed manifest with the one
describing its own `current/` and fetches whatever differs. So publishing is:
upload every file the manifest names that the bucket does not already hold,
then replace the manifest. The manifest is written LAST — until it changes, no
client is looking at the new blobs, so a half-finished upload is invisible
rather than broken.

CONTENT-ADDRESSED STORAGE. Files live at `blob/<sha256>`, not under a version
directory. Each distinct file is stored once for ever, a release uploads only
what is genuinely new (a typical one: the wasm module and `app.js`, ~1.5 MB out
of 29), and a reader who skipped ten versions still downloads only the files
they are actually missing.

THE FILE LIST IS NOT REPEATED HERE. `desktop/build.rs` declares it and writes
the manifest it built to `desktop/target/payload-manifest.json`; this reads that
back. Two hand-maintained lists that must agree eventually will not.

SOURCE GOES WITH IT. AGPL-3.0 requires the corresponding source to be offered
from the same place as the binary, so `source.zip` is published beside the
manifest on every release and the client's About panel links to the one matching
the version it is running. That also means the network drive only ever needs the
installer once — see docs/DESKTOP.md.

Usage:
    python scripts/release_desktop.py [--dry-run]
"""
import hashlib
import json
import pathlib
import subprocess
import sys
import zipfile
import io

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import cos  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
BUILT_MANIFEST = ROOT / "desktop" / "target" / "payload-manifest.json"

# Where clients look, in order. Written INTO the manifest so the channel can be
# moved without shipping a new shell — a client adopts the list it is served.
SOURCES = [
    "https://wfsim-1388973035.cos.ap-shanghai.myqcloud.com",
    "https://wfsim.app",
]


def updatekit(*args: str) -> None:
    cargo = pathlib.Path.home() / ".cargo" / "bin" / "cargo.exe"
    exe = str(cargo) if cargo.exists() else "cargo"
    r = subprocess.run(
        [exe, "run", "--quiet", "--manifest-path", str(ROOT / "desktop" / "Cargo.toml"),
         "--example", "updatekit", "--", *args],
        capture_output=True, text=True,
    )
    if r.returncode != 0:
        sys.exit(f"updatekit {' '.join(args)} failed:\n{r.stdout}\n{r.stderr}")
    print(r.stdout.strip())


def source_zip() -> bytes:
    """The repository as AGPL requires it, minus what is generated.

    `site/` is a build product of this very tree and 67 MB of it; shipping it
    as "source" would be both wrong and enormous. Everything needed to rebuild
    it is here.
    """
    tracked = subprocess.run(
        ["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout.split("\n")
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for rel in tracked:
            rel = rel.strip()
            if not rel or rel.startswith("site/") or rel.startswith("vendor/"):
                continue
            p = ROOT / rel
            if p.is_file():
                z.write(p, f"wfsim/{rel}")
    return buf.getvalue()


def main() -> None:
    dry = "--dry-run" in sys.argv

    if not BUILT_MANIFEST.exists():
        sys.exit(
            f"missing {BUILT_MANIFEST}\n"
            "Build the shell first so it can declare the payload:\n"
            "  cargo build --manifest-path desktop/Cargo.toml"
        )
    manifest = json.loads(BUILT_MANIFEST.read_text(encoding="utf-8"))
    manifest["sources"] = SOURCES
    version = manifest["version"]
    files = manifest["files"]
    print(f"version {version}: {len(files)} files, "
          f"{sum(f['n'] for f in files) / 1e6:.1f} MB total")

    c = cos.creds()
    print(f"bucket  {cos.host(c)}\n")

    # 1. blobs. Skipped when the bucket already holds that exact content, which
    #    after the first release is almost all of them. ONE list rather than a
    #    HEAD per file: 764 round trips was four and a half minutes of a publish
    #    that uploads 1.5 MB.
    have = cos.list_keys(c, "blob/")
    print(f"bucket holds {len(have)} blobs")
    new, kept, sent = 0, 0, 0
    for f in files:
        key = f"blob/{f['h']}"
        if key in have:
            kept += 1
            continue
        body = (SITE / f["p"]).read_bytes()
        if hashlib.sha256(body).hexdigest() != f["h"]:
            sys.exit(f"{f['p']} does not match the manifest — rebuild the shell")
        if not dry:
            cos.put(c, key, body, cos.MIME.get("." + f["p"].rsplit(".", 1)[-1]))
        new += 1
        sent += len(body)
    print(f"blobs   {new} uploaded ({sent / 1e6:.2f} MB), {kept} already present")

    # 2. source archive, named by version so the About panel can link the exact
    #    tree the reader is running.
    src = source_zip()
    if not dry:
        cos.put(c, f"src/{version}/source.zip", src, "application/zip")
        cos.put(c, "src/source.zip", src, "application/zip")
    print(f"source  source.zip {len(src) / 1e6:.2f} MB")

    # 3. the manifest, signed, LAST. Until this lands no client sees any of the
    #    above; once it lands, everything it references is already there.
    body = json.dumps(manifest, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    out = ROOT / "desktop" / "target" / "manifest.json"
    out.write_bytes(body)
    updatekit("sign", str(out))
    sig = (out.parent / "manifest.json.sig").read_bytes()
    if not dry:
        cos.put(c, "manifest.json", body, "application/json; charset=utf-8")
        cos.put(c, "manifest.json.sig", sig, "text/plain; charset=utf-8")
    print(f"manifest {len(body)} bytes, signed" + ("  [DRY RUN — nothing uploaded]" if dry else ""))
    print(f"\nlive at {SOURCES[0]}/manifest.json")


if __name__ == "__main__":
    main()
