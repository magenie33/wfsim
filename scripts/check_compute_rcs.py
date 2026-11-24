# SPDX-License-Identifier: AGPL-3.0-or-later
"""Exercise offline RCS calibration, FIFO replay and bounded tuning."""

import copy
import hashlib
import subprocess
import sys
from pathlib import Path

from compute_rcs import calibrate, digest, file_sha256, measure, replay, tune


def rejects(run, data):
    try:
        run(data)
    except ValueError:
        return
    raise AssertionError("expected rejection")


reference = {"version": "test", "environment": "fixture"}
rows = []
for estimator, training, holdout in (("simulate-v1", 125, 125), ("optimize-v1", 75, 125)):
    for split, observed in (("train", training), ("holdout", holdout)):
        for index in range(3):
            task = f"{estimator}/{split}/{index}"
            rows.append({"task": task, "input_sha256": digest(task), "estimator": estimator,
                         "split": split, "base_rcs_ms": 100, "cpu_ms": [observed, observed, 90_000]})
priced = calibrate({"reference": reference, "measurements": rows})
assert priced["model"]["factors"] == {"simulate-v1": [5, 4]}
assert priced["evaluation"]["optimize-v1"]["status"] == "retain"
underpriced = [{**row, "base_rcs_ms": 10} for row in rows if row["estimator"] == "simulate-v1"]
assert calibrate({"reference": reference, "measurements": underpriced})["model"]["factors"] == {}
assert priced == calibrate({"reference": reference, "measurements": rows})
assert calibrate({"reference": reference, "measurements": rows[:2]})["evaluation"]["simulate-v1"]["status"] == "insufficient_samples"
rejects(calibrate, {"reference": reference, "measurements": rows + [{**rows[0], "task": "alias"}]})

eligible = [{"task": f"s{index}", "estimator": "simulate-v1", "base_rcs_ms": 120_000,
             "zone": 1, "position": index + 1} for index in range(5)]
eligible.insert(2, {"task": "heavy", "estimator": "optimize-v1", "base_rcs_ms": 1_800_000,
                    "zone": 1, "position": 3})
eligible[3:] = [{**row, "position": row["position"] + 1} for row in eligible[3:]]
queue = {"reference": reference, "model": priced["model"], "target_rcs_ms": 600_000,
         "max_rcs_ms": 700_000, "max_rows": 40, "eligible": eligible}
assert replay(queue)["packs"] == [{"tasks": ["s0", "s1"], "rcs_ms": 300_000},
                                  {"tasks": ["heavy"], "rcs_ms": 1_800_000},
                                  {"tasks": ["s2", "s3", "s4"], "rcs_ms": 450_000}]
assert len(replay({**queue, "max_rows": 1})["packs"]) == 6
rejects(replay, {**queue, "eligible": eligible[::-1]})
rejects(replay, {**queue, "model": {**priced["model"], "version": "changed"}})
sample = {"accounts": 10, "foreground_grants": 10, "eligible_slot_ms": 100_000,
          "idle_slot_ms": 1_500, "first_grant_p95_ms": 100, "spare_hold_p95_ms": 200,
          "lease_expiry_bp": 1, "requests_per_1000_rcs": 3}
policy = {"incumbent_rcs_ms": 600_000, "minimum_windows": 3, "minimum_accounts_per_arm": 5,
          "minimum_foreground_grants_per_arm": 5, "minimum_eligible_slot_ms_per_arm": 100_000,
          "max_idle_bp": 100,
          "guardrails": {**sample, "spare_hold_p95_ms": 300}, "windows": []}
for target in (480_000, 720_000):
    for index in range(3):
        policy["windows"].append({"candidate_rcs_ms": target, "window": str(index),
                                  "dispatch_healthy": True, "baseline": sample.copy(),
                                  "trial": {**sample, "idle_slot_ms": 700, "spare_hold_p95_ms": 250}})
assert tune(policy)["target_rcs_ms"] == 480_000
adverse = copy.deepcopy(policy)
adverse["windows"][0]["trial"]["first_grant_p95_ms"] = 110
assert tune(adverse)["target_rcs_ms"] == 720_000
adverse["windows"][3]["trial"]["accounts"] = 1
assert tune(adverse)["status"] == "retain"
adverse["windows"][4]["dispatch_healthy"] = False
assert tune(adverse)["status"] == "hold_unhealthy_dispatch"
rejects(tune, {**policy, "windows": policy["windows"] + policy["windows"][:1]})

binary = Path(sys.executable).resolve()
command = [str(binary), "-c", "print(sum(i*i for i in range(150_000)))"]
output = subprocess.check_output(command)
task = {"task": "generic", "estimator": "generic-v1", "split": "train", "base_rcs_ms": 30,
        "command": command, "stdin": "", "input": {"integers": 150_000},
        "stdout_sha256": hashlib.sha256(output).hexdigest(), "executable_sha256": file_sha256(binary)}
suite = {"reference": reference, "samples": 3, "timeout_seconds": 10, "tasks": [task]}
assert len(measure(suite)["measurements"][0]["cpu_ms"]) == 3
rejects(measure, {**suite, "tasks": [{**task, "stdout_sha256": "changed"}]})
print("RCS measurements, held-out calibration, FIFO packing and tuning guardrails pass")
