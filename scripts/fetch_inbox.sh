#!/usr/bin/env bash
# Hourly and continuous intake use the same bounded input reader.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
if [ "${1:-}" = "--self-test" ]; then
  exec node "$HERE/check_inbox_reader.mjs"
fi
exec node "$HERE/inbox_reader.mjs" --out "${1:?usage: fetch_inbox.sh <out.ndjson>}" --receipt "$1.receipt.json"
