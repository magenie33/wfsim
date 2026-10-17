# SPDX-License-Identifier: AGPL-3.0-or-later
"""THE ENGINE'S DATA PACKS, taken from where cargo's wasm build left them.

The wasm module is code; `engine/build.rs` writes every lean data file into a
pack (`data`, `text`, `site`) under the engine's `OUT_DIR`, and a host installs
them before its first call (docs/WASM.md §"Code and data"). This finds that
directory through cargo's own report of the build and publishes the packs.

    python scripts/data_packs.py <dir>     # pack.<name>.json into <dir>, for a
                                           # host loading a plain `pkg` folder
"""

import hashlib
import json
import os
import subprocess
from datetime import datetime, timezone
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WASM_BUILD = ["cargo", "build", "--release", "-p", "wfsim-wasm", "--target", "wasm32-unknown-unknown"]

# WHEN THE SHIPPED ENGINE STOPS CARRYING ITS DATA (UTC). Until then the module
# and the native library embed it and also take a pack, so a client written
# before packs keeps working through the notice the clients were given
# (docs/CLIENTS.md §"The official data pack"); from then on a host must install.
APART_FROM = datetime(2026, 10, 25, 1, 0, tzinfo=timezone.utc)


def apart(now: datetime | None = None) -> bool:
    """Is a build made now shipped without its data? `WFSIM_DATA_APART_NOW=1`
    builds as after `APART_FROM`, so the build that day brings is tried first."""
    if os.environ.get("WFSIM_DATA_APART_NOW") == "1":
        return True
    return (now or datetime.now(timezone.utc)) >= APART_FROM


def build_env() -> dict:
    """The environment a SHIPPED engine is built in: `WFSIM_DATA_APART`
    (engine/build.rs) set once `apart`, and absent before, as in any plain build."""
    env = {k: v for k, v in os.environ.items() if k != "WFSIM_DATA_APART"}
    return {**env, "WFSIM_DATA_APART": "1"} if apart() else env


def build_wasm(env: dict | None = None) -> Path:
    """Build the module and return the directory its packs were written to.
    Cargo replays a fresh build script's report, so a build with nothing to do
    still names it. `env` None is the caller's own, which a check that has just
    built the module passes so that this build is that one."""
    p = subprocess.run([*WASM_BUILD, "--message-format=json-render-diagnostics"], cwd=ROOT, env=env,
                       stdout=subprocess.PIPE, text=True, encoding="utf-8")
    if p.returncode:
        sys.exit("data_packs: the wasm build failed")
    out = None
    for line in p.stdout.splitlines():
        try:
            m = json.loads(line)
        except ValueError:
            continue
        if m.get("reason") == "build-script-executed" and "wfsim-engine" in m.get("package_id", "") \
                and "wasm32" in m.get("out_dir", ""):
            out = Path(m["out_dir"]) / "packs"
    if out is None or not out.is_dir():
        sys.exit("data_packs: cargo named no wasm build of wfsim-engine with packs")
    return out


def packs(where: Path) -> dict[str, bytes]:
    """name -> the pack's bytes, every pack the build wrote."""
    return {p.stem: p.read_bytes() for p in sorted(where.glob("*.json"))}


def hashed_name(name: str, body: bytes) -> str:
    return f"pack.{name}.{hashlib.sha256(body).hexdigest()[:12]}.json"


def publish(where: Path, out: Path, hashed: bool) -> list[str]:
    """Copy every pack into `out`; the names written, in pack order."""
    out.mkdir(parents=True, exist_ok=True)
    names = []
    for name, body in packs(where).items():
        dest = hashed_name(name, body) if hashed else f"pack.{name}.json"
        (out / dest).write_bytes(body)
        names.append(dest)
    return names


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    for n in publish(build_wasm(), Path(sys.argv[1]), hashed=False):
        print(f"+ {n}")
