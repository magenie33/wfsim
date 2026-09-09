#!/usr/bin/env bash
# WHAT INTAKE MADE OF THE QUEUE, PUT IN THE LIBRARY.
#
#   scripts/ship_builds.sh builds.ndjson done.txt
#   scripts/ship_builds.sh --self-test
#
# `wfsim-intake` reads the inbox and writes two things: the builds it derived,
# and the queue ids it has finished with — which includes the records it
# REFUSED, since a record nobody could equip will never become legal and a queue
# that keeps what it cannot use grows for ever.
#
# THE BUILDS LAND FIRST AND THE QUEUE IS SPENT SECOND. Stopped in between, the
# inbox rows are still there and the next run derives the same builds from them:
# the id is a hash of the build, so writing one twice writes one row. The other
# order loses a submission the moment a write fails.
set -euo pipefail

# AS MANY ROWS A STATEMENT AS D1'S HUNDRED BOUND PARAMETERS ALLOW, and both
# counts are DERIVED so that a column added below shrinks the batch rather than
# putting every write one parameter over the limit.
BUILD_COLUMNS=3
BUILD_BATCH=$((100 / BUILD_COLUMNS))
DELETE_BATCH=100

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
# PARAMETERS ARE BOUND, NEVER INTERPOLATED. A record is built from ids that
# arrived at a public endpoint, and this is where that text meets a language.
#
# `INSERT OR REPLACE` on the build's own id: the id is a hash of what makes the
# build one, so a build derived twice is one row and there is nothing to ask the
# table first. The record it replaces is the same record.
#
# `from` IS NOT WRITTEN. It says which queue rows produced this build in THIS
# pass, which is true of the pass and not of the build — it is what the deletes
# below are for.
build_batches() {
  jq -s -c --argjson n "$BUILD_BATCH" '
    . as $all
    | range(0; ($all | length); $n)
    | . as $i
    | $all[$i : $i + $n] as $chunk
    | {
        sql: ("INSERT OR REPLACE INTO builds (id, at, record) VALUES "
              + ([$chunk[] | "(?,?,?)"] | join(","))),
        params: [$chunk[] | .id, .at, (.record | tojson)]
      }
  ' "$1"
}

