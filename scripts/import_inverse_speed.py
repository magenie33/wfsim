#!/usr/bin/env python3
"""Verify the closed inverse-speed delivery and retain its scoped native vectors."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[1]
BATCH = "20261110T043544Z-inverse-speed-client-only"
PIN = "4ae59166c4188923db55772aea194a4198da4c99cbe325ca5b991285cc721851"
FIXTURE = ROOT / "tests/fixtures/client/inverse-speed-2026-10-28.json"


def checked(data, row):
    assert len(data) == row["bytes"] and hashlib.sha256(data).hexdigest() == row["sha256"], row["path"]
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exchange", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    batch = args.exchange / "batches" / BATCH
    raw = (batch / "manifest.json").read_bytes()
    manifest = json.loads(raw)
    assert hashlib.sha256(raw).hexdigest() == json.loads((batch / "READY.json").read_text())["manifest_sha256"] == PIN
    assert manifest["batch_id"] == BATCH
    outer = {r["path"]: checked((batch / r["path"]).read_bytes(), r) for r in manifest["files"]}
    selected = {}
    count = 0
    for name, rows in manifest["archive_members"].items():
        with zipfile.ZipFile(io.BytesIO(outer[name])) as archive:
            assert set(archive.namelist()) == {r["path"] for r in rows}
            for row in rows:
                data = checked(archive.read(row["path"]), row)
                count += 1
                if name == "client-evidence.zip" and row["path"] in ["verification.json", "bindings.json", "native-evidence.json"]:
                    selected[row["path"]] = json.loads(data)
    vectors = selected["verification.json"]
    assert vectors["passed"] == 322 and vectors["failed"] == 0 and len(vectors["vectors"]) == 322
    fixture = {
        "source": {"batch_id": BATCH, "manifest_sha256": PIN, "client_build": manifest["client_build"],
            "research_commit": manifest["research_commit"], "extractor_commit": manifest["extractor_commit"],
            "extractor_sha256": manifest["extractor_sha256"], "files": manifest["files"],
            "archive_members": manifest["archive_members"], "outer_files_verified": len(outer), "members_verified": count},
        "verification": vectors,
        "bindings": selected["bindings.json"],
        "limits": json.loads(outer["coverage.json"])["limitations"],
        "model_scope": {
            "applied": ["default positive-base native refire floor", "WS_RELOAD speed denominator",
                "selected generic charge-speed denominator", "burst scalar minimum", "projectile initial-speed setter"],
            "retained": ["nonpositive configured-rate state permissions", "positive runtime weapon-override lifecycle",
                "scalar query routing outside the declared draw model", "heavy-charge flags and later FSM",
                "casting playback and marker helpers versus complete cast lock", "projectile impulse/maximum speed/update lifecycle",
                "nock and mode-transition denominator consumers"]},
    }
    text = json.dumps(fixture, indent=2, allow_nan=False) + "\n"
    same = FIXTURE.exists() and FIXTURE.read_text() == text
    if args.check:
        assert same, "inverse-speed fixture differs from the pinned delivery"
    else:
        FIXTURE.write_text(text)
    print(json.dumps({"batch": BATCH, "outer_files": len(outer), "members": count,
        "vectors": len(vectors["vectors"]), "check": args.check, "changed": not same}))


if __name__ == "__main__":
    main()
