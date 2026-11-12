#!/usr/bin/env bash
# Repair already queued work; an explicit inventory request remains read-only.
#
#   scripts/reconcile_queue.sh <library.json> <facts.ndjson> <queue.ndjson>
#
# Missing library coverage is not a task source. Only existing queue entries
# are recovered here; new boards and missing scores require an explicit plan.
set -euo pipefail

LIB="${1:?usage: reconcile_queue.sh <library.json> <facts.ndjson> <queue.ndjson>}"
FACTS="${2:?usage: reconcile_queue.sh <library.json> <facts.ndjson> <queue.ndjson>}"
OUT="${3:?usage: reconcile_queue.sh <library.json> <facts.ndjson> <queue.ndjson>}"
BOARD="${WFSIM_BOARD:-./target/release/wfsim-board}"
INVENTORY_ONLY="${INVENTORY_ONLY:-0}"
case "$INVENTORY_ONLY" in 0|1) ;; *) echo "inventory: INVENTORY_ONLY must be 0 or 1" >&2; exit 2 ;; esac
# EVERY OWED ROW, whoever is measuring it: a hold here would claim and hide rows.
unset CLAIMED

if [ "$INVENTORY_ONLY" = 0 ]; then
  bash scripts/ship_queue.sh --backfill
  bash scripts/fetch_queue.sh "$OUT"
  exit 0
fi

inventory=$(mktemp)
python3 scripts/inventory_phase.py capture -- node scripts/capture_compute_inventory.mjs "$LIB" "$FACTS" "$OUT" "$inventory"

missing=$(mktemp)
: > "$missing"
orphans=$(mktemp)
: > "$orphans"
parts=$(mktemp -d)
# `--gate` IS WHAT MAKES THE ARRIVALS PATH OBEY THE ENTRY LINE. A build that
# reaches the engine's entry line keeps earning every row it lacks; one
# that reaches it nowhere keeps the facts it has and is asked for nothing more.
# The RESCORE button in `queue.yml` deliberately does NOT pass it — a model
# correction is the case where a parked build may deserve another look.
# EVERY RULER IN ONE PROCESS (`--each-ruler`), the library and the facts read
# once, each ruler into its own files, joined in roster order.
python3 scripts/inventory_phase.py enumerate -- "$BOARD" --each-ruler --dry-run --inventory-workers "${WFSIM_INVENTORY_WORKERS:-2}" --gate --facts-in "$FACTS" --queue-in "$OUT" \
  --inventory-builds "$inventory.canonical" \
  --queue-missing "$parts/{id}.missing" --queue-orphans "$parts/{id}.orphans" < "$LIB" > /dev/null
roster=$(mktemp)
"$BOARD" --rulers | tr -d '\r' > "$roster"
test -s "$roster"
while IFS= read -r id; do
  cat "$parts/$id.missing" >> "$missing"
  cat "$parts/$id.orphans" >> "$orphans"
  rm -f -- "$parts/$id.missing" "$parts/$id.orphans"
done < "$roster"
rm -rf "$parts"

# Count the complete engine-admitted workload before any dispatch pacing.
python3 scripts/inventory_phase.py bind -- node scripts/bind_inventory_builds.mjs "$inventory.builds" "$inventory.canonical" "$missing" "$OUT" "$orphans" "$roster"
python3 scripts/inventory_phase.py classify -- node scripts/compute_inventory.mjs "$OUT" "$missing" "$orphans" "$FACTS" "$roster" "$inventory"
rm -f "$inventory" "$inventory.builds" "$inventory.canonical" "$inventory.tasks" "$inventory.json"
rm -f "$roster"

rm -f "$missing" "$orphans"
