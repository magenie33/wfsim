#!/usr/bin/env bash
# THE LIBRARY, READ OUT OF THE DATABASE.
#
#   scripts/fetch_library.sh library.json
#   scripts/fetch_library.sh --self-test
#
# Replaces a listing, a cache and a retry. KV had no bulk read and no index but
# a key listing, so reading the library was one HTTP request per build — 2,447
# of them at the API's four a second — and everything around it existed to avoid
# paying that twice: a cached `library.json`, a pruning pass, a three-try retry
# because a cold read runs AT the rate limit by construction, and a listing
# metered at a thousand operations a DAY that took the board down when it ran
# out. One query replaces all of it.
#
# IT ALSO WRITES THE SNAPSHOT, keys and all, because the two readers want the
# same rows: the scorer takes the values and the backup takes key-and-value.
# Reading twice would be two queries that can disagree.
set -euo pipefail

# A PAGE IS A ROUND TRIP through the tunnel, a process and a connection, so it
# is large: 10,000 records is about ten megabytes of json.
PAGE=10000

. "$(dirname "${BASH_SOURCE[0]}")/db.sh"
configured() { db_configured; }

# ORDERED BY THE KEY, so paging is stable: two pages of an unordered query may
# overlap or skip, and a skipped build is one the board silently never holds.
# A PAGE STARTS AFTER THE LAST ID, never at an OFFSET, which walks every row
# before it again. `$2` is the previous page's last id, empty for the first.
page_body() {
  jq -n -c --argjson limit "$1" --arg after "${2:-}" '
    {
      sql: ("SELECT id, record FROM builds"
            + (if $after != "" then " WHERE id > ?" else "" end)
            + " ORDER BY id LIMIT ?"),
      params: ((if $after != "" then [$after] else [] end) + [$limit])
    }'
}

# ---- the read -------------------------------------------------------------
fetch() {
  local out="$1" after="" got total=0 last
  : > "$out.ndjson"
  while :; do
    if ! db_query "$(page_body "$PAGE" "$after")"; then
      echo "::error::library: the database refused the read [HTTP $DB_CODE]"
      [ -s "$DB_OUT" ] && { head -c 500 "$DB_OUT"; echo; }
      return 1
    fi
    got=$(jq -r '.result[0].results | length' < "$DB_OUT")
    # ONE RECORD A LINE, key beside value. `record` is stored as json TEXT, so
    # it is parsed here rather than handed on as a string.
    jq -c '.result[0].results[] | {k: .id, v: (.record | fromjson)}' \
      < "$DB_OUT" >> "$out.ndjson"
    total=$((total + got))
    [ "$got" -lt "$PAGE" ] && break
    # AN ID THAT DID NOT MOVE is a page read again for ever: stop and say so.
    last=$(jq -r '.result[0].results[-1].id' < "$DB_OUT")
    if [ "$last" = "$after" ]; then
      echo "::error::library: the page after $after started at the same id"
      return 1
    fi
    after="$last"
  done
  jq -s '[.[].v]' "$out.ndjson" > "$out"
  echo "library: $total builds"
}

