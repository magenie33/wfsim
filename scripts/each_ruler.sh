#!/usr/bin/env bash
# ONE BODY PER RULER, SEVERAL AT ONCE.
#
#   bash scripts/each_ruler.sh 'BOARD "$id" --dry-run ... < library.json > /dev/null'
#   bash scripts/each_ruler.sh --self-test
#
# The body is bash with `$id` set to the ruler; what it needs from the caller is
# exported. Each ruler's walk of the library is CPU alone — a minute and a half
# of validating every build, the facts read in seconds — and one after another
# on a runner of four cores that was a third of an hourly run. Each body's
# output is kept apart and printed in roster order once all are done, so a log
# reads as it did serially; any body failing fails the whole.
set -euo pipefail

BOARD="${WFSIM_BOARD:-./target/release/wfsim-board}"
JOBS="${EACH_RULER_JOBS:-$(nproc 2>/dev/null || echo 2)}"

each_ruler() {
  local body="$1" dir id failed=0
  dir=$(mktemp -d)
  "$BOARD" --rulers | tr -d '\r' > "$dir/roster"
  export EACH_BODY="$body" EACH_DIR="$dir"
  xargs -P "$JOBS" -I{} bash -c 'id="$1"; export id; if bash -c "$EACH_BODY" > "$EACH_DIR/$id.log" 2>&1; then :; else echo $? > "$EACH_DIR/$id.failed"; fi' _ {} < "$dir/roster"
  while IFS= read -r id; do
    cat "$dir/$id.log"
    if [ -e "$dir/$id.failed" ]; then
      echo "::error::$id: exit $(cat "$dir/$id.failed")"
      failed=1
    fi
  done < "$dir/roster"
  rm -rf "$dir"
  return "$failed"
}

self_test() {
  local ok=0 bad=0 tmp out
  say() { if [ "$1" = ok ]; then ok=$((ok + 1)); else bad=$((bad + 1)); fi; echo "  $1    $2"; }
  tmp=$(mktemp -d)
  printf '#!/usr/bin/env bash\nprintf "c\\nb\\na\\n"\n' > "$tmp/board"; chmod +x "$tmp/board"
  BOARD="$tmp/board" JOBS=3
  out=$(each_ruler 'sleep "$([ "$id" = c ] && echo 1 || echo 0)"; echo "ruler $id"') \
    && [ "$out" = "$(printf 'ruler c\nruler b\nruler a')" ] \
    && say ok "every ruler runs once, its output in roster order whoever finished first" || say FAIL "$out"
  export SEEN="$tmp/seen"
  each_ruler 'echo "$id" >> "$SEEN"' > /dev/null
  [ "$(sort "$SEEN" | tr '\n' ,)" = "a,b,c," ] && say ok "...with what the caller exported" || say FAIL "$(cat "$SEEN")"
  if out=$(each_ruler '[ "$id" != b ] || exit 3; echo "ruler $id"'); then
    say FAIL "a failed ruler passed"
  else
    printf '%s' "$out" | grep -q '::error::b: exit 3' && printf '%s' "$out" | grep -q 'ruler a' \
      && say ok "a ruler that fails fails the whole, and says which, the others still run" || say FAIL "$out"
  fi
  rm -rf "$tmp"
  echo
  echo "$ok ok, $bad failed"
  [ "$bad" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then self_test; exit $?; fi
each_ruler "${1:?usage: each_ruler.sh <body, with \$id the ruler>}"
