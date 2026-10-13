#!/usr/bin/env bash
# WHAT NOBODY HAS ASKED FOR YET, ASKED FOR.
#
#   scripts/ship_queue.sh <batch-id> <why> missing.ndjson
#   scripts/ship_queue.sh --backfill                  an order for every owed row that has none
#   scripts/ship_queue.sh --self-test
#
# The reconciliation's write half. `wfsim-board --queue-missing` walks the
# library and names every (build, ruler, mode) that has neither a score nor a
# queue row; this puts them in a batch.
#
# WHY A RECONCILIATION EXISTS AT ALL. The queue is written by hand — intake
# writes it for an arrival, a person writes it for a rescore — and a hand-written
# list's one failure is a row nobody wrote, which would never be computed and
# never be noticed. `builds x rulers x modes` is the DEFINITION of what should
# exist; this closes the gap to it every run, in seconds.
#
# IT IS NOT A SECOND SOURCE. It only ever ADDS, and only rows that have no score
# — so the worst it can do is ask for something already owed, which the primary
# key absorbs.
set -euo pipefail

# AS MANY ROWS A STATEMENT AS A HUNDRED BOUND PARAMETERS ALLOW, DERIVED so a
# column added below shrinks the batch rather than putting every write one
# parameter over the limit.
QUEUE_COLUMNS=4
QUEUE_BATCH=$((100 / QUEUE_COLUMNS))

. "$(dirname "${BASH_SOURCE[0]}")/db.sh"
configured() { db_configured; }

# ---- the statements -------------------------------------------------------
#
# PARAMETERS ARE BOUND, NEVER INTERPOLATED. A build id is a hash and a ruler is
# a filename, but a queue row is assembled from ids that arrived at a public
# endpoint and this is where that text meets a language.
#
# THE BATCH ROW GOES FIRST AND CARRIES THE COUNT. `total` is a statement about
# the past — how many rows this group was created with — so progress is that
# minus what is still owed, and nothing has to be kept up to date.
batch_body() {
  jq -n -c --arg id "$1" --arg why "$2" --argjson total "$3" '
    {
      sql: ("INSERT INTO batches (id, at, why, total) VALUES (?,?,?,?)"
            + " ON CONFLICT(id) DO UPDATE SET total = batches.total + excluded.total"),
      params: [$id, $id, $why, $total]
    }'
}

# `ON CONFLICT DO NOTHING`, because asking twice for one row is one row. The primary
# key is (batch, build, ruler, mode) and that is the whole of the idempotence.
queue_batches() {
  jq -R -s -c --arg batch "$1" --argjson n "$QUEUE_BATCH" --arg owed "$(owed_only "$1" v.column2 v.column3 v.column4)" '
    [splits("\n")] | map(select(length > 0)) | map(fromjson) as $all
    | range(0; ($all | length); $n)
    | . as $i
    | $all[$i : $i + $n] as $chunk
    | {
        sql: ("INSERT INTO queue (batch, build_id, ruler, mode)"
              + " SELECT v.column1, v.column2, v.column3, v.column4 FROM (VALUES "
              + ([$chunk[] | "(?,?,?,?)"] | join(",")) + ") AS v WHERE " + $owed
              + " ON CONFLICT DO NOTHING"),
        params: [$chunk[] | $batch, .build_id, .ruler, .mode]
      }
  ' "$2"
}

# THE FACT FACTORY ASKS FOR EACH ROW (docs/BOARD.md §"The fact factory"):
# the site's worker turns a row into its question — the build's record, the
# ruler, the mode — and decides which producer asks, a new build's or a sweep's.
# `FACTORY_TOKEN` is the factory's own secret.
FACTORY_ORIGIN="${FACTORY_ORIGIN:-https://wfsim.app}"
ASK_ROWS=200
ASK_OUT=""
ASK_CODE=""
factory_ask() {
  [ -n "$ASK_OUT" ] || ASK_OUT=$(mktemp)
  ASK_CODE=$(curl -s --connect-timeout 15 --max-time 180 -o "$ASK_OUT" -w '%{http_code}' -X POST \
    -H "Authorization: Bearer ${FACTORY_TOKEN:-x}" \
    -H "content-type: application/json" \
    -H "user-agent: wfsim-queue" \
    --data-binary "$1" \
    "$FACTORY_ORIGIN/api/board/ask") || ASK_CODE="000"
  [ "$ASK_CODE" = "200" ]
}

