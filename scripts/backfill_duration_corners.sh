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
# THE LIBRARY ROWS TO SEND BACK, as inbox rows. Only the UTC day of `$2`
# becomes each new build's `at`, as in the anonymous library.
#
# ONE ROW A SHAPE, NOT ONE A STORED CORNER: `library_build` ignores the rolls
# and puts every card at max rank, so records differing only there come out as
# the same set of corners, and the library holds each shape once per corner.
# Sent whole, a pass derives each shape once for every corner it holds.
again_rows() {
  jq -c -n --arg at "$2" '
    def bare: if type == "string" then sub("@[0-9]+$"; "") else . end;
    foreach (inputs | select(.v.riven_neg == "status_duration")) as $r ({seen: {}, out: null};
      ($r.v | del(.riven_rolls) | .mods |= ((. // []) | map(bare)) | .exilus |= bare
        | tostring) as $shape
      | if .seen[$shape] then .out = null else .seen[$shape] = true | .out = $r end;
      .out // empty)
    | {id: ("library:" + .k), at: $at[0:10], record: .v}' "$1"
}

# INTAKE ON EVERY CORE: each record is derived alone, so shards of the rows are
# independent passes and their outputs concatenate.
intake_sharded() {
  local in="$1" out="$2" n; n="${BACKFILL_JOBS:-$(nproc 2>/dev/null || echo 2)}"
  local dir; dir=$(mktemp -d)
  split -n "l/$n" -d "$in" "$dir/in."
  local pids=() f
  for f in "$dir"/in.*; do
    "$INTAKE" < "$f" > "$f.out" &
    pids+=("$!")
  done
  for f in "${pids[@]}"; do wait "$f"; done
  cat "$dir"/in.*.out > "$out"
  rm -rf "$dir"
}

# WHAT INTAKE MADE OF THEM THAT IS NEW: a status duration malus at an inside
# roll, under an id the library does not hold. The malus is the roll after the
# bonuses (`with_rolls`).
fresh_corners() {
  local held; held=$(mktemp)
  jq -c '.k' "$2" > "$held"
  jq -cn --slurpfile have "$held" '
    ($have | map({key: ., value: true}) | from_entries) as $held
    | inputs
    | select($held[.id] | not)
    | select(.record.riven_neg == "status_duration")
    | (.record.riven_rolls // [])[(.record.riven_pos // []) | length] as $r
    | select($r != null and $r > 0.9 + 1e-9 and $r < 1.1 - 1e-9)
    | del(.from)' "$1" \
  | jq -c -n 'foreach inputs as $b ({seen: {}, out: null};
      if .seen[$b.id] then .out = null else .seen[$b.id] = true | .out = $b end;
      .out // empty)'
  rm -f "$held"
}

sample_corners() {
  jq -cn 'limit(20; inputs | {id, weapon: .record.weapon, rolls: .record.riven_rolls})' < "$1"
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
  local cleanup; printf -v cleanup 'rm -rf -- %q' "$work"
  trap "$cleanup" EXIT
  bash "$HERE/fetch_library.sh" "$work/library.json"
  again_rows "$work/library.json.ndjson" "$(date -u +%Y-%m-%dT%H:%M:%S.000Z)" > "$work/again.ndjson"
  echo "backfill: $(grep -c . < "$work/again.ndjson" || true) shape(s) go back through intake"
  intake_sharded "$work/again.ndjson" "$work/corners.ndjson"
  fresh_corners "$work/corners.ndjson" "$work/library.json.ndjson" > "$work/fresh.ndjson"
  local n_again n_fresh
  n_again=$(grep -c . < "$work/again.ndjson") || n_again=0
  n_fresh=$(grep -c . < "$work/fresh.ndjson") || n_fresh=0
  echo "backfill: $n_again stored shape(s) with a status duration malus, $n_fresh new corner(s)"
  if [ "${DRY_RUN:-}" = "1" ]; then
    sample_corners "$work/fresh.ndjson"
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
    # One shape with a card at a lower rank, which intake reads at max rank.
    jq -n -c '{k: "r3", v: {weapon: "furis", mods: ["serration@3"], riven_pos: ["fire_rate"], riven_neg: "status_duration", riven_rolls: [1.1, 0.9]}}'
    jq -n -c '{k: "r5", v: {weapon: "furis", mods: ["serration"], riven_pos: ["fire_rate"], riven_neg: "status_duration", riven_rolls: [1.1, 0.9]}}'
  } > library.ndjson

  again_rows library.ndjson "2026-10-23T00:00:00.000Z" > again.ndjson
  [ "$(ids again.ndjson)" = "library:lo library:r3 " ] \
    && say ok "each status duration malus shape goes back through intake once" \
    || say FAIL "sent back: $(ids again.ndjson)"
  jq -e -s 'all(.at == "2026-10-23" and (.record | type) == "object")' again.ndjson >/dev/null \
    && say ok "...as inbox rows, with the record whole and only its UTC day" \
    || say FAIL "$(head -1 again.ndjson)"

  # What intake would make of them: every corner, some already held.
  {
    for r in 0.9 0.95 1.0 1.05 1.1; do
      id="c$r"; [ "$r" = 0.9 ] && id=lo; [ "$r" = 1.1 ] && id=hi
      jq -n -c --arg id "$id" --argjson r "$r" \
        '{id: $id, at: "t", from: ["library:lo"], record: {weapon: "furis", riven_pos: ["fire_rate", "toxin"], riven_neg: "status_duration", riven_rolls: [1.1, 1.1, $r]}}'
    done
    # The same corner out of two shards, written once.
    jq -n -c '{id: "c0.95", at: "t", record: {weapon: "furis", riven_pos: ["fire_rate", "toxin"], riven_neg: "status_duration", riven_rolls: [1.1, 1.1, 0.95]}}'
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

  jq -cn 'range(0;2000) | {id: ("corner-" + tostring), record: {weapon: "furis", riven_rolls: [1.1, 1.1, 1.0]}}' > many.ndjson
  if sample_corners many.ndjson > sample.ndjson && [ "$(wc -l < sample.ndjson)" -eq 20 ]; then
    say ok "a large dry run prints twenty sample corners and succeeds"
  else
    say FAIL "the dry-run sample failed or printed the wrong number of corners"
  fi

  mkdir -p "$DIR/stub" "$DIR/bin"
  printf '%s\n' '#!/usr/bin/env bash' 'cp "$BACKFILL_TEST_LIBRARY" "$1.ndjson"' > "$DIR/stub/fetch_library.sh"
  printf '%s\n' '#!/usr/bin/env bash' 'exit 0' > "$DIR/bin/intake"
  chmod +x "$DIR/bin/intake"
  if (
    HERE="$DIR/stub" INTAKE="$DIR/bin/intake" PG_URL=stub DRY_RUN=1
    export BACKFILL_TEST_LIBRARY="$DIR/library.ndjson"
    intake_sharded() { cp "$DIR/corners.ndjson" "$2"; }
    run
  ) > "$DIR/dry.log" 2>&1; then
    say ok "a complete dry run returns successfully after its cleanup"
  else
    say FAIL "a complete dry run failed: $(tail -3 "$DIR/dry.log")"
  fi

  echo "backfill_duration_corners: $ok ok, $bad failed"
  [ "$bad" = 0 ]
}

if [ "${1:-}" = "--self-test" ]; then
  self_test
else
  run
fi
