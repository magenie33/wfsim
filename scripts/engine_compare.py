# SPDX-License-Identifier: AGPL-3.0-or-later
"""Freeze native executables and compare complete API responses in ABBA order."""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import statistics
import struct
import subprocess
import sys
import time

SOURCE_PATHS = ["engine", "optimizer", "webapi", "tools", "data", "Cargo.toml", "Cargo.lock"]
BUILD_ENV = ["RUSTC", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC_WRAPPER",
             "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_TARGET", "WFSIM_DATA_APART"]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"),
                      parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)))


def write_json(path, value):
    destination = Path(path)
    temporary = destination.with_suffix(destination.suffix + ".tmp")
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n",
                         encoding="utf-8")
    temporary.replace(destination)


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args]).decode().strip()


def source_files(root, runtime="native"):
    names = git(root, "ls-files", "--cached", "--others", "--exclude-standard", "-z",
                "--", *SOURCE_PATHS, *(["wasm"] if runtime == "wasm" else [])).split("\0")
    return {name: digest((root / name).read_bytes()) for name in sorted(filter(None, names))}


def freeze(args):
    root, output = args.worktree.resolve(), args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    before = source_files(root, args.runtime)
    command = ["cargo", "build", "--release", "--locked", "--jobs", str(args.jobs), "--message-format=json"]
    command += (["--bin", "one_fight", "--example", "simulation_cost"] if args.runtime == "native"
                else ["-p", "wfsim-wasm", "--target", "wasm32-unknown-unknown"])
    environment = {**os.environ, "CARGO_TARGET_DIR": str(root / "target")}
    build = subprocess.run(command, cwd=root, env=environment, stdout=subprocess.PIPE, check=True)
    (output / "cargo-artifacts.jsonl").write_bytes(build.stdout)
    if source_files(root, args.runtime) != before:
        raise ValueError("calculation sources changed while compiling")
    artifacts = {}
    for line in build.stdout.decode().splitlines():
        row = json.loads(line)
        if row.get("reason") == "compiler-artifact" and row.get("executable"):
            if row["target"]["name"] in ("simulation_cost", "one_fight"):
                executable = Path(row["executable"])
                destination = output / executable.name
                shutil.copy2(executable, destination)
                artifacts[row["target"]["name"]] = {
                    "file": destination.name, "sha256": digest(destination.read_bytes())}
    dependencies = {}
    if args.runtime == "wasm":
        module = root / "target/wasm32-unknown-unknown/release/wfsim_wasm.wasm"
        subprocess.run(["wasm-bindgen", "--target", "no-modules", "--out-dir", str(output), str(module)], check=True)
        for name in ["wasm_engine.mjs", "wasm_compare.mjs"]:
            shutil.copy2(Path(__file__).with_name(name), output / name)
        wrapper = output / "simulation_cost"
        wrapper.write_text(
            f"#!{sys.executable}\n"
            "import json,pathlib,subprocess,sys,tempfile\n"
            "root=pathlib.Path(__file__).resolve().parent\n"
            "calls=json.loads(pathlib.Path(sys.argv[1]).read_text())\n"
            "for row in calls: row['body_text']=json.dumps(row['body'],ensure_ascii=False,allow_nan=False)\n"
            "with tempfile.TemporaryDirectory(prefix='wfsim-wasm-calls-') as tmp:\n"
            " path=pathlib.Path(tmp)/'calls.json'\n"
            " path.write_text(json.dumps(calls,ensure_ascii=False))\n"
            " subprocess.run(['node',str(root/'wasm_compare.mjs'),str(path),*sys.argv[2:]],check=True)\n",
            encoding="utf-8")
        wrapper.chmod(0o755)
        artifacts = {"simulation_cost": {"file": wrapper.name, "sha256": digest(wrapper.read_bytes())}}
        dependencies = {p.name: digest(p.read_bytes()) for p in output.iterdir()
                        if p.suffix in (".js", ".mjs", ".wasm")}
    elif set(artifacts) != {"simulation_cost", "one_fight"}:
        raise ValueError("missing comparison executables")
    manifest = {
        "schema": 1, "runtime": args.runtime, "worktree": str(root), "commit": git(root, "rev-parse", "HEAD"),
        "working_changes": git(root, "status", "--porcelain"),
        "source_files": before, "source_sha256": digest(json.dumps(before, sort_keys=True).encode()),
        "cargo": subprocess.check_output(["cargo", "--version"]).decode().strip(),
        "rustc": subprocess.check_output(["rustc", "-vV"]).decode().strip(),
        "build_command": command, "build_environment": {k: os.environ.get(k) for k in BUILD_ENV},
        "platform": platform.platform(), "executables": artifacts, "files": dependencies,
        "runtime_tools": ({"node": subprocess.check_output(["node", "--version"]).decode().strip(),
                           "wasm_bindgen": subprocess.check_output(["wasm-bindgen", "--version"]).decode().strip()}
                          if args.runtime == 'wasm' else {}),
    }
    write_json(output / "manifest.json", manifest)
    print(json.dumps({"snapshot": str(output), "commit": manifest["commit"],
                      "source_sha256": manifest["source_sha256"]}))
    return 0


