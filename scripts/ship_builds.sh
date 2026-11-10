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

SHIP_HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SHIP_FAMILY="$SHIP_HERE/ship_family.mjs"

. "$(dirname "${BASH_SOURCE[0]}")/db.sh"
configured() { db_configured; }

# ---- the statements -------------------------------------------------------
#
# PARAMETERS ARE BOUND, NEVER INTERPOLATED. A record is built from ids that
# arrived at a public endpoint, and this is where that text meets a language.
#
# AN UPSERT on the build's own id: the id is a hash of what makes the
# build one, so a build derived twice is one row and there is nothing to ask the
# table first. The record it replaces is the same record.
#
# `from` IS NOT WRITTEN. It says which queue rows produced this build in THIS
# pass, which is true of the pass and not of the build — it is what the deletes
# below are for.
build_batches() {
  command node "$SHIP_HERE/build_batches.mjs" builds "$1"
}

delete_batches() {
  command node "$SHIP_HERE/build_batches.mjs" deletes "$1"
}

# These upserts and inbox deletes are idempotent even if a cut lost the reply.
# Other database writes retain db.mjs's rule against automatic replay.
send_idempotent() {
  local body="$1" attempt=1 code delay="${SHIP_RETRY_DELAY:-2}"
  [[ "$delay" =~ ^[0-9]+$ ]] || { echo "builds: invalid retry delay" >&2; return 1; }
  while ! db_query "$body"; do
    code=$(jq -r '.errors[0].code // ""' "$DB_OUT" 2>/dev/null) || code=""
    case "$code" in
      CONNECTION_CLOSED|CONNECTION_ENDED|CONNECTION_DESTROYED|CONNECT_TIMEOUT|ECONNRESET|ECONNREFUSED|EPIPE) ;;
      *) return 1 ;;
    esac
    [ "$attempt" -lt 3 ] || return 1
    echo "builds: retrying an idempotent write after $code (attempt $((attempt + 1))/3)" >&2
    sleep "$((delay * attempt))"
    attempt=$((attempt + 1))
  done
}

send() {
  local what="$1" sent=0 failed=0 body
  while IFS= read -r body; do
    [ -n "$body" ] || continue
    if send_idempotent "$body"; then
      sent=$((sent + 1))
    else
      failed=$((failed + 1))
      # THE DIAGNOSIS GOES TO STDERR. This function's stdout is the two
      # counters its caller reads, and a message on the same channel is read as
      # a count — which is how a refusal came back as "4 builds in ::error::
      # statements".
      if [ "$failed" -eq 1 ]; then
        {
          echo "::error::entity_weapon_builds: the database refused a $what [HTTP $DB_CODE]"
          head -c 500 "$DB_OUT" || true
          echo
        } >&2
      fi
      break
    fi
  done
  echo "$sent $failed"
}

