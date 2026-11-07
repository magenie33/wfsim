# SPDX-License-Identifier: AGPL-3.0-or-later
"""Prove the preparation comparison refuses a wrong run index, then restore it."""
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent
source = ROOT / "webapi/examples/prepared_cost/row.rs"
original = source.read_text()
assert original.count(".shard(index, 1,") == 1
where = ROOT / "target/bake"
cases = json.loads((where / "cases.json").read_text())
(where / "control-case.json").write_text(json.dumps(cases[:1]))
env = {**os.environ, "CARGO_TARGET_DIR": str(ROOT / "target/prepared-cost")}
command = ["cargo", "build", "--release", "--locked", "-p", "wfsim-webapi", "--example", "prepared_cost"]
try:
    source.write_text(original.replace(".shard(index, 1,", ".shard(0, 1,"))
    subprocess.run(command, cwd=ROOT, env=env, check=True)
    with (where / "wrong-index.log").open("w") as log:
        result = subprocess.run([str(ROOT / "target/prepared-cost/release/examples/prepared_cost"),
                                 str(where / "control-case.json"), str(where / "wrong-index.json"), "1"],
                                cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
    assert result.returncode != 0
    assert "assertion `left == right` failed" in (where / "wrong-index.log").read_text()
    print("Wrong run index rejected against the ordinary engine accumulator.")
finally:
    source.write_text(original)
    subprocess.run(command, cwd=ROOT, env=env, check=True)
subprocess.run([str(ROOT / "target/prepared-cost/release/examples/prepared_cost"), str(where / "control-case.json"),
                str(where / "restored-control.json"), "1"], cwd=ROOT, check=True)
