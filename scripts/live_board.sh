#!/usr/bin/env bash
# THE LIVE BOARD, AND THE ORDERS BEHIND IT — on the board server
# (`deploy/wfsim-live.service`); `publish.yml` keeps a daily snapshot in git.
#
#   scripts/live_board.sh <work-dir> <bin-dir>
#
# TWO LOOPS. The INTAKE loop takes a submission in the moment it lands — its
# builds into the library, every row it owes into the queue and opened as a
# compute order (`ship_queue.sh`) — on its own clock, so a submission never
# waits for the board. EVERY CYCLE of the main loop the rows `scores` gained
# are read and handed to the ranker, and the board moves: `verified/`,
# `scores` alone, pushed to R2 for the site. A client result is on no board until it is a fact. This
# server fights nothing: what only the official machines may settle is `settle.yml`'s.
# docs/BOARD.md §"The fact factory", §"The live board".
#
# Needs the library's database — PG_URL (Postgres, scripts/db.sh) —
# BOARD_PUSH_TOKEN, and FACTORY_TOKEN (the fact factory's secret:
# ship_queue.sh, live_orders.mjs).
set -euo pipefail

WORK="${1:?usage: live_board.sh <work-dir> <bin-dir>}"
BIN="${2:?usage: live_board.sh <work-dir> <bin-dir>}"
HERE="$(cd "$(dirname "$0")" && pwd)"
POLL_SECONDS="${POLL_SECONDS:-20}"
REFRESH_SECONDS="${REFRESH_SECONDS:-3600}"
INCREMENTAL_INVENTORY="${INCREMENTAL_INVENTORY:-0}"
case "$INCREMENTAL_INVENTORY" in 0|1) ;; *) echo "live: INCREMENTAL_INVENTORY must be 0 or 1" >&2; exit 1 ;; esac
mkdir -p "$WORK"
cd "$WORK"
WORK="$(pwd)"
if [ "$INCREMENTAL_INVENTORY" = 1 ]; then
  exec 8> "$WORK/inventory.lock"
  flock -n 8 || { echo "live: another inventory coordinator owns this work directory" >&2; exit 1; }
  mkdir -p "$WORK/tmp"
  export TMPDIR="$WORK/tmp" SQLITE_TMPDIR="$WORK/tmp"
fi
touch new-builds.ndjson

. "$HERE/db.sh"

# WHAT ARRIVED, TAKEN IN NOW: the same intake `queue.yml` runs hourly, which
# stays as the fallback — the builds land and the inbox rows are spent
# (`ship_builds.sh`), then every row the builds owe is queued and opened as an
# order. Twice is harmless: a build is keyed by its hash and a queue row by its
# own key.
intake_arrivals() {
  # WHO ASKED rides with the record to the rows it owes (docs/BOARD.md §"Who asked").
  db_query '{"sql":"SELECT id, at, record, initiator, family_source FROM (SELECT id, at, record, initiator, family_source FROM inbox UNION ALL SELECT id, at, record, NULL, NULL FROM equipment_inbox) AS arrivals ORDER BY id LIMIT 500"}' \
    || { echo "live: inbox read refused [HTTP $DB_CODE]" >&2; return 1; }
  jq -c '.result[0].results[]' < "$DB_OUT" > inbox-new.ndjson
  [ -s inbox-new.ndjson ] || return 1
  node --max-old-space-size=96 "$HERE/live_intake.mjs" "$WORK" "$BIN" "$INCREMENTAL_INVENTORY" inbox-new.ndjson \
    >> intake.log 2>&1
}

# THE INTAKE LOOP: every `POLL_SECONDS`, whatever the main loop is doing. Its
# own `DB_OUT`: a variable the main loop set would name the same file.
intake_loop() {
  DB_OUT=""
  while true; do
    intake_arrivals || true
    sleep "$POLL_SECONDS"
  done
}

# WHAT THE INTAKE LOOP TOOK IN SINCE THE LAST CYCLE, out of the mailbox into
# `arrived-now.ndjson` for the ranker, and kept in `new-builds.ndjson` until a
# read of the store holds it.
take_arrivals() {
  flock "$WORK/mailbox.lock" node --max-old-space-size=64 "$HERE/live_mailbox.mjs" take "$WORK"
}

