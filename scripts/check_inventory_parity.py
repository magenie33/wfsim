#!/usr/bin/env python3
"""Compare frozen dry-run inventory outputs and count actual input work.

snapshot copies tracked source into an isolated, instrumented build directory.
run accepts two such wfsim-board binaries; all engine calls use --dry-run.
"""

import argparse
import hashlib
import itertools
import json
import os
from pathlib import Path
import random
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time


ROOT = Path(__file__).resolve().parent.parent
READ_MARKER = "inventory-read "
WORK_MARKER = "inventory-work "


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def write_rows(path, rows):
    with path.open("w") as out:
        for row in rows:
            out.write(encoded(row) + "\n")


def read_rows(path):
    if not path.is_file():
        raise AssertionError(f"missing output: {path}")
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def digest(path):
    with path.open("rb") as source:
        hashed = hashlib.sha256()
        for block in iter(lambda: source.read(1024 * 1024), b""):
            hashed.update(block)
        return hashed.hexdigest()


def instrument(copy):
    if (copy / ".git").exists() or not (copy / ".inventory-benchmark-copy").is_file():
        raise ValueError("instrumentation requires an isolated snapshot copy")
    queue = copy / "cli/src/board/queue.rs"
    text = queue.read_text()
    needle = "std::fs::read_to_string(&path)"
    count = text.count(needle)
    if not count:
        needle = "std::fs::File::open(&path)"
        count = text.count(needle)
    if count not in (1, 2):
        raise ValueError(f"expected one new or two old queue read sites, found {count}")
    marked = '({ eprintln!("inventory-read {{\\\"phase\\\":\\\"queue\\\"}}"); ' + needle + " })"
    queue.write_text(text.replace(needle, marked))
    entry = copy / "cli/src/board/entry.rs"
    text = entry.read_text()
    begin = text.index("pub(crate) fn park_under_entry_line(")
    end = text.index("/// ONE ROW PER SHAPE", begin)
    part = text[begin:end]
    needle = "for f in cross {"
    if part.count(needle) != 2:
        raise ValueError("entry gate must expose exactly two cross-fact passes")
    marked = 'eprintln!("inventory-work {{\\\"phase\\\":\\\"gate\\\",\\\"rows\\\":{}}}", cross.len());\n    ' + needle
    entry.write_text(text[:begin] + part.replace(needle, marked) + text[end:])
    run = copy / "cli/src/board/run.rs"
    text = run.read_text()
    needle = "    let ids: Vec<&str> = roster.iter().map(String::as_str).collect();"
    if text.count(needle) != 1:
        raise ValueError("cannot locate unique each-ruler roster")
    limit = '''    let roster: Vec<String> = roster.into_iter().take(
        std::env::var("WFSIM_INVENTORY_BENCH_RULERS").ok()
            .and_then(|s| s.parse::<usize>().ok()).unwrap_or(usize::MAX)
    ).collect();
'''
    run.write_text(text.replace(needle, limit + needle))
    return {"queue_read_sites": count, "gate_loop_sites": 2, "roster_limit": True}


def snapshot(args):
    repo, output = args.repo.resolve(), args.output.resolve()
    if output.exists():
        raise ValueError("snapshot output must not exist")
    revision = args.revision or "HEAD"
    commit = subprocess.check_output(["git", "rev-parse", revision], cwd=repo, text=True).strip()
    output.mkdir(parents=True)
    with tempfile.TemporaryFile() as archive:
        subprocess.run(["git", "archive", "--format=tar", commit], cwd=repo, stdout=archive, check=True)
        archive.seek(0)
        with tarfile.open(fileobj=archive, mode="r|") as entries:
            for entry in entries:
                relative = Path(entry.name)
                if relative.is_absolute() or ".." in relative.parts or entry.issym() or entry.islnk():
                    raise ValueError(f"unsupported archive entry: {entry.name}")
                destination = output / relative
                if entry.isdir():
                    destination.mkdir(parents=True, exist_ok=True)
                elif entry.isfile():
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    with entries.extractfile(entry) as source, destination.open("wb") as target:
                        shutil.copyfileobj(source, target)
                    destination.chmod(entry.mode)
    changed = []
    patch_hash = None
    if not args.revision:
        patch = subprocess.check_output(["git", "diff", "--binary", "HEAD"], cwd=repo)
        patch_hash = hashlib.sha256(patch).hexdigest()
        changed = subprocess.check_output(["git", "diff", "--name-only", "-z", "HEAD"], cwd=repo).decode().split("\0")
        for name in filter(None, changed):
            source, target = repo / name, output / name
            if source.is_file():
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, target)
            elif target.is_file():
                target.unlink()
    (output / ".inventory-benchmark-copy").write_text("isolated source instrumentation\n")
    manifest = {"commit": commit, "dirty_patch_sha256": patch_hash, "overlaid_files": list(filter(None, changed)),
                "instrumentation": instrument(output)}
    write_json(output / "inventory-benchmark-source.json", manifest)
    print(encoded(manifest))


