#!/usr/bin/env python3
"""Emit one immutable catalog manifest into the batch driver's reusable scratch directory."""
import contextlib
import json
import sqlite3
import sys
from pathlib import Path

from stage_rank_catalog import manifest


def emit(capture, directory, weapon, ruler):
    with contextlib.closing(sqlite3.connect(capture.resolve().as_uri() + "?mode=ro", uri=True)) as store:
        store.execute("BEGIN")
        return manifest(store, directory, weapon, ruler)


if __name__ == "__main__":
    capture, directory, weapon, ruler = sys.argv[1:]
    print(json.dumps(emit(Path(capture), Path(directory), weapon, ruler)))