# THE STORE READ AFRESH, BESIDE THE BOARD: the read takes most of an hour and
# the board keeps moving meanwhile. Into spares, `fresh.ready` marking both
# whole: a failed read truncates its output, and an empty facts file would
# rank a board with every scored row gone.
FETCH_PID=""
start_fetch() {
  rm -f fresh.ready
  wc -l < new-builds.ndjson > fetch-mark.txt
  ( bash "$HERE/fetch_library.sh" library.fresh >&2 && bash "$HERE/fetch_facts.sh" facts.fresh >&2 \
      && [ -s library.fresh ] && [ -s facts.fresh ] && touch fresh.ready ) &
  FETCH_PID=$!
}

# …AND SWAPPED IN once whole. A build taken in since the read began may be
# missing from it, so it is kept: one in both is one build twice, which the
# ranker lists once. The facts' cursor moves to the read's newest row, and the
# next delta asks again for the two hours before it.
swap_store() {
  local mark
  mark=$(cat fetch-mark.txt 2>/dev/null || echo 0)
  mv library.fresh library.json
  rm -f library.fresh.ndjson
  mv facts.fresh facts-known.ndjson
  tail -n +"$((mark + 1))" new-builds.ndjson > new-builds.next
  mv new-builds.next new-builds.ndjson
  node "$HERE/live_publish.mjs" cursor "$WORK" >&2
  rm -f fresh.ready
}

# THE RANKER (`wfsim-board --serve`): every ruler's board held in memory and
# moved a fact at a time, so a cycle costs the weapons it moved and never the
# library. It starts from the files on disk, which hold everything it was ever
# sent, so a ranker that stops — a reply other than `ok`, a crash, a deploy —
# is started again from them on the next cycle and loses nothing.
start_ranker() {
  local ranker_library="$WORK/library.json" ranker_builds="$WORK/new-builds.ndjson" ranker_facts="$WORK/facts-known.ndjson"
  if [ "$INCREMENTAL_INVENTORY" = 1 ]; then
    mkdir -p "$WORK/inventory"
    ranker_library="$WORK/inventory/empty-library.json"
    ranker_builds="$WORK/inventory/empty-builds.ndjson"
    ranker_facts="$WORK/inventory/empty-facts.ndjson"
    printf '[]' > "$ranker_library"
    : > "$ranker_builds"
    : > "$ranker_facts"
  fi
  mkdir -p pub/data
  [ -f pub/data/board_state.yaml ] || printf 'boards: {}\n' > pub/data/board_state.yaml
  # THE LOG IS WHAT THIS RANKER REFUSED, said once a record: it starts afresh.
  : > project.log
  coproc RANKER { exec "$BIN/wfsim-board" --serve "$WORK/verified" --library "$ranker_library" \
      --builds "$ranker_builds" --facts-in "$ranker_facts" \
      --state "$WORK/pub/data/board_state.yaml" 2>> "$WORK/project.log"; }
  RANKER_INSTANCE="$(date +%s)-$RANKER_PID"
  RANKER_PUBLISHED_EPOCH=""
  RANKER_PUBLISH_PENDING=0
  ask "" || { stop_ranker; return 1; }
}

# ONE COMMAND, ONE LINE BACK. An empty command only reads (the `ready` line).
ask() {
  local reply
  if [ -n "$1" ]; then
    printf '%s\n' "$1" >&"${RANKER[1]}" || return 1
  fi
  IFS= read -r reply <&"${RANKER[0]}" || return 1
  RANKER_REPLY="$reply"
  case "$reply" in
    ok*) echo "live: ranker $reply" >&2 ;;
    *) echo "live: the ranker answered: $reply" >&2; return 1 ;;
  esac
}

publish_incremental() {
  local epoch="$1"
  if [ -z "$epoch" ] || { [ "$epoch" = "$RANKER_PUBLISHED_EPOCH" ] && [ "$RANKER_PUBLISH_PENDING" = 0 ]; }; then
    return 0
  fi
  ask "publish_step 8" || return 1
  if [[ "$RANKER_REPLY" =~ ^ok\ publish_step\ ([0-9]+)\ ([0-9]+)\ ([01])\ ([01])$ ]]; then
    local round="${BASH_REMATCH[3]}" dirty="${BASH_REMATCH[4]}"
    RANKER_PUBLISH_PENDING=$((round || dirty))
    if [ "$round" = 0 ]; then
      changed=1
      RANKER_PUBLISHED_EPOCH="$epoch"
    fi
  else
    echo "live: invalid publication reply: $RANKER_REPLY" >&2
    return 1
  fi
}

