#!/usr/bin/env bash
# THE LIBRARY, READ OUT OF THE DATABASE.
#
#   scripts/fetch_library.sh submissions.json
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

PAGE=2000
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

# ORDERED BY THE KEY, so paging is stable: two pages of an unordered query may
# overlap or skip, and a skipped build is one the board silently never holds.
page_body() {
  jq -n -c --argjson limit "$1" --argjson offset "$2" '
    {
      sql: ("SELECT identity, record FROM builds ORDER BY identity LIMIT ? OFFSET ?"),
      params: [$limit, $offset]
    }'
}

# ---- the read -------------------------------------------------------------
fetch() {
  local out="$1" offset=0 got total=0 page
  : > "$out.ndjson"
  for page in $(seq 1 "$MAX_PAGES"); do
    if ! d1 "$(page_body "$PAGE" "$offset")"; then
      echo "::error::library: the database refused the read [HTTP $D1_CODE]"
      [ -s "$D1_OUT" ] && { head -c 500 "$D1_OUT"; echo; }
      return 1
    fi
    got=$(jq -r '.result[0].results | length' < "$D1_OUT")
    # ONE RECORD A LINE, key beside value. `record` is stored as json TEXT, so
    # it is parsed here rather than handed on as a string.
    jq -c '.result[0].results[] | {k: .identity, v: (.record | fromjson)}' \
      < "$D1_OUT" >> "$out.ndjson"
    total=$((total + got))
    [ "$got" -lt "$PAGE" ] && break
    offset=$((offset + PAGE))
    if [ "$page" -eq "$MAX_PAGES" ]; then
      echo "::error::library: stopped at $MAX_PAGES pages — the library is larger than this reads"
      return 1
    fi
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
  DIR=$(mktemp -d)
  trap 'rm -rf "$DIR"' EXIT
  mkdir -p "$DIR/bin" "$DIR/work"
  say() { if [ "$1" = ok ]; then ok=$((ok + 1)); else bad=$((bad + 1)); fi; echo "  $1    $2"; }
  cd "$DIR/work"

  local body; body=$(page_body 10 20)
  printf '%s' "$body" | jq -e '.params == [10, 20]' >/dev/null \
    && say ok "the page and the offset are bound" \
    || say FAIL "params: $(printf '%s' "$body" | jq -c .params)"
  printf '%s' "$body" | jq -e '.sql | contains("ORDER BY identity")' >/dev/null \
    && say ok "...and the read is ORDERED, so two pages cannot overlap or skip" \
    || say FAIL "no ORDER BY"

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
  jq -n -c '{result:[{results:[range(0;3)|{identity:("k"+(.|tostring)),record:("{\"weapon\":\"w" + (.|tostring) + "\",\"at\":\"2026-01-01\"}")}],success:true}],success:true}' > "$out"
else
  jq -n -c '{result:[{results:[{identity:"last",record:"{\"weapon\":\"zzz\",\"at\":\"2026-01-02\"}"}],success:true}],success:true}' > "$out"
fi
printf '200'
PAGES
  chmod +x "$DIR/bin/curl"
  PAGE=3
  fetch submissions.json > out.txt 2>&1 && grep -q "4 builds" out.txt \
    && say ok "a library longer than one page is read whole" || say FAIL "$(cat out.txt)"
  [ "$(jq 'length' submissions.json)" = "4" ] \
    && say ok "...as the array the scorer takes" || say FAIL "$(jq 'length' submissions.json)"
  # THE RECORD IS PARSED, not handed on as the text the column holds: a scorer
  # given a string where a build should be refuses every row.
  jq -e '.[0].weapon == "w0"' < submissions.json >/dev/null \
    && say ok "...with each record parsed, not carried as text" \
    || say FAIL "$(head -c 120 submissions.json)"
  # …AND THE SNAPSHOT BESIDE IT, key and value, which is what a restore needs.
  jq -e -s '.[3].k == "last" and .[0].k == "k0"' < submissions.json.ndjson >/dev/null \
    && say ok "...and the snapshot carries the key beside each record" \
    || say FAIL "$(head -c 120 submissions.json.ndjson)"

  guard_shrink 3 submissions.json > /dev/null 2>&1 \
    && say ok "a library at its floor passes" || say FAIL "the tripwire fired on a full library"
  if guard_shrink 100 submissions.json > /dev/null 2>&1; then
    say FAIL "a truncated library was allowed to publish"
  else
    say ok "...and a short one refuses"
  fi

  cat > "$DIR/bin/curl" <<'DEAD'
#!/usr/bin/env bash
out=/dev/null; prev=
for a in "$@"; do [ "$prev" = "-o" ] && out="$a"; prev="$a"; done
printf '{"success":false,"errors":[{"code":7403,"message":"D1 not authorized"}]}' > "$out"
printf '403'
DEAD
  chmod +x "$DIR/bin/curl"
  if fetch submissions.json > out.txt 2>&1; then
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

OUT="${1:?the file to write, the array the scorer takes}"

# A DATABASE ID WITHOUT CREDENTIALS IS A MISTAKE, NOT AN ABSENCE: the id is
# committed in `wrangler.jsonc`, so its presence says this repo HAS a library.
if [ -n "${CF_D1_DATABASE:-}" ] && ! configured; then
  echo "::error::library: a database is declared but the credentials are not set"
  echo "::error::CF_ACCOUNT=${CF_ACCOUNT:+set}${CF_ACCOUNT:-MISSING} CF_TOKEN=${CF_TOKEN:+set}${CF_TOKEN:-MISSING}"
  exit 1
fi
if ! configured; then
  echo "no library database configured — see docs/BOARD.md §Setup"
  printf '[]' > "$OUT"
  exit 0
fi

fetch "$OUT"
guard_shrink "${FLOOR:-0}" "$OUT"
