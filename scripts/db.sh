# THE LIBRARY'S DATABASE, as a shell script reaches it — sourced, never run.
# docs/BOARD.md §"Postgres". `db_query BODY` sends one body ({ sql, params })
# to the Postgres PG_URL names, through `pgq.mjs`, and leaves its answer in
# $DB_OUT (`.result[0].results`) and a status in $DB_CODE: 200, or 500 for a
# refusal, whose errors are in the answer.
DB_HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
DB_OUT=""
# THE SCRIPT'S TEMP DIRECTORY: every `mktemp` in it and its children lands
# here, and it goes when the script exits. A trap replaces a trap, so a script
# that sets its own EXIT trap calls `db_cleanup` in it — a file a run left
# behind is memory on the bot server, where the live board runs these all day.
DB_TMP=$(mktemp -d)
export TMPDIR="$DB_TMP"
db_cleanup() { rm -rf "$DB_TMP"; }
trap db_cleanup EXIT
DB_CODE=""

db_configured() {
  [ -n "${PG_URL:-}" ]
}

# THE TRANSPORT, a body on stdin and the answer on stdout. A self-test
# redefines it after sourcing this, so the stub never reaches a database.
# ONE PROCESS AND ONE CONNECTION FOR THE SCRIPT (`pgq.mjs --lines`): a process
# and a connect a statement cost seconds a page, minutes a paged read. A
# subshell cannot reach its parent's coprocess, so one sends on its own. The
# answer comes back through a file, never the pipe (`pgq.mjs --lines`).
DB_PROC_OWNER=""
db_send() {
  local body line
  body=$(jq -c . | tr -d '\r') || return 1
  if [ "$DB_PROC_OWNER" != "$BASHPID" ]; then
    if [ -n "$DB_PROC_OWNER" ]; then printf '%s' "$body" | node "$DB_HERE/pgq.mjs"; return; fi
    DB_PROC_ANSWER=$(mktemp)
    coproc DB_PROC { node "$DB_HERE/pgq.mjs" --lines "$DB_PROC_ANSWER"; }
    DB_PROC_OWNER=$BASHPID
  fi
  printf '%s\n' "$body" >&"${DB_PROC[1]}"
  IFS= read -r line <&"${DB_PROC[0]}" || return 1
  cat "$DB_PROC_ANSWER"
  [ "$line" = ok ]
}

db_query() {
  [ -n "$DB_OUT" ] || DB_OUT=$(mktemp)
  if ! db_configured; then
    printf '%s' '{"success":false,"errors":[{"code":"","message":"PG_URL is not set"}]}' > "$DB_OUT"
    echo "::error::PG_URL is not set — the library's Postgres, docs/BOARD.md §\"Postgres\"" >&2
    DB_CODE="000"
    return 1
  fi
  # NOT A PIPE: each side of one is a subshell, which `db_send` cannot keep open.
  if db_send <<< "$1" > "$DB_OUT"; then DB_CODE=200; else DB_CODE=500; fi
  [ "$DB_CODE" = "200" ]
}
