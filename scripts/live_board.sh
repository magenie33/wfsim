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
mkdir -p "$WORK"
cd "$WORK"
WORK="$(pwd)"
touch new-builds.ndjson

. "$HERE/db.sh"

# WHAT ARRIVED, TAKEN IN NOW: the same intake `queue.yml` runs hourly, which
# stays as the fallback — the builds land and the inbox rows are spent
# (`ship_builds.sh`), then every row the builds owe is queued and opened as an
# order. Twice is harmless: a build is keyed by its hash and a queue row by its
# own key.
intake_arrivals() {
  # WHO ASKED rides with the record to the rows it owes (docs/BOARD.md §"Who asked").
  db_query '{"sql":"SELECT id, at, record, initiator FROM inbox ORDER BY id LIMIT 500"}' \
    || { echo "live: inbox read refused [HTTP $DB_CODE]" >&2; return 1; }
  jq -c '.result[0].results[]' < "$DB_OUT" > inbox-new.ndjson
  [ -s inbox-new.ndjson ] || return 1
  "$BIN/wfsim-intake" --done done.txt --orders orders-new.ndjson --asked asked-new.ndjson \
    < inbox-new.ndjson > builds-new.ndjson 2>> intake.log
  bash "$HERE/ship_builds.sh" builds-new.ndjson done.txt >&2 || return 1
  # …INTO THE MAILBOX the main loop takes them from (`take_arrivals`).
  { flock 9
    cat builds-new.ndjson >> arrived-builds.ndjson
  } 9> mailbox.lock
  # THE FIGHT EACH ARRIVAL NAMED GOES FIRST, asked as waited on (`named`): the
  # player just ran it and is looking for that row. The build's other rows
  # follow as ordinary ones, the named row left out so it is asked once.
  local day; day="arrivals-$(date -u +%Y-%m-%d)"
  jq -c '. + {named: true}' < asked-new.ndjson > named-new.ndjson
  jq -r '"\(.build_id)|\(.ruler)|\(.mode)"' < asked-new.ndjson > named-keys.txt
  jq -n -c --rawfile named named-keys.txt \
    '($named | split("\n") | map({key: ., value: true}) | from_entries) as $s
     | inputs | select($s["\(.build_id)|\(.ruler)|\(.mode)"] | not)' < orders-new.ndjson > rest-new.ndjson
  local f
  for f in named-new.ndjson rest-new.ndjson; do
    bash "$HERE/ship_queue.sh" "$day" "the rows a new build owes" "$f" >&2 \
      || echo "live: the new rows were not queued; the hourly reconciliation asks for them" >&2
  done
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
  { flock 9
    touch arrived-builds.ndjson
    mv arrived-builds.ndjson arrived-now.ndjson
    : > arrived-builds.ndjson
  } 9> mailbox.lock
  cat arrived-now.ndjson >> new-builds.ndjson
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
  mkdir -p pub/data
  [ -f pub/data/board_state.yaml ] || printf 'boards: {}\n' > pub/data/board_state.yaml
  # THE LOG IS WHAT THIS RANKER REFUSED, said once a record: it starts afresh.
  : > project.log
  coproc RANKER { exec "$BIN/wfsim-board" --serve "$WORK/verified" --library "$WORK/library.json" \
      --builds "$WORK/new-builds.ndjson" --facts-in "$WORK/facts-known.ndjson" \
      --state "$WORK/pub/data/board_state.yaml" 2>> "$WORK/project.log"; }
  ask "" || { stop_ranker; return 1; }
}

# ONE COMMAND, ONE LINE BACK. An empty command only reads (the `ready` line).
ask() {
  local reply
  if [ -n "$1" ]; then
    printf '%s\n' "$1" >&"${RANKER[1]}" || return 1
  fi
  IFS= read -r reply <&"${RANKER[0]}" || return 1
  case "$reply" in
    ok*) echo "live: ranker $reply" >&2 ;;
    *) echo "live: the ranker answered: $reply" >&2; return 1 ;;
  esac
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

# WHAT THE SITE READS BESIDE THE ROWS: each weapon's heads, and the stamp.
stamp() {
  node "$HERE/board_heads.mjs" verified >&2 \
    && python3 "$HERE/board_meta.py" verified pub/data/board_state.yaml verified/meta.json >&2
}

# ONE CYCLE AND OUT takes its arrivals in line; a service runs the intake loop
# beside the main one, and starts it again if it ever stops.
INTAKE_PID=""
start_intake() {
  intake_loop &
  INTAKE_PID=$!
}
if [ "${ONCE:-}" != 1 ]; then
  start_intake
fi
trap '[ -n "$INTAKE_PID" ] && kill "$INTAKE_PID" 2>/dev/null; [ -n "$FETCH_PID" ] && kill "$FETCH_PID" 2>/dev/null; stop_ranker; db_cleanup' EXIT

# A FIRST START HAS NO STORE TO RANK FROM, and waits for one.
if [ ! -s library.json ]; then
  start_fetch
  wait "$FETCH_PID" || true
  FETCH_PID=""
  if [ -f fresh.ready ]; then
    swap_store
    date +%s > fetch-at.txt
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
  if [ -z "$FETCH_PID" ] && [ $(( $(date +%s) - last_fetch )) -ge "$REFRESH_SECONDS" ]; then
    start_fetch
  fi
  take_arrivals
  # WHAT WAS VERIFIED SINCE THE LAST READ, without waiting for the hourly one.
  moved=0
  if [ -s facts-known.ndjson ]; then
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
  elif [ "$reloaded" = 1 ]; then
    # THE READ HOLDS EVERYTHING the cycle took in, so it is the whole message.
    if ask "reload $WORK/library.json $WORK/new-builds.ndjson $WORK/facts-known.ndjson"; then
      changed=1
    else
      stop_ranker
    fi
  elif [ -s arrived-now.ndjson ] || [ "$moved" = 1 ]; then
    if { [ ! -s arrived-now.ndjson ] || ask "builds $WORK/arrived-now.ndjson"; } \
      && { [ "$moved" = 0 ] || ask "facts $WORK/moved-facts.ndjson"; } \
      && ask "publish"; then
      changed=1
    else
      stop_ranker
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
  if [ -f verified/meta.json ]; then
    node "$HERE/live_publish.mjs" push "$WORK" verified || echo "live: the push did not land; the next cycle sends it" >&2
  fi
  # ONE CYCLE AND OUT, for a check run by hand.
  if [ "${ONCE:-}" = 1 ]; then
    break
  fi
  sleep "$POLL_SECONDS"
done
