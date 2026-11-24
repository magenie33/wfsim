# SPDX-License-Identifier: AGPL-3.0-or-later
"""Measure, calibrate, replay and evaluate reference core-second dispatch offline."""

import argparse
from collections import defaultdict
import hashlib
import json
from fractions import Fraction
from pathlib import Path
import resource
import statistics
import subprocess
import sys


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                     allow_nan=False).encode()).hexdigest()


def positive(value):
    if type(value) is not int or value <= 0:
        raise ValueError("expected a positive integer")
    return value


def nonnegative(value):
    if type(value) is not int or value < 0:
        raise ValueError("expected a nonnegative integer")
    return value


def unique(rows, field):
    indexed = {row[field]: row for row in rows}
    if not all(indexed) or len(indexed) != len(rows):
        raise ValueError(f"duplicate or empty {field}")
    return indexed


def file_sha256(path):
    hashed = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            hashed.update(block)
    return hashed.hexdigest()


def measure(data):
    tasks = data["tasks"]
    unique(tasks, "task")
    samples = positive(data["samples"])
    if samples < 3:
        raise ValueError("reference measurements require at least three repetitions")
    timeout = positive(data["timeout_seconds"])
    results = []
    for task in tasks:
        command = task["command"]
        binary = Path(command[0])
        if not binary.is_absolute():
            raise ValueError("reference commands require an absolute executable path")
        initial_hash = file_sha256(binary)
        if initial_hash != task["executable_sha256"]:
            raise ValueError(f"reference executable mismatch for {task['task']}")
        cpu_ms = []
        executions = positive(task.get("executions_per_sample", 1))
        expected = task["stdout_sha256"]
        body = task["stdin"].encode()
        for run in range(samples + 1):
            before = resource.getrusage(resource.RUSAGE_CHILDREN)
            result = subprocess.run(command, input=body, capture_output=True,
                                    check=True, timeout=timeout)
            after = resource.getrusage(resource.RUSAGE_CHILDREN)
            if hashlib.sha256(result.stdout).hexdigest() != expected:
                raise ValueError(f"reference result mismatch for {task['task']}")
            elapsed = after.ru_utime - before.ru_utime + after.ru_stime - before.ru_stime
            if run:
                cpu_ms.append(positive(round(elapsed * 1000 / executions)))
        if file_sha256(binary) != initial_hash:
            raise ValueError(f"reference executable changed for {task['task']}")
        results.append({**{k: task[k] for k in ("task", "estimator", "split", "base_rcs_ms")},
                        "cpu_ms": cpu_ms, "artifact_sha256": initial_hash,
                        "input_sha256": digest(task["input"]), "output_sha256": expected})
    return {"reference": data["reference"], "measurements": results}


def median_error(rows, factor):
    return statistics.median(abs(Fraction(row["base_rcs_ms"], row["observed_ms"]) * factor - 1)
                             for row in rows)


def calibrate(data):
    rows = data["measurements"]
    unique(rows, "task")
    unique(rows, "input_sha256")
    groups = defaultdict(lambda: {"train": [], "holdout": []})
    minimum = max(3, positive(data.get("minimum_tasks_per_split", 3)))
    median_limit = Fraction(positive(data.get("max_median_error_bp", 2500)), 10_000)
    worst_limit = Fraction(positive(data.get("max_worst_error_bp", 10_000)), 10_000)
    for row in rows:
        positive(row["base_rcs_ms"])
        samples = [positive(n) for n in row["cpu_ms"]]
        if len(samples) < 3:
            raise ValueError("each reference task requires three repetitions")
        groups[row["estimator"]][row["split"]].append(
            {**row, "observed_ms": round(statistics.median(samples))})
    incumbent = data.get("incumbent")
    reference = data["reference"]
    if not reference:
        raise ValueError("a versioned reference environment is required")
    if incumbent and read_model(incumbent)["reference"] != reference:
        raise ValueError("incumbent and samples must use the same reference")
    reported = {}
    factors = dict(incumbent["factors"]) if incumbent else {}
    for estimator, group in sorted(groups.items()):
        train, holdout = group["train"], group["holdout"]
        if min(len(train), len(holdout)) < minimum:
            reported[estimator] = {"status": "insufficient_samples",
                                   "train": len(train), "holdout": len(holdout)}
            continue
        candidate = statistics.median(Fraction(row["observed_ms"], row["base_rcs_ms"])
                                      for row in train)
        old = Fraction(*factors.get(estimator, (1, 1)))
        # Limit one promotion to a 25% increase or its reciprocal decrease.
        candidate = min(old * Fraction(5, 4), max(old * Fraction(4, 5), candidate))
        before = median_error(holdout, old)
        after = median_error(holdout, candidate)
        worst_before = max(median_error([row], old) for row in holdout)
        worst_after = max(median_error([row], candidate) for row in holdout)
        improved = after < before and worst_after <= worst_before
        qualified = improved and after <= median_limit and worst_after <= worst_limit
        if qualified:
            factors[estimator] = [candidate.numerator, candidate.denominator]
        reported[estimator] = {"status": "candidate" if qualified else "retain",
                               "train": len(train), "holdout": len(holdout),
                               "median_error": [float(before), float(after)],
                               "worst_error": [float(worst_before), float(worst_after)]}
    model = {"reference": reference, "factors": factors}
    return {"model": {**model, "version": digest(model)}, "evaluation": reported,
            "evidence_sha256": digest(rows)}


