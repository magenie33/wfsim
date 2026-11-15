#!/usr/bin/env python3
"""Import pinned original-client N07A vectors without executing exchange code."""

import argparse
import hashlib
import json
from pathlib import Path
import zipfile

PIN = "ea4b3815f48d45b79fb63f5fc1af573ee008d2532049778db15852439382a1ec"
FIXTURE = Path(__file__).resolve().parents[1] / "tests/fixtures/client/ordinary-proc-2026-10-28.json"


def checked(data, entry):
    assert len(data) == entry["bytes"]
    assert hashlib.sha256(data).hexdigest() == entry["sha256"], entry["path"]


def import_vectors(exchange):
    raw = (exchange / "manifest.json").read_bytes()
    assert hashlib.sha256(raw).hexdigest() == PIN
    assert json.loads((exchange / "READY.json").read_bytes())["manifest_sha256"] == PIN
    manifest = json.loads(raw)
    for entry in manifest["files"]:
        checked((exchange / entry["path"]).read_bytes(), entry)
    with zipfile.ZipFile(exchange / "client-evidence.zip") as archive:
        for entry in manifest["archive_members"]["client-evidence.zip"]:
            checked(archive.read(entry["path"]), entry)
        results = json.loads(archive.read("results.json"))
        evidence = json.loads(archive.read("native-evidence.json"))
        coverage = json.loads(archive.read("native-executed-coverage.json"))
        algorithm = json.loads(archive.read("algorithm.json"))
    assert len(results["weight_vectors"]) == 9
    assert len(results["sampling_vectors"]) == 1895
    graphs, components, samples = [], [], []
    for vector in results["sampling_vectors"]:
        graph, damage = vector["projected_evaluators"], vector["components"]
        if graph not in graphs:
            graphs.append(graph)
        if damage not in components:
            components.append(damage)
        assert vector["packet_bytes_except_declared_writes_unchanged"]
        assert len(vector["native_proc_counters"]) == len(vector["initial_proc_counters"]) == 30
        assert vector["observed_original_rng_state_writes"] == [
            {"purpose": draw["purpose"], "state": draw["state"]} for draw in vector["draws"]]
        sample = {k: v for k, v in vector.items() if k not in {
            "components", "projected_evaluators", "observed_original_rng_state_writes",
            "packet_bytes_except_declared_writes_unchanged"}}
        sample.update(graph=graphs.index(graph), components=components.index(damage))
        samples.append(sample)
    return dict(source=dict(batch=exchange.name, manifest_sha256=PIN,
        client_build=algorithm["client_build"], source_mode=algorithm["source_mode"],
        validation=algorithm["validation"], scope=algorithm["scope"],
        coverage=coverage, outer_files=len(manifest["files"]),
        archive_members=len(manifest["archive_members"]["client-evidence.zip"])),
        graphs=graphs, components=components, weight_vectors=results["weight_vectors"],
        selector_vectors=evidence["damage_group_predicate_vectors"],
        damage_enum=evidence["damage_enum"], enum_values=evidence["damage_enum_values"],
        sampling_vectors=samples)


def encode(fixture):
    # One native vector per line keeps repeated imports and reviews bounded.
    sections = []
    for key, value in fixture.items():
        if isinstance(value, list):
            encoded = "[\n" + ",\n".join(json.dumps(row, separators=(",", ":")) for row in value) + "\n]"
        else:
            encoded = json.dumps(value, separators=(",", ":"))
        sections.append(json.dumps(key) + ":" + encoded)
    return "{\n" + ",\n".join(sections) + "\n}\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exchange", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    expected = encode(import_vectors(args.exchange))
    if args.check:
        assert FIXTURE.read_text() == expected, "ordinary-proc fixture differs from pinned native evidence"
    else:
        FIXTURE.write_text(expected)
    print("N07A: 8 files / 223 members verified; 9 weight / 1895 sampling vectors " +
          ("checked" if args.check else "imported"))


if __name__ == "__main__":
    main()
