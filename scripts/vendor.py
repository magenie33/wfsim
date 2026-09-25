#!/usr/bin/env python3
"""Fetch/refresh the vendored reference data into vendor/ (gitignored).

Currently: DE's Public Export, English and Chinese (`scripts/de_export.py`).
Files are content-addressed by DE's own hash, so a rerun downloads only what
changed.
"""

import sys

import de_export


def main() -> int:
    for lang in ("en", "zh"):
        de_export.fetch(lang)
    return 0


if __name__ == "__main__":
    sys.exit(main())