ranker_alive() {
  [ -n "${RANKER_PID:-}" ] && kill -0 "$RANKER_PID" 2>/dev/null
}

stop_ranker() {
  if [ -n "${RANKER_PID:-}" ]; then
    kill "$RANKER_PID" 2>/dev/null || true
    wait "$RANKER_PID" 2>/dev/null || true
  fi
  RANKER_PID=""
}

PUSH_PID=""
start_push() {
  if [ -n "$PUSH_PID" ]; then
    if kill -0 "$PUSH_PID" 2>/dev/null; then return; fi
    wait "$PUSH_PID" || echo "live: the upload stopped; confirmed files remain checkpointed" >&2
  fi
  ( WFSIM_BOARD_PUSH_TIMEOUT_MS="${WFSIM_BOARD_PUSH_TIMEOUT_MS:-120000}" \
    exec node --max-old-space-size=64 "$HERE/live_publish.mjs" push "$WORK" verified < /dev/null ) &
  PUSH_PID=$!
}

stop_push() {
  if [ -n "$PUSH_PID" ]; then
    kill "$PUSH_PID" 2>/dev/null || true
    wait "$PUSH_PID" 2>/dev/null || true
  fi
}

# WHAT THE SITE READS BESIDE THE ROWS: each weapon's heads, and the stamp.
stamp() {
  node "$HERE/board_heads.mjs" verified >&2 \
    && python3 "$HERE/board_meta.py" verified pub/data/board_state.yaml verified/meta.json >&2
}

# ONE CYCLE AND OUT takes its arrivals in line; a service runs the intake loop
# beside the main one, and starts it again if it ever stops.
INTAKE_PID=""
if [ "$INCREMENTAL_INVENTORY" = 0 ]; then
  flock "$WORK/mailbox.lock" node --max-old-space-size=64 "$HERE/live_mailbox.mjs" recover "$WORK"
fi
start_intake() {
  intake_loop &
  INTAKE_PID=$!
}
if [ "${ONCE:-}" != 1 ]; then
  start_intake
fi
trap '[ -n "$INTAKE_PID" ] && kill "$INTAKE_PID" 2>/dev/null; [ -n "$FETCH_PID" ] && kill "$FETCH_PID" 2>/dev/null; stop_push; stop_ranker; db_cleanup' EXIT
trap 'exit 143' TERM
trap 'exit 130' INT

if [ "$INCREMENTAL_INVENTORY" = 1 ]; then
  if [ "${ONCE:-}" = 1 ]; then intake_arrivals || true; fi
  node --max-old-space-size=192 "$HERE/inventory_pipeline.mjs" "$WORK" "$BIN" --serve
  exit $?
fi

# A FIRST START HAS NO STORE TO RANK FROM, and waits for one.
if [ ! -s library.json ]; then
  if [ "$INCREMENTAL_INVENTORY" = 1 ]; then
    printf '[]' > library.json
    : > facts-known.ndjson
  else
    start_fetch
    wait "$FETCH_PID" || true
    FETCH_PID=""
    if [ -f fresh.ready ]; then
      swap_store
      date +%s > fetch-at.txt
    fi
  fi
fi

