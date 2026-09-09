#!/usr/bin/env bash
# WHAT HAS ARRIVED AND NOT BEEN THROUGH INTAKE YET.
#
#   scripts/fetch_inbox.sh inbox.ndjson
#   scripts/fetch_inbox.sh --self-test
#
# The door stores a submission verbatim, because it has no game data and cannot
# say what a build IS. This reads those rows back for `wfsim-intake`, which has
# the engine.
#
# THE SCORER NEVER SPEAKS TO THE DATABASE — nor does intake. They read and write
# files, and the network lives in scripts a stub `curl` can drive, which is what
# makes every hop here testable without one.
set -euo pipefail

# A PAGE, AND THE LOOP IS BOUNDED. The queue is small by construction — it is
# emptied every run — so one page is almost always the whole of it, and a table
# larger than this reads is a finding rather than a page to fetch quietly.
PAGE=5000
MAX_PAGES=50

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
# ORDERED BY THE KEY, so paging is stable: without an ORDER BY two pages of one
# query may overlap or skip, and an order that is not UNIQUE is the same bug
# wearing an ORDER BY. `id` is the primary key, so it is total.
#
# THE RECORD COMES BACK AS TEXT and is handed on as text. Intake parses either,
# and parsing here would only give the shell a chance to reshape a record whose
# whole point is that it arrived untouched.
page_body() {
  jq -n -c --argjson limit "$1" --argjson offset "$2" '
    {
      sql: ("SELECT id, at, record FROM inbox ORDER BY id LIMIT ? OFFSET ?"),
      params: [$limit, $offset]
    }'
}

fetch() {
  local out="$1" offset=0 got total=0 page
  : > "$out"
  for page in $(seq 1 "$MAX_PAGES"); do
    if ! d1 "$(page_body "$PAGE" "$offset")"; then
      echo "::error::inbox: the database refused the read [HTTP $D1_CODE]"
      [ -s "$D1_OUT" ] && { head -c 500 "$D1_OUT"; echo; }
      return 1
    fi
    got=$(jq -r '.result[0].results | length' < "$D1_OUT")
    jq -c '.result[0].results[]' < "$D1_OUT" >> "$out"
    total=$((total + got))
    [ "$got" -lt "$PAGE" ] && break
    offset=$((offset + PAGE))
    if [ "$page" -eq "$MAX_PAGES" ]; then
      echo "::error::inbox: stopped at $MAX_PAGES pages — the queue is larger than this reads"
      return 1
    fi
  done
  echo "inbox: read $total record(s)"
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
  printf '%s' "$body" | jq -e '.sql | contains("ORDER BY id LIMIT")' >/dev/null \
    && say ok "...and the read is ordered by the key, which is total" \
    || say FAIL "$(printf '%s' "$body" | jq -r .sql)"

  export PATH="$DIR/bin:$PATH"
  export CF_ACCOUNT=a CF_D1_DATABASE=d CF_TOKEN=t

  # TWO PAGES, so the paging is exercised rather than described.
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
  jq -n -c '{result:[{results:[range(0;3)|{id:("u"+(.|tostring)),at:"2026-01-01",record:"{\"weapon\":\"torid\"}"}],success:true}],success:true}' > "$out"
else
  jq -n -c '{result:[{results:[{id:"last",at:"2026-01-02",record:"{\"weapon\":\"boar\"}"}],success:true}],success:true}' > "$out"
fi
printf '200'
PAGES
  chmod +x "$DIR/bin/curl"
  PAGE=3
  fetch got.ndjson > out.txt 2>&1 && grep -q "read 4 record" out.txt \
    && say ok "a queue longer than one page is read whole" || say FAIL "$(cat out.txt)"
  jq -e -s '.[3].id == "last" and .[0].id == "u0"' < got.ndjson >/dev/null \
    && say ok "...and the second page is appended, not written over the first" \
    || say FAIL "$(head -c 200 got.ndjson)"
  # THE RECORD STAYS TEXT. A shell that parsed it could reshape the one thing
  # this table exists to keep untouched.
  jq -e -s '.[0].record | type == "string"' < got.ndjson >/dev/null \
    && say ok "...with the record handed on exactly as it was stored" \
    || say FAIL "the record was reshaped on the way through"

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

# UNCONFIGURED IS AN EMPTY QUEUE, not a failure: a run with no database is the
# pipeline as it was before the door existed, and it still scores what is held.
if ! configured; then
  echo "inbox: no database configured, nothing to take in"
  : > "${1:?usage: fetch_inbox.sh <out.ndjson>}"
  exit 0
fi
fetch "${1:?usage: fetch_inbox.sh <out.ndjson>}"
