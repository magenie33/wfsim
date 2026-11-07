# SPDX-License-Identifier: AGPL-3.0-or-later
"""Build a native-host PGO library through the unchanged official C ABI."""
from __future__ import annotations

import argparse
import ctypes as C
import hashlib
import json
import os
from pathlib import Path
import statistics
import struct
import subprocess
import sys
import tempfile

import data_packs

ROOT = Path(__file__).resolve().parent.parent
TRAIN = ROOT / "tests/fixtures/simulation-cost.json"
HOLDOUT = ROOT / "tests/gas/rows.json"


def write(path: Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def numbers(value):
    if isinstance(value, bool) or value is None or isinstance(value, str):
        return value
    if isinstance(value, (int, float)):
        return ("number", struct.pack(">d", float(value)).hex())
    if isinstance(value, list):
        return [numbers(v) for v in value]
    return {k: numbers(v) for k, v in sorted(value.items())}


def exact(expected, actual) -> None:
    if numbers(expected) != numbers(actual):
        raise ValueError("PGO changed a complete API response, score, count, gas or work")


def host() -> str:
    version = subprocess.check_output(["rustc", "-vV"], text=True, encoding="utf-8")
    return next(line.split(": ", 1)[1] for line in version.splitlines() if line.startswith("host: "))


def flags(target: str, *pgo: str) -> list[str]:
    base = ["-Cstrip=none"] if "apple-darwin" in target else []
    if "windows-msvc" in target:
        base = ["-Ctarget-feature=+crt-static"]
    return [*base, *pgo]


class Native:
    def __init__(self, library: Path, pack: Path):
        self.lib = C.CDLL(str(library.resolve()))
        self.lib.wfsim_api.argtypes = [C.c_char_p, C.c_size_t, C.c_char_p, C.c_size_t, C.POINTER(C.c_size_t)]
        self.lib.wfsim_api.restype = C.c_void_p
        self.lib.wfsim_install_data.argtypes = [C.c_char_p, C.c_size_t, C.POINTER(C.c_size_t)]
        self.lib.wfsim_install_data.restype = C.c_void_p
        self.lib.wfsim_free.argtypes = [C.c_void_p, C.c_size_t]
        self.lib.wfsim_free.restype = None
        body = pack.read_bytes()
        error = self.output(self.lib.wfsim_install_data, body, len(body)).decode("utf-8")
        if error:
            raise ValueError(error)

    def output(self, function, *args) -> bytes:
        length = C.c_size_t()
        ptr = function(*args, C.byref(length))
        try:
            return C.string_at(ptr, length.value)
        finally:
            self.lib.wfsim_free(ptr, length.value)

    def api(self, path: str, body: dict):
        p = path.encode("utf-8")
        b = json.dumps(body, separators=(",", ":"), ensure_ascii=False, allow_nan=False).encode("utf-8")
        value = json.loads(self.output(self.lib.wfsim_api, p, len(p), b, len(b)))
        if value.get("ok") is not True:
            raise ValueError(f"{path}: {value}")
        return value

    def rows(self):
        for i, source in enumerate(json.loads(HOLDOUT.read_text(encoding="utf-8"))):
            request = self.api("/api/board/order", source)["request"]
            yield {"id": f'{i}-{source["record"]["weapon"]}', "ruler": source["ruler"],
                   "request": {**request, "runs": 40}}

    def score(self, row: dict, width: int = 40, reverse: bool = False):
        request, acc = row["request"], None
        if reverse:
            pieces = []
            for start in reversed(range(0, 40, width)):
                out = self.api("/api/board/runs", {"request": request, "from": start, "count": min(width, 40 - start)})
                pieces.append((start, out["shards"]))
            shards = [s for _, part in sorted(pieces) for s in part]
            acc = self.api("/api/board/fold", {"request": request, "pieces": shards})["acc"]
        else:
            for start in range(0, 40, width):
                acc = self.api("/api/board/fold", {"request": request, "from": start,
                                                   "count": min(width, 40 - start), "acc": acc})["acc"]
        return self.api("/api/board/score", {**row, "acc": acc})


def exercise(args) -> None:
    engine = Native(args.library, args.pack)
    calls = json.loads(TRAIN.read_text(encoding="utf-8"))
    if args.mode == "train":
        for _ in range(12):
            for call in calls:
                engine.api(call["path"], call["body"])
        write(args.output, {"api_cases": len(calls), "repeats": 12})
    elif args.mode == "verify":
        results = {"api": {c["id"]: engine.api(c["path"], c["body"]) for c in calls}, "rows": {}}
        for row in engine.rows():
            expected = engine.score(row)
            for width, reverse in [(1, False), (7, False), (7, True)]:
                exact(expected, engine.score(row, width, reverse))
            results["rows"][row["id"]] = expected
        write(args.output, results)
    else:
        import time
        result = {}
        for row in engine.rows():
            expected = engine.score(row)
            times = []
            for _ in range(3):
                start = time.perf_counter_ns()
                out = engine.score(row)
                times.append((time.perf_counter_ns() - start) / 1e6)
                exact(expected, out)
            result[row["id"]] = {"ms": times, "result": expected}
        write(args.output, result)


def select(baseline: dict, candidate: dict) -> tuple[str, dict]:
    if not baseline or baseline.keys() != candidate.keys():
        raise ValueError("missing PGO timing cases")
    ratios = {key: statistics.median(candidate[key]) / statistics.median(baseline[key]) for key in baseline}
    chosen = "pgo" if statistics.median(ratios.values()) < 0.98 and max(ratios.values()) <= 1.05 else "baseline"
    return chosen, ratios


def source_hash() -> str:
    names = subprocess.check_output(["git", "ls-files", "-z", "--", "engine", "optimizer", "webapi", "native",
                                     "tools/gas-macro", "Cargo.toml", "Cargo.lock"], cwd=ROOT).decode("utf-8").split("\0")
    h = hashlib.sha256()
    for name in sorted(n for n in names if n):
        h.update(name.encode("utf-8") + b"\0" + (ROOT / name).read_bytes() + b"\0")
    for path in [TRAIN, HOLDOUT, Path(__file__)]:
        h.update(path.name.encode("utf-8") + path.read_bytes())
    return h.hexdigest()


def build(args) -> None:
    target = args.target or host()
    if target != host():
        raise ValueError("PGO requires execution on the target host; cross-compilation cannot train it")
    if os.environ.get("WFSIM_GAS_OFF") is not None:
        raise ValueError("PGO builds retain gas metering; unset WFSIM_GAS_OFF")
    rust_flags = [k for k, v in os.environ.items() if v and
                  (k in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS") or k.startswith("CARGO_TARGET_") and k.endswith("_RUSTFLAGS"))]
    if rust_flags:
        raise ValueError(f"unset inherited compiler flags before the PGO build: {', '.join(rust_flags)}")
    args.work_dir.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="run-", dir=args.work_dir)).resolve()
    profile = work / "profile"
    profile.mkdir()
    frozen = source_hash()
    version = subprocess.check_output(["rustc", "-vV"], text=True, encoding="utf-8")
    sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], text=True, encoding="utf-8").strip())
    profdata = sysroot / "lib/rustlib" / target / "bin" / ("llvm-profdata.exe" if os.name == "nt" else "llvm-profdata")
    if not profdata.is_file():
        raise ValueError("install matching LLVM tools: rustup component add llvm-tools-preview")
    env = data_packs.build_env()
    artifacts, packs, build_flags = {}, {}, {}

    def compiled(stage: str, *pgo: str) -> None:
        if source_hash() != frozen:
            raise ValueError("source changed during PGO compilation")
        target_dir = work / stage
        build_flags[stage] = flags(target, *pgo)
        print(f"native PGO: build {stage} ({target})", flush=True)
        command = ["cargo", "build", "--release", "--locked", "-p", "wfsim-native", "--target", target,
                   "--message-format=json-render-diagnostics"]
        with (work / f"build-{stage}.jsonl").open("w", encoding="utf-8") as log:
            subprocess.run(command, cwd=ROOT, env={**env, "CARGO_TARGET_DIR": str(target_dir),
                                                 "CARGO_ENCODED_RUSTFLAGS": "\x1f".join(build_flags[stage])}, stdout=log, check=True)
        reports = [json.loads(line) for line in (work / f"build-{stage}.jsonl").read_text(encoding="utf-8").splitlines()]
        pack = next(Path(r["out_dir"]) / "packs/data.json" for r in reports
                    if r.get("reason") == "build-script-executed" and "wfsim-engine" in r.get("package_id", ""))
        packs[stage] = pack
        name = "wfsim_native.dll" if "windows" in target else "libwfsim_native.dylib" if "darwin" in target else "libwfsim_native.so"
        artifacts[stage] = target_dir / target / "release" / name
        if sha(pack) != sha(packs["baseline"]):
            raise ValueError("PGO stages compiled different data packs")

    def exercised(stage: str, mode: str, name: str, extra_env: dict | None = None):
        out = work / f"{name}.json"
        subprocess.run([sys.executable, str(Path(__file__).resolve()), "exercise", "--library", str(artifacts[stage]),
                        "--pack", str(packs[stage]), "--mode", mode, "--output", str(out)], cwd=ROOT,
                       env={**env, **(extra_env or {})}, check=True)
        return json.loads(out.read_text(encoding="utf-8"))

    compiled("baseline")
    expected = exercised("baseline", "verify", "verify-baseline")
    compiled("generate", f"-Cprofile-generate={profile}")
    exercised("generate", "train", "training", {"LLVM_PROFILE_FILE": str(profile / "%m_%p.profraw")})
    raw = sorted(profile.glob("*.profraw"))
    if not raw:
        raise ValueError("training produced no LLVM profile")
    merged = profile / "merged.profdata"
    subprocess.run([str(profdata), "merge", "-o", str(merged), *(str(p) for p in raw)], check=True)
    compiled("pgo", f"-Cprofile-use={merged}", "-Cllvm-args=-pgo-warn-missing-function")
    exact(expected, exercised("pgo", "verify", "verify-pgo"))
    timings = {stage: {} for stage in ["baseline", "pgo"]}
    for i, stage in enumerate(["baseline", "pgo", "pgo", "baseline"] * 2):
        print(f"native PGO: timing {i + 1}/8 ({stage})", flush=True)
        for key, entry in exercised(stage, "bench", f"bench-{i}-{stage}").items():
            exact(expected["rows"][key], entry["result"])
            timings[stage].setdefault(key, []).append(statistics.median(entry["ms"]))
    chosen, ratios = select(timings["baseline"], timings["pgo"])
    if source_hash() != frozen:
        raise ValueError("source changed during PGO verification")
    receipt = {"target": target, "rustc": version, "source_sha256": frozen, "data_sha256": sha(packs["baseline"]),
               "profile_sha256": sha(merged), "training_sha256": sha(TRAIN), "holdout_sha256": sha(HOLDOUT),
               "build_flags": build_flags, "selected": chosen, "cost_ratios": ratios,
               "median_cost_ratio": statistics.median(ratios.values()), "timings_ms": timings,
               "verified_api_cases": len(expected["api"]), "verified_rows": len(expected["rows"]),
               "baseline_sha256": sha(artifacts["baseline"]), "pgo_sha256": sha(artifacts["pgo"]),
               "selected_sha256": sha(artifacts[chosen]), "artifacts": str(work)}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(artifacts[chosen].read_bytes())
    verification = args.output.with_suffix(".verify.json")
    write(verification, expected)
    receipt["verification_sha256"] = sha(verification)
    write(args.output.with_suffix(".pgo.json"), receipt)
    print(f"native PGO: selected {chosen}; median cost ratio {receipt['median_cost_ratio']:.4f}; {args.output}", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("build")
    p.add_argument("--target")
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--work-dir", type=Path, default=ROOT / "target/native-pgo")
    p = sub.add_parser("exercise")
    p.add_argument("--library", type=Path, required=True)
    p.add_argument("--pack", type=Path, required=True)
    p.add_argument("--mode", choices=["train", "verify", "bench"], required=True)
    p.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "build":
        build(args)
    else:
        exercise(args)


if __name__ == "__main__":
    main()