ship() {
  local builds="$1" done_ids="$2"
  # `grep -c` PRINTS ZERO AND EXITS 1 on an empty file, so a `|| echo 0` after
  # it appends a SECOND zero: the count reads as two lines and never equals "0".
  local n_builds; n_builds=$(grep -c . < "$builds" 2>/dev/null) || n_builds=0
  local n_done; n_done=$(grep -c . < "$done_ids" 2>/dev/null) || n_done=0
  if [ "$n_builds" = "0" ] && [ "$n_done" = "0" ]; then
    echo "builds: nothing to ship"
    return 0
  fi

  local r
  if [ "$n_builds" != "0" ]; then
    r=$(build_batches "$builds" | send "build") || return 1
    set -- $r
    echo "builds: $n_builds build(s) in $1 statement(s), $2 refused"
    # THE QUEUE IS NOT SPENT WHILE A BUILD IS MISSING. Deleting here would drop
    # a submission nothing in the library holds.
    [ "$2" = "0" ] || return 1
  fi
  node "$SHIP_FAMILY" "$builds" || return 1
  if [ "$n_done" != "0" ]; then
    r=$(delete_batches "$done_ids" | send "delete") || return 1
    set -- $r
    echo "inbox: $n_done row(s) spent in $1 statement(s), $2 refused"
    [ "$2" = "0" ] || return 1
  fi
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

  : > builds.ndjson
  for i in 1 2 3 4; do
    jq -n -c --arg i "$i" '{id:("h"+$i),at:"2026-01-01",record:{weapon:"torid",mods:["serration"]},from:[("u"+$i)]}' \
      >> builds.ndjson
  done
  printf 'u1\nu2\nu3\nu4\nu5\n' > done.txt

  local first; first=$(build_batches builds.ndjson | head -1)
  [ "$(printf '%s' "$first" | jq -r '.params | length')" = 1 ] \
    && say ok "a build batch binds one JSON parameter" \
    || say FAIL "$(printf '%s' "$first" | jq -r '.params|length') parameters"
  [ "$(printf '%s' "$first" | jq -r '.params[0] | fromjson | length')" = 4 ] \
    && say ok "...and the parameter carries every input row" \
    || say FAIL "the batch lost an input row"
  # THE RECORD IS BOUND AS TEXT, which is what the column holds. Handed over as
  # an object it would arrive as the string `[object Object]` or be refused.
  printf '%s' "$first" | jq -e '.params[0] | fromjson | .[0].record | type == "string" and contains("torid")' >/dev/null \
    && say ok "...and the record is bound as the json text the column takes" \
    || say FAIL "the record was not JSON text"
  printf '%s' "$first" | jq -e '.sql | contains("torid") | not' >/dev/null \
    && say ok "...never interpolated into the statement" \
    || say FAIL "a record reached the sql"
  # `from` IS FOR THE DELETES, NOT FOR THE TABLE.
  printf '%s' "$first" | jq -e '.sql | contains("from") | not' >/dev/null \
    && say ok "...and the pass's own bookkeeping is not written down" \
    || say FAIL "`from` reached the statement"

  local del; del=$(delete_batches done.txt | head -1)
  printf '%s' "$del" | jq -e '.params[0] | fromjson == ["u1","u2","u3","u4","u5"]' >/dev/null \
    && say ok "every spent queue row is named, refusals included" \
    || say FAIL "$(printf '%s' "$del" | jq -c .params)"

  export PATH="$DIR/bin:$PATH"
  export PG_URL=stub
  db_send() { "$DIR/bin/db"; }
  cat > "$DIR/bin/db" <<'OK'
#!/usr/bin/env bash
body=$(cat)
printf '%s\n' "$body" >> "$PWD/sent.log"
printf '{"result":[{"results":[],"success":true}],"success":true}'
OK
  chmod +x "$DIR/bin/db"
  : > sent.log
  ship builds.ndjson done.txt > out.txt 2>&1 \
    && say ok "a pass ships its builds and spends its queue" || say FAIL "$(cat out.txt)"
  # THE ORDER IS THE WHOLE SAFETY ARGUMENT. Stopped between the two, the queue
  # still holds what the library is missing.
  grep -n "INSERT INTO entity_weapon_builds" sent.log | head -1 | cut -d: -f1 > a.txt
  grep -n "DELETE FROM record_build_submissions" sent.log | head -1 | cut -d: -f1 > b.txt
  [ "$(cat a.txt)" -lt "$(cat b.txt)" ] \
    && say ok "...with every build written before any queue row is spent" \
    || say FAIL "the delete went first"

  node() { printf '%s\n' 'family transfers' >> "$PWD/sent.log"; [ "${FAMILY_TEST_FAIL:-0}" = 0 ]; }
  : > sent.log
  ship builds.ndjson done.txt > out.txt 2>&1 || say FAIL "$(cat out.txt)"
  local before_family after_family
  before_family=$(grep -n 'INSERT INTO entity_weapon_builds' sent.log | head -1 | cut -d: -f1)
  after_family=$(grep -n 'DELETE FROM record_build_submissions' sent.log | head -1 | cut -d: -f1)
  local family_at; family_at=$(grep -n 'family transfers' sent.log | head -1 | cut -d: -f1)
  [ "$before_family" -lt "$family_at" ] && [ "$family_at" -lt "$after_family" ] \
    && say ok "family admission is after source storage and before spending arrivals" \
    || say FAIL "family admission was outside its durable boundary"
  : > sent.log
  export FAMILY_TEST_FAIL=1
  if ship builds.ndjson done.txt > out.txt 2>&1; then
    say FAIL "a failed family admission spent its arrivals"
  else
    grep -q 'DELETE FROM record_build_submissions' sent.log \
      && say FAIL "arrivals were spent before family admission" \
      || say ok "an interrupted family admission leaves arrivals available for retry"
  fi
  unset FAMILY_TEST_FAIL
  unset -f node

  export SHIP_RETRY_DELAY=0
  cat > "$DIR/bin/db" <<'CUT'
#!/usr/bin/env bash
body=$(cat)
printf '%s\n' "$body" >> "$PWD/sent.log"
if [ ! -f "$PWD/committed.txt" ]; then
  printf '%s' "$body" > "$PWD/committed.txt"
  printf '{"success":false,"errors":[{"code":"CONNECTION_CLOSED","message":"reply lost after commit"}]}'
  exit 1
fi
printf '{"result":[{"results":[],"success":true}],"success":true}'
CUT
  chmod +x "$DIR/bin/db"
  : > sent.log
  if ship builds.ndjson done.txt > out.txt 2>&1; then
    local sent_first sent_second
    sent_first=$(sed -n '1p' sent.log); sent_second=$(sed -n '2p' sent.log)
    [ "$sent_first" = "$sent_second" ] && [ "$(grep -c 'INSERT INTO entity_weapon_builds' sent.log)" = 2 ] \
      && say ok "a committed write whose reply was lost is retried with identical bytes" \
      || say FAIL "an uncertain write changed during retry"
  else say FAIL "$(cat out.txt)"; fi

  cat > "$DIR/bin/db" <<'LOST'
#!/usr/bin/env bash
cat >> "$PWD/sent.log"
printf '\n' >> "$PWD/sent.log"
printf '{"success":false,"errors":[{"code":"CONNECTION_CLOSED","message":"connection lost"}]}'
exit 1
LOST
  chmod +x "$DIR/bin/db"
  : > sent.log
  if ship builds.ndjson done.txt > out.txt 2>&1; then say FAIL "repeated cuts passed";
  else
    [ "$(grep -c 'INSERT INTO entity_weapon_builds' sent.log)" = 3 ] && ! grep -q 'DELETE FROM' sent.log \
      && say ok "transient cuts stop after three attempts without spending arrivals" \
      || say FAIL "retry attempts were unbounded or arrivals were spent"
  fi

  cat > "$DIR/bin/db" <<'DEAD'
#!/usr/bin/env bash
cat >> "$PWD/sent.log"
printf '\n' >> "$PWD/sent.log"
printf '{"success":false,"errors":[{"code":"42P01","message":"relation does not exist"}]}'
exit 1
DEAD
  chmod +x "$DIR/bin/db"
  : > sent.log
  if ship builds.ndjson done.txt > out.txt 2>&1; then
    say FAIL "a refused write passed"
  else
    grep -q "does not exist" out.txt \
      && say ok "a refused write reports what the database said" || say FAIL "$(cat out.txt)"
    grep -q "DELETE FROM record_build_submissions" sent.log \
      && say FAIL "the queue was spent after a refused write" \
      || say ok "...and nothing in the queue is spent"
    [ "$(grep -c 'INSERT INTO entity_weapon_builds' sent.log)" = 1 ] \
      && say ok "a permanent SQL refusal is never retried" \
      || say FAIL "a permanent refusal was replayed"
  fi
  unset SHIP_RETRY_DELAY

  unset PG_URL
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