# EVERY OWED ROW IS ASKED FOR, or the clients are never handed it and only the
# scorer measures it: a run stopped between the queue's write and the ask
# leaves exactly that. The worker asks for a few hundred a call, until none is.
backfill() {
  local asked=0 n i
  for i in $(seq 1 500); do
    factory_ask '{"backfill":true}' || { echo "::warning::queue: the backfill ask was refused [HTTP $ASK_CODE]"; return 0; }
    n=$(jq -r '.asked // 0' < "$ASK_OUT")
    asked=$((asked + n))
    [ "$(jq -r '.left' < "$ASK_OUT")" = "0" ] && break
  done
  echo "queue: $asked owed row(s) nobody had asked for, now asked"
}

# A NEW BUILD'S ROWS ARE ASKED FOR ONLY WHERE NO SCORE IS: a resubmission the
# page did not recognise — a board copy that lagged, a row under the entry line
# it never saw — is the same build, and reopening it fought it again. A
# rescore's batch asks for scored rows on purpose. Prints the SQL predicate.
owed_only() {
  case "$1" in
    arrivals-*) echo "NOT EXISTS (SELECT 1 FROM scores s WHERE s.identity = $2 AND s.ruler = $3 AND s.mode = $4)" ;;
    *) echo "true" ;;
  esac
}

# …AND EVERY ROW ASKED OF THE FACT FACTORY, `ASK_ROWS` a call.
ask_batches() {
  jq -R -s -c --argjson n "$ASK_ROWS" --arg batch "$2" '
    [splits("\n")] | map(select(length > 0)) | map(fromjson) as $all
    | range(0; ($all | length); $n)
    | . as $i
    | { batch: $batch, rows: [$all[$i : $i + $n][] | { identity: .build_id, ruler, mode }] }
  ' "$1"
}

send() {
  local what="$1" sent=0 failed=0 body
  while IFS= read -r body; do
    [ -n "$body" ] || continue
    if db_query "$body"; then
      sent=$((sent + 1))
    else
      failed=$((failed + 1))
      # THE DIAGNOSIS GOES TO STDERR, because this function's stdout is the two
      # counters its caller reads and a message on the same channel is read as
      # a count.
      if [ "$failed" -eq 1 ]; then
        {
          echo "::error::queue: the database refused a $what [HTTP $DB_CODE]"
          head -c 500 "$DB_OUT" || true
          echo
        } >&2
      fi
    fi
  done
  echo "$sent $failed"
}

ship() {
  local batch="$1" why="$2" file="$3" n r
  # `grep -c` PRINTS ZERO AND EXITS 1 on an empty file, so a `|| echo 0` after
  # it appends a SECOND zero: the count reads as two lines, never equals "0",
  # and the early return below never fires. An hourly reconciliation with
  # nothing to ask for then minted an empty batch every hour.
  n=$(grep -c . < "$file" 2>/dev/null) || n=0
  if [ "$n" = "0" ]; then
    echo "queue: nothing to ask for"
    return 0
  fi
  # THE BATCH BEFORE ITS ROWS. `fetch_queue.sh` joins the two, so a row whose
  # batch is not there yet is a row no run can see — the other order hides work
  # for as long as the write takes.
  if ! db_query "$(batch_body "$batch" "$why" "$n")"; then
    echo "::error::queue: the database refused the batch [HTTP $DB_CODE]"
    head -c 500 "$DB_OUT" || true
    return 1
  fi
  # A CHUNK WRITTEN, THEN ASKED, THEN THE NEXT: a row written and not asked is
  # one the scorer reads as nobody's, so a run its clock stops leaves at most
  # one chunk of those rather than everything after the write.
  # A ROW NOT ASKED COSTS SPEED, NEVER A ROW: the queue still owes it, the
  # backfill asks for it later, and the scorer measures it meanwhile.
  local chunk statements=0 failed=0 refused=0 body
  chunk=$(mktemp)
  for ((at = 1; at <= n; at += ASK_ROWS)); do
    sed -n "${at},$((at + ASK_ROWS - 1))p" "$file" > "$chunk"
    r=$(queue_batches "$batch" "$chunk" | send "queue row")
    set -- $r
    statements=$((statements + $1)); failed=$((failed + $2))
    [ "$2" = "0" ] || break
    while IFS= read -r body; do
      factory_ask "$body" || refused=$((refused + 1))
    done < <(ask_batches "$chunk" "$batch")
  done
  rm -f "$chunk"
  echo "queue: $n row(s) asked for in $statements statement(s), $failed refused"
  [ "$refused" = "0" ] || echo "::warning::queue: $refused ask(s) of the fact factory refused [HTTP $ASK_CODE] — the backfill asks again"
  [ "$failed" = "0" ]
}

