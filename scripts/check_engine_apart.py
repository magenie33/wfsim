# SPDX-License-Identifier: AGPL-3.0-or-later
"""THE ENGINE IS ITS CODE: one source builds one module, wherever it is built,
and a data edit moves the data pack and never the module.

    python scripts/check_engine_apart.py

Builds the wasm module twice with `WFSIM_DATA_APART=1` (the shipped build,
engine/build.rs): once here, once in a second checkout at another path in
which one weapon's number and one note were changed. The two modules must be
the same bytes, and the two `data` packs must differ in exactly the edited
file. A module that reads a path, a date or a data file at compile time fails
the first; a pack that missed the edit fails the second.
"""
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import data_packs  # noqa: E402

ROOT = data_packs.ROOT
EDITED = "data/weapons/primary/braton.yaml"
bad = 0


def check(name, ok, detail=""):
    global bad
    print(f"  {'ok  ' if ok else 'FAIL'}  {name}" + ("" if ok else f"  — {detail}"))
    bad += 0 if ok else 1


def build(root: Path, target: Path) -> tuple[str, dict]:
    """The raw module's digest and the `data` pack's files, built at `root`."""
    env = {**os.environ, "WFSIM_DATA_APART": "1", "CARGO_TARGET_DIR": str(target)}
    p = subprocess.run([*data_packs.WASM_BUILD, "--message-format=json-render-diagnostics"], cwd=root, env=env,
                       stdout=subprocess.PIPE, text=True, encoding="utf-8")
    if p.returncode:
        sys.exit(f"check_engine_apart: the build at {root} failed")
    packs = None
    for line in p.stdout.splitlines():
        try:
            m = json.loads(line)
        except ValueError:
            continue
        if m.get("reason") == "build-script-executed" and "wfsim-engine" in m.get("package_id", ""):
            packs = Path(m["out_dir"]) / "packs"
    module = target / "wasm32-unknown-unknown" / "release" / "wfsim_wasm.wasm"
    data = json.loads((packs / "data.json").read_text(encoding="utf-8"))["files"]
    return hashlib.sha256(module.read_bytes()).hexdigest(), data


def edit(text: str) -> str:
    """One number of the weapon moved, and a note added: the file's meaning moves."""
    lines = text.split("\n")
    i = next(n for n, l in enumerate(lines) if l.lstrip().startswith(("critical_chance:", "crit_chance:")))
    key, value = lines[i].split(":", 1)
    lines[i] = f"{key}: {float(value.split('#')[0]) + 0.01}"
    return "# a note the engine never reads\n" + "\n".join(lines)


here_target = ROOT / "target" / "apart-here"
with tempfile.TemporaryDirectory(prefix="wfsim-apart-") as d:
    there = Path(d) / "elsewhere"
    subprocess.run(["git", "worktree", "add", "--detach", str(there), "HEAD"], cwd=ROOT, check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        # THE TREE AS COMMITTED, here and there, so only the edit differs.
        src = (there / EDITED).read_text(encoding="utf-8")
        (there / EDITED).write_text(edit(src), encoding="utf-8", newline="\n")
        a, data_a = build(ROOT, here_target)
        b, data_b = build(there, Path(d) / "target")
    finally:
        subprocess.run(["git", "worktree", "remove", "--force", str(there)], cwd=ROOT,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        shutil.rmtree(there, ignore_errors=True)

rel = EDITED.removeprefix("data/")
check("a data edit at another path builds the same module, byte for byte", a == b, f"{a[:12]} against {b[:12]}")
moved = sorted(p for p in set(data_a) | set(data_b) if data_a.get(p) != data_b.get(p))
check("…and moves the data pack in exactly the edited file", moved == [rel], moved[:5])
print(f"\n{bad} failed" if bad else "\nthe engine is its code, and its data is the pack")
sys.exit(1 if bad else 0)