def read_model(model):
    content = {key: model[key] for key in ("reference", "factors")}
    if not content["reference"] or model["version"] != digest(content):
        raise ValueError("invalid pricing model version")
    for numerator, denominator in model["factors"].values():
        positive(numerator)
        positive(denominator)
    return model


def replay(data):
    model = read_model(data["model"])
    if model["reference"] != data["reference"]:
        raise ValueError("queue and prices must use the same reference")
    target = positive(data["target_rcs_ms"])
    maximum = positive(data["max_rcs_ms"])
    rows_limit = positive(data["max_rows"])
    if maximum < target:
        raise ValueError("maximum work must cover the target")
    rows = data["eligible"]
    unique(rows, "task")
    keys = [(nonnegative(row["zone"]), positive(row["position"])) for row in rows]
    if any(zone > 99 for zone, _ in keys) or keys != sorted(set(keys)):
        raise ValueError("eligible tasks must be in unique zone/FIFO order")
    packs, batch = [], []
    cost = 0
    for row in rows:
        raw = positive(row["base_rcs_ms"])
        factor = Fraction(*model["factors"].get(row["estimator"], (1, 1)))
        rcs_ms = max(1, round(raw * factor))
        if batch and (cost + rcs_ms > maximum or rcs_ms > target):
            packs.append({"tasks": batch, "rcs_ms": cost})
            batch, cost = [], 0
        batch.append(row["task"])
        cost += rcs_ms
        if cost >= target or len(batch) >= rows_limit:
            packs.append({"tasks": batch, "rcs_ms": cost})
            batch, cost = [], 0
    if batch:
        packs.append({"tasks": batch, "rcs_ms": cost})
    return {"model_version": model["version"], "target_rcs_ms": target, "packs": packs}


def tune(data):
    incumbent = positive(data["incumbent_rcs_ms"])
    minimum = max(3, positive(data["minimum_windows"]))
    max_idle_bp = nonnegative(data["max_idle_bp"])
    limits = data["guardrails"]
    minimum_accounts = positive(data["minimum_accounts_per_arm"])
    minimum_slots = positive(data["minimum_eligible_slot_ms_per_arm"])
    minimum_foreground = positive(data["minimum_foreground_grants_per_arm"])
    if max_idle_bp > 10_000 or nonnegative(limits["lease_expiry_bp"]) > 10_000:
        raise ValueError("idle threshold exceeds 100%")
    candidates = defaultdict(list)
    seen = set()
    for row in data["windows"]:
        target = positive(row["candidate_rcs_ms"])
        key = (target, row["window"])
        if key in seen:
            raise ValueError("duplicate trial window")
        seen.add(key)
        if not Fraction(4, 5) * incumbent <= target <= Fraction(6, 5) * incumbent or target == incumbent:
            raise ValueError("trial target must differ by at most 20% from incumbent")
        if row["dispatch_healthy"] is not True:
            return {"status": "hold_unhealthy_dispatch", "target_rcs_ms": incumbent,
                    "evidence_sha256": digest(data)}
        candidates[target].append(row)
    qualified = []
    report = {}
    fields = ("first_grant_p95_ms", "spare_hold_p95_ms", "lease_expiry_bp", "requests_per_1000_rcs")
    for target, rows in sorted(candidates.items()):
        if len(rows) < minimum:
            report[target] = "insufficient_windows"
            continue
        if any(nonnegative(arm["accounts"]) < minimum_accounts
               or nonnegative(arm["foreground_grants"]) < minimum_foreground
               or nonnegative(arm["eligible_slot_ms"]) < minimum_slots
               for row in rows for arm in (row["baseline"], row["trial"])):
            report[target] = "insufficient_coverage"
            continue
        idle = []
        for row in rows:
            baseline, trial = row["baseline"], row["trial"]
            for arm in (baseline, trial):
                busy_window = positive(arm["eligible_slot_ms"])
                idle_window = nonnegative(arm["idle_slot_ms"])
                if idle_window > busy_window:
                    raise ValueError("idle time exceeds eligible slot time")
            idle.append(Fraction(trial["idle_slot_ms"], trial["eligible_slot_ms"]))
            if any(nonnegative(trial[field]) > nonnegative(limits[field]) for field in fields):
                break
            if trial["first_grant_p95_ms"] > nonnegative(baseline["first_grant_p95_ms"]):
                break
                break
            if target > incumbent and idle[-1] >= Fraction(baseline["idle_slot_ms"], baseline["eligible_slot_ms"]):
                break
        else:
            if max(idle) * 10_000 <= max_idle_bp:
                qualified.append(target)
                report[target] = "qualified"
                continue
        report[target] = "retain"
    smaller = [target for target in qualified if target < incumbent]
    larger = [target for target in qualified if target > incumbent]
    target = min(smaller or larger or [incumbent])
    return {"status": "candidate" if target != incumbent else "retain",
            "target_rcs_ms": target, "evaluation": report, "evidence_sha256": digest(data)}


def main():
    commands = {"measure": measure, "calibrate": calibrate, "replay": replay, "tune": tune}
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=commands)
    parser.add_argument("input", type=Path)
    args = parser.parse_args()
    try:
        result = commands[args.command](json.loads(args.input.read_text()))
    except (KeyError, ValueError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, f"{args.command}: {error}\n")
    json.dump(result, sys.stdout, indent=2, allow_nan=False)
    print()


if __name__ == "__main__":
    main()