def invoke(binary, arguments, library, directory, rulers=None, deadline=180):
    directory.mkdir(parents=True, exist_ok=False)
    env = {**os.environ, "WFSIM_INVENTORY_BENCH_RULERS": str(rulers or 1000000)}
    command = [str(binary), *map(str, arguments)]
    started = time.monotonic()
    with library.open("rb") as source, (directory / "stdout").open("wb") as out, (directory / "stderr").open("wb") as err:
        child = subprocess.Popen(command, stdin=source, stdout=out, stderr=err, env=env)
        try:
            while True:
                pid, status, usage = os.wait4(child.pid, os.WNOHANG)
                if pid:
                    child.returncode = os.waitstatus_to_exitcode(status)
                    break
                if time.monotonic() - started > deadline:
                    raise TimeoutError(f"inventory child exceeded {deadline}s: {directory}")
                time.sleep(0.02)
        except BaseException:
            child.kill()
            child.wait()
            raise
    stderr = (directory / "stderr").read_text()
    events, reads, visits = [], 0, 0
    for line in stderr.splitlines():
        if line.startswith(READ_MARKER):
            json.loads(line[len(READ_MARKER):])
            reads += 1
        elif line.startswith(WORK_MARKER):
            visits += json.loads(line[len(WORK_MARKER):])["rows"]
        elif line.startswith("inventory-phase "):
            events.append(json.loads(line[len("inventory-phase "):]))
    result = {"command": command, "exit": child.returncode, "wall_seconds": time.monotonic() - started,
              "user_seconds": usage.ru_utime, "system_seconds": usage.ru_stime,
              "peak_rss_bytes": usage.ru_maxrss * (1 if sys.platform == "darwin" else 1024),
              "queue_reads": reads, "gate_fact_visits": visits, "phase_events": events}
    write_json(directory / "metrics.json", result)
    if child.returncode:
        raise AssertionError(f"inventory exited {child.returncode}; see {directory / 'stderr'}")
    return result


def library_records(count):
    seeds = [row["record"] for row in json.loads((ROOT / "tests/gas/rows.json").read_text())]
    records, seen = [], set()
    # Lower ranks retain legal cards and capacity; aliases cannot inflate B.
    for ranks in itertools.chain([None], itertools.product(range(4), repeat=5)):
        for seed in seeds:
            mods = seed.get("mods", [])
            if ranks is not None:
                mods = [f"{mod.split('@')[0]}@{ranks[i]}" if i < len(ranks) else mod for i, mod in enumerate(mods)]
            record = {**seed, "mods": mods}
            key = encoded(record)
            if key not in seen:
                seen.add(key)
                records.append(record)
                if len(records) == count:
                    break
        if len(records) == count:
            return records
    raise ValueError("legal fixture rank combinations exhausted")


def output_arguments(directory):
    return ["--inventory-builds", directory / "canonical.ndjson",
            "--queue-missing", directory / "{id}.missing", "--queue-orphans", directory / "{id}.orphans"]


def outputs(directory, roster):
    return {"canonical": read_rows(directory / "canonical.ndjson"),
            "missing": {r: read_rows(directory / f"{r}.missing") for r in roster},
            "orphans": {r: read_rows(directory / f"{r}.orphans") for r in roster}}


def compare_outputs(before, after):
    if before != after:
        for kind in before:
            if before[kind] != after[kind]:
                raise AssertionError(f"full ordered {kind} records differ")


def summary(output):
    missing = [r for rows in output["missing"].values() for r in rows]
    orphans = [r for rows in output["orphans"].values() for r in rows]
    return {"canonical_rows": len(output["canonical"]), "canonical_ids": len({r["id"] for r in output["canonical"]}),
            "missing_rows": len(missing), "missing_identities": len({(r["build_id"], r["ruler"], r["mode"]) for r in missing}),
            "orphan_rows": len(orphans), "outputs_sha256": hashlib.sha256(encoded(output).encode()).hexdigest()}


