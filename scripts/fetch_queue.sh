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
# network lives in scripts a stub `curl` can drive.
set -euo pipefail

# A PAGE, AND THE LOOP IS BOUNDED. The queue is EMPTY at rest — it holds only
# what is still owed — so a full page is the unusual case: a fresh library, or a
# rescore somebody just asked for.
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
# ORDERED TOTALLY, so paging is stable. `batches.at` sets which group goes
# first; the primary key breaks every tie under it, and without that last part
# two pages of one query may overlap or skip — a row the run then owes for ever.
#
# A ROW WHOSE BATCH IS GONE IS NOT READ. An inner join, so deleting a batch is
# the whole of cancelling it: the rows stop being asked for and nothing has to
# go and find them.
page_body() {
  jq -n -c --argjson limit "$1" --argjson offset "$2" '
    {
      sql: ("SELECT q.batch, q.build_id, q.ruler, q.mode FROM queue q"
            + " JOIN batches b ON b.id = q.batch"
            + " ORDER BY b.at, q.batch, q.build_id, q.ruler, q.mode"
            + " LIMIT ? OFFSET ?"),
      params: [$limit, $offset]
    }'
}

fetch() {
  local out="$1" offset=0 got total=0 page
  : > "$out"
  for page in $(seq 1 "$MAX_PAGES"); do
    if ! d1 "$(page_body "$PAGE" "$offset")"; then
      echo "::error::queue: the database refused the read [HTTP $D1_CODE]"
      [ -s "$D1_OUT" ] && { head -c 500 "$D1_OUT"; echo; }
      return 1
    fi
    got=$(jq -r '.result[0].results | length' < "$D1_OUT")
    jq -c '.result[0].results[]' < "$D1_OUT" >> "$out"
    total=$((total + got))
    [ "$got" -lt "$PAGE" ] && break
    offset=$((offset + PAGE))
    if [ "$page" -eq "$MAX_PAGES" ]; then
      echo "::error::queue: stopped at $MAX_PAGES pages — more is owed than this reads"
      return 1
    fi
  done
  echo "queue: $total row(s) owed"
}

# ---- self-test ------------------------------------------------------------
self_test() {
  ok=0; bad=0
  DIR=$(mktemp -d)
  trap 'rm -rf "$DIR"' EXIT
  mkdir -p "$DIR/bin" "$DIR/work"
  say() { if [ "$1" = ok ]; then ok=$((ok + 1)); else bad=$((bad + 1)); fi; echo "  $1    $2"; }
  cd "$DIR/work"

  local body; body=$(page_body 5000 0)
  printf '%s' "$body" | jq -e '.sql | contains("ORDER BY b.at, q.batch, q.build_id, q.ruler, q.mode")' >/dev/null \
    && say ok "the order is the batch's, then the key" \
    || say FAIL "$(printf '%s' "$body" | jq -r .sql)"
  printf '%s' "$body" | jq -e '.sql | contains("JOIN batches")' >/dev/null \
    && say ok "...and a row whose batch is gone is not read" \
    || say FAIL "no join — deleting a batch would leave its rows owed"
  printf '%s' "$body" | jq -e '.params == [5000, 0]' >/dev/null \
    && say ok "...and the page is bound, not interpolated" \
    || say FAIL "$(printf '%s' "$body" | jq -c .params)"

  export PATH="$DIR/bin:$PATH"
  export CF_ACCOUNT=a CF_D1_DATABASE=d CF_TOKEN=t
  cat > "$DIR/bin/curl" <<'OK'
#!/usr/bin/env bash
out=/dev/null; prev=
for a in "$@"; do [ "$prev" = "-o" ] && out="$a"; prev="$a"; done
n=$(cat "$PWD/page.n" 2>/dev/null || echo 0)
echo $((n + 1)) > "$PWD/page.n"
if [ "$n" = "0" ]; then
  jq -n -c '{result:[{results:[range(0;3)|{batch:"arrivals",build_id:("b"+(.|tostring)),ruler:"single_target",mode:"base"}],success:true}],success:true}' > "$out"
else
  jq -n -c '{result:[{results:[],success:true}],success:true}' > "$out"
fi
printf '200'
OK
  chmod +x "$DIR/bin/curl"
  rm -f page.n
  fetch got.ndjson > out.txt 2>&1 \
    && say ok "a read pages until the answer runs out" || say FAIL "$(cat out.txt)"
  [ "$(grep -c . got.ndjson)" = "3" ] \
    && say ok "...and every row lands in the file" || say FAIL "$(grep -c . got.ndjson) rows"
  jq -e -s '.[0].batch == "arrivals" and .[0].build_id == "b0"' < got.ndjson >/dev/null \
    && say ok "...in the order the database gave them" || say FAIL "$(head -1 got.ndjson)"

  cat > "$DIR/bin/curl" <<'DEAD'
#!/usr/bin/env bash
out=/dev/null; prev=
for a in "$@"; do [ "$prev" = "-o" ] && out="$a"; prev="$a"; done
printf '{"success":false,"errors":[{"code":7403,"message":"not authorized"}]}' > "$out"
printf '403'
DEAD
  chmod +x "$DIR/bin/curl"
  # A REFUSED READ IS NOT AN EMPTY QUEUE. Reported as an error rather than as
  # "nothing owed", which a run would act on by scoring nothing and publishing.
  if fetch got.ndjson > out.txt 2>&1; then
    say FAIL "a refused read passed as an empty queue"
  else
    grep -q "not authorized" out.txt \
      && say ok "a refused read says what the database said" || say FAIL "$(cat out.txt)"
  fi

  unset CF_D1_DATABASE
  configured && say FAIL "unconfigured read as configured" \
    || say ok "no database configured is a working state"

  echo
  echo "$ok ok, $bad failed"
  [ "$bad" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then self_test; exit $?; fi

if ! configured; then
  # NOTHING OWED IS A WORKING STATE, and it is what a machine with no
  # credentials should compute: nothing.
  : > "${1:?usage: fetch_queue.sh <out.ndjson>}"
  echo "queue: no database configured, nothing owed"
  exit 0
fi
fetch "${1:?usage: fetch_queue.sh <out.ndjson>}"
