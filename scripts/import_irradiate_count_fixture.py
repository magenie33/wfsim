#!/usr/bin/env python3
"""Import bounded native count oracles; never infer a production comparator."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

MANIFEST = "d0266d52e23aaece03c1654190c2a7f75c252b67754fe22c3116c9a1a970176f"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("batch", type=Path)
    args = parser.parse_args()
    raw_manifest = (args.batch / "manifest.json").read_bytes()
    assert digest(raw_manifest) == MANIFEST, "unreviewed source manifest"
    manifest = json.loads(raw_manifest)
    for entry in manifest["files"]:
        raw = (args.batch / entry["path"]).read_bytes()
        assert len(raw) == entry["bytes"] and digest(raw) == entry["sha256"]
    with zipfile.ZipFile(args.batch / "evidence.zip") as archive:
        for entry in manifest["archive_members"]["evidence.zip"]:
            raw = archive.read(entry["path"])
            assert len(raw) == entry["bytes"] and digest(raw) == entry["sha256"]
        name = "count-replay-v2/RESULT.json"
        result_raw = archive.read(name)
        result = json.loads(result_raw)
    assert len(result["vectors"]) == 14
    assert len(result["executed_rvas"]) == 816
    assert result["all_executed_addresses_preserved"] is True
    vectors = result["vectors"]
    for vector in vectors:
        if "hit_types" in vector:
            # These bounded inputs hold the remaining comparator fields equal.
            types = vector["hit_types"]
            vector["supplied_compatible_groups"] = [
                [i for i, value in enumerate(types) if value == kind]
                for kind in dict.fromkeys(types)
            ]
        elif vector["case"] == "different-comparator-byte-0x210":
            vector["supplied_compatible_groups"] = [[0], [1]]
    fixture = {
        "source_manifest_sha256": MANIFEST,
        "client_build": manifest["client_build"],
        "executable_sha256": result["executable_sha256"],
        "result_sha256": digest(result_raw),
        "executed_original_addresses": 816,
        "scope": "Count projection with supplied compatibility; not a native comparator or scheduler replay in Rust",
        "function_adapters": result["function_adapters"],
        "not_replayed": result["not_replayed"],
        "vectors": vectors,
    }
    out = Path(__file__).resolve().parents[1] / "tests/fixtures/client/irradiate-count-2026-10-28.json"
    out.write_text(json.dumps(fixture, indent=2, ensure_ascii=False) + "\n")
    print(f"verified 5 files, 1292 members; imported {len(vectors)} count vectors: {out}")


if __name__ == "__main__":
    main()
