#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""THE ENGINE AS A LIBRARY FOR EVERY CLIENT, published beside the desktop channel.

    python scripts/release_engine_lib.py [--stage] [--dry-run]

A third-party client loads the library the served engine was built into rather
than building its own, so an engine that moves every day costs it a download,
never a rebuild (docs/CLIENTS.md §"The official engine library"). One library
per target, each under its own digest in `engine/<engine>/`, and
`engine/<engine>.json` naming them — signed with the update key the desktop
shell verifies, one object carrying its own signature as `channel.json` does —
then `engine/latest.json`, the same object, last.

STAGED BEFORE THE SITE (`--stage`): `ship.py` publishes the library and its
`engine/<engine>.json` before the push that serves the engine, so a client told
the new engine finds its library already there; `latest.json` moves after the
push, so it never names an engine the site does not serve yet.

BUILT ONLY FOR AN ENGINE NOT YET PUBLISHED: the Windows library is the one
`desktop/payload/native/` holds for this engine; the Linux one is built in
Docker on Debian bullseye, so it loads on any glibc from 2.31.

ITS DATA TRAVELS BESIDE IT (abi 2): the official data pack the site serves,
under `engine/<engine>/`, signed in the same body as `data`, and installed
by a client before the first call (docs/CLIENTS.md §"The official data pack").
"""
import hashlib
import json
import pathlib
import subprocess
import sys
import urllib.request

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import cos  # noqa: E402
import data_packs  # noqa: E402
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
                        "-e", "CARGO_TARGET_DIR=/target",
                        *(["-e", "WFSIM_DATA_APART=1"] if data_packs.apart() else []),
                        "-w", "/src", f"rust:{toolchain()}-bullseye", "bash", "-c",
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
    dry, stage = "--dry-run" in sys.argv, "--stage" in sys.argv
    release = json.loads((ROOT / "site" / "release.json").read_text(encoding="utf-8"))
    engine, commit = release["engine"], release["commit"]
    c = cos.creds()
    if not dry and cos.head(c, f"engine/{engine}.json") is not None:
        if not stage:
            with urllib.request.urlopen(f"https://{cos.host(c)}/engine/{engine}.json", timeout=60) as r:
                cos.put(c, "engine/latest.json", r.read(), "application/json; charset=utf-8")
        print(f"engine library {engine}: already published" + ("" if stage else ", latest.json names it"))
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
    data_name = f"pack.data.{release['data']}.json"
    pack = (ROOT / "site" / "pkg" / data_name).read_bytes()
    data = {"path": f"engine/{engine}/{data_name}", "sha256": hashlib.sha256(pack).hexdigest(), "size": len(pack)}
    if not dry:
        cos.put(c, data["path"], pack, "application/json; charset=utf-8")
    print(f"engine library data: {data_name}, {len(pack) / 1e6:.1f} MB")
    body = json.dumps({"engine": engine, "commit": commit, "abi": 2, "files": files, "data": data},
                      ensure_ascii=False, separators=(",", ":"), sort_keys=True)
    pointer = signed(body)
    if not dry:
        cos.put(c, f"engine/{engine}.json", pointer, "application/json; charset=utf-8")
        if not stage:
            cos.put(c, "engine/latest.json", pointer, "application/json; charset=utf-8")
    print(f"engine library {engine}: {', '.join(files)}" + ("  [DRY RUN — nothing uploaded]" if dry else ""))


if __name__ == "__main__":
    main()
