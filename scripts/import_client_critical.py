# SPDX-License-Identifier: AGPL-3.0-or-later
"""Import the original critical consumer and signed health guard vectors."""

import argparse
import hashlib
import json
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[1]
BATCH = "20261112T155444Z-negative-stats"
MANIFEST = "716f6de875d69cde57420248d2fdd9bfa632ed005878b2b08789b525ae468fe4"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exchange", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    folder = args.exchange / "batches" / BATCH
    raw = (folder / "manifest.json").read_bytes()
    assert hashlib.sha256(raw).hexdigest() == MANIFEST
    assert json.loads((folder / "READY.json").read_bytes())["manifest_sha256"] == MANIFEST
    manifest = json.loads(raw)
    data = (folder / "client-evidence.zip").read_bytes()
    ref = next(row for row in manifest["files"] if row["path"] == "client-evidence.zip")
    assert len(data) == ref["bytes"] and hashlib.sha256(data).hexdigest() == ref["sha256"]
    bindings = []
    with zipfile.ZipFile(folder / "client-evidence.zip") as archive:
        members = {row["path"]: row for row in manifest["archive_members"]["client-evidence.zip"]}
        names = [name for name in archive.namelist() if name.endswith("asm.txt")]
        names += ["replay.json", "provenance.json", "native-evidence.json"]
        for name in names:
            data = archive.read(name)
            ref = members[name]
            assert len(data) == ref["bytes"] and hashlib.sha256(data).hexdigest() == ref["sha256"]
            bindings.append(ref)
        vectors = json.loads(archive.read("replay.json"))
    fixture = {"schema": 1, "batch": BATCH, "manifest_sha256": MANIFEST,
               "client_build": manifest["client_build"], "source_members": bindings,
               "scope": "Ordinary critical roll/consumer, canonical scalar quantizer and neutral health guard; installer and pool minimum overrides are separate",
               **vectors}
    path = ROOT / "tests/fixtures/client/critical-2026-10-28.json"
    output = json.dumps(fixture, indent=2) + "\n"
    if args.check:
        assert path.read_text() == output, str(path)
    else:
        path.write_text(output)
    print(f"Verified {sum(len(rows) for rows in vectors.values())} native cases and {len(bindings)} source members")
    batch = "20261112T151305Z-magnetic-proc-context"
    pin = "c634b163c105bc230695f778dacbcb01edbd96e28e2370268a9847ef143d150e"
    folder = args.exchange / "batches" / batch
    raw = (folder / "manifest.json").read_bytes()
    assert hashlib.sha256(raw).hexdigest() == pin == json.loads((folder / "READY.json").read_bytes())["manifest_sha256"]
    manifest = json.loads(raw)
    data = (folder / "client-evidence.zip").read_bytes()
    ref = next(row for row in manifest["files"] if row["path"] == "client-evidence.zip")
    assert len(data) == ref["bytes"] and hashlib.sha256(data).hexdigest() == ref["sha256"]
    bindings = []
    with zipfile.ZipFile(folder / "client-evidence.zip") as archive:
        members = {row["path"]: row for row in manifest["archive_members"]["client-evidence.zip"]}
        names = [name for name in archive.namelist() if name.endswith("asm.txt")]
        names += ["replay.json", "native-evidence.json"]
        for name in names:
            data = archive.read(name)
            ref = members[name]
            assert len(data) == ref["bytes"] and hashlib.sha256(data).hexdigest() == ref["sha256"]
            bindings.append(ref)
        vectors = json.loads(archive.read("replay.json"))
    fixture = {"schema": 1, "batch": batch, "manifest_sha256": pin,
               "scope": "Supplied native scalar/canonicalization/proc-filter and record-group boundaries; live producer/context ownership is separate",
               "source_members": bindings, **vectors}
    path = ROOT / "tests/fixtures/client/packet-context-2026-10-28.json"
    output = json.dumps(fixture, indent=2) + "\n"
    if args.check:
        assert path.read_text() == output, str(path)
    else:
        path.write_text(output)
    print(f"Verified {len(vectors['two_stage_upgrade_cases'])} parent/child scalar cases and {len(bindings)} source members")


if __name__ == "__main__":
    main()
