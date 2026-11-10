# SPDX-License-Identifier: AGPL-3.0-or-later
"""Publish prepared CLI bytes to the existing mirror; never rebuild them."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

import cos


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    root = args.directory
    raw = (root / "release.json").read_bytes()
    manifest = json.loads(raw)
    subprocess.run(["node", str(Path(__file__).with_name("sign_client_release.mjs")), "--verify", str(root)], check=True)
    version = manifest["version"]
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", version):
        raise ValueError("invalid CLI release version")
    items = []
    expected = {"x86_64-linux", "x86_64-windows", "aarch64-macos", "x86_64-macos"}
    if set(manifest["files"]) != expected:
        raise ValueError("one build for every supported target is required")
    for target, item in manifest["files"].items():
        name = f"wfsim-{target}" + (".exe" if target.endswith("windows") else "")
        if item["name"] != name:
            raise ValueError("unexpected CLI artifact name")
        body = (root / name).read_bytes()
        if len(body) != item["size"] or hashlib.sha256(body).hexdigest() != item["sha256"]:
            raise ValueError(f"{name}: the prepared bytes changed")
        items.append((name, body, "application/octet-stream"))
    for name, mime in [("source.zip", "application/zip"), ("SHA256SUMS", "text/plain"), ("release.json", "application/json"), ("release.json.sig", "text/plain")]:
        items.append((name, (root / name).read_bytes(), mime))
    credentials = None if args.dry_run else cos.creds()
    for name, body, mime in items:
        key = f"cli/releases/{version}/{name}"
        if not args.dry_run:
            cos.put(credentials, key, body, mime)
        print(f"CLI {key}: {len(body)} bytes")
    if not args.dry_run:
        cos.put(credentials, "cli/latest.json", raw, "application/json; charset=utf-8")
    print(f"CLI {version}: {'prepared files checked; no upload' if args.dry_run else 'published'}")


if __name__ == "__main__":
    main()
