#!/usr/bin/env bash
# QUEUED ROWS CANCELLED, AND EVERYTHING ASKED FOR THEM WITHDRAWN — docs/BOARD.md
# "AN ORDER IS CANCELLED WITH `cancel_queue.sh`".
#
#   scripts/cancel_queue.sh --all
#   scripts/cancel_queue.sh --batch <prefix> [--ruler <id>]
#   scripts/cancel_queue.sh --ruler <id>
#   scripts/cancel_queue.sh ... --dry-run     how many queue rows it names
#   scripts/cancel_queue.sh --self-test
#
# Each write is its own transaction taking rows with SKIP LOCKED: one
# transaction for a whole cancel deadlocked with `board_sync`.
set -euo pipefail

. "$(dirname "${BASH_SOURCE[0]}")/db.sh"

SLICE=${CANCEL_SLICE:-20000}
# THE BOARD'S PRODUCERS, worker/board_kinds.js `BOARD_KINDS` (the self-test
# holds the two equal).
BOARD_KINDS='["new_build","survey_build","refresh","sweep"]'
# THE ROUNDS THE FINAL PASS GIVES A ROW STILL LOCKED, a pause apart.
FINAL_TRIES=3

# THE SELECTOR, as two bound parameters: a batch prefix and a ruler, either
# null; both null is `--all`.
SELECT_ROW='(?1::text IS NULL OR starts_with(x.batch, ?1::text)) AND (?2::text IS NULL OR x.ruler = ?2::text)'

# ---- the statements, each a body { sql, params } ---------------------------

# THE NEXT OPEN QUESTIONS, in the order a client is handed them (the index
# `questions_order`), with a live board demand whose row the selector names;
# those demands.
pick_body() {
  jq -n -c --argjson sel "$1" --argjson kinds "$BOARD_KINDS" --argjson n "$SLICE" --arg row "$SELECT_ROW" '{
    sql: ("WITH named AS (SELECT d.producer, d.ref, d.question, d.payload::jsonb ->> '"'"'identity'"'"' AS build_id,"
      + " d.payload::jsonb ->> '"'"'ruler'"'"' AS ruler, d.payload::jsonb ->> '"'"'mode'"'"' AS mode FROM demands d"
      + " WHERE d.state = '"'"'live'"'"' AND d.producer IN (SELECT jsonb_array_elements_text(?3::jsonb)))"
      + " SELECT n.* FROM (SELECT q.id, q.waits, q.due_at FROM questions q WHERE q.state = '"'"'open'"'"'"
      + " AND EXISTS (SELECT 1 FROM named n WHERE n.question = q.id AND EXISTS (SELECT 1 FROM queue x"
      + " WHERE x.build_id = n.build_id AND x.ruler = n.ruler AND x.mode = n.mode AND " + $row + "))"
      + " ORDER BY q.waits DESC, q.due_at, q.id LIMIT ?4::bigint) q JOIN named n ON n.question = q.id"
      + " WHERE EXISTS (SELECT 1 FROM queue x WHERE x.build_id = n.build_id AND x.ruler = n.ruler AND x.mode = n.mode AND " + $row + ")"
      + " ORDER BY q.waits DESC, q.due_at, q.id, n.producer, n.ref"),
    params: [$sel.batch, $sel.ruler, ($kinds | tojson), $n] }'
}

# THE SELECTED QUEUE ROWS OF THOSE KEYS ($2: [{ build_id, ruler, mode }]) deleted.
delete_body() {
  jq -n -c --argjson sel "$1" --argjson keys "$2" --arg row "$SELECT_ROW" '{
    sql: ("DELETE FROM queue WHERE (batch, build_id, ruler, mode) IN (SELECT x.batch, x.build_id, x.ruler, x.mode FROM queue x"
      + " JOIN jsonb_to_recordset(?3::jsonb) AS k(build_id text, ruler text, mode text)"
      + " ON x.build_id = k.build_id AND x.ruler = k.ruler AND x.mode = k.mode"
      + " WHERE " + $row + " ORDER BY x.batch, x.build_id, x.ruler, x.mode FOR UPDATE OF x SKIP LOCKED)"),
    params: [$sel.batch, $sel.ruler, ($keys | tojson)] }'
}

