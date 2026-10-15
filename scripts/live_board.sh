#!/usr/bin/env bash
# THE LIVE BOARDS, AND THE ORDERS BEHIND THEM — on the bot server
# (`deploy/wfsim-live.service`); `publish.yml` keeps a daily snapshot in git.
#
#   scripts/live_board.sh <work-dir> <bin-dir>
#
# TWO LOOPS. The INTAKE loop takes a submission in the moment it lands — its
# builds into the library, every row it owes into the queue and opened as a
# compute order (`ship_queue.sh`) — on its own clock, so a submission never
# waits for a projection pass. EVERY CYCLE of the main loop the rows `scores`
# gained are read, and the boards move:
# `verified/`, `scores` alone, pushed to R2 for the site; `board/`, with every
# client result not yet a fact, for the owner and the bot. This server fights
# nothing: what only the official machines may settle is `settle.yml`'s.
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
touch new-builds.ndjson unverified.ndjson

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
    jq -r '.id' < builds-new.ndjson >> arrived-ids.txt
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

# WHAT THE INTAKE LOOP TOOK IN SINCE THE LAST CYCLE, moved out of the mailbox
# after the store's read, so a build that landed meanwhile is in one or the other.
take_arrivals() {
  { flock 9
    touch arrived-builds.ndjson arrived-ids.txt
    cat arrived-builds.ndjson >> new-builds.ndjson
    cat arrived-ids.txt >> touched-ids.txt
    : > arrived-builds.ndjson
    : > arrived-ids.txt
  } 9> mailbox.lock
}

# READ INTO A SPARE AND SWAPPED ON SUCCESS: a failed read truncates its output,
# and an empty facts file would project a board with every scored row gone.
refresh_store() {
  bash "$HERE/fetch_library.sh" library.fresh >&2 || return 1
  bash "$HERE/fetch_facts.sh" facts.fresh >&2 || return 1
  [ -s library.fresh ] && [ -s facts.fresh ] || return 1
  mv library.fresh library.json
  # …AND WHICH BUILD IS WHICH WEAPON, which a subset pass is cut by.
  mv library.fresh.ndjson library-ids.ndjson
  mv facts.fresh facts-known.ndjson
  # A BUILD THE LIBRARY NOW HOLDS is in the read itself.
  : > new-builds.ndjson
}

# A PROJECTION, into a directory swapped in whole, so a reader never sees a
# board half written. Every ruler writes into the same directory: a weapon's
# file carries its rows under every ruler (`write_pages`). Run from `<cwd>`, so
# a `data/board_state.yaml` there is the one `wfsim-board` stamps.
#
# WITH A SUBSET LIBRARY (`$4`) it starts from the board as it stands and
# re-ranks only the weapons that library holds (`--subset`); the hourly read
# still ranks everything, so nothing a subset missed outlives the hour.
project_into() {
  local out="$1" facts="$2" cwd="$3" lib="library-live.json" flag="" r code
  rm -rf "$out.next"
  if [ -n "${4:-}" ] && [ -d "$out" ]; then
    cp -a "$out" "$out.next" && rm -f "$out.next/meta.json"
    lib="$4"
    facts="$4.facts"
    flag="--subset"
  else
    mkdir "$out.next"
  fi
  for r in $("$BIN/wfsim-board" --rulers); do
    code=0
    (cd "$cwd" && "$BIN/wfsim-board" "$r" "$WORK/$out.next" --project $flag --facts-in "$WORK/$facts" \
      < "$WORK/$lib" > /dev/null 2>> "$WORK/project.log") || code=$?
    if [ "$code" != "0" ] && [ "$code" != "2" ]; then
      echo "live: $r failed with $code" >&2
      return 1
    fi
  done
  rm -rf "$out.old"
  [ -d "$out" ] && mv "$out" "$out.old"
  mv "$out.next" "$out"
}

library_live() {
  jq -s '.[0] + [.[1][] | .record]' library.json <(jq -s . new-builds.ndjson) > library-live.json
}

# THE BUILDS OF THE WEAPONS A LIST OF BUILD IDS TOUCHES, into `$2`, and their
# facts out of `$3` into `$2.facts` — or a failure when the list names none.
subset_of() {
  node "$HERE/live_publish.mjs" subset "$WORK" "$1" "$2" "$3" >&2
}