# ---- self-test ------------------------------------------------------------
self_test() {
  ok=0; bad=0
  unset PG_URL
  DIR=$(mktemp -d)
  trap 'rm -rf "$DIR"' EXIT
  mkdir -p "$DIR/bin" "$DIR/work"
  say() { if [ "$1" = ok ]; then ok=$((ok + 1)); else bad=$((bad + 1)); fi; echo "  $1    $2"; }
  cd "$DIR/work"

  : > missing.ndjson
  for i in 1 2 3 4; do
    jq -n -c --arg i "$i" '{build_id:("h"+$i),ruler:"standard_single_target",mode:"base"}' >> missing.ndjson
  done

  local first; first=$(queue_batches "arrivals" missing.ndjson | head -1)
  [ "$(printf '%s' "$first" | jq -r '.params | length')" = "$((4 * QUEUE_COLUMNS))" ] \
    && say ok "$QUEUE_COLUMNS bound parameters a row" \
    || say FAIL "$(printf '%s' "$first" | jq -r '.params|length') parameters"
  [ $((QUEUE_BATCH * QUEUE_COLUMNS)) -le 100 ] \
    && say ok "...and a statement stays inside a hundred" \
    || say FAIL "$((QUEUE_BATCH * QUEUE_COLUMNS)) is over"
  printf '%s' "$first" | jq -e '.sql | endswith("ON CONFLICT DO NOTHING")' >/dev/null \
    && say ok "...and asking twice for one row is one row" \
    || say FAIL "$(printf '%s' "$first" | jq -r .sql)"
  printf '%s' "$first" | jq -e '.sql | contains("h1") | not' >/dev/null \
    && say ok "...never interpolated into the statement" \
    || say FAIL "an id reached the sql"
  printf '%s' "$(batch_body b why 4)" | jq -e '.params == ["b","b","why",4]' >/dev/null \
    && say ok "a batch carries the count it was asked for with" \
    || say FAIL "$(printf '%s' "$(batch_body b why 4)" | jq -c .params)"
  # …AND A SECOND PASS INTO ONE BATCH ADDS TO IT rather than replacing it. Two
  # runs in one day both find arrivals; `total` is a statement about the past —
  # how many rows this group was ever asked for — so progress is that minus what
  # is still owed, and a replace would report the group shrinking as it worked.
  printf '%s' "$(batch_body b why 4)" | jq -e '.sql | contains("total = batches.total + excluded.total")' >/dev/null \
    && say ok "...and a second pass into it adds rather than replaces" \
    || say FAIL "$(printf '%s' "$(batch_body b why 4)" | jq -r .sql)"

  export PATH="$DIR/bin:$PATH"
  export PG_URL=stub
  db_send() { "$DIR/bin/db"; }
  # THE DATABASE AND THE FACTORY BOTH LOG what they were sent, in one file, so
  # the order between a write and an ask is visible.
  cat > "$DIR/bin/curl" <<'ASK'
#!/usr/bin/env bash
prev=; body=; out=/dev/null
for a in "$@"; do
  [ "$prev" = "-o" ] && out="$a"
  [ "$prev" = "--data-binary" ] && body="$a"
  prev="$a"
done
printf '%s\n' "$body" >> "$PWD/sent.log"
printf '{"result":[{"results":[],"success":true}],"success":true}' > "$out"
printf '200'
ASK
  chmod +x "$DIR/bin/curl"
  cat > "$DIR/bin/db" <<'OK'
#!/usr/bin/env bash
cat >> "$PWD/sent.log"; echo >> "$PWD/sent.log"
printf '{"result":[{"results":[],"success":true}],"success":true}'
OK
  chmod +x "$DIR/bin/db"
  # NOTHING TO ASK FOR IS NOT AN EMPTY BATCH. An hourly reconciliation finds
  # nothing most hours, and a group with no rows in it is a row in a table a
  # person reads — twenty-four of them a day.
  : > sent.log; : > none.ndjson
  ship "arrivals" "nothing" none.ndjson > out.txt 2>&1     && grep -q "nothing to ask for" out.txt     && say ok "nothing to ask for asks for nothing" || say FAIL "$(cat out.txt)"
  [ ! -s sent.log ]     && say ok "...and writes no batch at all" || say FAIL "$(head -c 200 sent.log)"
  : > sent.log
  ship arrivals "what arrived" missing.ndjson > out.txt 2>&1 \
    && say ok "a pass asks for what it was handed" || say FAIL "$(cat out.txt)"
  # THE BATCH FIRST, or its rows are invisible to `fetch_queue.sh`'s join for as
  # long as the write takes.
  a=$(grep -n "INTO batches" sent.log | head -1 | cut -d: -f1)
  b=$(grep -n "INTO queue" sent.log | head -1 | cut -d: -f1)
  [ -n "$a" ] && [ -n "$b" ] && [ "$a" -lt "$b" ] \
    && say ok "...naming the batch before the rows in it" || say FAIL "the rows went first"
  grep -q '"rows":\[{"identity":"h1"' sent.log     && say ok "...and asks the fact factory for each row" || say FAIL "no ask was sent"
  # A CHUNK ASKED BEFORE THE NEXT IS WRITTEN, so a stopped run leaves at most
  # one chunk written and unasked — which the scorer would read as nobody's.
  : > sent.log
  ASK_ROWS=2 ship arrivals "what arrived" missing.ndjson > out.txt 2>&1
  order=$(grep -o 'INTO queue\|"rows":' sent.log | tr '\n' ' ')
  [ "$order" = 'INTO queue "rows": INTO queue "rows": ' ] \
    && say ok "...a chunk at a time, each asked before the next is written" || say FAIL "order: $order"

  cat > "$DIR/bin/db" <<'DEAD'
#!/usr/bin/env bash
printf '{"success":false,"errors":[{"code":"42P01","message":"relation queue does not exist"}]}'
exit 1
DEAD
  chmod +x "$DIR/bin/db"
  ship arrivals why missing.ndjson > out.txt 2>&1 \
    && say FAIL "a refused write passed" \
    || { grep -q "does not exist" out.txt \
         && say ok "a refused write reports what the database said" \
         || say FAIL "$(cat out.txt)"; }

  unset PG_URL
  configured && say FAIL "unconfigured read as configured" \
    || say ok "no database configured is a working state"

  echo
  echo "$ok ok, $bad failed"
  [ "$bad" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then self_test; exit $?; fi
# THE STATEMENTS THEMSELVES, for check_board_verify.mjs to run against the schema.
if [ "${1:-}" = "--body" ]; then
  "${2:?queue|ask}_batches" "${@:3}"
  exit $?
fi

if ! configured; then
  echo "queue: no database configured, nothing asked for"
  exit 0
fi
if [ "${1:-}" = "--backfill" ]; then backfill; exit 0; fi
ship "${1:?usage: ship_queue.sh <batch-id> <why> <missing.ndjson>}" \
     "${2:?usage: ship_queue.sh <batch-id> <why> <missing.ndjson>}" \
     "${3:?usage: ship_queue.sh <batch-id> <why> <missing.ndjson>}"
