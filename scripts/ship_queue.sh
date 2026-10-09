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

# AS MANY ROWS A STATEMENT AS D1'S HUNDRED BOUND PARAMETERS ALLOW, DERIVED so a
# column added below shrinks the batch rather than putting every write one
# parameter over the limit.
# AN INSTANT IS ISO 8601 TO THE MILLISECOND (docs/NAMING.md §9), as SQL:
# now, and `?` seconds ago.
ISO_NOW="strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"
QUEUE_COLUMNS=4
QUEUE_BATCH=$((100 / QUEUE_COLUMNS))

configured() {
  [ -n "${CF_ACCOUNT:-}" ] && [ -n "${CF_D1_DATABASE:-}" ] && [ -n "${CF_TOKEN:-}" ]
}

D1_OUT=""
D1_CODE=""
d1() {
  [ -n "$D1_OUT" ] || D1_OUT=$(mktemp)
  # A CALL THAT NEVER ANSWERS IS THE WORST SHAPE THIS PIPELINE HAS. Without a
  # clock a hung connection holds the step until GitHub's six-hour cap, and the
  # board's workflows keep ONE run pending per concurrency group — so every
  # hourly tick behind it is cancelled and nothing is scored until somebody
  # looks. Measured 2026-09-21: one hung call cost seventeen hours of ticks.
  D1_CODE=$(curl -s --connect-timeout 15 --max-time 180     -o "$D1_OUT" -w '%{http_code}' -X POST \
    -H "Authorization: Bearer ${CF_TOKEN:-x}" \
    -H "content-type: application/json" \
    --data-binary "$1" \
    "https://api.cloudflare.com/client/v4/accounts/${CF_ACCOUNT:-x}/d1/database/${CF_D1_DATABASE:-x}/query") \
    || D1_CODE="000"
  [ "$D1_CODE" = "200" ]
}

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
            + " ON CONFLICT(id) DO UPDATE SET total = total + excluded.total"),
      params: [$id, $id, $why, $total]
    }'
}

# `INSERT OR IGNORE`, because asking twice for one row is one row. The primary
# key is (batch, build, ruler, mode) and that is the whole of the idempotence.
queue_batches() {
  jq -R -s -c --arg batch "$1" --argjson n "$QUEUE_BATCH" --arg owed "$(owed_only "$1" v.column2 v.column3 v.column4)" '
    [splits("\n")] | map(select(length > 0)) | map(fromjson) as $all
    | range(0; ($all | length); $n)
    | . as $i
    | $all[$i : $i + $n] as $chunk
    | {
        sql: ("INSERT OR IGNORE INTO queue (batch, build_id, ruler, mode)"
              + " SELECT v.column1, v.column2, v.column3, v.column4 FROM (VALUES "
              + ([$chunk[] | "(?,?,?,?)"] | join(",")) + ") AS v WHERE " + $owed),
        params: [$chunk[] | $batch, .build_id, .ruler, .mode]
      }
  ' "$2"
}

# A NEW BUILD'S ROWS ARE LEASED BEFORE A RESCORE'S (`priority` 0): an `arrivals-`
# batch's row whose build was submitted that day or the one before. An older
# build asked for a ruler it lacks lands in the same daily batch, and is a sweep.
# Prints the SQL expression; `$1` is the batch (a column or `?`, bound twice).
priority_of() {
  echo "CASE WHEN $1 LIKE 'arrivals-%' AND $2 >= date(substr($1, 10), '-1 day') THEN 0 ELSE 1 END"
}

