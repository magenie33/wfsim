# SPDX-License-Identifier: AGPL-3.0-or-later
import json
from pathlib import Path
import tempfile
from native_artifact import FIELDS, load, stamp

with tempfile.TemporaryDirectory(prefix="wfsim-native-artifact-") as tmp:
    library = Path(tmp) / "libwfsim_native.so"
    release = {"engine": "engine1", "wasm": "module1", "data": "data1", "commit": "source1+"}
    assert load(library, release) is None
    library.write_bytes(b"library fixture")
    try:
        load(library, release)
    except FileNotFoundError:
        pass
    else:
        raise AssertionError("an unstamped library passed validation")
    stamp(library, release)
    assert load(library, release) == b"library fixture"
    for key in FIELDS:
        try:
            load(library, {**release, key: "other"})
        except ValueError as error:
            assert key in str(error)
        else:
            raise AssertionError(f"a changed {key} reused the library")
    library.write_bytes(b"corrupt fixture")
    try:
        load(library, release)
    except ValueError:
        pass
    else:
        raise AssertionError("corrupt bytes passed validation")
    stamp(library, release)
    manifest = library.with_name("manifest.json")
    record = json.loads(manifest.read_text())
    record["file"] = "../other.so"
    manifest.write_text(json.dumps(record))
    try:
        load(library, release)
    except ValueError:
        pass
    else:
        raise AssertionError("another file's manifest passed validation")
print("Native artifacts bind engine, module, data, source and exact bytes; mismatches fail.")
