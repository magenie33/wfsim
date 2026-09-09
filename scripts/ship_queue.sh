#!/usr/bin/env bash
# WHAT NOBODY HAS ASKED FOR YET, ASKED FOR.
#
#   scripts/ship_queue.sh <batch-id> <why> missing.ndjson
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
QUEUE_COLUMNS=4
QUEUE_BATCH=$((100 / QUEUE_COLUMNS))

configured() {
  [ -n "${CF_ACCOUNT:-}" ] && [ -n "${CF_D1_DATABASE:-}" ] && [ -n "${CF_TOKEN:-}" ]
}

D1_OUT=""
D1_CODE=""
d1() {
  [ -n "$D1_OUT" ] || D1_OUT=$(mktemp)
  D1_CODE=$(curl -s -o "$D1_OUT" -w '%{http_code}' -X POST \
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
  jq -R -s -c --arg batch "$1" --argjson n "$QUEUE_BATCH" '
    [splits("\n")] | map(select(length > 0)) | map(fromjson) as $all
    | range(0; ($all | length); $n)
    | . as $i
    | $all[$i : $i + $n] as $chunk
    | {
        sql: ("INSERT OR IGNORE INTO queue (batch, build_id, ruler, mode) VALUES "
              + ([$chunk[] | "(?,?,?,?)"] | join(","))),
        params: [$chunk[] | $batch, .build_id, .ruler, .mode]
      }
  ' "$2"
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
    jq -n -c --arg i "$i" '{build_id:("h"+$i),ruler:"single_target",mode:"base"}' >> missing.ndjson
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

if ! configured; then
  echo "queue: no database configured, nothing asked for"
  exit 0
fi
ship "${1:?usage: ship_queue.sh <batch-id> <why> <missing.ndjson>}" \
     "${2:?usage: ship_queue.sh <batch-id> <why> <missing.ndjson>}" \
     "${3:?usage: ship_queue.sh <batch-id> <why> <missing.ndjson>}"
