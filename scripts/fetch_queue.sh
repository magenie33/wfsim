#!/usr/bin/env bash
# WHAT HAS BEEN ASKED FOR, IN THE ORDER IT WILL BE DONE.
#
#   scripts/fetch_queue.sh queue.ndjson
#   scripts/fetch_queue.sh --self-test
#
# A run does not decide what to compute; it reads it. `queue` holds one row per
# (batch, build, ruler, mode) somebody asked for and `batches.at` is the order,
# so this file is the whole of "what is this run doing" — a list a person can
# read, ordered by a column a person can change.
#
# THE ORDER IS THE ANSWER, so it is taken here and never re-derived downstream:
# every shard is handed this same file and takes its slice by POSITION, which is
# how they agree on who owns which row without talking.
#
# THE SCORER NEVER SPEAKS TO THE DATABASE. It reads and writes files, and the
# network lives in scripts a stub `curl` can drive.
set -euo pipefail

# A PAGE, AND THE LOOP IS BOUNDED. The queue is EMPTY at rest — it holds only
# what is still owed — so a full page is the unusual case: a fresh library, or a
# rescore somebody just asked for.
# AN INSTANT IS ISO 8601 TO THE MILLISECOND (docs/NAMING.md §9), as SQL:
# now, and `?` seconds ago.
ISO_NOW="strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"
ISO_AGO="strftime('%Y-%m-%dT%H:%M:%fZ', unixepoch() - ?, 'unixepoch')"
PAGE=5000
MAX_PAGES=200

configured() {
  [ -n "${CF_ACCOUNT:-}" ] && [ -n "${CF_D1_DATABASE:-}" ] && [ -n "${CF_TOKEN:-}" ]
}

D1_OUT=""
D1_CODE=""
d1() {
  [ -n "$D1_OUT" ] || D1_OUT=$(mktemp)
  # A CALL THAT NEVER ANSWERS IS THE WORST SHAPE THIS PIPELINE HAS. Without a
  # clock a hung connection holds the step until GitHub's six-hour cap, and the
  # board's workflows keep ONE run pending per concurrency group — so every
  # hourly tick behind it is cancelled and nothing is scored until somebody
  # looks. Measured 2026-09-21: one hung call cost seventeen hours of ticks.
  D1_CODE=$(curl -s --connect-timeout 15 --max-time 180     -o "$D1_OUT" -w '%{http_code}' -X POST \
    -H "Authorization: Bearer ${CF_TOKEN:-x}" \
    -H "content-type: application/json" \
    --data-binary "$1" \
    "https://api.cloudflare.com/client/v4/accounts/${CF_ACCOUNT:-x}/d1/database/${CF_D1_DATABASE:-x}/query") \
    || D1_CODE="000"
  [ "$D1_CODE" = "200" ]
}