delete_batches() {
  jq -R -s -c --argjson n "$DELETE_BATCH" '
    [splits("\n")] | map(select(length > 0)) as $all
    | range(0; ($all | length); $n)
    | . as $i
    | $all[$i : $i + $n] as $chunk
    | {
        sql: ("DELETE FROM inbox WHERE id IN ("
              + ([$chunk[] | "?"] | join(",")) + ")"),
        params: $chunk
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
      # THE DIAGNOSIS GOES TO STDERR. This function's stdout is the two
      # counters its caller reads, and a message on the same channel is read as
      # a count — which is how a refusal came back as "4 builds in ::error::
      # statements".
      if [ "$failed" -eq 1 ]; then
        {
          echo "::error::builds: the database refused a $what [HTTP $D1_CODE]"
          head -c 500 "$D1_OUT" || true
          echo
        } >&2
      fi
    fi
  done
  echo "$sent $failed"
}

ship() {
  local builds="$1" done_ids="$2"
  local n_builds; n_builds=$(grep -c . < "$builds" 2>/dev/null || echo 0)
  local n_done; n_done=$(grep -c . < "$done_ids" 2>/dev/null || echo 0)
  if [ "$n_builds" = "0" ] && [ "$n_done" = "0" ]; then
    echo "builds: nothing to ship"
    return 0
  fi

  local r
  if [ "$n_builds" != "0" ]; then
    r=$(build_batches "$builds" | send "build")
    set -- $r
    echo "builds: $n_builds build(s) in $1 statement(s), $2 refused"
    # THE QUEUE IS NOT SPENT WHILE A BUILD IS MISSING. Deleting here would drop
    # a submission nothing in the library holds.
    [ "$2" = "0" ] || return 1
  fi
  if [ "$n_done" != "0" ]; then
    r=$(delete_batches "$done_ids" | send "delete")
    set -- $r
    echo "inbox: $n_done row(s) spent in $1 statement(s), $2 refused"
    [ "$2" = "0" ] || return 1
  fi
}

# ---- self-test ------------------------------------------------------------
self_test() {
  ok=0; bad=0
  DIR=$(mktemp -d)
  trap 'rm -rf "$DIR"' EXIT
  mkdir -p "$DIR/bin" "$DIR/work"
  say() { if [ "$1" = ok ]; then ok=$((ok + 1)); else bad=$((bad + 1)); fi; echo "  $1    $2"; }
  cd "$DIR/work"

  : > builds.ndjson
  for i in 1 2 3 4; do
    jq -n -c --arg i "$i" '{id:("h"+$i),at:"2026-01-01",record:{weapon:"torid",mods:["serration"]},from:[("u"+$i)]}' \
      >> builds.ndjson
  done
  printf 'u1\nu2\nu3\nu4\nu5\n' > done.txt

  local first; first=$(build_batches builds.ndjson | head -1)
  [ "$(printf '%s' "$first" | jq -r '.params | length')" = "$((4 * BUILD_COLUMNS))" ] \
    && say ok "$BUILD_COLUMNS bound parameters a build" \
    || say FAIL "$(printf '%s' "$first" | jq -r '.params|length') parameters"
  [ $((BUILD_BATCH * BUILD_COLUMNS)) -le 100 ] \
    && say ok "...and a batch stays inside D1's hundred" \
    || say FAIL "$((BUILD_BATCH * BUILD_COLUMNS)) is over"
  # THE RECORD IS BOUND AS TEXT, which is what the column holds. Handed over as
  # an object it would arrive as the string `[object Object]` or be refused.
  printf '%s' "$first" | jq -e '.params[2] | type == "string" and contains("torid")' >/dev/null \
    && say ok "...and the record is bound as the json text the column takes" \
    || say FAIL "$(printf '%s' "$first" | jq -c '.params[2]')"
  printf '%s' "$first" | jq -e '.sql | contains("torid") | not' >/dev/null \
    && say ok "...never interpolated into the statement" \
    || say FAIL "a record reached the sql"
  # `from` IS FOR THE DELETES, NOT FOR THE TABLE.
  printf '%s' "$first" | jq -e '.sql | contains("from") | not' >/dev/null \
    && say ok "...and the pass's own bookkeeping is not written down" \
    || say FAIL "`from` reached the statement"

  local del; del=$(delete_batches done.txt | head -1)
  printf '%s' "$del" | jq -e '.params == ["u1","u2","u3","u4","u5"]' >/dev/null \
    && say ok "every spent queue row is named, refusals included" \
    || say FAIL "$(printf '%s' "$del" | jq -c .params)"

  export PATH="$DIR/bin:$PATH"
  export CF_ACCOUNT=a CF_D1_DATABASE=d CF_TOKEN=t
  cat > "$DIR/bin/curl" <<'OK'
#!/usr/bin/env bash
out=/dev/null; prev=; body=
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
  ship builds.ndjson done.txt > out.txt 2>&1 \
    && say ok "a pass ships its builds and spends its queue" || say FAIL "$(cat out.txt)"
  # THE ORDER IS THE WHOLE SAFETY ARGUMENT. Stopped between the two, the queue
  # still holds what the library is missing.
  grep -n "INSERT OR REPLACE INTO builds" sent.log | head -1 | cut -d: -f1 > a.txt
  grep -n "DELETE FROM inbox" sent.log | head -1 | cut -d: -f1 > b.txt
  [ "$(cat a.txt)" -lt "$(cat b.txt)" ] \
    && say ok "...with every build written before any queue row is spent" \
    || say FAIL "the delete went first"

  cat > "$DIR/bin/curl" <<'DEAD'
#!/usr/bin/env bash
out=/dev/null; prev=
for a in "$@"; do [ "$prev" = "-o" ] && out="$a"; prev="$a"; done
printf '{"success":false,"errors":[{"code":7500,"message":"no such table"}]}' > "$out"
printf '500'
DEAD
  chmod +x "$DIR/bin/curl"
  : > sent.log
  if ship builds.ndjson done.txt > out.txt 2>&1; then
    say FAIL "a refused write passed"
  else
    grep -q "no such table" out.txt \
      && say ok "a refused write reports what the database said" || say FAIL "$(cat out.txt)"
    grep -q "DELETE FROM inbox" sent.log \
      && say FAIL "the queue was spent after a refused write" \
      || say ok "...and nothing in the queue is spent"
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
  echo "builds: no database configured, nothing shipped"
  exit 0
fi
ship "${1:?usage: ship_builds.sh <builds.ndjson> <done.txt>}" \
     "${2:?usage: ship_builds.sh <builds.ndjson> <done.txt>}"
