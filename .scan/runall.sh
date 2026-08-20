#!/usr/bin/env bash
# Run every committed check, one at a time, and record the exit code.
cd /e/src/github.com/magenie33/wfsim || exit 1
export WFSIM_PORT=8799
out=/tmp/scan_checks.txt
: > "$out"
for f in scripts/check_*.mjs; do
  name=$(basename "$f" .mjs)
  start=$(date +%s)
  log=/tmp/scan_$name.log
  node "$f" > "$log" 2>&1
  code=$?
  took=$(( $(date +%s) - start ))
  printf '%-28s exit=%-3s %4ss\n' "$name" "$code" "$took" >> "$out"
done
echo "ALL DONE" >> "$out"