# EVERY SELECTED QUEUE ROW LEFT, a slice of them deleted, whatever asks for it.
delete_rest_body() {
  jq -n -c --argjson sel "$1" --argjson n "$SLICE" --arg row "$SELECT_ROW" '{
    sql: ("DELETE FROM queue WHERE (batch, build_id, ruler, mode) IN (SELECT x.batch, x.build_id, x.ruler, x.mode FROM queue x"
      + " WHERE " + $row + " ORDER BY x.batch, x.build_id, x.ruler, x.mode LIMIT ?3::bigint FOR UPDATE OF x SKIP LOCKED)"
      + " RETURNING build_id, ruler, mode"),
    params: [$sel.batch, $sel.ruler, $n] }'
}

# THE LIVE BOARD DEMANDS OF THESE ROWS ($1: [{ build_id, ruler, mode }]).
demands_of_body() {
  jq -n -c --argjson keys "$1" --argjson kinds "$BOARD_KINDS" '{
    sql: ("SELECT d.producer, d.ref, d.question FROM demands d"
      + " JOIN jsonb_to_recordset(?1::jsonb) AS k(build_id text, ruler text, mode text) ON d.ref = k.build_id || '"'"'|'"'"' || k.ruler || '"'"'|'"'"' || k.mode"
      + " WHERE d.state = '"'"'live'"'"' AND d.producer IN (SELECT jsonb_array_elements_text(?2::jsonb))"),
    params: [($keys | tojson), ($kinds | tojson)] }'
}

# THOSE DEMANDS ($1: [{ producer, ref }]) WITHDRAWN where their row is owed no more.
withdraw_body() {
  jq -n -c --argjson ds "$1" '{
    sql: ("UPDATE demands SET state = '"'"'withdrawn'"'"' WHERE (producer, ref) IN (SELECT d.producer, d.ref FROM demands d"
      + " JOIN jsonb_to_recordset(?1::jsonb) AS k(producer text, ref text) ON d.producer = k.producer AND d.ref = k.ref"
      + " WHERE d.state = '"'"'live'"'"' AND NOT board_owed(d.payload::jsonb ->> '"'"'identity'"'"', d.payload::jsonb ->> '"'"'ruler'"'"',"
      + " d.payload::jsonb ->> '"'"'mode'"'"') ORDER BY d.producer, d.ref FOR UPDATE OF d SKIP LOCKED)"),
    params: [($ds | tojson)] }'
}

# THOSE QUESTIONS ($1: [id]) RESTATED, in id order; the ids it reached.
restate_body() {
  jq -n -c --argjson ids "$1" '{
    sql: ("SELECT g.id, factory_restate(g.id) FROM (SELECT id FROM questions"
      + " WHERE id IN (SELECT jsonb_array_elements_text(?1::jsonb)) ORDER BY id FOR UPDATE SKIP LOCKED) g"),
    params: [($ids | tojson)] }'
}

# HOW MANY QUEUE ROWS THE SELECTOR NAMES.
count_body() {
  jq -n -c --argjson sel "$1" --arg row "$SELECT_ROW" '{
    sql: ("SELECT count(*) AS n FROM queue x WHERE " + $row), params: [$sel.batch, $sel.ruler] }'
}

# ---- the run ---------------------------------------------------------------

ask() {
  if ! db_query "$1"; then
    echo "::error::cancel_queue: the database refused a statement [HTTP $DB_CODE]" >&2
    head -c 500 "$DB_OUT" >&2 || true
    return 1
  fi
}
rows() { jq -c '.result[0].results // []' "$DB_OUT"; }
changes() { jq -r '.result[0].meta.changes // 0' "$DB_OUT"; }

