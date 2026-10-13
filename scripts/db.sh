# THE LIBRARY'S DATABASE, as a shell script reaches it — sourced, never run.
# docs/BOARD.md §"Postgres". `db_query BODY` sends one body ({ sql, params })
# to the Postgres PG_URL names, through `pgq.mjs`, and leaves its answer in
# $DB_OUT (`.result[0].results`) and a status in $DB_CODE: 200, or 500 for a
# refusal, whose errors are in the answer.
DB_HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
DB_OUT=""
DB_CODE=""

db_configured() {
  [ -n "${PG_URL:-}" ]
}

# THE TRANSPORT, a body on stdin and the answer on stdout. A self-test
# redefines it after sourcing this, so the stub never reaches a database.
db_send() {
  node "$DB_HERE/pgq.mjs"
}

db_query() {
  [ -n "$DB_OUT" ] || DB_OUT=$(mktemp)
  if ! db_configured; then
    printf '%s' '{"success":false,"errors":[{"code":"","message":"PG_URL is not set"}]}' > "$DB_OUT"
    echo "::error::PG_URL is not set — the library's Postgres, docs/BOARD.md §\"Postgres\"" >&2
    DB_CODE="000"
    return 1
  fi
  if printf '%s' "$1" | db_send > "$DB_OUT"; then DB_CODE=200; else DB_CODE=500; fi
  [ "$DB_CODE" = "200" ]
}
