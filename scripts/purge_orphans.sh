#!/usr/bin/env bash
# ROWS OWED THAT NO BUILD CAN PAY, FORGOTTEN.
#
#   scripts/purge_orphans.sh <orphans.ndjson>
#   scripts/purge_orphans.sh --self-test
#
# A queue row is deleted beside the fact that settles it. A row no build in the
# library can produce is never settled, and "N rows still measuring" gets a
# floor no run can lower. Three shapes of it have been seen: a build the ruler
# refuses, a mode the weapon lost, and — most of them — an id the identity rule
# has since moved, the build long scored under its new one.
#
# `wfsim-board --queue-orphans` names them exactly: the queued (build, mode)
# pairs a full walk of the library never produced. This deletes those rows and
# nothing else.
#
# SAFE THE WAY `purge_queue.sh` IS: the queue is derived. Anything wrongly
# forgotten is found missing by the next reconciliation and asked for again.
set -euo pipefail

. "$(dirname "${BASH_SOURCE[0]}")/db.sh"
configured() { db_configured; }

# Thirty-three rows a statement: three parameters each, at most a
# hundred. BOUND, never interpolated. NO `batch` in the key: a row two groups
# asked for is one debt, the same rule `ship_facts.sh` keeps.
purge_statements() {
  jq -s -c '
    . as $all
    | range(0; length; 33) as $i
    | $all[$i : $i + 33] as $chunk
    | {
        sql: ("DELETE FROM queue WHERE (build_id, ruler, mode) IN (VALUES "
              + ([$chunk[] | "(?,?,?)"] | join(",")) + ")"),
        params: [$chunk[] | .build_id, .ruler, .mode]
      }'
}

purge() {
  local src="$1" n body gone=0
  n=$(grep -c . "$src" 2>/dev/null || true)
  if [ "${n:-0}" -eq 0 ]; then
    echo "queue: no orphaned rows"
    return 0
  fi
  if ! configured; then
    echo "queue: $n orphaned row(s) — not configured, nothing forgotten"
    return 0
  fi
  while IFS= read -r body; do
    [ -n "$body" ] || continue
    if db_query "$body"; then
      gone=$((gone + $(jq -r '.result[0].meta.changes // 0' "$DB_OUT" 2>/dev/null || echo 0)))
    else
      echo "::error::queue: the database refused the orphan purge [HTTP $DB_CODE]" >&2
      head -c 500 "$DB_OUT" || true
      return 1
    fi
  done < <(purge_statements < "$src")
  echo "queue: $gone row(s) no build can pay, forgotten ($n named)"
}

# ---- self-test ------------------------------------------------------------
self_test() {
  ok=0; bad=0
  unset PG_URL
  say() { if [ "$1" = ok ]; then ok=$((ok + 1)); else bad=$((bad + 1)); fi; echo "  $1    $2"; }

  local body
  body=$(printf '%s\n' \
    '{"build_id":"07ed","ruler":"standard_single_target","mode":"base"}' \
    '{"build_id":"07ed","ruler":"standard_single_target","mode":"cycle"}' | purge_statements)
  printf '%s' "$body" | jq -e '.sql == "DELETE FROM queue WHERE (build_id, ruler, mode) IN (VALUES (?,?,?),(?,?,?))"' >/dev/null \
    && say ok "each row is named whole — build, ruler AND mode" \
    || say FAIL "$(printf '%s' "$body" | jq -r .sql)"
  printf '%s' "$body" | jq -e '.params == ["07ed","standard_single_target","base","07ed","standard_single_target","cycle"]' >/dev/null \
    && say ok "…and bound, not interpolated" \
    || say FAIL "$(printf '%s' "$body" | jq -c .params)"
  printf '%s' "$body" | jq -e '.sql | contains("batch") | not' >/dev/null \
    && say ok "…for every group that asked for it, not one" \
    || say FAIL "the delete named a batch"

  local many
  many=$(for i in $(seq 1 100); do echo "{\"build_id\":\"b$i\",\"ruler\":\"r\",\"mode\":\"base\"}"; done \
    | purge_statements | jq -s 'map(.params | length) | max')
  [ "$many" -le 100 ] && say ok "no statement binds more than a hundred parameters" || say FAIL "$many parameters"

  # AN EMPTY LIST TOUCHES NOTHING, configured or not.
  local empty; empty=$(mktemp)
  ( PG_URL=stub purge "$empty" ) | grep -q "no orphaned rows" \
    && say ok "an empty list sends nothing" \
    || say FAIL "an empty list was sent"
  rm -f "$empty"

  echo
  echo "$ok ok, $bad failed"
  [ "$bad" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then
  self_test
else
  purge "${1:?usage: purge_orphans.sh <orphans.ndjson>}"
fi
