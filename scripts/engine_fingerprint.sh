#!/usr/bin/env bash
# WHAT A SCORE DEPENDS ON THAT IS NOT THE BUILD AND NOT THE DATA: the code.
#
# `engine`, `webapi` and the scorer — what decides a number — hashed from the
# index, so the answer is the same on any checkout of the same commit and does
# not depend on line endings or on what a working tree happens to hold.
#
# NOT `data/`. Hashing it here makes adding a weapon — a file no existing row
# reads — invalidate every stored score. The data half is asked PER ROW from
# the files that row actually reads (`engine::data_fingerprint`).
#
# NOT ALL OF `cli` EITHER — only the scorer. `cli/src/main.rs` is a demo that
# shoots a training dummy and cannot reach the board, and hashing it invalidated
# every stored score for an edit the board never sees. The paths here have to be
# paths that WAKE the board, or the rescore they buy lands on whichever run
# happens next; `check_rescore_paths` asserts exactly that against the trigger
# in `.github/workflows/board.yml`.
#
# IT LIVES IN ONE FILE because two callers ask it and they have to get the same
# answer: the board computes it to decide what to reuse, and the audit computes
# it to decide whether the published board is even the same generation. Two
# copies of this pipeline would agree until one of them was edited.
set -euo pipefail
git ls-files -s -- engine webapi cli/src/bin/wfsim-board.rs | git hash-object --stdin
