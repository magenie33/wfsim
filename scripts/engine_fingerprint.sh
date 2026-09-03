#!/usr/bin/env bash
# WHAT A SCORE DEPENDS ON THAT IS NOT THE BUILD AND NOT THE DATA: the code.
#
# `engine`, `webapi` and `cli` — the three crates that decide a number — hashed
# from the index, so the answer is the same on any checkout of the same commit
# and does not depend on line endings or on what a working tree happens to hold.
#
# NOT `data/`. Hashing it here makes adding a weapon — a file no existing row
# reads — invalidate every stored score. The data half is asked PER ROW from
# the files that row actually reads (`engine::data_fingerprint`).
#
# IT LIVES IN ONE FILE because two callers ask it and they have to get the same
# answer: the board computes it to decide what to reuse, and the audit computes
# it to decide whether the published board is even the same generation. Two
# copies of this pipeline would agree until one of them was edited.
set -euo pipefail
git ls-files -s -- engine webapi cli | git hash-object --stdin
