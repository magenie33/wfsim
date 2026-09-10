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
# WHOLE WEAPONS, because a weapon is the publication unit: its file is written
# whole, so half of one re-measured is a ranking of two generations against each
# other.
#
# `share` IS THE COST CONTROL and the only knob: a fifth of the library a night
# crosses all of it in five, and the hourly scorer spreads each night's batch
# over the hours after it rather than paying for it at once.
#
# IT IS A TARGET, NOT A CEILING. Whole weapons go in until the share is PASSED,
# so a night is always a little over — by at most the last weapon in. Used as a
# ceiling instead, a weapon bigger than the share would fit no night ever, and
# that would be the BIGGEST weapon, the one most people submit to.
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
# ONE ROW PER WEAPON: how many builds it has, the OLDEST measurement any of them
# carries, and how many days ago that was. The oldest rather than the newest,
# because the weapon is taken whole — one row left behind is the one that makes
# the file a mixture.
#
# `builds` IS COUNTED FROM THE LIBRARY, not from the facts, so a weapon whose
# rows are half unmeasured is priced at what re-measuring it would actually
# cost rather than at what has been measured before.
stale_body() {
  jq -n -c --arg days "$1" '
    {
      sql: ("SELECT b.weapon AS weapon,"
            + " count(DISTINCT b.id) AS builds,"
            + " min(s.finished_at) AS oldest,"
            + " julianday(?) - julianday(min(s.finished_at)) AS overdue"
            + " FROM builds b LEFT JOIN scores s ON s.identity = b.id"
            + " GROUP BY b.weapon"
            + " HAVING oldest IS NOT NULL"
            + " AND oldest < datetime(?, ?)"
            + " ORDER BY oldest"),
      params: ["now", "now", ("-" + $days + " days")]
    }'
}

total_body() {
  jq -n -c '{ sql: "SELECT count(*) AS n FROM builds", params: [] }'
}

# ---- the draw -------------------------------------------------------------
#
# RANDOM AMONG THE OVERDUE, WEIGHTED BY HOW OVERDUE. Everything in the pool has
# already passed the age threshold, so which of them goes tonight is a choice
# with no right answer — and making it the SAME choice every night means the
# same weapons are always sampled first and the rest are reached only when those
# go quiet. Drawing makes the sweep a sample of the whole library rather than a
# queue of its front.
#
# THE WEIGHT IS WHAT KEEPS IT A SAFETY NET. Uniformly random, a weapon can be
# unlucky for a month; weighted by days overdue, the longer one waits the harder
# it is to keep missing. `key = u^(1/w)`, take the largest — the standard draw
# without replacement, and it degenerates to oldest-first as the ages spread.
#
# `SWEEP_SEED` PINS THE DRAW and only the self-test sets it: a run nobody can
# reproduce is a run nobody can ask "why that weapon".
#
# ONE LINE ENDING, WHATEVER JQ FEELS LIKE. This writes a file another program
# reads, and jq emits CRLF on some hosts — an id ending in a carriage return
# matches no weapon.
draw() {
  jq -r '.[] | [.weapon, .builds, (.overdue // 1)] | @tsv' \
    | tr -d '\015' \
    | awk -v seed="${SWEEP_SEED:-}" '
        BEGIN { if (seed == "") srand(); else srand(seed) }
        {
          w = $3 + 0; if (w < 0.001) w = 0.001
          printf "%.17g\t%s\t%d\n", rand() ^ (1 / w), $1, $2
        }' \
    | sort -g -r \
    | cut -f2,3
}

# ---- …AND HOW MANY OF THEM THE NIGHT TAKES --------------------------------
#
# WHOLE WEAPONS IN THE DRAWN ORDER, UNTIL THE SHARE IS PASSED — one rule, and
# it is deliberately "until PASSED" rather than "while it still fits".
#
# A WEAPON IS INDIVISIBLE. Half of one re-measured is a file ranking two
# generations against each other, and a weapon's file is written whole. So the
# share cannot be a ceiling the batch stays under: a weapon bigger than a fifth
# of the library would fit no night ever, and it would be the BIGGEST weapon —
# the one most people submit to — that the sweep could never reach.
#
# SO EVERY BATCH IS A LITTLE OVER THE SHARE, and the overshoot is at most one
# weapon. That is the whole cost of never having an unreachable corner, and it
# needs no special case for the large ones: they are simply the nights that end
# after one.
#
# IT NEVER SKIPS. Whatever the draw put next is what goes in, so the order the
# weights produced is the order that is honoured.
fill() {
  local cap="$1"
  awk -v cap="$cap" '{ used += $2; print $1; if (used >= cap) exit }'
}

