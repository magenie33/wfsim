#!/usr/bin/env bash
# Retry transient cycles; an unresolved failure cannot report a successful run.
set -euo pipefail
end=$(( $(date +%s) + ${SETTLE_MINUTES:-30} * 60 ))
failures=0
mkdir -p "${1:-work}"
while [ "$(date +%s)" -lt "$end" ]; do
  code=0
  node scripts/live_orders.mjs settle "${1:-work}" "${2:-.}" || code=$?
  [ "$code" = 3 ] && { echo "settle: nothing left"; exit 0; }
  if [ "$code" = 0 ]; then
    failures=0
  else
    failures=$((failures + 1))
    echo "settle: cycle failed ($code)"
    [ "$failures" -lt 3 ] || { echo "::error::settle: three consecutive cycles failed"; exit "$code"; }
    sleep 30
  fi
done
[ "$failures" = 0 ] || { echo "::error::settle: final cycle did not complete"; exit 1; }