# ONE SLICE'S DEMANDS ($2) WITHDRAWN AND THEIR QUESTIONS RESTATED; the queue
# rows deleted first when $3 says so. Adds to the totals; returns 1 when it
# moved nothing.
GONE=0 WITHDRAWN=0 RESTATED=0
SKIPPED=$(mktemp)
settle() {
  local sel="$1" ds="$2" with_queue="$3" moved=0 ids reached
  if [ "$with_queue" = 1 ]; then
    ask "$(delete_body "$sel" "$(jq -c '[.[] | { build_id, ruler, mode }] | unique' <<< "$ds")")" || exit 1
    moved=$(($(changes))); GONE=$((GONE + $(changes)))
  fi
  ask "$(withdraw_body "$(jq -c '[.[] | { producer, ref }]' <<< "$ds")")" || exit 1
  moved=$((moved + $(changes))); WITHDRAWN=$((WITHDRAWN + $(changes)))
  ids=$(jq -c '[.[].question] | unique' <<< "$ds")
  ask "$(restate_body "$ids")" || exit 1
  reached=$(jq -c '[.result[0].results[].id]' "$DB_OUT")
  RESTATED=$((RESTATED + $(jq 'length' <<< "$reached")))
  jq -c --argjson r "$reached" '. - $r | .[]' <<< "$ids" >> "$SKIPPED"
  [ "$moved" -gt 0 ]
}

# EVERY SLICE, the head of the order first, until none is left or one moves nothing.
slices() {
  local sel="$1" ds n=0
  while :; do
    ask "$(pick_body "$sel")" || exit 1
    ds=$(rows)
    [ "$(jq 'length' <<< "$ds")" -gt 0 ] || return 0
    n=$((n + 1))
    settle "$sel" "$ds" 1 || return 0
    echo "cancel: slice $n — $GONE queue row(s), $WITHDRAWN demand(s) withdrawn, $RESTATED question(s) restated so far"
  done
}

cancel() {
  local sel="$1" left ids reached deleted
  ask "$(count_body "$sel")" || exit 1
  echo "cancel: $(jq -r '.result[0].results[0].n' "$DB_OUT") queue row(s) named"
  slices "$sel"
  # THE FINAL PASS: what a lock skipped, a pause later.
  for _ in $(seq 1 "$FINAL_TRIES"); do
    slices "$sel"
    ids=$(jq -s -c 'unique' "$SKIPPED")
    : > "$SKIPPED"
    if [ "$(jq 'length' <<< "$ids")" -gt 0 ]; then
      ask "$(restate_body "$ids")" || exit 1
      reached=$(jq -c '[.result[0].results[].id]' "$DB_OUT")
      RESTATED=$((RESTATED + $(jq 'length' <<< "$reached")))
      jq -c --argjson r "$reached" '. - $r | .[]' <<< "$ids" >> "$SKIPPED"
    fi
    # …AND QUEUE ROWS NO OPEN QUESTION ASKS FOR, whatever their demands.
    while :; do
      ask "$(delete_rest_body "$sel")" || exit 1
      deleted=$(rows)
      [ "$(jq 'length' <<< "$deleted")" -gt 0 ] || break
      GONE=$((GONE + $(jq 'length' <<< "$deleted")))
      ask "$(demands_of_body "$(jq -c 'unique' <<< "$deleted")")" || exit 1
      [ "$(jq 'length' <<< "$(rows)")" -gt 0 ] && settle "$sel" "$(rows)" 0 || true
    done
    ask "$(count_body "$sel")" || exit 1
    left=$(jq -r '.result[0].results[0].n' "$DB_OUT")
    [ "$left" -gt 0 ] || [ -s "$SKIPPED" ] || break
    sleep "${CANCEL_PAUSE:-5}"
  done
  echo "cancel: $GONE queue row(s) deleted, $WITHDRAWN demand(s) withdrawn, $RESTATED question(s) restated"
  if [ "$left" -gt 0 ] || [ -s "$SKIPPED" ]; then
    echo "::warning::cancel_queue: $left queue row(s) and $(grep -c . "$SKIPPED" || true) question(s) still locked — run it again" >&2
    return 1
  fi
}

