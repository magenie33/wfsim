#!/usr/bin/env python3
"""The client's manifest, built from `site/` without compiling the shell.

WHY THIS EXISTS AT ALL. `desktop/build.rs` already writes this file, and it is
the authority — but it needs a Tauri toolchain, which a Linux runner staging a
release has no reason to install. So the LIST is declared once
(`desktop/payload.lst`, read by both) and this reproduces the description of it.

WHAT IT MUST MATCH, byte for byte: `serde_json` sorts map keys, so the encoding
here is compact and key-sorted, and the file ORDER is the walk order — all of
`site/`, path-sorted, less what the list excludes, then all of `desktop/payload/`
(what the client adds: the native engine), path-sorted. `ship.py`
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
# WHAT THE CLIENT ADDS TO `site/`, at the paths it is served under — generated
# by `build_site_app.py` like `site/`, and never deployed to the web.
ADDED = ROOT / "desktop" / "payload"


def rules() -> list[str]:
    return [line.strip() for line in LIST.read_text(encoding="utf-8").splitlines()
            if line.strip() and not line.strip().startswith("#")]


def generation(d: pathlib.Path) -> list[str] | None:
    try:
        return json.loads((d / "generation.json").read_text(encoding="utf-8"))["names"]
    except (OSError, ValueError, KeyError):
        return None


def excluded(rel: str) -> bool:
    """`build.rs`'s `excluded`: the list's lines, then the generation rule."""
    for line in rules():
        if line.startswith("*"):
            hit = rel.endswith(line[1:])
        elif line.endswith("/"):
            hit = rel.startswith(line)
        else:
            hit = rel == line
        if hit:
            return True
    segs = rel.split("/")
    for i in range(len(segs)):
        names = generation(SITE.joinpath(*segs[:i]))
        if names is not None and segs[i] != "generation.json" and segs[i] not in names:
            return True
    return False


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


def files() -> list[tuple[str, pathlib.Path]]:
    """Each payload path and the file it is: `site/` less the exclusions, then
    `desktop/payload/` whole. A path in both is refused — two files, one name."""
    out = [(p.relative_to(SITE).as_posix(), p) for p in walk(SITE)
           if not excluded(p.relative_to(SITE).as_posix())]
    added = [(p.relative_to(ADDED).as_posix(), p) for p in walk(ADDED)]
    both = {r for r, _ in out} & {r for r, _ in added}
    if both:
        sys.exit(f"desktop/payload/ and site/ both hold {sorted(both)}")
    return out + added


def source(rel: str) -> pathlib.Path:
    """The file on disk a payload path names."""
    added = ADDED / rel
    return added if added.is_file() else SITE / rel


def build() -> bytes:
    index = []
    for rel, p in files():
        body = p.read_bytes()
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