pick() {
  draw | fill "$1"
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
  printf '%s' "$body" | jq -e '.params == ["now", "now", "-7 days"]' >/dev/null \
    && say ok "the age is bound, not interpolated" \
    || say FAIL "$(printf '%s' "$body" | jq -c .params)"
  # COUNTED FROM `builds`, so a weapon half unmeasured is priced at the whole.
  printf '%s' "$body" | jq -e '.sql | contains("FROM builds b LEFT JOIN scores")' >/dev/null \
    && say ok "...and a weapon is priced from the library, not from the facts" \
    || say FAIL "$(printf '%s' "$body" | jq -r .sql)"
  printf '%s' "$body" | jq -e '.sql | contains("min(s.finished_at)")' >/dev/null \
    && say ok "...and judged by its OLDEST row, because it is taken whole" \
    || say FAIL "$(printf '%s' "$body" | jq -r .sql)"

  local rows='[{"weapon":"a","builds":50,"overdue":9},{"weapon":"b","builds":30,"overdue":8},{"weapon":"c","builds":40,"overdue":9},{"weapon":"d","builds":10,"overdue":7}]'

  # IT STOPS THE MOMENT IT HAS PASSED THE SHARE, rather than walking the pool
  # out. Forty weapons of twenty builds against a share of a hundred: a night
  # takes five or six of them and leaves the rest for the nights after.
  local many; many=$(jq -n -c '[range(0;40) | {weapon: ("w" + (.|tostring)), builds: 20, overdue: 9}]')
  local ran_on=0 i
  for i in $(seq 1 6); do
    [ "$(SWEEP_SEED=$i pick 100 <<< "$many" | grep -c . || true)" -le 6 ] || ran_on=$((ran_on + 1))
  done
  [ "$ran_on" -eq 0 ] \
    && say ok "a night stops once it has passed the share" \
    || say FAIL "$ran_on of 6 nights kept going"

  # THE SAME SEED IS THE SAME NIGHT, or a run cannot be asked "why that weapon".
  [ "$(SWEEP_SEED=7 pick 100 <<< "$rows")" = "$(SWEEP_SEED=7 pick 100 <<< "$rows")" ] \
    && say ok "one seed is one draw" || say FAIL "the same seed drew twice"
  # ...AND DIFFERENT SEEDS ARE DIFFERENT NIGHTS, which is the whole of drawing
  # rather than sorting: a fixed order samples the front of the library for ever.
  local orders
  orders=$(for i in $(seq 1 10); do SWEEP_SEED=$i pick 100 <<< "$rows" | tr '\n' ' '; echo; done \
           | sort -u | grep -c .)
  [ "$orders" -gt 1 ] && say ok "...and ten seeds are not one order ($orders of them)" \
    || say FAIL "every seed drew the same order"

  # THE LONGER THE WAIT, THE HARDER TO KEEP MISSING. A year overdue against a
  # week should lead far more often than a coin would give it.
  local wins
  wins=$(for i in $(seq 1 40); do
           SWEEP_SEED=$i pick 100 <<< '[{"weapon":"old","builds":10,"overdue":400},{"weapon":"new","builds":10,"overdue":7}]' | head -1
         done | grep -c '^old$' || true)
  [ "$wins" -ge 30 ] && say ok "...and the longest wait leads ($wins of 40)" \
    || say FAIL "the year-overdue weapon led only $wins of 40"

  # A WEAPON BIGGER THAN THE WHOLE SHARE IS STILL REACHABLE, which is the case
  # this rule is shaped around. Splitting one is off the table, so a share used
  # as a CEILING would leave the biggest weapon — the one most people submit to
  # — swept never.
  local big='[{"weapon":"huge","builds":5000,"overdue":40},{"weapon":"small","builds":5,"overdue":9}]'
  [ "$(printf '%s' "$big" | pick 100 | tr '\n' ' ')" = "huge " ] \
    && say ok "one bigger than the whole share is still reachable" \
    || say FAIL "$(printf '%s' "$big" | pick 100 | tr '\n' ' ')"

  # ...AND THE NIGHT ENDS ON WHATEVER PASSED THE SHARE. Nothing follows it, so
  # the overshoot is always exactly the LAST weapon in — and if one weapon is a
  # third of the library then that night is a third of the library. That is the
  # price of never leaving a corner unreachable, and it is worth saying out loud.
  local eq='[{"weapon":"huge","builds":5000,"overdue":9},{"weapon":"small","builds":5,"overdue":9},{"weapon":"mid","builds":40,"overdue":9}]'
  local followed=0
  for i in $(seq 1 12); do
    local night; night=$(SWEEP_SEED=$i pick 100 <<< "$eq" | tr '\n' ' ')
    case "$night" in
      *huge*) case "$night" in *"huge ") : ;; *) followed=$((followed + 1)) ;; esac ;;
    esac
  done
  [ "$followed" -eq 0 ] \
    && say ok "...and the night ends on whichever weapon passed it" \
    || say FAIL "something followed it $followed time(s)"

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