# ---- the library may not SHRINK by surprise --------------------------------
#
# THE ONE IRREPLACEABLE THING HERE. The boards are derived, the site is
# generated, the code is in git; the library is what players sent and there is
# no other copy of it in the live system.
#
# The way it is destroyed is not a delete, it is a QUIET TRUNCATION: a short
# read publishes a board built from a short list, and every row missing from it
# is gone with nothing saying so. A run that comes back materially short
# REFUSES. Ninety percent, because records do leave — and losing a tenth of
# them between two runs minutes apart is not attrition.
guard_shrink() {
  local floor="${1:-0}" have
  have=$(jq 'length' "$2")
  if [ "$floor" -gt 0 ] && [ "$have" -lt "$floor" ]; then
    echo "::error::the library came back with $have builds, under the floor of $floor."
    echo "::error::refusing to publish a board from a short list — see scripts/fetch_library.sh."
    return 1
  fi
  echo "library floor: $have >= ${floor:-0}"
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

  local body; body=$(page_body 10 k2)
  printf '%s' "$body" | jq -e '.params == ["k2", 10]' >/dev/null \
    && say ok "the last id and the page are bound" \
    || say FAIL "params: $(printf '%s' "$body" | jq -c .params)"
  printf '%s' "$body" | jq -e '.sql | contains("WHERE id > ?") and (contains("OFFSET") | not)' >/dev/null \
    && say ok "...a page starts after the last id, never at an offset" \
    || say FAIL "$(printf '%s' "$body" | jq -r .sql)"
  page_body 10 "" | jq -e '.params == [10] and (.sql | contains("WHERE") | not)' >/dev/null \
    && say ok "...and the first page has no id to start after" \
    || say FAIL "$(page_body 10 "")"
  printf '%s' "$body" | jq -e '.sql | contains("ORDER BY id")' >/dev/null \
    && say ok "...and the read is ORDERED, so two pages cannot overlap or skip" \
    || say FAIL "no ORDER BY"

  export PATH="$DIR/bin:$PATH"
  export PG_URL=stub
  db_send() { "$DIR/bin/db"; }

  # TWO PAGES, so the paging is exercised rather than described.
  cat > "$DIR/bin/db" <<'PAGES'
#!/usr/bin/env bash
after=$(jq -r '.params | if length == 1 then "" else .[0] end')
if [ -z "$after" ]; then
  jq -n -c '{result:[{results:[range(0;3)|{id:("k"+(.|tostring)),record:("{\"weapon\":\"w" + (.|tostring) + "\",\"at\":\"2026-01-01\"}")}],success:true}],success:true}'
elif [ "$after" = "k2" ]; then
  jq -n -c '{result:[{results:[{id:"last",record:"{\"weapon\":\"zzz\",\"at\":\"2026-01-02\"}"}],success:true}],success:true}'
else
  printf '{"success":false,"errors":[{"code":"","message":"started after %s"}]}' "$after"
  exit 1
fi
PAGES
  chmod +x "$DIR/bin/db"
  PAGE=3
  fetch library.json > out.txt 2>&1 && grep -q "4 builds" out.txt \
    && say ok "a library longer than one page is read whole" || say FAIL "$(cat out.txt)"
  [ "$(jq 'length' library.json)" = "4" ] \
    && say ok "...as the array the scorer takes" || say FAIL "$(jq 'length' library.json)"
  # THE RECORD IS PARSED, not handed on as the text the column holds: a scorer
  # given a string where a build should be refuses every row.
  jq -e '.[0].weapon == "w0"' < library.json >/dev/null \
    && say ok "...with each record parsed, not carried as text" \
    || say FAIL "$(head -c 120 library.json)"
  # …AND THE SNAPSHOT BESIDE IT, key and value, which is what a restore needs.
  jq -e -s '.[3].k == "last" and .[0].k == "k0"' < library.json.ndjson >/dev/null \
    && say ok "...and the snapshot carries the key beside each record" \
    || say FAIL "$(head -c 120 library.json.ndjson)"

  guard_shrink 3 library.json > /dev/null 2>&1 \
    && say ok "a library at its floor passes" || say FAIL "the tripwire fired on a full library"
  if guard_shrink 100 library.json > /dev/null 2>&1; then
    say FAIL "a truncated library was allowed to publish"
  else
    say ok "...and a short one refuses"
  fi

  cat > "$DIR/bin/db" <<'DEAD'
#!/usr/bin/env bash
printf '{"success":false,"errors":[{"code":"42501","message":"permission denied for table builds"}]}'
exit 1
DEAD
  chmod +x "$DIR/bin/db"
  if fetch library.json > out.txt 2>&1; then
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

OUT="${1:?the file to write, the array the scorer takes}"

if ! configured; then
  echo "no library database configured — see docs/BOARD.md §Setup"
  printf '[]' > "$OUT"
  exit 0
fi

fetch "$OUT"
guard_shrink "${FLOOR:-0}" "$OUT"