def load_snapshot(folder):
    manifest = read_json(folder / "manifest.json")
    if manifest.get("schema") != 1:
        raise ValueError("unsupported snapshot schema")
    for row in manifest["executables"].values():
        name = row["file"]
        if Path(name).name != name or digest((folder / name).read_bytes()) != row["sha256"]:
            raise ValueError("snapshot executable hash mismatch")
    for name, expected in manifest.get("files", {}).items():
        if Path(name).name != name or digest((folder / name).read_bytes()) != expected:
            raise ValueError("snapshot dependency hash mismatch")
    return manifest, folder / manifest["executables"]["simulation_cost"]["file"]


def first_difference(before, after, pointer=""):
    if type(before) is not type(after):
        return {"pointer": pointer, "kind": "type", "before": before, "after": after}
    if isinstance(before, dict):
        for key in sorted(set(before) | set(after)):
            at = pointer + "/" + key.replace("~", "~0").replace("/", "~1")
            if key not in before or key not in after:
                return {"pointer": at, "kind": "presence", "before_present": key in before,
                        "after_present": key in after}
            difference = first_difference(before[key], after[key], at)
            if difference is not None:
                return difference
    elif isinstance(before, list):
        for index, (left, right) in enumerate(zip(before, after)):
            difference = first_difference(left, right, pointer + "/" + str(index))
            if difference is not None:
                return difference
        if len(before) != len(after):
            return {"pointer": pointer, "kind": "length", "before": len(before), "after": len(after)}
    elif isinstance(before, float):
        if struct.pack(">d", before) != struct.pack(">d", after):
            return {"pointer": pointer, "kind": "float64", "before": before, "after": after,
                    "before_bits": struct.pack(">d", before).hex(),
                    "after_bits": struct.pack(">d", after).hex()}
    elif before != after:
        return {"pointer": pointer, "kind": "value", "before": before, "after": after}
    return None


def execute(command, output, label):
    start = time.perf_counter()
    with (output / f"{label}.stdout").open("wb") as stdout:
        with (output / f"{label}.stderr").open("wb") as stderr:
            process = subprocess.Popen(command, stdout=stdout, stderr=stderr)
            usage = None
            try:
                if hasattr(os, "wait4"):
                    _, status, usage = os.wait4(process.pid, 0)
                    process.returncode = os.waitstatus_to_exitcode(status)
                else:
                    process.wait()
            except BaseException:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                raise
    if process.returncode:
        raise ValueError(f"{label} failed ({process.returncode}); see retained stderr")
    return {"wall_seconds": time.perf_counter() - start,
            "cpu_seconds": usage.ru_utime + usage.ru_stime if usage else None,
            "peak_rss_bytes": int(usage.ru_maxrss * (1 if sys.platform == "darwin" else 1024))
            if usage else None}


def load_rows(path, calls):
    rows = read_json(path)
    if not isinstance(rows, list) or [r.get("id") for r in rows] != [r["id"] for r in calls]:
        raise ValueError("result case identity/order mismatch")
    results = {row['id']: row['result'] for row in rows}
    for row, call in zip(rows, calls):
        if row["result"].get("ok") is not True:
            raise ValueError(f"{row['id']}: endpoint failed")
        if call["path"] == "/api/log" and row["result"].get("dropped") != 0:
            raise ValueError(f"{row['id']}: truncated combat record")
        if not row["milliseconds"] or any(not math.isfinite(x) or x < 0 for x in row["milliseconds"]):
            raise ValueError("invalid timing sample")
        coverage = call.get("coverage", {})
        sources = {source["source"] for source in row["result"].get("damage_sources", []) if source["dmg"] > 0}
        if not set(coverage.get("damage_sources", [])).issubset(sources):
            raise ValueError(f"{row['id']}: required damage source did not execute")
        kinds = {event["kind"] for event in row["result"].get("events", [])}
        if not set(coverage.get("event_kinds", [])).issubset(kinds):
            raise ValueError(f"{row['id']}: required event did not execute")
        if len(row["result"].get("combatants", [])) < coverage.get("combatants", 0):
            raise ValueError(f"{row['id']}: required combatant did not execute")
        if 'more_procs_than' in coverage:
            control = results.get(coverage['more_procs_than'], {})
            actual, reference = row['result'].get('procs'), control.get('procs')
            if not isinstance(actual, (int, float)) or not isinstance(reference, (int, float)) or actual <= reference:
                raise ValueError(f"{row['id']}: required additional procs did not execute")
    return rows


