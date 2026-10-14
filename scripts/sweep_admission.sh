#!/usr/bin/env bash
# QUEUE ROWS THE BOARD DOES NOT ADMIT, NAMED — and forgotten only when asked.
#
#   scripts/sweep_admission.sh <refused.ndjson>            read only: name them
#   scripts/sweep_admission.sh <refused.ndjson> --delete   …and delete them
#
# Every queued (build, ruler, mode) is put through `wfsim-intake
# --sweep-admission`, the same door `/api/board/order` opens a row through
# (`owed_rows`), so a row it names is one no client can ever answer: handed out
# and returned for ever. The deletion is `purge_orphans.sh`'s, the queue being
# derived — anything wrongly forgotten is asked again by the next reconciliation.
set -euo pipefail

OUT="${1:?usage: sweep_admission.sh <refused.ndjson> [--delete]}"
INTAKE="${WFSIM_INTAKE:-./target/release/wfsim-intake}"
HERE="$(dirname "${BASH_SOURCE[0]}")"

. "$HERE/db.sh"
db_query '{"sql":"SELECT q.build_id, q.ruler, q.mode, b.record FROM queue q JOIN builds b ON b.id = q.build_id","params":[]}' \
  || { echo "::error::sweep: the queue read was refused [HTTP $DB_CODE]" >&2; exit 1; }
# A RECORD IS STORED AS TEXT, and the sweep reads it as the object it is.
jq -c '.result[0].results[] | .record = (.record | if type == "string" then fromjson else . end)' < "$DB_OUT" \
  | "$INTAKE" --sweep-admission > "$OUT"

if [ "${2:-}" = "--delete" ]; then
  bash "$HERE/purge_orphans.sh" "$OUT"
else
  echo "sweep: $(grep -c . "$OUT" || true) row(s) named in $OUT, none deleted (pass --delete to forget them)"
fi