# EVERY OWED ROW HAS AN ORDER, or the clients are never handed it and only the
# scorer measures it: a run stopped between the queue's write and the orders'
# leaves exactly that, and no later run wrote either again. A row is owed while
# its batch stands; these statements open what is missing, a few thousand at a
# time, until none is.
BACKFILL_ROWS=2000
backfill_body() {
  jq -n -c --argjson n "$BACKFILL_ROWS" --arg now "$ISO_NOW" --arg priority "$(priority_of q.batch b.at)" '
    {
      sql: ("INSERT INTO orders (identity, ruler, mode, record, state, engine, slot, at, priority)"
            + " SELECT q.build_id, q.ruler, q.mode, b.record, ?, ?, abs(random()) % 2147483647, " + $now + ","
            + " MIN(" + $priority + ")"
            + " FROM queue q JOIN batches t ON t.id = q.batch JOIN builds b ON b.id = q.build_id"
            + " WHERE NOT EXISTS (SELECT 1 FROM orders o WHERE o.identity = q.build_id AND o.ruler = q.ruler AND o.mode = q.mode)"
            + " GROUP BY q.build_id, q.ruler, q.mode LIMIT ?"),
      params: ["todo", "", $n]
    }'
}
backfill() {
  local opened=0 n i
  for i in $(seq 1 200); do
    d1 "$(backfill_body)" || { echo "::warning::queue: the order backfill was refused [HTTP $D1_CODE]"; return 0; }
    n=$(jq -r '.result[0].meta.changes // 0' < "$D1_OUT")
    opened=$((opened + n))
    [ "$n" -gt 0 ] || break
  done
  echo "queue: $opened owed row(s) had no order, now opened"
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

# …AND A COMPUTE ORDER BESIDE EVERY ROW, built from the library's record of the
# build (docs/BOARD.md §"Compute orders"). One the queue asks for again — a
# rescore, a sweep — is opened afresh unless clients or the server are still
# working on it. Three bound parameters a row, so the same chunk fits.
orders_batches() {
  jq -R -s -c --argjson n "$QUEUE_BATCH" --arg now "$ISO_NOW" --arg batch "${2:-}" --arg priority "$(priority_of "?" b.at)" --arg owed "$(owed_only "${2:-}" v.column1 v.column2 v.column3)" '
    [splits("\n")] | map(select(length > 0)) | map(fromjson) as $all
    | range(0; ($all | length); $n)
    | . as $i
    | $all[$i : $i + $n] as $chunk
    | {
        sql: ("INSERT INTO orders (identity, ruler, mode, record, state, engine, slot, at, priority)"
              + " SELECT v.column1, v.column2, v.column3, b.record, ?, ?,"
              + " abs(random()) % 2147483647, " + $now + ", " + $priority
              + " FROM (VALUES " + ([$chunk[] | "(?,?,?)"] | join(",")) + ") AS v"
              + " JOIN builds b ON b.id = v.column1 WHERE " + $owed
              + " ON CONFLICT (identity, ruler, mode) DO UPDATE SET state = ?, engine = ?,"
              + " slot = excluded.slot, record = excluded.record, score = NULL, metric = NULL,"
              + " produced_by = NULL, verifier = NULL, disputed = NULL, lease = NULL,"
              + " lease_until = NULL, leased_to = NULL, at = excluded.at,"
              + " priority = excluded.priority, carried_from = NULL"
              + " WHERE orders.state IN (?, ?, ?)"),
        params: (["todo", "", $batch, $batch] + [$chunk[] | .build_id, .ruler, .mode]
                 + ["todo", "", "verified", "rejected", "settled"])
      }
  ' "$1"
}

send() {
  local what="$1" sent=0 failed=0 body
  while IFS= read -r body; do
    [ -n "$body" ] || continue
    if d1 "$body"; then
      sent=$((sent + 1))
    else
      failed=$((failed + 1))
      # THE DIAGNOSIS GOES TO STDERR, because this function's stdout is the two
      # counters its caller reads and a message on the same channel is read as
      # a count.
      if [ "$failed" -eq 1 ]; then
        {
          echo "::error::queue: the database refused a $what [HTTP $D1_CODE]"
          head -c 500 "$D1_OUT" || true
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
  if ! d1 "$(batch_body "$batch" "$why" "$n")"; then
    echo "::error::queue: the database refused the batch [HTTP $D1_CODE]"
    head -c 500 "$D1_OUT" || true
    return 1
  fi
  r=$(queue_batches "$batch" "$file" | send "queue row")
  set -- $r
  echo "queue: $n row(s) asked for in $1 statement(s), $2 refused"
  [ "$2" = "0" ] || return 1
  # AN ORDER THAT DID NOT OPEN COSTS SPEED, NEVER A ROW: the queue still owes
  # it and the scorer measures it as it always has.
  r=$(orders_batches "$file" "$batch" | send "compute order")
  set -- $r
  [ "$2" = "0" ] || echo "::warning::queue: $2 compute-order statement(s) refused — the scorer will measure those rows"
}

# ---- self-test ------------------------------------------------------------
self_test() {
  ok=0; bad=0
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
    && say ok "...and a statement stays inside D1's hundred" \
    || say FAIL "$((QUEUE_BATCH * QUEUE_COLUMNS)) is over"
  printf '%s' "$first" | jq -e '.sql | contains("INSERT OR IGNORE")' >/dev/null \
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
  printf '%s' "$(batch_body b why 4)" | jq -e '.sql | contains("total = total + excluded.total")' >/dev/null \
    && say ok "...and a second pass into it adds rather than replaces" \
    || say FAIL "$(printf '%s' "$(batch_body b why 4)" | jq -r .sql)"

  export PATH="$DIR/bin:$PATH"
  export CF_ACCOUNT=a CF_D1_DATABASE=d CF_TOKEN=t
  cat > "$DIR/bin/curl" <<'COUNT'
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
COUNT
  chmod +x "$DIR/bin/curl"
  # NOTHING TO ASK FOR IS NOT AN EMPTY BATCH. An hourly reconciliation finds
  # nothing most hours, and a group with no rows in it is a row in a table a
  # person reads — twenty-four of them a day.
  : > sent.log; : > none.ndjson
  ship "arrivals" "nothing" none.ndjson > out.txt 2>&1     && grep -q "nothing to ask for" out.txt     && say ok "nothing to ask for asks for nothing" || say FAIL "$(cat out.txt)"
  [ ! -s sent.log ]     && say ok "...and writes no batch at all" || say FAIL "$(head -c 200 sent.log)"
  cat > "$DIR/bin/curl" <<'OK'
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
OK
  chmod +x "$DIR/bin/curl"
  : > sent.log
  ship arrivals "what arrived" missing.ndjson > out.txt 2>&1 \
    && say ok "a pass asks for what it was handed" || say FAIL "$(cat out.txt)"
  # THE BATCH FIRST, or its rows are invisible to `fetch_queue.sh`'s join for as
  # long as the write takes.
  a=$(grep -n "INTO batches" sent.log | head -1 | cut -d: -f1)
  b=$(grep -n "INTO queue" sent.log | head -1 | cut -d: -f1)
  [ -n "$a" ] && [ -n "$b" ] && [ "$a" -lt "$b" ] \
    && say ok "...naming the batch before the rows in it" || say FAIL "the rows went first"

  cat > "$DIR/bin/curl" <<'DEAD'
#!/usr/bin/env bash
out=/dev/null; prev=
for a in "$@"; do [ "$prev" = "-o" ] && out="$a"; prev="$a"; done
printf '{"success":false,"errors":[{"code":7500,"message":"no such table"}]}' > "$out"
printf '500'
DEAD
  chmod +x "$DIR/bin/curl"
  ship arrivals why missing.ndjson > out.txt 2>&1 \
    && say FAIL "a refused write passed" \
    || { grep -q "no such table" out.txt \
         && say ok "a refused write reports what the database said" \
         || say FAIL "$(cat out.txt)"; }

  unset CF_D1_DATABASE
  configured && say FAIL "unconfigured read as configured" \
    || say ok "no database configured is a working state"

  echo
  echo "$ok ok, $bad failed"
  [ "$bad" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then self_test; exit $?; fi
# THE STATEMENTS THEMSELVES, for check_board_verify.mjs to run against the schema.
if [ "${1:-}" = "--body" ]; then
  if [ "${2:-}" = "backfill" ]; then backfill_body; else "${2:?queue|orders|backfill}_batches" "${@:3}"; fi
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