# THE HOUR IS COUNTED FROM THE END OF THE LAST READ, kept on disk so a restart
# — every deploy is one — starts no read the hour does not owe.
last_fetch=$(cat fetch-at.txt 2>/dev/null || echo 0)
stamp_owed=0
while true; do
  if [ "${ONCE:-}" = 1 ]; then
    intake_arrivals || true
  elif ! kill -0 "$INTAKE_PID" 2>/dev/null; then
    echo "live: the intake loop stopped; starting it again" >&2
    start_intake
  fi
  reloaded=0
  if [ -n "$FETCH_PID" ] && ! kill -0 "$FETCH_PID" 2>/dev/null; then
    wait "$FETCH_PID" || true
    FETCH_PID=""
    if [ -f fresh.ready ]; then
      swap_store
      reloaded=1
      last_fetch=$(date +%s)
      echo "$last_fetch" > fetch-at.txt
    else
      echo "live: the store could not be read; keeping the last copy" >&2
    fi
  fi
  if [ "$INCREMENTAL_INVENTORY" = 0 ] && [ -z "$FETCH_PID" ] && [ $(( $(date +%s) - last_fetch )) -ge "$REFRESH_SECONDS" ]; then
    start_fetch
  fi
  take_arrivals
  # WHAT WAS VERIFIED SINCE THE LAST READ, without waiting for the hourly one.
  moved=0
  if [ "$INCREMENTAL_INVENTORY" = 0 ] && [ -s facts-known.ndjson ]; then
    code=0
    node "$HERE/live_publish.mjs" delta "$WORK" || code=$?
    [ "$code" = 0 ] && moved=1
  fi
  changed=0
  if ! ranker_alive; then
    if [ -s library.json ] && start_ranker; then
      changed=1
    else
      echo "live: the ranker did not start; the last board stands" >&2
    fi
  elif [ "$INCREMENTAL_INVENTORY" = 0 ] && [ "$reloaded" = 1 ]; then
    # THE READ HOLDS EVERYTHING the cycle took in, so it is the whole message.
    if ask "reload $WORK/library.json $WORK/new-builds.ndjson $WORK/facts-known.ndjson"; then
      changed=1
    else
      stop_ranker
    fi
  elif [ "$INCREMENTAL_INVENTORY" = 0 ] && { [ -s arrived-now.ndjson ] || [ "$moved" = 1 ]; }; then
    if { [ ! -s arrived-now.ndjson ] || ask "builds $WORK/arrived-now.ndjson"; } \
      && { [ "$moved" = 0 ] || ask "facts $WORK/moved-facts.ndjson"; } \
      && ask "publish"; then
      changed=1
    else
      stop_ranker
    fi
  fi
  inventory_pending=0
  if [ "$INCREMENTAL_INVENTORY" = 1 ]; then
    changed=0
    if ranker_alive; then
      if flock -n "$WORK/inventory.lock" node --max-old-space-size=192 "$HERE/inventory_pipeline.mjs" "$WORK" "$BIN" "$RANKER_INSTANCE" \
          <&"${RANKER[0]}" >&"${RANKER[1]}"; then
        inventory_pending=$(jq -r 'if .pending then 1 else 0 end' inventory/status.json)
        inventory_engine_epoch=$(jq -r '.engine_epoch // empty' inventory/status.json)
        publish_incremental "$inventory_engine_epoch" || stop_ranker
      else
        echo "live: inventory cycle failed; restarting the ranker from authoritative inputs" >&2
        stop_ranker
      fi
    fi
  fi
  # A STAMP THAT FAILED IS OWED until one lands.
  [ "$changed" = 1 ] && stamp_owed=1
  if [ "$stamp_owed" = 1 ]; then
    if stamp; then
      stamp_owed=0
    else
      echo "live: the board was not stamped; the next cycle stamps it" >&2
    fi
  fi
  # PUSHED EVERY CYCLE, and a file only when its bytes moved: a push the
  # worker refused is sent again on the next one rather than waiting for R2
  # to be asked by the next fact.
  if { [ "$INCREMENTAL_INVENTORY" = 0 ] || [ "$changed" = 1 ]; } && [ -f verified/meta.json ]; then
    if [ "$INCREMENTAL_INVENTORY" = 1 ]; then
      start_push
    else
      node "$HERE/live_publish.mjs" push "$WORK" verified || echo "live: the push did not land; the next cycle sends it" >&2
    fi
  fi
  if [ "$INCREMENTAL_INVENTORY" = 1 ] && [ -n "$PUSH_PID" ] && ! kill -0 "$PUSH_PID" 2>/dev/null \
      && ranker_alive && [ -n "$RANKER_PUBLISHED_EPOCH" ]; then
    start_push
  fi
  # ONE CYCLE AND OUT, for a check run by hand.
  if [ "${ONCE:-}" = 1 ]; then
    break
  fi
  if [ "$inventory_pending" = 1 ] || [ "${RANKER_PUBLISH_PENDING:-0}" = 1 ]; then sleep 1; else sleep "$POLL_SECONDS"; fi
done
