#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""THE ENGINE AS A LIBRARY FOR EVERY CLIENT, published beside the desktop channel.

    python scripts/release_engine_lib.py [--dry-run]

A third-party client loads the library the served engine was built into rather
than building its own, so an engine that moves every day costs it a download,
never a rebuild (docs/CLIENTS.md §"The official engine library"). One library
per target, each under its own digest in `engine/<engine>/`, and
`engine/<engine>.json` naming them — signed with the update key the desktop
shell verifies, one object carrying its own signature as `channel.json` does —
then `engine/latest.json`, the same object, last.

BUILT ONLY FOR AN ENGINE NOT YET PUBLISHED: the Windows library is the one
`desktop/payload/native/` holds for this engine; the Linux one is built in
Docker on Debian bullseye, so it loads on any glibc from 2.31.
"""
import hashlib
import json
import pathlib
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import cos  # noqa: E402
from release_desktop import updatekit  # noqa: E402
from ship_bot import toolchain  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
NATIVE = ROOT / "desktop" / "payload" / "native"
OUT = ROOT / "target" / "engine-lib"


def linux_library() -> bytes | None:
    """`libwfsim_native.so` for x86_64 Linux, or None with the reason said."""
    if subprocess.run(["docker", "version"], capture_output=True).returncode != 0:
        print("engine library: Docker is not running — no Linux library this time")
        return None
    OUT.mkdir(parents=True, exist_ok=True)
    r = subprocess.run(["docker", "run", "--rm", "-v", f"{ROOT}:/src:ro", "-v", f"{OUT}:/out",
                        "-v", "wfsim-lib-target:/target", "-v", "wfsim-cargo-reg:/usr/local/cargo/registry",
                        "-e", "CARGO_TARGET_DIR=/target", "-w", "/src", f"rust:{toolchain()}-bullseye", "bash", "-c",
                        "cargo build --release --locked -p wfsim-native && cp /target/release/libwfsim_native.so /out/"],
                       cwd=ROOT, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if r.returncode != 0:
        print(f"engine library: the Linux build failed — {r.stderr.strip()[-400:]}")
        return None
    return (OUT / "libwfsim_native.so").read_bytes()


def signed(body: str) -> bytes:
    """`{sig, signed}`: the body and its signature as one object, so a reader
    never meets a new body beside an old signature."""
    OUT.mkdir(parents=True, exist_ok=True)
    f = OUT / "engine-body.json"
    f.write_text(body, encoding="utf-8", newline="")
    updatekit("sign", str(f))
    sig = (OUT / "engine-body.json.sig").read_text(encoding="utf-8").strip()
    return json.dumps({"sig": sig, "signed": body}, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def main() -> None:
    dry = "--dry-run" in sys.argv
    release = json.loads((ROOT / "site" / "release.json").read_text(encoding="utf-8"))
    engine, commit = release["engine"], release["commit"]
    c = cos.creds()
    if not dry and cos.head(c, f"engine/{engine}.json") is not None:
        print(f"engine library {engine}: already published")
        return
    libs = {}
    named = json.loads((NATIVE / "engine.json").read_text(encoding="utf-8")) if (NATIVE / "engine.json").is_file() else {}
    if named.get("engine") == engine and (NATIVE / str(named.get("file"))).is_file():
        libs["x86_64-windows"] = ("dll", (NATIVE / named["file"]).read_bytes())
    linux = linux_library()
    if linux:
        libs["x86_64-linux"] = ("so", linux)
    if not libs:
        print(f"engine library {engine}: nothing built — not published")
        return
    files = {}
    for target, (ext, body) in libs.items():
        sha = hashlib.sha256(body).hexdigest()
        path = f"engine/{engine}/wfsim_engine-{target}.{sha[:12]}.{ext}"
        if not dry:
            cos.put(c, path, body, "application/octet-stream")
        files[target] = {"path": path, "sha256": sha, "size": len(body)}
        print(f"engine library {target}: {len(body) / 1e6:.1f} MB")
    body = json.dumps({"engine": engine, "commit": commit, "abi": 1, "files": files},
                      ensure_ascii=False, separators=(",", ":"), sort_keys=True)
    pointer = signed(body)
    if not dry:
        cos.put(c, f"engine/{engine}.json", pointer, "application/json; charset=utf-8")
        cos.put(c, "engine/latest.json", pointer, "application/json; charset=utf-8")
    print(f"engine library {engine}: {', '.join(files)}" + ("  [DRY RUN — nothing uploaded]" if dry else ""))


if __name__ == "__main__":
    main()