# THE ARGUMENTS, as the selector's JSON ({ batch, ruler }), or a refusal.
selector() {
  local batch=null ruler=null all=0
  DRY=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --all) all=1 ;;
      --batch) [ -n "${2:-}" ] || { echo "cancel_queue: --batch needs a prefix" >&2; return 2; }
               batch=$(jq -n --arg v "$2" '$v'); shift ;;
      --ruler) [ -n "${2:-}" ] || { echo "cancel_queue: --ruler needs an id" >&2; return 2; }
               ruler=$(jq -n --arg v "$2" '$v'); shift ;;
      --dry-run) DRY=1 ;;
      *) echo "cancel_queue: unknown argument $1" >&2; return 2 ;;
    esac
    shift
  done
  # NOTHING NAMED IS NEVER EVERYTHING: the whole queue is `--all`, said.
  if [ "$all" = 1 ] && { [ "$batch" != null ] || [ "$ruler" != null ]; }; then
    echo "cancel_queue: --all names every row; drop --batch and --ruler" >&2; return 2
  fi
  if [ "$all" = 0 ] && [ "$batch" = null ] && [ "$ruler" = null ]; then
    echo "cancel_queue: name the rows: --all, --batch <prefix> or --ruler <id>" >&2; return 2
  fi
  SEL=$(jq -n -c --argjson b "$batch" --argjson r "$ruler" '{ batch: $b, ruler: $r }')
}

# ---- self-test ------------------------------------------------------------
#
# The statements and the selector are built without a database; the run itself
# is checked against Postgres by scripts/check_pg_scripts.mjs.
self_test() {
  ok=0; bad=0
  unset PG_URL
  say() { if [ "$1" = ok ]; then ok=$((ok + 1)); else bad=$((bad + 1)); fi; echo "  $1    $2"; }

  ( selector ) >/dev/null 2>&1 && say FAIL "no selector was taken as one" || say ok "nothing named is refused, never read as everything"
  ( selector --all --ruler x ) >/dev/null 2>&1 && say FAIL "--all with a ruler was taken" || say ok "--all names every row and nothing else"
  ( selector --batch "" ) >/dev/null 2>&1 && say FAIL "an empty prefix was taken" || say ok "an empty prefix is refused"

  selector --batch "refresh-'; DROP TABLE queue; --" --ruler sst
  local body
  body=$(pick_body "$SEL")
  printf '%s' "$body" | jq -e '.params[0] == "refresh-'"'"'; DROP TABLE queue; --" and .params[1] == "sst"' >/dev/null \
    && ! printf '%s' "$body" | jq -r .sql | grep -q "DROP TABLE" \
    && say ok "the selector is bound, not interpolated" || say FAIL "$body"
  printf '%s' "$body" | jq -e '.sql | test("ORDER BY q.waits DESC, q.due_at, q.id")' >/dev/null \
    && say ok "a slice is the head of the order a client is handed" || say FAIL "$(printf '%s' "$body" | jq -r .sql)"
  local locks=0
  for b in "$(delete_body "$SEL" '[]')" "$(delete_rest_body "$SEL")" "$(withdraw_body '[]')" "$(restate_body '[]')"; do
    printf '%s' "$b" | jq -e '.sql | test("FOR UPDATE (OF [a-z]+ )?SKIP LOCKED")' >/dev/null || locks=$((locks + 1))
  done
  [ "$locks" = 0 ] && say ok "every write skips a row another holds, never waits on it" || say FAIL "$locks write(s) wait on locks"
  selector --all
  [ "$(pick_body "$SEL" | jq -c '.params[0:2]')" = "[null,null]" ] && say ok "--all binds no selector" || say FAIL "$(pick_body "$SEL" | jq -c .params)"

  # THE PRODUCERS ARE THE WORKER'S.
  local kinds
  kinds=$(sed -n 's/^export const BOARD_KINDS = \(\[.*\]\);\r*$/\1/p' "$(dirname "${BASH_SOURCE[0]}")/../worker/board_kinds.js" | jq -c . 2>&1)
  [ "$kinds" = "$BOARD_KINDS" ] && say ok "the board's producers are worker/board_kinds.js's" || say FAIL "$kinds ≠ $BOARD_KINDS"

  echo
  echo "$ok ok, $bad failed"
  [ "$bad" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then
  self_test
else
  selector "$@" || exit 2
  if [ "$DRY" = 1 ]; then
    db_configured || { echo "cancel_queue: PG_URL is not set" >&2; exit 1; }
    ask "$(count_body "$SEL")"
    echo "cancel: $(jq -r '.result[0].results[0].n' "$DB_OUT") queue row(s) named — nothing changed"
  else
    cancel "$SEL"
  fi
fi