# ---- the read -------------------------------------------------------------
#
# ORDERED TOTALLY, so paging is stable. `batches.at` sets which group goes
# first; the primary key breaks every tie under it, and without that last part
# two pages of one query may overlap or skip — a row the run then owes for ever.
#
# A ROW WHOSE BATCH IS GONE IS NOT READ. An inner join, so deleting a batch is
# the whole of cancelling it: the rows stop being asked for and nothing has to
# go and find them.
# `HOLD_SECONDS` LEAVES A YOUNG COMPUTE ORDER TO THE CLIENTS, and an old one to
# whoever reaches it first (docs/BOARD.md §"Compute orders"). A run CLAIMS the
# old ones before it reads (`claim_body`), and reads only what it claimed or
# what has no order the clients could take, so no row is fought twice — a
# claimed OPEN order first, since one fight settles it and pays its client,
# and one the run does not reach waits another hour. Unset,
# every owed row is read — which is what a reconciliation needs.
page_body() {
  if [ -n "${HOLD_SECONDS:-}" ]; then
    jq -n -c --argjson limit "$1" --argjson offset "$2" --argjson hold "$HOLD_SECONDS" --arg now "$ISO_NOW" --arg ago "$ISO_AGO" '
      {
        sql: ("SELECT q.batch, q.build_id, q.ruler, q.mode FROM queue q"
              + " JOIN batches b ON b.id = q.batch"
              + " LEFT JOIN orders c ON c.identity = q.build_id AND c.ruler = q.ruler AND c.mode = q.mode"
              + " WHERE NOT EXISTS (SELECT 1 FROM orders o WHERE o.identity = q.build_id"
              + " AND o.ruler = q.ruler AND o.mode = q.mode"
              + " AND (o.at > " + $ago + " OR o.state IN (?, ?)))"
              + " ORDER BY (c.state IN (?, ?)) DESC, b.at, q.batch, q.build_id, q.ruler, q.mode"
              + " LIMIT ? OFFSET ?"),
        params: [$hold, "todo", "open", "scoring:open", "scoring:fresh", $limit, $offset]
      }'
    return
  fi
  jq -n -c --argjson limit "$1" --argjson offset "$2" '
    {
      sql: ("SELECT q.batch, q.build_id, q.ruler, q.mode FROM queue q"
            + " JOIN batches b ON b.id = q.batch"
            + " ORDER BY b.at, q.batch, q.build_id, q.ruler, q.mode"
            + " LIMIT ? OFFSET ?"),
      params: [$limit, $offset]
    }'
}

# THE CLIENTS' RESERVE, asked before the claim: how many unmeasured orders a
# client could take now (`todo`, young or old, no live lease), how many of
# those are old enough for this run, and how many facts the clients made in the
# last hour. The run leaves them `RESERVE_FACTOR` hours of that, never fewer
# than `RESERVE_MIN` orders, and claims only the old `todo` beyond it — so when
# many computers are on they keep most of the work, and at night the run takes it.
reserve_body() {
  jq -n -c --argjson hold "$HOLD_SECONDS" --arg now "$ISO_NOW" --arg ago "$ISO_AGO" '
    {
      sql: ("SELECT"
            + " (SELECT COUNT(*) FROM orders WHERE state = ? AND (lease_until IS NULL OR lease_until < " + $now + ")) AS open_to_clients,"
            + " (SELECT COUNT(*) FROM orders WHERE state = ? AND at <= " + $ago
            + "   AND (lease_until IS NULL OR lease_until < " + $now + ")) AS old,"
            + " (SELECT COUNT(*) FROM scores WHERE measured_by LIKE ?"
            + "   AND finished_at >= strftime(?, unixepoch() - 3600, ?)) AS facts_last_hour"),
      params: ["todo", "todo", $hold, "verified:%", "%Y-%m-%dT%H:%M:%fZ", "unixepoch"]
    }'
}

# HOW MANY TO CLAIM, from the reserve's three numbers (`old open facts`).
claim_rows() {
  local old="$1" open="$2" facts="$3"
  local reserve; reserve=$(awk -v f="$facts" -v k="${RESERVE_FACTOR:-2}" -v m="${RESERVE_MIN:-500}" \
    'BEGIN { r = int(f * k + 0.5); print (r < m ? m : r) }')
  local take=$(( open - reserve ))
  [ "$take" -gt "$old" ] && take=$old
  [ "$take" -lt 0 ] && take=0
  echo "$take"
}