def compare(args):
    calls_raw = args.calls.read_bytes()
    calls = read_json(args.calls)
    if not isinstance(calls, list) or not calls:
        raise ValueError("nonempty frozen call list required")
    identities = [c["id"] for c in calls]
    if any(not isinstance(x, str) or not x for x in identities) or len(set(identities)) != len(identities):
        raise ValueError("case ids must be unique nonempty strings")
    if any(not isinstance(c.get("body"), dict) or not c.get("path", "").startswith("/api/") for c in calls):
        raise ValueError("each case needs an API path and object body")
    snapshots = {"A": load_snapshot(args.baseline.resolve()), "B": load_snapshot(args.candidate.resolve())}
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    frozen_calls = output / "calls.json"
    frozen_calls.write_bytes(calls_raw)
    report = {"schema": 1, "calls_sha256": digest(calls_raw), "case_count": len(calls),
              "comparison_tool": {"sha256": digest(Path(__file__).read_bytes()),
                                  "python": sys.version, "platform": platform.platform()},
              "snapshots": {k: v[0] for k, v in snapshots.items()}, "order": [],
              "differences": [], "observations": [], "complete": False}
    write_json(output / "report.json", report)
    references, timing = {}, {k: {case: [] for case in identities} for k in snapshots}
    for index, side in enumerate("ABBA" * args.rounds):
        label = f"{index:03d}-{side}"
        manifest, binary = snapshots[side]
        load_snapshot(args.baseline.resolve() if side == "A" else args.candidate.resolve())
        expected = manifest["executables"]["simulation_cost"]["sha256"]
        if digest(binary.read_bytes()) != expected:
            raise ValueError("snapshot changed during comparison")
        if digest(frozen_calls.read_bytes()) != report["calls_sha256"]:
            raise ValueError("frozen calls changed during comparison")
        result_path = output / f"{label}.json"
        metrics = execute([str(binary), str(frozen_calls), str(result_path), str(args.repeats)], output, label)
        if digest(binary.read_bytes()) != expected or digest(frozen_calls.read_bytes()) != report["calls_sha256"]:
            raise ValueError("executable or frozen calls changed during execution")
        load_snapshot(args.baseline.resolve() if side == 'A' else args.candidate.resolve())
        rows = load_rows(result_path, calls)
        report["order"].append(side)
        report["observations"].append({"side": side, "result_file": result_path.name,
                                        "result_sha256": digest(result_path.read_bytes()),
                                        "linear_memory_bytes": max((row.get("memory_bytes", 0) for row in rows), default=0), **metrics})
        for row in rows:
            identity, result = row["id"], row["result"]
            timing[side][identity].extend(row["milliseconds"])
            own_key = (side, identity)
            if own_key in references:
                difference = first_difference(references[own_key], result)
                if difference is not None:
                    report["differences"].append({"case": identity, "comparison": "unstable",
                                                  "side": side, "sample": label, **difference})
            else:
                references[own_key] = result
            if side == "B":
                difference = first_difference(references[("A", identity)], result)
                if difference is not None:
                    report["differences"].append({"case": identity, "comparison": "between_versions",
                                                  "sample": label, **difference})
        write_json(output / "report.json", report)
        print(f"{label}: {len(rows)} cases", flush=True)
    report["timing"] = {
        side: {case: {"samples": len(values), "minimum_ms": min(values),
                      "median_ms": statistics.median(values), "maximum_ms": max(values)}
               for case, values in cases.items()} for side, cases in timing.items()}
    report["complete"] = True
    report["exact"] = not report["differences"]
    write_json(output / "report.json", report)
    print(json.dumps({"exact": report["exact"], "cases": len(calls), "invocations": len(report["order"]),
                      "differences": len(report["differences"]), "report": str(output / "report.json")}))
    return 0 if report["exact"] else 1


def positive(value):
    number = int(value)
    if number < 1:
        raise argparse.ArgumentTypeError("must be positive")
    return number


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    capture = commands.add_parser("freeze")
    capture.add_argument("--worktree", type=Path, required=True)
    capture.add_argument("--output", type=Path, required=True)
    capture.add_argument("--jobs", type=positive, default=2)
    capture.add_argument("--runtime", choices=["native", "wasm"], default="native")
    run = commands.add_parser("run")
    run.add_argument("--baseline", type=Path, required=True)
    run.add_argument("--candidate", type=Path, required=True)
    run.add_argument("--calls", type=Path, required=True)
    run.add_argument("--output", type=Path, required=True)
    run.add_argument("--rounds", type=positive, default=2)
    run.add_argument("--repeats", type=positive, default=3)
    args = parser.parse_args()
    try:
        return freeze(args) if args.command == "freeze" else compare(args)
    except (ValueError, OSError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        print(f"engine compare: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
