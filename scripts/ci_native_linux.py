# SPDX-License-Identifier: AGPL-3.0-or-later
"""Build the Linux slice before publishing, without signing or network writes."""
import json
from pathlib import Path

import native_artifact
from release_engine_lib import linux_library


def main() -> None:
    body = linux_library(prebuilt=False)
    if body is None:
        raise SystemExit("Linux native build failed")
    library = Path("out/linux/libwfsim_native.so")
    library.parent.mkdir(parents=True, exist_ok=True)
    library.write_bytes(body)
    native_artifact.stamp(library, json.loads(Path("site/release.json").read_text(encoding="utf-8")))


if __name__ == "__main__":
    main()