# THE CLAIM makes an order `scoring:<state>`, which no lease seeks
# (worker/verify.js reads `todo` and `open` only), so a client is never handed
# a row this run is fighting. Two of them:
# - EVERY old `open` order — one result in and no second client after the hold,
#   which a lone computer can never give itself; one fight here settles it and
#   pays that client (`order_credit.mjs`). Kept back, it would wait for ever.
#   …AND every old `fresh` one, a first result the live loop has not ranked:
#   that loop runs on one server, and a result must not wait on it being up.
# - the first `rows` old `todo` orders in the queue's own order; what the queue
#   asks for last is what the clients keep.
# Neither takes an order a client holds a live lease on.
claim_body() {
  jq -n -c --argjson hold "$HOLD_SECONDS" --arg now "$ISO_NOW" --arg ago "$ISO_AGO" --arg state "${1:?state}" --argjson rows "${2:--1}" '
    {
      sql: ("UPDATE orders SET state = ? || state WHERE rowid IN ("
            + "SELECT o.rowid FROM orders o JOIN queue q ON q.build_id = o.identity"
            + " AND q.ruler = o.ruler AND q.mode = o.mode JOIN batches b ON b.id = q.batch"
            + " WHERE o.state = ? AND o.at <= " + $ago
            + " AND (o.lease_until IS NULL OR o.lease_until < " + $now + ")"
            + " ORDER BY b.at, q.batch, q.build_id, q.ruler, q.mode LIMIT ?)"),
      params: ["scoring:", $state, $hold, $rows]
    }'
}

# THE RELEASE: what a run claimed and did not settle goes back to the clients.
# Run when the run ends, and before every claim, since runs are serialized and
# a claim left over is one whose run died.
release_body() {
  jq -n -c '{ sql: "UPDATE orders SET state = substr(state, 9) WHERE state IN (?, ?, ?)", params: ["scoring:todo", "scoring:open", "scoring:fresh"] }'
}

fetch() {
  local out="$1" offset=0 got total=0 page
  : > "$out"
  for page in $(seq 1 "$MAX_PAGES"); do
    if ! d1 "$(page_body "$PAGE" "$offset")"; then
      echo "::error::queue: the database refused the read [HTTP $D1_CODE]"
      [ -s "$D1_OUT" ] && { head -c 500 "$D1_OUT"; echo; }
      return 1
    fi
    got=$(jq -r '.result[0].results | length' < "$D1_OUT")
    jq -c '.result[0].results[]' < "$D1_OUT" >> "$out"
    total=$((total + got))
    [ "$got" -lt "$PAGE" ] && break
    offset=$((offset + PAGE))
    if [ "$page" -eq "$MAX_PAGES" ]; then
      echo "::error::queue: stopped at $MAX_PAGES pages — more is owed than this reads"
      return 1
    fi
  done
  echo "queue: $total row(s) owed"
}

