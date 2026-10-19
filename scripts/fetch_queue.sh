#!/usr/bin/env bash
# WHAT HAS BEEN ASKED FOR, IN THE ORDER IT WILL BE DONE.
#
#   scripts/fetch_queue.sh queue.ndjson
#   scripts/fetch_queue.sh --self-test
#
# A run does not decide what to compute; it reads it. `queue` holds one row per
# (batch, build, ruler, mode) somebody asked for and `batches.at` is the order,
# so this file is the whole of "what is this run doing" — a list a person can
# read, ordered by a column a person can change.
#
# THE ORDER IS THE ANSWER, so it is taken here and never re-derived downstream:
# every shard is handed this same file and takes its slice by POSITION, which is
# how they agree on who owns which row without talking.
#
# THE SCORER NEVER SPEAKS TO THE DATABASE. It reads and writes files, and the
# database and the factory live in scripts a stub can drive.
set -euo pipefail

# A PAGE, AND THE LOOP IS BOUNDED. A page is at most what the factory's claimed
# report reads (10,000 rows, worker/factory.js `report`); the bound is ten
# million rows, which a survey's arrivals have reached a third of.
PAGE=5000
MAX_PAGES=2000

. "$(dirname "${BASH_SOURCE[0]}")/db.sh"
configured() { db_configured; }

# ---- the read -------------------------------------------------------------
#
# ORDERED TOTALLY, so paging is stable. `batches.at` sets which group goes
# first; the primary key breaks every tie under it, and without that last part
# two pages of one query may overlap or skip — a row the run then owes for ever.
#
# A ROW WHOSE BATCH IS GONE IS NOT READ. An inner join, so deleting a batch is
# the whole of cancelling it: the rows stop being asked for and nothing has to
# go and find them.
# A CLAIMED RUN READS WHAT IT CLAIMED (docs/BOARD.md §"The fact factory"): the
# official machines take only questions past their deadline, so a young row is
# the clients' and an overdue one this run's — claimed from the factory before
# it reads (`claim`), never a row a client holds. It also reads what nobody
# asked the factory for, which no client can be handed. Which rows those are
# the factory says of each page (`report claimed`): its tables may be in
# another database than the queue. `CLAIMED` unset, every owed row is read —
# which is what a reconciliation needs.
#
# A BATCH AT A TIME, EACH PAGE AFTER THE LAST ROW: the batches in their order,
# then each one's rows along the primary key. An OFFSET sorts every row it
# skips, so a read of millions slowed with each page and outran the job.
batches_body() {
  jq -n -c '
    {
      sql: ("SELECT b.id AS batch FROM batches b"
            + " WHERE EXISTS (SELECT 1 FROM queue q WHERE q.batch = b.id)"
            + " ORDER BY b.at, b.id"),
      params: []
    }'
}
page_body() {
  jq -n -c --argjson limit "$1" --arg batch "$2" --arg build "${3:-}" --arg ruler "${4:-}" --arg mode "${5:-}" '
    {
      sql: ("SELECT q.batch, q.build_id, q.ruler, q.mode FROM queue q"
            + " JOIN batches b ON b.id = q.batch"
            + " WHERE q.batch = ? AND (q.batch, q.build_id, q.ruler, q.mode) > (?, ?, ?, ?)"
            + " ORDER BY q.batch, q.build_id, q.ruler, q.mode"
            + " LIMIT ?"),
      params: [$batch, $batch, $build, $ruler, $mode, $limit]
    }'
}

# THE OFFICIAL WITNESS (worker/factory.js `officialRoute`), one per run — runs
# are serialized — speaking to the factory with its own secret.
FACTORY_ORIGIN="${FACTORY_ORIGIN:-https://wfsim.app}"
# THE BOARD'S KINDS (worker/board_tasks.js `BOARD_PRODUCERS`): every question a
# board row is, whoever asked it.
BOARD_KINDS='["new_build","survey_build","refresh","sweep"]'
WITNESS="${WITNESS:-official:scorer}"
CLAIM_PAGE=2000
CLAIM_MAX="${CLAIM_MAX:-40000}"
F_OUT=""
F_IN=""
F_CODE=""
factory() {
  [ -n "$F_OUT" ] || F_OUT=$(mktemp)
  # THE BODY GOES THROUGH A FILE: a page of claimed rows passed as an argument
  # broke the kernel's argument limit ("Argument list too long") once the queue grew.
  [ -n "$F_IN" ] || F_IN=$(mktemp)
  printf '%s' "$2" > "$F_IN"
  F_CODE=$(curl -s --connect-timeout 15 --max-time 180 -o "$F_OUT" -w '%{http_code}' -X POST \
    -H "Authorization: Bearer ${FACTORY_TOKEN:-x}" -H "content-type: application/json" -H "user-agent: wfsim-scorer" \
    --data-binary "@$F_IN" "$FACTORY_ORIGIN/api/tasks/official/$1") || F_CODE="000"
  [ "$F_CODE" = "200" ]
}
claim_body() {
  jq -n -c --arg witness "$WITNESS" --arg engine "${ENGINE:-unknown}" --argjson max "$CLAIM_PAGE" \
    --argjson kinds "$BOARD_KINDS" '{ witness: $witness, engine: $engine, states: ["open"], producers: $kinds, max: $max, lease_ms: 43200000 }'
}
release_body() { jq -n -c --arg witness "$WITNESS" '{ witness: $witness }'; }

