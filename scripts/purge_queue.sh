#!/usr/bin/env bash
# ROWS OWED UNDER A RULER THAT NO LONGER EXISTS, FORGOTTEN.
#
#   scripts/purge_queue.sh
#   scripts/purge_queue.sh --self-test
#
# A queue row is deleted in exactly one place: beside the fact that settles it,
# after the score is banked. A retired ruler never produces a fact — nothing
# invokes the scorer with an id `wfsim-board --rulers` no longer lists — so its rows
# are owed for ever, and `queue: N row(s) owed` never returns to zero. That
# count is the signal a person reads to know whether the board is behind, and a
# floor under it that never moves is a signal that has stopped working.
#
# THE DEFINITION IS `builds x rulers x modes`, which is the same definition
# `reconcile_queue.sh` adds against. A row whose ruler is not in it is not a row
# that is late; it is a row that cannot ever be paid.
#
# DELETING FROM THE QUEUE IS SAFE IN A WAY DELETING A FACT IS NOT. The queue is
# DERIVED — the reconciliation rebuilds what is owed from the library and the
# facts every run — so the worst this can do is cost one cycle. A fact is a
# measurement and deleting one throws away CPU nobody can get back, which is
# why that stays a person's decision and this does not.
#
# THE ONE WAY IT COULD BE WRONG is an empty ruler list, which would delete the
# whole queue. It refuses to run on one rather than treating "I found no
# benchmarks" as "no benchmark is legal".
set -euo pipefail

. "$(dirname "${BASH_SOURCE[0]}")/db.sh"
configured() { db_configured; }

# EVERY RULER THE ROSTER HAS, one per line, from the engine: a file in
# `data/benchmarks/` is only a ruler's base, and its ladder's cells are rulers
# too. Read off the files, every row owed under a cell would be purged.
live_rulers() {
  "${WFSIM_BOARD:-./target/release/wfsim-board}" --rulers
}

# PARAMETERS ARE BOUND, NEVER INTERPOLATED — the same rule the queue's writer
# keeps, and for the same reason: these ids reach SQL from a directory listing
# and a directory is not a trusted grammar.
purge_statement() {
  jq -R -s -c '
    [splits("\n")] | map(select(length > 0)) as $live
    | {
        sql: ("DELETE FROM queue WHERE ruler NOT IN ("
              + ([$live[] | "?"] | join(",")) + ")"),
        params: $live
      }'
}

purge() {
  local live count body
  live=$(live_rulers)
  count=$(printf '%s\n' "$live" | grep -c . || true)
  if [ "$count" -eq 0 ]; then
    echo "::error::purge_queue: no benchmarks found — refusing to treat that as 'none is legal'" >&2
    return 1
  fi
  body=$(printf '%s\n' "$live" | purge_statement)
  if ! configured; then
    echo "queue: not configured — nothing purged"
    return 0
  fi
  if db_query "$body"; then
    local gone
    gone=$(jq -r '.result[0].meta.changes // 0' "$DB_OUT" 2>/dev/null || echo 0)
    echo "queue: $gone row(s) owed under a retired ruler, forgotten ($count live)"
  else
    echo "::error::queue: the database refused the purge [HTTP $DB_CODE]" >&2
    head -c 500 "$DB_OUT" || true
    return 1
  fi
}

# ---- self-test ------------------------------------------------------------
#
# The SQL is built without a database, so the shape of it is checkable here —
# which is the half that can be wrong in a way nobody sees until a queue is
# gone.
self_test() {
  ok=0; bad=0
  unset PG_URL
  say() { if [ "$1" = ok ]; then ok=$((ok + 1)); else bad=$((bad + 1)); fi; echo "  $1    $2"; }

  local body
  body=$(printf 'standard_single_target\nstandard_multi_target\n' | purge_statement)

  [ "$(printf '%s' "$body" | jq -r '.params | length')" = "2" ] \
    && say ok "one bound parameter per live ruler" \
    || say FAIL "$(printf '%s' "$body" | jq -r '.params | length') parameters"

  printf '%s' "$body" | jq -e '.sql == "DELETE FROM queue WHERE ruler NOT IN (?,?)"' >/dev/null \
    && say ok "…and the ids are bound, not interpolated" \
    || say FAIL "$(printf '%s' "$body" | jq -r .sql)"

  # THE GUARD, which is the only way this could empty the queue: a roster that
  # lists nothing, and a scorer that is not there to ask.
  local fake
  fake=$(mktemp -d)
  printf '#!/usr/bin/env bash\n' > "$fake/empty"
  printf '#!/usr/bin/env bash\nprintf "a\\na__level170\\n"\n' > "$fake/two"
  chmod +x "$fake/empty" "$fake/two"
  WFSIM_BOARD="$fake/empty" bash scripts/purge_queue.sh >/dev/null 2>&1 \
    && say FAIL "an empty roster was accepted" \
    || say ok "an empty roster is refused rather than obeyed"
  WFSIM_BOARD="$fake/missing" bash scripts/purge_queue.sh >/dev/null 2>&1 \
    && say FAIL "a missing scorer was read as an empty roster" \
    || say ok "a roster nobody can read is refused"

  # …AND THE ROSTER IT WOULD ACTUALLY SEND is the scorer's, cells included.
  local roster
  roster=$(WFSIM_BOARD="$fake/two" live_rulers | tr '\n' ' ')
  [ "$roster" = "a a__level170 " ] \
    && say ok "the roster is every ruler the scorer lists, a ladder's cells included" \
    || say FAIL "live_rulers read: $roster"

  echo
  echo "$ok ok, $bad failed"
  [ "$bad" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then
  self_test
else
  purge
fi