# ---- self-test ------------------------------------------------------------
self_test() {
  ok=0; bad=0
  DIR=$(mktemp -d)
  trap 'rm -rf "$DIR"' EXIT
  mkdir -p "$DIR/bin" "$DIR/work"
  say() { if [ "$1" = ok ]; then ok=$((ok + 1)); else bad=$((bad + 1)); fi; echo "  $1    $2"; }
  cd "$DIR/work"

  local body; body=$(page_body 5000 0)
  printf '%s' "$body" | jq -e '.sql | contains("ORDER BY b.at, q.batch, q.build_id, q.ruler, q.mode")' >/dev/null \
    && say ok "the order is the batch's, then the key" \
    || say FAIL "$(printf '%s' "$body" | jq -r .sql)"
  printf '%s' "$body" | jq -e '.sql | contains("JOIN batches")' >/dev/null \
    && say ok "...and a row whose batch is gone is not read" \
    || say FAIL "no join — deleting a batch would leave its rows owed"
  printf '%s' "$body" | jq -e '.params == [5000, 0]' >/dev/null \
    && say ok "...and the page is bound, not interpolated" \
    || say FAIL "$(printf '%s' "$body" | jq -c .params)"

  export PATH="$DIR/bin:$PATH"
  export CF_ACCOUNT=a CF_D1_DATABASE=d CF_TOKEN=t
  cat > "$DIR/bin/curl" <<'OK'
#!/usr/bin/env bash
out=/dev/null; prev=
for a in "$@"; do [ "$prev" = "-o" ] && out="$a"; prev="$a"; done
n=$(cat "$PWD/page.n" 2>/dev/null || echo 0)
echo $((n + 1)) > "$PWD/page.n"
if [ "$n" = "0" ]; then
  jq -n -c '{result:[{results:[range(0;3)|{batch:"arrivals",build_id:("b"+(.|tostring)),ruler:"standard_single_target",mode:"base"}],success:true}],success:true}' > "$out"
else
  jq -n -c '{result:[{results:[],success:true}],success:true}' > "$out"
fi
printf '200'
OK
  chmod +x "$DIR/bin/curl"
  rm -f page.n
  fetch got.ndjson > out.txt 2>&1 \
    && say ok "a read pages until the answer runs out" || say FAIL "$(cat out.txt)"
  [ "$(grep -c . got.ndjson)" = "3" ] \
    && say ok "...and every row lands in the file" || say FAIL "$(grep -c . got.ndjson) rows"
  jq -e -s '.[0].batch == "arrivals" and .[0].build_id == "b0"' < got.ndjson >/dev/null \
    && say ok "...in the order the database gave them" || say FAIL "$(head -1 got.ndjson)"

  cat > "$DIR/bin/curl" <<'DEAD'
#!/usr/bin/env bash
out=/dev/null; prev=
for a in "$@"; do [ "$prev" = "-o" ] && out="$a"; prev="$a"; done
printf '{"success":false,"errors":[{"code":7403,"message":"not authorized"}]}' > "$out"
printf '403'
DEAD
  chmod +x "$DIR/bin/curl"
  # A REFUSED READ IS NOT AN EMPTY QUEUE. Reported as an error rather than as
  # "nothing owed", which a run would act on by scoring nothing and publishing.
  if fetch got.ndjson > out.txt 2>&1; then
    say FAIL "a refused read passed as an empty queue"
  else
    grep -q "not authorized" out.txt \
      && say ok "a refused read says what the database said" || say FAIL "$(cat out.txt)"
  fi

  unset CF_D1_DATABASE
  configured && say FAIL "unconfigured read as configured" \
    || say ok "no database configured is a working state"

  echo
  echo "$ok ok, $bad failed"
  [ "$bad" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then self_test; exit $?; fi
# THE STATEMENTS THEMSELVES, for check_board_verify.mjs to run against the schema.
if [ "${1:-}" = "--body" ]; then "${2:?claim|release|page}_body" "${@:3}"; exit $?; fi

# A RELEASE OR A CLAIM THAT FAILS FAILS THE RUN: a read without its claim is a
# list the clients are still being handed, which is every row fought twice.
send_one() {
  if ! d1 "$1"; then
    echo "::error::queue: the database refused the $2 [HTTP $D1_CODE]"
    [ -s "$D1_OUT" ] && { head -c 500 "$D1_OUT"; echo; }
    return 1
  fi
  echo "queue: $2 — $(jq -r '.result[0].meta.changes // 0' < "$D1_OUT") order(s)"
}
if [ "${1:-}" = "--release" ]; then
  configured || { echo "queue: no database configured, nothing to release"; exit 0; }
  send_one "$(release_body)" "release"; exit $?
fi

if ! configured; then
  # NOTHING OWED IS A WORKING STATE, and it is what a machine with no
  # credentials should compute: nothing.
  : > "${1:?usage: fetch_queue.sh <out.ndjson>}"
  echo "queue: no database configured, nothing owed"
  exit 0
fi
if [ -n "${HOLD_SECONDS:-}" ]; then
  send_one "$(release_body)" "release of a claim left over"
  if ! d1 "$(reserve_body)"; then
    echo "::error::queue: the database refused the reserve [HTTP $D1_CODE]"
    exit 1
  fi
  read -r open old facts < <(jq -r '.result[0].results[0] | "\(.open_to_clients) \(.old) \(.facts_last_hour)"' < "$D1_OUT")
  rows=$(claim_rows "$old" "$open" "$facts")
  echo "queue: the clients made $facts fact(s) in the last hour; $open unmeasured order(s) open to them, $old old — this run claims $rows"
  send_one "$(claim_body open)" "claim of every old open order"
  send_one "$(claim_body fresh)" "claim of every old unranked first result"
  send_one "$(claim_body todo "$rows")" "claim"
fi
fetch "${1:?usage: fetch_queue.sh <out.ndjson>}"