# THE CLAIM, a page at a time until the factory has nothing past its deadline
# or `CLAIM_MAX` is reached: every question the run fights is one no client is
# handed meanwhile, and one it does not reach is the clients' again at release.
claim() {
  local total=0 got
  while [ "$total" -lt "$CLAIM_MAX" ]; do
    factory claim "$(claim_body)" || { echo "::error::queue: the factory refused the claim [HTTP $F_CODE]"; head -c 500 "$F_OUT"; echo; return 1; }
    got=$(jq -r '.tasks | length' < "$F_OUT")
    total=$((total + got))
    [ "$got" -lt "$CLAIM_PAGE" ] && break
  done
  echo "queue: claimed $total question(s) past their deadline"
}
release() {
  factory release "$(release_body)" || { echo "::error::queue: the factory refused the release [HTTP $F_CODE]"; head -c 500 "$F_OUT"; echo; return 1; }
  echo "queue: released what $WITNESS held"
}

refused() {
  echo "::error::queue: the database refused the read [HTTP $DB_CODE]"
  [ -s "$DB_OUT" ] && { head -c 500 "$DB_OUT"; echo; }
  return 1
}

fetch() {
  local out="$1" got total=0 pages=0 batch build ruler mode batches
  : > "$out"
  db_query "$(batches_body)" || { refused; return 1; }
  # A WINDOWS jq ENDS A LINE IN \r, which would become part of the name.
  batches=$(jq -r '.result[0].results[].batch' < "$DB_OUT" | tr -d '\r')
  while IFS= read -r batch; do
    [ -n "$batch" ] || continue
    build="" ruler="" mode=""
    while :; do
      if [ "$pages" -ge "$MAX_PAGES" ]; then
        echo "::error::queue: stopped at $MAX_PAGES pages — more is owed than this reads"
        return 1
      fi
      pages=$((pages + 1))
      db_query "$(page_body "$PAGE" "$batch" "$build" "$ruler" "$mode")" || { refused; return 1; }
      got=$(jq -r '.result[0].results | length' < "$DB_OUT")
      if [ -n "${CLAIMED:-}" ]; then
        if ! factory report "$(jq -c --arg witness "$WITNESS" '{ witness: $witness, report: "claimed", rows: .result[0].results }' < "$DB_OUT")"; then
          echo "::error::queue: the factory refused the claimed read [HTTP $F_CODE]"; head -c 500 "$F_OUT"; echo
          return 1
        fi
        jq -c '.rows[]' < "$F_OUT" >> "$out"
      else
        jq -c '.result[0].results[]' < "$DB_OUT" >> "$out"
      fi
      total=$((total + got))
      [ "$got" -lt "$PAGE" ] && break
      build=$(jq -r '.result[0].results[-1].build_id' < "$DB_OUT" | tr -d '\r')
      ruler=$(jq -r '.result[0].results[-1].ruler' < "$DB_OUT" | tr -d '\r')
      mode=$(jq -r '.result[0].results[-1].mode' < "$DB_OUT" | tr -d '\r')
    done
  done <<< "$batches"
  echo "queue: $total row(s) owed"
}

# ---- self-test ------------------------------------------------------------
self_test() {
  ok=0; bad=0
  unset PG_URL
  DIR=$(mktemp -d)
  trap 'rm -rf "$DIR"; db_cleanup' EXIT
  mkdir -p "$DIR/bin" "$DIR/work"
  say() { if [ "$1" = ok ]; then ok=$((ok + 1)); else bad=$((bad + 1)); fi; echo "  $1    $2"; }
  cd "$DIR/work"

  local body; body=$(page_body 5000 arrivals b1 r m)
  printf '%s' "$body" | jq -e '.sql | contains("ORDER BY q.batch, q.build_id, q.ruler, q.mode")' >/dev/null \
    && batches_body | jq -e '.sql | contains("ORDER BY b.at, b.id")' >/dev/null \
    && say ok "the order is the batch's, then the key" \
    || say FAIL "$(printf '%s' "$body" | jq -r .sql)"
  printf '%s' "$body" | jq -e '.sql | contains("JOIN batches")' >/dev/null \
    && say ok "...and a row whose batch is gone is not read" \
    || say FAIL "no join — deleting a batch would leave its rows owed"
  printf '%s' "$body" | jq -e '.params == ["arrivals", "arrivals", "b1", "r", "m", 5000] and (.sql | contains("OFFSET") | not)' >/dev/null \
    && say ok "...and a page starts after the last row, bound, not interpolated" \
    || say FAIL "$(printf '%s' "$body" | jq -c .params)"

  export PATH="$DIR/bin:$PATH"
  export PG_URL=stub
  db_send() { "$DIR/bin/db"; }
  # THE FACTORY keeps b1 of whatever page it is shown.
  cat > "$DIR/bin/curl" <<'OK'
#!/usr/bin/env bash
out=/dev/null; prev=; in=
for a in "$@"; do [ "$prev" = "-o" ] && out="$a"; case "$a" in @*) in="${a#@}";; esac; prev="$a"; done
jq -c '{ok:true,rows:[.rows[] | select(.build_id == "b1")]}' < "$in" > "$out"
printf '200'
OK
  chmod +x "$DIR/bin/curl"
  # A DATABASE OF TWO BATCHES, answering each statement as Postgres would.
  cat > "$DIR/bin/db" <<'OK'