# THE SITE'S BOARD: `scores` alone, stamped by `board_meta.py` and pushed to
# R2, file by file as they move (`live_publish.mjs`). `$1` is a subset library.
publish_verified() {
  mkdir -p pub/data
  [ -f pub/data/board_state.yaml ] || printf 'boards: {}\n' > pub/data/board_state.yaml
  project_into verified facts-known.ndjson pub "${1:-}" || return 1
  python3 "$HERE/board_meta.py" verified pub/data/board_state.yaml verified/meta.json >&2
}

# THE OWNER'S BOARD: the facts, and every client result no fact has answered
# yet — `facts-live.ndjson`, which a subset pass also cuts its facts from.
open_results() {
  jq -r '"\(.identity)|\(.ruler)|\(.mode)"' < facts-known.ndjson | sort -u > known-keys.txt
  jq -n -c --rawfile known known-keys.txt \
    '($known | split("\n") | map({key: ., value: true}) | from_entries) as $s
     | inputs | select($s["\(.identity)|\(.ruler)|\(.mode)"] | not)' \
    < unverified.ndjson > unverified-open.ndjson
  cat facts-known.ndjson unverified-open.ndjson > facts-live.ndjson
}

project_private() {
  [ -n "${1:-}" ] || open_results
  project_into board facts-live.ndjson . "${1:-}" || return 1
  echo "live: projected $(wc -l < unverified-open.ndjson) client result(s) beside $(wc -l < facts-known.ndjson) fact(s)" >&2
}

# WHICH BUILDS' CLIENT RESULTS CHANGED since the last read, into `touched-ids.txt`.
read_unverified() {
  node "$HERE/live_orders.mjs" unverified "$WORK" unverified.next || return 1
  # ONE COLLATION FOR `sort` AND `comm`, BYTEWISE: under en_US.UTF-8 comm
  # finds sort's own output out of order and may diff the wrong lines.
  jq -r '"\(.identity)|\(.ruler)|\(.mode)|\(.score)"' < unverified.ndjson | LC_ALL=C sort > unverified.was
  jq -r '"\(.identity)|\(.ruler)|\(.mode)|\(.score)"' < unverified.next | LC_ALL=C sort > unverified.now
  LC_ALL=C comm -3 unverified.was unverified.now | tr -d '\t' | cut -d'|' -f1 | sort -u >> touched-ids.txt
  mv unverified.next unverified.ndjson
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
  trap '[ -n "$INTAKE_PID" ] && kill "$INTAKE_PID" 2>/dev/null' EXIT
fi

last_refresh=0
while true; do
  whole=0
  facts_moved=0
  : > touched-ids.txt
  now=$(date +%s)
  if [ $((now - last_refresh)) -ge "$REFRESH_SECONDS" ] || [ ! -s library.json ]; then
    if refresh_store; then
      last_refresh=$now
      whole=1
    else
      echo "live: the store could not be read; keeping the last copy" >&2
    fi
  fi
  if [ "${ONCE:-}" = 1 ]; then
    intake_arrivals || true
  elif ! kill -0 "$INTAKE_PID" 2>/dev/null; then
    echo "live: the intake loop stopped; starting it again" >&2
    start_intake
  fi
  take_arrivals
  # WHAT WAS VERIFIED SINCE THE LAST READ, without waiting for the hourly one.
  if [ -s facts-known.ndjson ]; then
    code=0
    node "$HERE/live_publish.mjs" delta "$WORK" || code=$?
    if [ "$code" = 0 ]; then
      facts_moved=1
      cat moved-ids.txt >> touched-ids.txt
    fi
  fi
  read_unverified || echo "live: the open orders could not be read" >&2
  if [ -s library.json ] && { [ "$whole" = 1 ] || [ "$facts_moved" = 1 ] || [ -s touched-ids.txt ] \
       || [ ! -d verified ] || [ ! -d board ]; }; then
    library_live
    # WHOLE ON THE HOUR AND ON A FIRST START; otherwise only the weapons this
    # cycle's facts touched.
    if [ "$whole" = 1 ] || [ ! -d verified ]; then
      publish_verified || echo "live: the site's board did not publish; the last one stands" >&2
    elif [ "$facts_moved" = 1 ] && subset_of moved-ids.txt verified-subset.json facts-known.ndjson; then
      publish_verified verified-subset.json || echo "live: the site's board did not publish; the last one stands" >&2
    fi
    if [ "$whole" = 1 ] || [ ! -d board ]; then
      project_private || echo "live: the projection failed; the last board stands" >&2
    elif [ -s touched-ids.txt ]; then
      open_results
      if subset_of touched-ids.txt private-subset.json facts-live.ndjson; then
        project_private private-subset.json || echo "live: the projection failed; the last board stands" >&2
      fi
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
