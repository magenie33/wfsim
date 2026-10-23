#!/usr/bin/env bash
# A STATUS DURATION MALUS'S INSIDE ROLLS, FOR THE BUILDS STORED WITHOUT THEM.
#
#   scripts/backfill_duration_corners.sh            # PG_URL, target/release/wfsim-intake
#   DRY_RUN=1 scripts/backfill_duration_corners.sh  # counts and a sample, writes nothing
#   scripts/backfill_duration_corners.sh --self-test
#
# Intake turns a riven SHAPE into its corners when the shape ARRIVES, and the
# inbox is emptied as it goes, so a corner rule that grows reaches new arrivals
# alone. A Status Duration malus is asked at five rolls (`DURATION_ROLLS`); a
# shape stored when it was asked at two ends holds 0.9 and 1.1 and nothing
# inside, where its best usually is.
#
# SO THE LIBRARY GOES BACK THROUGH INTAKE: every record whose malus is status
# duration, as an inbox row. `library_build` reads the shape and ignores the
# stored rolls and ranks, so each comes out as its whole set of corners — and
# what is written is only the corners at an INSIDE roll that the library does
# not hold. The two ends are never rewritten: a record canonicalised a little
# differently since it was stored would come back under a new id, and writing
# it would put one build in the library twice.
#
# NO FIGHT IS ASKED HERE. A corner is a build with no fact; the board asks for
# it once its default has cleared the entry line (`default_corner`,
# docs/BOARD.md §Rivens), as it does for every corner intake stores.
set -euo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
INTAKE="${INTAKE:-$HERE/../target/release/wfsim-intake}"

# ---- the two filters ------------------------------------------------------
#
# THE LIBRARY ROWS TO SEND BACK, as inbox rows. `$2` is the instant they are
# taken at, which becomes each new build's `at`.
again_rows() {
  jq -c --arg at "$2" '
    select(.v.riven_neg == "status_duration")
    | {id: ("library:" + .k), at: $at, record: .v}' "$1"
}

