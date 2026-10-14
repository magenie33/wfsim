#!/usr/bin/env bash
# THE FACTS, READ BACK OUT OF THE DATABASE.
#
#   scripts/fetch_facts.sh facts.ndjson
#   scripts/fetch_facts.sh --self-test
#
# The other half of `ship_facts.sh`. A scoring run asks what is already measured
# so it can compute the difference, and the assembly asks the same question so it
# can rank. One query, paged.
#
# THE SCORER NEVER SPEAKS TO THE DATABASE. It reads and writes files, and the
# database lives in scripts a stub transport can drive — which is what makes
# every hop here testable without a database.
set -euo pipefail

# A PAGE IS A ROUND TRIP through the tunnel, a process and a connection, so it
# is large: 50,000 rows is about ten megabytes of json.
#
# READ ONCE A RUN, NEVER ONCE A JOB. Thirty-two shards each asking the same
# question is thirty-two times the rows through one tunnel for one answer:
# about half a million a run. The submissions job reads it and hands it
# down as an artifact; only the ASSEMBLY reads again, because the shards were
# still shipping when that artifact was taken.
PAGE=50000

. "$(dirname "${BASH_SOURCE[0]}")/db.sh"
configured() { db_configured; }

# ---- the read -------------------------------------------------------------
#
# ORDERED BY THE KEY, so paging is stable. Without an ORDER BY two pages of one
# query may overlap or skip, and the gap is a row the run recomputes for ever —
# and an order that is not UNIQUE is the same bug wearing an ORDER BY, since
# rows that tie may come back in either order on either page. `(identity, ruler,
# mode)` is the primary key, so it is total.
#
# A PAGE STARTS AFTER THE LAST KEY, never at an OFFSET: an offset walks every
# row before it again, so a read of the whole table was quadratic in it. `$2`
# is the previous page's last key as a json array, empty for the first page.
#
# `metric` IS WRITTEN AND NOT READ BACK, deliberately. It says what a stored
# number is IN, which only a reader of the archive needs; nothing in a run
# decides on it, so selecting it would carry that weight on every row of every
# run for a question no caller asks.
page_body() {
  jq -n -c --argjson limit "$1" --argjson after "${2:-null}" '
    {
      sql: ("SELECT identity, ruler, mode, measured_by, score,"
            + " cost_seconds, started_at, finished_at FROM scores"
            + (if $after then " WHERE (identity, ruler, mode) > (?, ?, ?)" else "" end)
            + " ORDER BY identity, ruler, mode LIMIT ?"),
      params: (($after // []) + [$limit])
    }'
}

fetch() {
  local out="$1" after="" got total=0 last
  :
  while :; do
    if ! db_query "$(page_body "$PAGE" "$after")"; then
      echo "::error::facts: the database refused the read [HTTP $DB_CODE]"
      [ -s "$DB_OUT" ] && { head -c 500 "$DB_OUT"; echo; }
      return 1
    fi
    got=$(jq -r '.result[0].results | length' < "$DB_OUT")
    jq -c '.result[0].results[]' < "$DB_OUT" >> "$out"
    total=$((total + got))
    [ "$got" -lt "$PAGE" ] && break
    # A KEY THAT DID NOT MOVE is a page read again for ever: stop and say so.
    last=$(jq -c '.result[0].results[-1] | [.identity, .ruler, .mode]' < "$DB_OUT")
    if [ "$last" = "$after" ]; then
      echo "::error::facts: the page after $after started at the same key"
      return 1
    fi
    after="$last"
  done
  echo "facts: read $total rows"
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

  local body; body=$(page_body 10 '["k2","standard_single_target","base"]')
  printf '%s' "$body" | jq -e '.params == ["k2", "standard_single_target", "base", 10]' >/dev/null \
    && say ok "the last key and the page are both bound" \
    || say FAIL "params: $(printf '%s' "$body" | jq -c .params)"
  printf '%s' "$body" | jq -e '.sql | contains("WHERE (identity, ruler, mode) > (?, ?, ?)") and (contains("OFFSET") | not)' >/dev/null \
    && say ok "...a page starts after the last key, never at an offset" \
    || say FAIL "$(printf '%s' "$body" | jq -r .sql)"
  page_body 10 "" | jq -e '.params == [10] and (.sql | contains("WHERE") | not)' >/dev/null \
    && say ok "...and the first page has no key to start after" \
    || say FAIL "$(page_body 10 "")"
  printf '%s' "$body" | jq -e '.sql | contains("ORDER BY")' >/dev/null \
    && say ok "...and the read is ORDERED, so two pages cannot overlap or skip" \
    || say FAIL "no ORDER BY: $(printf '%s' "$body" | jq -r .sql)"
  # THE ORDER HAS TO BE THE KEY, and the key has to be TOTAL. Rows that tie may
  # come back in either order on either page, so an order finer than the key is
  # unnecessary and one coarser is the no-ORDER-BY bug with an ORDER BY on it.
  printf '%s' "$body" | jq -e '.sql | contains("ORDER BY identity, ruler, mode LIMIT")' >/dev/null \
    && say ok "...by the primary key, which is total, so no two rows can tie" \
    || say FAIL "$(printf '%s' "$body" | jq -r .sql)"
  printf '%s' "$body" | jq -e '.sql | contains("generation") | not' >/dev/null \
    && say ok "...and no generation is asked for: an older engine is not a wrong score" \
    || say FAIL "the statement still names a generation"

  export PATH="$DIR/bin:$PATH"
  export PG_URL=stub
  db_send() { "$DIR/bin/db"; }

  # TWO PAGES, so the paging is exercised rather than described: the first
  # answers a full page and the second the remainder.
  cat > "$DIR/bin/db" <<'PAGES'
#!/usr/bin/env bash
body=$(cat)
after=$(printf '%s' "$body" | jq -r '.params | if length == 1 then "" else .[0] end')
if [ -z "$after" ]; then
  jq -n -c '{result:[{results:[range(0;3)|{identity:("k"+(.|tostring)),ruler:"standard_single_target",mode:"base",measured_by:"abc",score:1.5,cost_seconds:2.0,started_at:"t0",finished_at:"t1"}],success:true}],success:true}'
elif [ "$after" = "k2" ]; then
  jq -n -c '{result:[{results:[{identity:"last",ruler:"standard_single_target",mode:"base",measured_by:"abc",score:9.0,cost_seconds:1.0,started_at:"t0",finished_at:"t1"}],success:true}],success:true}'
else
  printf '{"success":false,"errors":[{"code":"","message":"started after %s"}]}' "$after"
  exit 1
fi
PAGES
  chmod +x "$DIR/bin/db"
  PAGE=3
  fetch got.ndjson > out.txt 2>&1 && grep -q "read 4 rows" out.txt \
    && say ok "a table longer than one page is read whole" || say FAIL "$(cat out.txt)"
  [ "$(wc -l < got.ndjson | tr -d ' ')" = "4" ] \
    && say ok "...one line a row" || say FAIL "$(wc -l < got.ndjson) lines"
  jq -e -s '.[3].identity == "last" and .[0].identity == "k0"' < got.ndjson >/dev/null \
    && say ok "...and the second page is appended, not written over the first" \
    || say FAIL "$(head -c 200 got.ndjson)"

  cat > "$DIR/bin/db" <<'DEAD'
#!/usr/bin/env bash
printf '{"success":false,"errors":[{"code":"42501","message":"permission denied for table scores"}]}'
exit 1
DEAD
  chmod +x "$DIR/bin/db"
  if fetch got.ndjson > out.txt 2>&1; then
    say FAIL "a refused read passed"
  elif grep -q "permission denied" out.txt; then
    say ok "a refused read reports what the database said"
  else
    say FAIL "$(cat out.txt)"
  fi

  unset PG_URL
  configured && say FAIL "unconfigured read as configured" \
    || say ok "no database configured is a working state"

  echo
  echo "$ok ok, $bad failed"
  [ "$bad" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then self_test; exit $?; fi

# UNCONFIGURED IS AN EMPTY TABLE, not a failure: a run with no database
# computes everything, which is what it did before there was one.
if ! configured; then
  : > "${1:?the file to write}"
  echo "no database configured — no facts read (docs/BOARD.md §Setup)"
  exit 0
fi

fetch "${1:?the file to write}"
