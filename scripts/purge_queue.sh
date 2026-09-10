#!/usr/bin/env bash
# ROWS OWED UNDER A RULER THAT NO LONGER EXISTS, FORGOTTEN.
#
#   scripts/purge_queue.sh
#   scripts/purge_queue.sh --self-test
#
# A queue row is deleted in exactly one place: beside the fact that settles it,
# after the score is banked. A retired ruler never produces a fact — nothing
# invokes the scorer with an id `data/benchmarks/` no longer has — so its rows
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

# EVERY RULER THE ROSTER HAS, one per line. The filename IS the id — the same
# thing `reconcile_queue.sh` and `publish.yml` both read it as.
live_rulers() {
  local f
  for f in data/benchmarks/*.yaml; do
    [ -e "$f" ] || continue
    basename "$f" .yaml
  done
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
  if d1 "$body"; then
    local gone
    gone=$(jq -r '.result[0].meta.changes // 0' "$D1_OUT" 2>/dev/null || echo 0)
    echo "queue: $gone row(s) owed under a retired ruler, forgotten ($count live)"
  else
    echo "::error::queue: the database refused the purge [HTTP $D1_CODE]" >&2
    head -c 500 "$D1_OUT" || true
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
  say() { if [ "$1" = ok ]; then ok=$((ok + 1)); else bad=$((bad + 1)); fi; echo "  $1    $2"; }

  local body
  body=$(printf 'single_target\ngroup_clear\n' | purge_statement)

  [ "$(printf '%s' "$body" | jq -r '.params | length')" = "2" ] \
    && say ok "one bound parameter per live ruler" \
    || say FAIL "$(printf '%s' "$body" | jq -r '.params | length') parameters"

  printf '%s' "$body" | jq -e '.sql == "DELETE FROM queue WHERE ruler NOT IN (?,?)"' >/dev/null \
    && say ok "…and the ids are bound, not interpolated" \
    || say FAIL "$(printf '%s' "$body" | jq -r .sql)"

  # THE GUARD, which is the only way this could empty the queue.
  ( cd "$(mktemp -d)" && mkdir -p data/benchmarks \
      && PATH="$PATH" bash "$OLDPWD/scripts/purge_queue.sh" >/dev/null 2>&1 ) \
    && say FAIL "an empty roster was accepted" \
    || say ok "an empty roster is refused rather than obeyed"

  # …AND THE ROSTER IT WOULD ACTUALLY SEND, so a rename of the directory or of
  # the suffix fails here rather than in production.
  [ "$(live_rulers | grep -c .)" -ge 2 ] \
    && say ok "the roster reads its rulers from data/benchmarks/" \
    || say FAIL "live_rulers found $(live_rulers | grep -c .)"

  echo
  echo "$ok ok, $bad failed"
  [ "$bad" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then
  self_test
else
  purge
fi
