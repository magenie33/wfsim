# SPDX-License-Identifier: AGPL-3.0-or-later
"""Exercise the actual preflight against isolated missing and corrupt inputs."""
import hashlib
import json
from pathlib import Path
import tempfile

import build_site_app as build


def refuses(action, message):
    try:
        action()
    except SystemExit as error:
        assert message in str(error), error
    else:
        raise AssertionError("a broken build input passed preflight")


with tempfile.TemporaryDirectory(prefix="wfsim-build-inputs-") as tmp:
    build.ROOT = Path(tmp)
    cache = build.ROOT / "web/cache"
    (cache / "img").mkdir(parents=True)
    (build.ROOT / "data").mkdir()
    (build.ROOT / "data/assets.yaml").write_text("weapons:\n  braton: Braton.png\n", encoding="utf-8")
    refuses(build.checked_art, "not cached")
    image = cache / "img/Braton.png"
    image.write_bytes(b"<html>error page</html>")
    refuses(build.checked_art, "not an image")
    image.write_bytes(b"\x89PNG\r\n\x1a\n" + b"fixture")
    assert build.checked_art() == {"Braton.png"}
    part = build.ROOT / "web/src/static/app/fixture.js"
    part.parent.mkdir(parents=True)
    part.write_text("model.bin", encoding="utf-8")
    (build.ROOT / "web/lib").mkdir()
    (cache / "lib").mkdir()
    body = b"pinned fixture"
    pins = [{"name": "model.bin", "sha256": hashlib.sha256(body).hexdigest()}]
    (build.ROOT / "web/lib/pins.json").write_text(json.dumps({"files": pins}), encoding="utf-8")
    check = lambda: build.checked_pins("lib", "fixture.js")
    refuses(check, "not the pinned bytes")
    (cache / "lib/model.bin").write_bytes(b"bad bytes")
    refuses(check, "not the pinned bytes")
    (cache / "lib/model.bin").write_bytes(body)
    assert check() == pins
    part.write_text("", encoding="utf-8")
    refuses(check, "does not ask")
    assert not (build.ROOT / "site").exists(), "preflight must write no generated site"
    for name in build.ENGINE_BUILD_INPUTS:
        item = build.ROOT / name
        item.parent.mkdir(parents=True, exist_ok=True)
        item.write_bytes(b"build input fixture")
    engine = build.engine_id()
    for name in build.ENGINE_BUILD_INPUTS:
        item = build.ROOT / name
        item.write_bytes(b"changed build input")
        assert build.engine_id() != engine, f"{name} must move native engine identity"
        item.write_bytes(b"build input fixture")
print("Build preflight rejects missing/corrupt art and pins before compilation, without writing site/.")
print("Native build flags/tooling and training inputs participate in engine identity.")
