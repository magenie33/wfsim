#!/usr/bin/env bash
# WHAT NOBODY HAS ASKED FOR YET, ASKED FOR — and the queue read back after.
#
#   scripts/reconcile_queue.sh <library.json> <facts.ndjson> <queue.ndjson>
#
# `builds x rulers x modes` is the DEFINITION of what should exist. The queue is
# written by hand — intake for an arrival, a person for a rescore — and a
# hand-written list's one failure is a row nobody wrote, which would never be
# computed and never be noticed. This closes the gap to the definition, in
# seconds, and it is the only thing standing between that failure and silence.
#
# IT IS NOT A SECOND SOURCE. It only ever ADDS, and only rows that have no
# score, so the worst it can do is ask for something already owed — which the
# queue's own primary key absorbs.
#
# ONE COPY, TWO TRIGGERS. `queue.yml` runs it on the clock and `scores.yml` runs
# it before it scores, so a button press is self-sufficient and an hourly run
# keeps the queue current. The same operation from two triggers is not two
# sources: running it twice is running it once.
#
# THE QUEUE IS READ, RECONCILED, THEN READ AGAIN. The second read is what hands
# a scoring run a queue that already holds this run's arrivals; without it a
# build waits a whole run just to be asked for.
set -euo pipefail

LIB="${1:?usage: reconcile_queue.sh <library.json> <facts.ndjson> <queue.ndjson>}"
FACTS="${2:?usage: reconcile_queue.sh <library.json> <facts.ndjson> <queue.ndjson>}"
OUT="${3:?usage: reconcile_queue.sh <library.json> <facts.ndjson> <queue.ndjson>}"
BOARD="${WFSIM_BOARD:-./target/release/wfsim-board}"

bash scripts/fetch_queue.sh "$OUT"

missing=$(mktemp)
: > "$missing"
for f in data/benchmarks/*.yaml; do
  id=$(basename "$f" .yaml)
  one=$(mktemp)
  "$BOARD" "$id" \
    --dry-run \
    --facts-in "$FACTS" \
    --queue-in "$OUT" \
    --queue-missing "$one" \
    < "$LIB" > /dev/null
  cat "$one" >> "$missing"
  rm -f "$one"
done

# ONE BATCH A DAY, named by the day. An hourly clock would otherwise mint
# twenty-four groups a day and a list nobody can read is a list nobody uses;
# `ship_queue.sh` ADDS to a batch that already exists, so the day's arrivals are
# one group however many times this runs.
bash scripts/ship_queue.sh "arrivals-$(date -u +'%Y-%m-%d')" \
  "builds nothing had asked for yet" "$missing"
rm -f "$missing"

# …AND THE OTHER HALF OF THE SAME DEFINITION. The loop above adds every
# (build, ruler, mode) that should exist and does not; this forgets the ones
# that exist and should not, which is a row under a ruler `data/benchmarks/`
# no longer has. Nothing else can: a queue row is deleted beside the fact that
# settles it, and a retired ruler never produces one.
bash scripts/purge_queue.sh

bash scripts/fetch_queue.sh "$OUT"