# WHAT INTAKE MADE OF THEM THAT IS NEW: a status duration malus at an inside
# roll, under an id the library does not hold. The malus is the roll after the
# bonuses (`with_rolls`).
fresh_corners() {
  local held; held=$(mktemp)
  jq -c '.k' "$2" > "$held"
  jq -c --slurpfile have "$held" '
    ($have | map({key: ., value: true}) | from_entries) as $held
    | select($held[.id] | not)
    | select(.record.riven_neg == "status_duration")
    | (.record.riven_rolls // [])[(.record.riven_pos // []) | length] as $r
    | select($r != null and $r > 0.9 + 1e-9 and $r < 1.1 - 1e-9)
    | del(.from)' "$1"
  rm -f "$held"
}

run() {
  if [ -z "${PG_URL:-}" ]; then
    echo "::error::PG_URL is not set — the library's Postgres, docs/BOARD.md §\"Postgres\""
    return 1
  fi
  if [ ! -x "$INTAKE" ]; then
    echo "::error::no wfsim-intake at $INTAKE — cargo build --release --bin wfsim-intake"
    return 1
  fi
  local work; work=$(mktemp -d)
  trap 'rm -rf "$work"' EXIT
  bash "$HERE/fetch_library.sh" "$work/library.json"
  again_rows "$work/library.json.ndjson" "$(date -u +%Y-%m-%dT%H:%M:%S.000Z)" > "$work/again.ndjson"
  "$INTAKE" < "$work/again.ndjson" > "$work/corners.ndjson"
  fresh_corners "$work/corners.ndjson" "$work/library.json.ndjson" > "$work/fresh.ndjson"
  local n_again n_fresh
  n_again=$(grep -c . < "$work/again.ndjson") || n_again=0
  n_fresh=$(grep -c . < "$work/fresh.ndjson") || n_fresh=0
  echo "backfill: $n_again stored build(s) with a status duration malus, $n_fresh new corner(s)"
  if [ "${DRY_RUN:-}" = "1" ]; then
    jq -c '{id, weapon: .record.weapon, rolls: .record.riven_rolls}' < "$work/fresh.ndjson" | head -20
    echo "backfill: DRY_RUN, nothing written"
    return 0
  fi
  # NO QUEUE ROWS TO SPEND: these came from the library, not the inbox.
  : > "$work/none.txt"
  bash "$HERE/ship_builds.sh" "$work/fresh.ndjson" "$work/none.txt"
}

# ---- self-test ------------------------------------------------------------
self_test() {
  ok=0; bad=0
  DIR=$(mktemp -d)
  trap 'rm -rf "$DIR"' EXIT
  say() { if [ "$1" = ok ]; then ok=$((ok + 1)); else bad=$((bad + 1)); fi; echo "  $1    $2"; }
  # The ids on one line. `\r` too: jq on Windows ends its lines with it.
  ids() { jq -r '.id' "$1" | tr -d '\r' | tr '\n' ' '; }
  cd "$DIR"

  # Two stored corners of one duration shape, and a shape with another malus.
  {
    jq -n -c '{k: "lo", v: {weapon: "furis", riven_pos: ["fire_rate", "toxin"], riven_neg: "status_duration", riven_rolls: [1.1, 1.1, 0.9]}}'
    jq -n -c '{k: "hi", v: {weapon: "furis", riven_pos: ["fire_rate", "toxin"], riven_neg: "status_duration", riven_rolls: [1.1, 1.1, 1.1]}}'
    jq -n -c '{k: "zm", v: {weapon: "furis", riven_pos: ["fire_rate", "toxin"], riven_neg: "zoom", riven_rolls: [1.1, 1.1, 0.9]}}'
    jq -n -c '{k: "plain", v: {weapon: "furis", mods: ["serration"]}}'
  } > library.ndjson

  again_rows library.ndjson "2026-10-23T00:00:00.000Z" > again.ndjson
  [ "$(ids again.ndjson)" = "library:lo library:hi " ] \
    && say ok "only the status duration malus goes back through intake" \
    || say FAIL "sent back: $(ids again.ndjson)"
  jq -e -s 'all(.at == "2026-10-23T00:00:00.000Z" and (.record | type) == "object")' again.ndjson >/dev/null \
    && say ok "...as inbox rows, the record whole and the instant given" \
    || say FAIL "$(head -1 again.ndjson)"

  # What intake would make of them: every corner, some already held.
  {
    for r in 0.9 0.95 1.0 1.05 1.1; do
      id="c$r"; [ "$r" = 0.9 ] && id=lo; [ "$r" = 1.1 ] && id=hi
      jq -n -c --arg id "$id" --argjson r "$r" \
        '{id: $id, at: "t", from: ["library:lo"], record: {weapon: "furis", riven_pos: ["fire_rate", "toxin"], riven_neg: "status_duration", riven_rolls: [1.1, 1.1, $r]}}'
    done
    # An END under a new id — a record canonicalised differently since — is not written.
    jq -n -c '{id: "drift", at: "t", record: {weapon: "furis", riven_pos: ["fire_rate", "toxin"], riven_neg: "status_duration", riven_rolls: [1.1, 1.1, 1.1]}}'
    # A status duration BONUS at an inside roll is not this backfill's.
    jq -n -c '{id: "bonus", at: "t", record: {weapon: "furis", riven_pos: ["status_duration", "toxin"], riven_neg: "zoom", riven_rolls: [1.0, 1.1, 0.9]}}'
  } > corners.ndjson

  fresh_corners corners.ndjson library.ndjson > fresh.ndjson
  [ "$(ids fresh.ndjson)" = "c0.95 c1.0 c1.05 " ] \
    && say ok "the three inside rolls are written, and nothing the library holds" \
    || say FAIL "written: $(ids fresh.ndjson)"
  jq -e -s 'all(has("from") | not)' fresh.ndjson >/dev/null \
    && say ok "...without a queue row to spend" \
    || say FAIL "$(head -1 fresh.ndjson)"

  echo "backfill_duration_corners: $ok ok, $bad failed"
  [ "$bad" = 0 ]
}

if [ "${1:-}" = "--self-test" ]; then
  self_test
else
  run
fi
