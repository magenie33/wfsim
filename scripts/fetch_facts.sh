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
# network lives in scripts that a stub `curl` can drive — which is what makes
# every hop here testable without a network.
set -euo pipefail

# A PAGE, AND THE LOOP IS BOUNDED. D1 answers a query whole, so the page is
# about the size of the answer rather than about a cursor: 5,000 rows is a few
# megabytes of json, and the table is about 22,656 rows once every row of the
# library has been measured.
#
# READ ONCE A RUN, NEVER ONCE A JOB. D1 meters rows READ — five million a day on
# the free plan — and thirty-two shards each asking the same question is
# thirty-two times the rows for one answer: about half a million a run, which
# spends the day's allowance in ten. The submissions job reads it and hands it
# down as an artifact; only the ASSEMBLY reads again, because the shards were
# still shipping when that artifact was taken.
PAGE=5000
MAX_PAGES=200

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

# ---- the read -------------------------------------------------------------
#
# ORDERED BY THE KEY, so paging is stable. Without an ORDER BY two pages of one
# query may overlap or skip, and the gap is a row the run recomputes for ever —
# and an order that is not UNIQUE is the same bug wearing an ORDER BY, since
# rows that tie may come back in either order on either page. `(identity, ruler,
# mode)` is the primary key, so it is total.
#
# `metric` IS WRITTEN AND NOT READ BACK, deliberately. It says what a stored
# number is IN, which only a reader of the archive needs; nothing in a run
# decides on it, so selecting it would carry that weight on every row of every
# run for a question no caller asks.
page_body() {
  jq -n -c --argjson limit "$1" --argjson offset "$2" '
    {
      sql: ("SELECT identity, ruler, mode, measured_by, score, rolls,"
            + " cost_seconds, started_at, finished_at FROM scores"
            + " ORDER BY identity, ruler, mode LIMIT ? OFFSET ?"),
      params: [$limit, $offset]
    }'
}

fetch() {
  local out="$1" offset=0 got total=0 page
  : > "$out"
  for page in $(seq 1 "$MAX_PAGES"); do
    if ! d1 "$(page_body "$PAGE" "$offset")"; then
      echo "::error::facts: the database refused the read [HTTP $D1_CODE]"
      [ -s "$D1_OUT" ] && { head -c 500 "$D1_OUT"; echo; }
      return 1
    fi
    got=$(jq -r '.result[0].results | length' < "$D1_OUT")
    jq -c '.result[0].results[]' < "$D1_OUT" >> "$out"
    total=$((total + got))
    [ "$got" -lt "$PAGE" ] && break
    offset=$((offset + PAGE))
    # THE LOOP IS BOUNDED AND SAYS SO. A table larger than this is a finding,
    # not a page to fetch quietly.
    if [ "$page" -eq "$MAX_PAGES" ]; then
      echo "::error::facts: stopped at $MAX_PAGES pages — the table is larger than this reads"
      return 1
    fi
  done
  echo "facts: read $total rows"
}

# ---- self-test ------------------------------------------------------------
self_test() {
  ok=0; bad=0
  DIR=$(mktemp -d)
  trap 'rm -rf "$DIR"' EXIT
  mkdir -p "$DIR/bin" "$DIR/work"
  say() { if [ "$1" = ok ]; then ok=$((ok + 1)); else bad=$((bad + 1)); fi; echo "  $1    $2"; }
  cd "$DIR/work"

  local body; body=$(page_body 10 20)
  printf '%s' "$body" | jq -e '.params == [10, 20]' >/dev/null \
    && say ok "the page and the offset are both bound" \
    || say FAIL "params: $(printf '%s' "$body" | jq -c .params)"
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
  export CF_ACCOUNT=a CF_D1_DATABASE=d CF_TOKEN=t

  # TWO PAGES, so the paging is exercised rather than described: the first
  # answers a full page and the second the remainder.
  cat > "$DIR/bin/curl" <<'PAGES'
#!/usr/bin/env bash
out=/dev/null; prev=; body=
for a in "$@"; do
  [ "$prev" = "-o" ] && out="$a"
  [ "$prev" = "--data-binary" ] && body="$a"
  prev="$a"
done
off=$(printf '%s' "$body" | jq -r '.params[1]')
if [ "$off" = "0" ]; then
  jq -n -c '{result:[{results:[range(0;3)|{identity:("k"+(.|tostring)),ruler:"single_target",mode:"base",measured_by:"abc",score:1.5,rolls:null,cost_seconds:2.0,started_at:"t0",finished_at:"t1"}],success:true}],success:true}' > "$out"
else
  jq -n -c '{result:[{results:[{identity:"last",ruler:"single_target",mode:"base",measured_by:"abc",score:9.0,rolls:null,cost_seconds:1.0,started_at:"t0",finished_at:"t1"}],success:true}],success:true}' > "$out"
fi
printf '200'
PAGES
  chmod +x "$DIR/bin/curl"
  PAGE=3
  fetch got.ndjson > out.txt 2>&1 && grep -q "read 4 rows" out.txt \
    && say ok "a table longer than one page is read whole" || say FAIL "$(cat out.txt)"
  [ "$(wc -l < got.ndjson | tr -d ' ')" = "4" ] \
    && say ok "...one line a row" || say FAIL "$(wc -l < got.ndjson) lines"
  jq -e -s '.[3].identity == "last" and .[0].identity == "k0"' < got.ndjson >/dev/null \
    && say ok "...and the second page is appended, not written over the first" \
    || say FAIL "$(head -c 200 got.ndjson)"

  cat > "$DIR/bin/curl" <<'DEAD'
#!/usr/bin/env bash
out=/dev/null; prev=
for a in "$@"; do [ "$prev" = "-o" ] && out="$a"; prev="$a"; done
printf '{"success":false,"errors":[{"code":7403,"message":"D1 not authorized"}]}' > "$out"
printf '403'
DEAD
  chmod +x "$DIR/bin/curl"
  if fetch got.ndjson > out.txt 2>&1; then
    say FAIL "a refused read passed"
  elif grep -q "D1 not authorized" out.txt; then
    say ok "a refused read reports what the database said"
  else
    say FAIL "$(cat out.txt)"
  fi

  unset CF_D1_DATABASE
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