def assert_work_bounds(metrics, empty_pending=False):
    if metrics["queue_reads"] != 1:
        raise AssertionError("queue snapshot was read more than once")
    if empty_pending and metrics["gate_fact_visits"] != 0:
        raise AssertionError("an empty pending set still walks cross-ruler facts")


def negative_control(metrics, empty_pending=False):
    try:
        assert_work_bounds(metrics, empty_pending)
    except AssertionError as error:
        return str(error)
    raise AssertionError("old implementation unexpectedly passed the work bound")


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    before, after = args.before.resolve(), args.after.resolve()
    roster = subprocess.check_output([str(before), "--rulers"], text=True).splitlines()
    if roster != subprocess.check_output([str(after), "--rulers"], text=True).splitlines():
        raise AssertionError("A/B ruler rosters differ")
    small, base, large = (16, 48, 144) if not args.large else (64, 256, 1024)
    scale = 1 if not args.large else 8
    cases = [("base", base, 256 * scale, 256 * scale, 8),
             ("builds-small", small, 256 * scale, 256 * scale, 8),
             ("builds-large", large, 256 * scale, 256 * scale, 8),
             ("facts-empty", base, 0, 256 * scale, 8),
             ("facts-large", base, 4096 * scale, 256 * scale, 8),
             ("queue-empty", base, 256 * scale, 0, 8),
             ("queue-large", base, 256 * scale, 4096 * scale, 8),
             ("queue-invalid-utf8", base, 256 * scale, 256 * scale, 8),
             ("rulers-one", base, 256 * scale, 256 * scale, 1),
             ("rulers-all", base, 256 * scale, 256 * scale, len(roster)),
             ("combined-scale", large, 4096 * scale, 4096 * scale, len(roster)),
             ("fully-queued", base, 256 * scale, -1, len(roster))]
    if args.case:
        unknown = set(args.case) - {case[0] for case in cases}
        if unknown:
            raise ValueError(f"unknown cases: {sorted(unknown)}")
        cases = [case for case in cases if case[0] in args.case]
    candidates = output / "fixture-candidates.json"
    write_json(candidates, library_records(large * 3))
    invoke(before, ["--inventory-builds"], candidates, output / "normalize", deadline=args.deadline)
    unique = {}
    for row in read_rows(output / "normalize/stdout"):
        unique.setdefault(row["id"], row["record"])
    if len(unique) < large:
        raise AssertionError("fixture candidates contain too few engine-validated distinct builds")
    corpus, frozen = list(unique.values())[:large], {}
    write_json(output / "fixture-source.json", {"candidates": large * 3, "validated_unique": len(unique),
               "candidate_sha256": digest(candidates), "seed_sha256": digest(ROOT / "tests/gas/rows.json")})
    empty = output / "empty.ndjson"
    empty.write_text("")
    for count in (small, base, large):
        directory = output / f"fixture-{count}"
        directory.mkdir()
        # Alias and malformed source records must not silently change the walk.
        library = directory / "library.json"
        write_json(library, corpus[:count] + [corpus[0], {**corpus[0], "ignored_source_field": True}, {"weapon": "not-a-weapon"}])
        census = directory / "census"
        metrics = invoke(before, ["--each-ruler", "--dry-run", "--queue-in", empty, *output_arguments(census)],
                         library, census, deadline=args.deadline)
        data = outputs(census, roster)
        if summary(data)["canonical_ids"] < count:
            raise AssertionError(f"legal fixture did not produce {count} distinct canonical builds")
        rows = [r for values in data["missing"].values() for r in values]
        if not rows:
            raise AssertionError("fixture census contains no legal rows")
        frozen[count] = (library, rows, data)
        write_json(directory / "manifest.json", {"builds": count, "library_sha256": digest(library),
                   "census": summary(data), "metrics": metrics})
    result = {"before_binary": {"path": str(before), "sha256": digest(before)},
              "after_binary": {"path": str(after), "sha256": digest(after)}, "roster": roster, "cases": []}
    for name, count, fact_count, queue_count, ruler_count in cases:
        directory = output / name
        directory.mkdir()
        library, census, canonical = frozen[count]
        facts = []
        for i, row in enumerate(census[:fact_count]):
            facts.append({"identity": row["build_id"], "ruler": row["ruler"], "mode": row["mode"],
                          "score": [0, 5, 20, 100][i % 4], "cost_seconds": 1,
                          "finished_at": "2026-11-01T00:00:00.000Z"})
        if facts:
            # Tombstones and older measurements retain their actual timestamp rules.
            facts += [{**facts[0], "retired_at": "2026-11-02T00:00:00.000Z"},
                      {**facts[0], "score": 200, "finished_at": "2026-10-01T00:00:00.000Z"},
                      {**facts[-1], "ruler": "retired-ruler", "score": 99999}]
        write_rows(directory / "facts.ndjson", facts)
        if queue_count == -1:
            queued = [dict(row, batch="all") for row in census]
        else:
            queued = [dict(census[i % len(census)], batch=f"batch-{i}") for i in range(queue_count)]
        if queued:
            queued += [{"build_id": "unknown-build", "ruler": roster[0], "mode": "base"},
                       {"build_id": census[0]["build_id"], "ruler": roster[0], "mode": "removed-mode"},
                       {"build_id": census[-1]["build_id"], "ruler": "retired-ruler"},
                       {"ruler": roster[0]}, {"build_id": "unscoped-arrival"}]
            random.Random(20261106).shuffle(queued)
        write_rows(directory / "queue.ndjson", queued)
        if name == "queue-invalid-utf8":
            with (directory / "queue.ndjson").open("ab") as out:
                out.write(b"\xff\n")
        selected = roster[:ruler_count]
        record = {"name": name, "B": count, "F": len(facts), "Q": len(queued), "R": len(selected),
                  "input_sha256": {file: digest(directory / file) for file in ("facts.ndjson", "queue.ndjson")}}
        sides = {}
        # Alternate A/B order across cases; timings remain measurements only.
        binaries = [("before", before), ("after", after)]
        if len(result["cases"]) % 2:
            binaries.reverse()
        record["run_order"] = [side for side, _ in binaries]
        for side, binary in binaries:
            destination = directory / side
            record[side] = invoke(binary, ["--each-ruler", "--dry-run", "--gate", "--facts-in", directory / "facts.ndjson",
                                  "--queue-in", directory / "queue.ndjson", *output_arguments(destination)],
                                  library, destination, ruler_count, args.deadline)
            sides[side] = outputs(destination, selected)
        compare_outputs(sides["before"], sides["after"])
        record["output"] = summary(sides["after"])
        # These markers surround real reads and loop entry in isolated source copies.
        if record["before"]["queue_reads"] != len(selected) + 1:
            raise AssertionError("baseline read instrumentation did not observe R+1 queue reads")
        assert_work_bounds(record["after"])
        record["old_read_bound_negative"] = negative_control(record["before"])
        if name == "fully-queued":
            if record["output"]["missing_rows"] != 0 or record["before"]["gate_fact_visits"] == 0:
                raise AssertionError("fully queued fixture did not exercise the old empty-pending gate")
            assert_work_bounds(record["after"], empty_pending=True)
            record["old_gate_bound_negative"] = negative_control({**record["before"], "queue_reads": 1}, empty_pending=True)
        result["cases"].append(record)
        write_json(output / "result.json", result)
        print(encoded({"case": name, "B": count, "F": len(facts), "Q": len(queued), "R": len(selected),
                       **record["output"], "queue_reads": [record[s]["queue_reads"] for s in ("before", "after")],
                       "gate_fact_visits": [record[s]["gate_fact_visits"] for s in ("before", "after")],
                       "wall_seconds": [round(record[s]["wall_seconds"], 3) for s in ("before", "after")]}), flush=True)
    result["complete"] = True
    write_json(output / "result.json", result)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    copy = commands.add_parser("snapshot", help="copy tracked sources and instrument only that copy")
    copy.add_argument("--repo", type=Path, default=ROOT)
    copy.add_argument("--revision", help="omit to overlay tracked working changes; use HEAD for the old source")
    copy.add_argument("--output", type=Path, required=True)
    check = commands.add_parser("run", help="run engine-legal frozen B/F/Q/R cases")
    check.add_argument("--before", type=Path, required=True)
    check.add_argument("--after", type=Path, required=True)
    check.add_argument("--output", type=Path, required=True)
    check.add_argument("--large", action="store_true", help="use 64/256/1024 builds and eightfold F/Q targets")
    check.add_argument("--case", action="append", help="select a named case; repeat to select several")
    check.add_argument("--deadline", type=float, default=180)
    args = parser.parse_args()
    {"snapshot": snapshot, "run": run}[args.command](args)


if __name__ == "__main__":
    main()