#!/usr/bin/env bash
jq -c '
  def row(b; id): {batch: b, build_id: id, ruler: "standard_single_target", mode: "base"};
  [row("first"; "b0"), row("first"; "b1"), row("first"; "b2"), row("second"; "c0"), row("second"; "c1")] as $q
  | if (.sql | contains("EXISTS")) then [{batch: "first"}, {batch: "second"}]
    else .params as $p | [$q[] | select(.batch == $p[0] and .build_id > $p[2])][:$p[5]] end
  | {result: [{results: ., success: true}], success: true}'
OK
  chmod +x "$DIR/bin/db"
  PAGE=2
  fetch got.ndjson > out.txt 2>&1 \
    && say ok "a read pages until the answer runs out" || say FAIL "$(cat out.txt)"
  [ "$(grep -c . got.ndjson)" = "5" ] \
    && say ok "...and every row lands in the file, once" || say FAIL "$(grep -c . got.ndjson) rows"
  [ "$(jq -r -s 'map(.build_id) | join(",")' < got.ndjson)" = "b0,b1,b2,c0,c1" ] \
    && say ok "...in the order of the batches, then the key" || say FAIL "$(jq -r -s 'map(.build_id) | join(",")' < got.ndjson)"
  CLAIMED=1 FACTORY_TOKEN=t fetch claimed.ndjson > out.txt 2>&1 || true
  [ "$(grep -c . claimed.ndjson 2>/dev/null)" = "1" ] && jq -e '.build_id == "b1"' < claimed.ndjson >/dev/null \
    && say ok "a claimed read keeps the rows the factory says are its" || say FAIL "$(cat out.txt; cat claimed.ndjson)"
  MAX_PAGES=2
  if fetch got.ndjson > out.txt 2>&1; then
    say FAIL "a read past its bound passed as complete"
  else
    grep -q "stopped at 2 pages" out.txt \
      && say ok "a read past its bound fails, and says so" || say FAIL "$(cat out.txt)"
  fi
  PAGE=5000 MAX_PAGES=2000

  cat > "$DIR/bin/db" <<'DEAD'
#!/usr/bin/env bash
printf '{"success":false,"errors":[{"code":"42501","message":"permission denied for table queue"}]}'
exit 1
DEAD
  chmod +x "$DIR/bin/db"
  # A REFUSED READ IS NOT AN EMPTY QUEUE. Reported as an error rather than as
  # "nothing owed", which a run would act on by scoring nothing and publishing.
  if fetch got.ndjson > out.txt 2>&1; then
    say FAIL "a refused read passed as an empty queue"
  else
    grep -q "permission denied" out.txt \
      && say ok "a refused read says what the database said" || say FAIL "$(cat out.txt)"
  fi

  unset PG_URL
  configured && say FAIL "unconfigured read as configured" \
    || say ok "no database configured is a working state"

  echo
  echo "$ok ok, $bad failed"
  [ "$bad" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then self_test; exit $?; fi
# THE STATEMENTS THEMSELVES, for check_board_verify.mjs to run against the schema.
if [ "${1:-}" = "--body" ]; then "${2:?claim|release|page}_body" "${@:3}"; exit $?; fi

# A RELEASE OR A CLAIM THAT FAILS FAILS THE RUN: a read without its claim is a
# list the clients are still being handed, which is every row fought twice.
if [ "${1:-}" = "--release" ]; then
  [ -n "${FACTORY_TOKEN:-}" ] || { echo "queue: no factory token, nothing to release"; exit 0; }
  release; exit $?
fi

if ! configured; then
  # NOTHING OWED IS A WORKING STATE, and it is what a machine with no
  # credentials should compute: nothing.
  : > "${1:?usage: fetch_queue.sh <out.ndjson>}"
  echo "queue: no database configured, nothing owed"
  exit 0
fi
if [ -n "${CLAIMED:-}" ]; then
  release
  claim
fi
fetch "${1:?usage: fetch_queue.sh <out.ndjson>}"
