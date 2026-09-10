#!/usr/bin/env bash
# WHICH WEAPONS NOBODY HAS LOOKED AT IN A WHILE — the sweep's shopping list.
#
#   scripts/pick_stale.sh <days> <share> > weapons.txt
#   scripts/pick_stale.sh --self-test
#
# A SAFETY NET, NOT A CORRECTION. Nothing here says a stored number is wrong:
# a board is a claim about what this code computes TODAY, and a row measured
# under an engine six weeks old is a claim nobody has checked since. What
# retires a fact on purpose is still a person; this is what happens when nobody
# remembers to be that person.
#
# WHOLE WEAPONS, because a weapon is the publication unit — its file is written
# whole, so half of one re-measured is a ranking of two generations against each
# other. The pick is therefore a set of WEAPONS whose builds add up to at most a
# `share`th of the library, taken oldest first.
#
# `share` IS THE COST CONTROL and the only knob: a fifth of the library a night
# crosses all of it in five, and the hourly scorer spreads each night's batch
# over the hours after it rather than paying for it at once.
set -euo pipefail

DAYS="${1:-7}"
SHARE="${2:-5}"

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

# ---- the question ---------------------------------------------------------
#
# ONE ROW PER WEAPON: how many builds it has, and the OLDEST measurement any of
# them carries. The oldest rather than the newest, because the weapon is taken
# whole — one row left behind is the one that makes the file a mixture.
#
# `builds` IS COUNTED FROM THE LIBRARY, not from the facts, so a weapon whose
# rows are half unmeasured is priced at what re-measuring it would actually
# cost rather than at what has been measured before.
stale_body() {
  jq -n -c --arg days "$1" '
    {
      sql: ("SELECT b.weapon AS weapon,"
            + " count(DISTINCT b.id) AS builds,"
            + " min(s.finished_at) AS oldest"
            + " FROM builds b LEFT JOIN scores s ON s.identity = b.id"
            + " GROUP BY b.weapon"
            + " HAVING oldest IS NOT NULL"
            + " AND oldest < datetime(?, ?)"
            + " ORDER BY oldest"),
      params: ["now", ("-" + $days + " days")]
    }'
}

total_body() {
  jq -n -c '{ sql: "SELECT count(*) AS n FROM builds", params: [] }'
}

# ---- the pick -------------------------------------------------------------
#
# ONE LINE ENDING, WHATEVER JQ FEELS LIKE. This writes a file another program
# reads, and jq emits CRLF on some hosts — a list whose ids end in a carriage
# return matches no weapon.
#
# GREEDY, OLDEST FIRST, AND IT DOES NOT BACKTRACK. Filling the share exactly is
# a knapsack and the answer would be worth nothing: what matters is that the
# oldest weapons go first and the night's bill has a ceiling. A weapon too big
# to fit is SKIPPED rather than ending the walk, or one weapon with a third of
# the library in it would stall the sweep for ever.
pick() {
  local cap="$1"
  jq -r --argjson cap "$cap" '
    reduce .[] as $w ({ used: 0, out: [] };
      if .used + $w.builds <= $cap
      then { used: (.used + $w.builds), out: (.out + [$w.weapon]) }
      else . end)
    | .out[]' | tr -d '\015'
}

run() {
  if ! d1 "$(total_body)"; then
    echo "::error::sweep: the database refused the count [HTTP $D1_CODE]" >&2
    head -c 300 "$D1_OUT" >&2 || true
    return 1
  fi
  local total cap
  total=$(jq -r '.result[0].results[0].n // 0' < "$D1_OUT")
  cap=$(( total / SHARE ))
  if ! d1 "$(stale_body "$DAYS")"; then
    echo "::error::sweep: the database refused the walk [HTTP $D1_CODE]" >&2
    head -c 300 "$D1_OUT" >&2 || true
    return 1
  fi
  local rows; rows=$(jq -c '.result[0].results' < "$D1_OUT")
  local n; n=$(printf '%s' "$rows" | jq 'length')
  printf '%s' "$rows" | pick "$cap"
  local took; took=$(printf '%s' "$rows" | pick "$cap" | grep -c . || true)
  echo "sweep: $n weapon(s) older than $DAYS days, took $took of them (cap $cap of $total builds)" >&2
}

# ---- self-test ------------------------------------------------------------
self_test() {
  ok=0; bad=0
  say() { if [ "$1" = ok ]; then ok=$((ok + 1)); else bad=$((bad + 1)); fi; echo "  $1    $2"; }

  local body; body=$(stale_body 7)
  printf '%s' "$body" | jq -e '.params == ["now", "-7 days"]' >/dev/null \
    && say ok "the age is bound, not interpolated" \
    || say FAIL "$(printf '%s' "$body" | jq -c .params)"
  # COUNTED FROM `builds`, so a weapon half unmeasured is priced at the whole.
  printf '%s' "$body" | jq -e '.sql | contains("FROM builds b LEFT JOIN scores")' >/dev/null \
    && say ok "...and a weapon is priced from the library, not from the facts" \
    || say FAIL "$(printf '%s' "$body" | jq -r .sql)"
  printf '%s' "$body" | jq -e '.sql | contains("min(s.finished_at)")' >/dev/null \
    && say ok "...and judged by its OLDEST row, because it is taken whole" \
    || say FAIL "$(printf '%s' "$body" | jq -r .sql)"

  local rows='[{"weapon":"a","builds":50},{"weapon":"b","builds":30},{"weapon":"c","builds":40},{"weapon":"d","builds":10}]'
  [ "$(printf '%s' "$rows" | pick 100 | tr '\n' ' ')" = "a b d " ] \
    && say ok "it fills the cap oldest-first and stops" \
    || say FAIL "$(printf '%s' "$rows" | pick 100 | tr '\n' ' ')"
  # A WEAPON TOO BIG IS SKIPPED, NEVER A FULL STOP: one weapon holding a third
  # of the library would otherwise stall the sweep for ever.
  [ "$(printf '%s' '[{"weapon":"huge","builds":500},{"weapon":"small","builds":5}]' | pick 100 | tr '\n' ' ')" = "small " ] \
    && say ok "...and steps over one too big to fit" \
    || say FAIL "a weapon over the cap ended the walk"
  [ -z "$(printf '%s' "$rows" | pick 0)" ] \
    && say ok "...and a cap of zero takes nothing" || say FAIL "a zero cap took something"

  configured && say FAIL "unconfigured read as configured" \
    || say ok "no database configured is a working state"

  echo
  echo "$ok ok, $bad failed"
  [ "$bad" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then self_test; exit $?; fi

if ! configured; then
  echo "sweep: no database configured, nothing to look at" >&2
  exit 0
fi
run
