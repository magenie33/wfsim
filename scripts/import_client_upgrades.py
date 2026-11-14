# SPDX-License-Identifier: AGPL-3.0-or-later
"""Import native attribute ids and original-instruction fold vectors."""

import argparse
import hashlib
import json
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[1]
BATCH = "20261107T040140Z-unified-precision"
MANIFEST = "b35a34aef95fea35f65d356a45790213706654a18df26ab8534b18c0dc394ffa"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exchange", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    folder = args.exchange / "batches" / BATCH
    raw = (folder / "manifest.json").read_bytes()
    ready = json.loads((folder / "READY.json").read_bytes())
    assert hashlib.sha256(raw).hexdigest() == MANIFEST == ready["manifest_sha256"]
    manifest = json.loads(raw)
    for name in ["evidence.zip", "calculation-verification.json"]:
        expected = next(row for row in manifest["files"] if row["path"] == name)
        data = (folder / name).read_bytes()
        assert len(data) == expected["bytes"]
        assert hashlib.sha256(data).hexdigest() == expected["sha256"]
    with zipfile.ZipFile(folder / "evidence.zip") as archive:
        consumed = {}
        for name in ["enums/enum-evidence.json", "data/operation-order-vectors.json",
                     "native/upgrade-manager-apply.asm.txt", "native/upgrade-operation-buckets.asm.txt"]:
            data = archive.read(name)
            expected = next(row for row in manifest["archive_members"]["evidence.zip"] if row["path"] == name)
            assert len(data) == expected["bytes"]
            assert hashlib.sha256(data).hexdigest() == expected["sha256"]
            consumed[name] = data
    enums = json.loads(consumed["enums/enum-evidence.json"])
    vectors = json.loads(consumed["data/operation-order-vectors.json"])
    assert len(enums["upgrades"]) == 428 and len(vectors) == 1115
    fixture = {"schema": 1, "batch": BATCH, "manifest_sha256": MANIFEST,
               "client_build": manifest["client_build"],
               "scope": "Native operation folding and enum identity; filtering and installation are caller-owned",
               "upgrades": enums["upgrades"], "vectors": vectors}
    ids = ["// SPDX-License-Identifier: AGPL-3.0-or-later",
           "// Native enum vocabulary; scripts/import_client_upgrades.py verifies the source binding.",
           "use super::UpgradeType;", "", "#[wfsim_gas_macro::metered]", "impl UpgradeType {"]
    for key, value in sorted(enums["upgrades"].items(), key=lambda row: int(row[0])):
        ids.append(f"    pub const {value}: Self = Self({key});")
    ids.extend(["}", ""])
    outputs = {
        ROOT / "engine/src/rules/upgrades/ids.rs": "\n".join(ids),
        ROOT / "tests/fixtures/client/upgrade-operations-2026-10-28.json": json.dumps(fixture, indent=2) + "\n",
    }
    status = {}
    status_pins = [
        ("contexts", "20261114T031604Z-status-arcane-contexts", "59a45246a8b48064228aaa999cffea1dd9de1633978545a3b96830fbb4b90f5b"),
        ("lifecycle", "20261114T085659Z-status-arcane-lifecycle", "a804daa5f7aa3cf422dd51a6313563d5e9c8f468b10e93986f6f98b4f3b92393"),
    ]
    for key, batch, pin in status_pins:
        directory = args.exchange / "batches" / batch
        raw = (directory / "manifest.json").read_bytes()
        assert hashlib.sha256(raw).hexdigest() == pin == json.loads((directory / "READY.json").read_bytes())["manifest_sha256"]
        index = json.loads(raw)
        data = (directory / "client-evidence.zip").read_bytes()
        expected = next(row for row in index["files"] if row["path"] == "client-evidence.zip")
        assert len(data) == expected["bytes"] and hashlib.sha256(data).hexdigest() == expected["sha256"]
        with zipfile.ZipFile(directory / "client-evidence.zip") as archive:
            payload = archive.read("results.json")
            expected = next(row for row in index["archive_members"]["client-evidence.zip"] if row["path"] == "results.json")
            assert len(payload) == expected["bytes"] and hashlib.sha256(payload).hexdigest() == expected["sha256"]
            status[key] = {"batch": batch, "manifest_sha256": pin, "results": json.loads(payload)}
    outputs[ROOT / "tests/fixtures/client/status-arcanes-2026-10-28.json"] = json.dumps(status, indent=2) + "\n"
    for path, text in outputs.items():
        if args.check:
            assert path.read_text() == text, str(path)
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
    print(f"Verified {len(enums['upgrades'])} native ids and {len(vectors)} original-instruction vectors")


if __name__ == "__main__":
    main()
