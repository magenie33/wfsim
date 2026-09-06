#!/usr/bin/env python3
"""The client's manifest, built from `site/` without compiling the shell.

WHY THIS EXISTS AT ALL. `desktop/build.rs` already writes this file, and it is
the authority — but it needs a Tauri toolchain, which a Linux runner staging a
release has no reason to install. So the LIST is declared once
(`desktop/payload.lst`, read by both) and this reproduces the description of it.

WHAT IT MUST MATCH, byte for byte: `serde_json` sorts map keys, so the encoding
here is compact and key-sorted, and the file ORDER is the walk order — the
declared root files first, then each declared tree, path-sorted. `ship.py`
asserts the two agree on every run rather than trusting this comment, because
two producers of one artefact are exactly the thing that drifts silently.

    python scripts/payload_manifest.py [--out FILE]
"""
import argparse
import hashlib
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
LIST = ROOT / "desktop" / "payload.lst"


def declared() -> tuple[list[str], list[str]]:
    files, dirs = [], []
    for line in LIST.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        (dirs if line.endswith("/") else files).append(line.rstrip("/"))
    return files, dirs


def walk(d: pathlib.Path) -> list[pathlib.Path]:
    """Depth-first, path-sorted — `build.rs`'s `walk`, which sorts each level's
    entries and recurses where it meets a directory."""
    out = []
    if not d.is_dir():
        return out
    for p in sorted(d.iterdir(), key=lambda p: str(p)):
        out.extend(walk(p)) if p.is_dir() else out.append(p)
    return out


def version() -> str:
    r = subprocess.run(["git", "rev-parse", "--short=8", "HEAD"], cwd=ROOT,
                       capture_output=True, text=True, encoding="utf-8")
    return r.stdout.strip() if r.returncode == 0 else "nogit"


def build() -> bytes:
    root_files, root_dirs = declared()
    paths = [SITE / f for f in root_files]
    for d in root_dirs:
        paths.extend(walk(SITE / d))

    index = []
    for p in paths:
        body = p.read_bytes()
        rel = p.relative_to(SITE).as_posix()
        index.append({"p": rel, "n": len(body), "h": hashlib.sha256(body).hexdigest()})
    return json.dumps({"version": version(), "files": index},
                      separators=(",", ":"), sort_keys=True).encode("utf-8")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", help="write here instead of stdout")
    args = ap.parse_args()
    body = build()
    digest = hashlib.sha256(body).hexdigest()
    if args.out:
        pathlib.Path(args.out).write_bytes(body)
        n = len(json.loads(body)["files"])
        print(f"payload: {n} files, manifest {len(body)} bytes, digest {digest[:12]}")
    else:
        sys.stdout.write(body.decode("utf-8"))


if __name__ == "__main__":
    main()
