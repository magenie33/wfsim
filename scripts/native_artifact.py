# SPDX-License-Identifier: AGPL-3.0-or-later
"""Bind a CI native library to the exact checked release inputs and bytes."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import sys

FIELDS = ("engine", "wasm", "data", "commit")


def stamp(library: Path, release: dict) -> None:
    body = library.read_bytes()
    record = {**{key: release[key] for key in FIELDS}, "file": library.name,
              "sha256": hashlib.sha256(body).hexdigest(), "size": len(body)}
    library.with_name("manifest.json").write_text(json.dumps(record, sort_keys=True) + "\n", encoding="utf-8")


def load(library: Path, release: dict) -> bytes | None:
    manifest = library.with_name("manifest.json")
    if not library.exists() and not manifest.exists():
        return None
    record = json.loads(manifest.read_text(encoding="utf-8"))
    if not isinstance(record, dict):
        raise ValueError("native artifact manifest must be an object")
    for key in FIELDS:
        if record.get(key) != release[key]:
            raise ValueError(f"native artifact {key} differs from the checked release")
    body = library.read_bytes()
    if record.get("file") != library.name or record.get("size") != len(body) or record.get("sha256") != hashlib.sha256(body).hexdigest():
        raise ValueError("native artifact bytes differ from its manifest")
    return body


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit("usage: native_artifact.py <library> <release.json>")
    stamp(Path(sys.argv[1]), json.loads(Path(sys.argv[2]).read_text(encoding="utf-8")))
