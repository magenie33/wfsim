# THE LIBRARY'S DATABASE, as a shell script reaches it — sourced, never run.
# docs/BOARD.md §"Postgres". `db_query BODY` sends one D1 query-API body
# ({ sql, params }) and leaves D1's answer in $DB_OUT (`.result[0].results`)
# and an HTTP status in $DB_CODE: D1's query API, or Postgres through
# `pgq.mjs` while PG_URL is set — the switch is that one value, so the SQL
# a script sends reads alike in both.
DB_HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
DB_OUT=""
DB_CODE=""

db_configured() {
  [ -n "${PG_URL:-}" ] || { [ -n "${CF_ACCOUNT:-}" ] && [ -n "${CF_D1_DATABASE:-}" ] && [ -n "${CF_TOKEN:-}" ]; }
}

db_query() {
  [ -n "$DB_OUT" ] || DB_OUT=$(mktemp)
  if [ -n "${PG_URL:-}" ]; then
    # A refusal is D1's shape too, and a status a script reports as it would D1's.
    if printf '%s' "$1" | node "$DB_HERE/pgq.mjs" > "$DB_OUT"; then DB_CODE=200; else DB_CODE=500; fi
  else
    # THE BODY TO A FILE, the status to a variable: `curl -sf` swallows the
    # body, and a refusal then cannot say why.
    # A CALL THAT NEVER ANSWERS IS THE WORST SHAPE THIS PIPELINE HAS: without a
    # clock a hung connection holds the step until GitHub's six-hour cap, and
    # every hourly tick behind it in its concurrency group is cancelled.
    DB_CODE=$(curl -s --connect-timeout 15 --max-time 180 -o "$DB_OUT" -w '%{http_code}' -X POST \
      -H "Authorization: Bearer ${CF_TOKEN:-x}" \
      -H "content-type: application/json" \
      --data-binary "$1" \
      "https://api.cloudflare.com/client/v4/accounts/${CF_ACCOUNT:-x}/d1/database/${CF_D1_DATABASE:-x}/query") \
      || DB_CODE="000"
  fi
  [ "$DB_CODE" = "200" ]
}
