#!/usr/bin/env bash
# SCORES OUTLIVE THE RUN THAT COMPUTED THEM.
#
# A shard banks what it scored the moment it finishes; every later run reads the
# lot back before deciding what is left to do. What that buys is that a run
# cancelled at 95% has kept 95% of its work instead of throwing all of it away —
# and it is why "did this run fit in twenty minutes" stops being a question
# about correctness at all.
#
#   scripts/score_store.sh get <dir>            # everything, into one directory
#   scripts/score_store.sh put <file> <key>     # bank one file
#   scripts/score_store.sh sweep                # drop the merged deltas
#   scripts/score_store.sh --self-test
#
# TWO SHAPES UNDER ONE PREFIX, and the difference is who writes them:
#
#   scores/<bench>.json           the merged set, written by `publish` alone
#   scores/delta/<run>-<id>.json  what one shard banked, merged and then swept
#
# WITHOUT THE MERGE THE STORE GROWS WITHOUT BOUND — a delta per shard per
# benchmark per run is a couple of hundred objects an hour, and every run reads
# all of them. Merging turns that back into three objects plus whatever the run
# in flight has banked.
#
# NOTHING HERE KNOWS ABOUT CODE VERSIONS. Every row inside a file carries the
# hash of what it READ, and the reader admits it only where that still matches,
# so a stored score proves itself without anyone declaring which binary wrote
# it. Whether the CODE moved a number is measured by the audit, never asserted.
#
# UNCONFIGURED IS A WORKING STATE, checked before anything else: with no
# endpoint every verb succeeds doing nothing and the pipeline is what it was
# before the store existed. That is the rollback, and it is three secrets.
set -euo pipefail

BUCKET="${R2_BUCKET:-wfsim}"
PREFIX="scores"

configured() {
  [ -n "${R2_ENDPOINT:-}" ] && [ -n "${R2_ACCESS_KEY_ID:-}" ] \
    && [ -n "${R2_SECRET_ACCESS_KEY:-}" ]
}

# THE AWS CLI RATHER THAN A SIGNATURE OF OUR OWN. R2 speaks S3, the CLI is
# preinstalled on the runners, and SigV4 by hand is thirty lines whose failure
# mode is a 403 nobody can read.
#
# THE CHECKSUM SETTINGS ARE NOT DECORATION: recent CLI versions send integrity
# headers R2 rejects outright, and `when_required` holds it to what S3 itself
# asks for. `auto` is R2's region, and the instance-metadata lookup is turned
# off because there is no profile to find and looking costs a timeout.
s3() {
  AWS_ACCESS_KEY_ID="$R2_ACCESS_KEY_ID" \
  AWS_SECRET_ACCESS_KEY="$R2_SECRET_ACCESS_KEY" \
  AWS_DEFAULT_REGION=auto \
  AWS_EC2_METADATA_DISABLED=true \
  AWS_REQUEST_CHECKSUM_CALCULATION=when_required \
  AWS_RESPONSE_CHECKSUM_VALIDATION=when_required \
  aws s3 "$@" --endpoint-url "$R2_ENDPOINT"
}

put_scores() {
  local file="$1" key="$2"
  [ -s "$file" ] || { echo "score_store: $file is empty, nothing banked"; return 0; }
  configured || { echo "score_store: not configured, nothing banked"; return 0; }
  # A FAILED WRITE IS NOT A FAILED RUN. The scores are still in this run's own
  # artifacts and the board is still assembled from them; what is lost is the
  # banking, so a later run recomputes this slice. Loud, and green.
  if s3 cp "$file" "s3://$BUCKET/$PREFIX/$key" > /dev/null; then
    echo "score_store: banked $(wc -c < "$file") bytes at $PREFIX/$key"
  else
    echo "score_store: could not bank $key — a later run recomputes this slice"
  fi
}

get_scores() {
  local dir="$1"
  mkdir -p "$dir"
  configured || { echo "score_store: not configured, starting from the boards alone"; return 0; }
  # FLATTENED, because the reader takes ONE directory and a delta is the same
  # kind of file as a merged set: each says what every row it holds read, and is
  # held to it row by row.
  if s3 sync "s3://$BUCKET/$PREFIX/" "$dir" --no-progress > /dev/null; then
    if [ -d "$dir/delta" ]; then
      find "$dir/delta" -name '*.json' -exec mv -f {} "$dir/" \; 2>/dev/null || true
      rm -rf "$dir/delta"
    fi
    echo "score_store: read $(find "$dir" -maxdepth 1 -name '*.json' | wc -l) file(s)"
  else
    echo "score_store: read failed, starting from the boards alone"
  fi
}

# WHAT THE MERGE HAS MADE REDUNDANT, and only after it is written: a sweep
# before the merged set lands drops work nothing else holds.
sweep_deltas() {
  configured || { echo "score_store: not configured, nothing to sweep"; return 0; }
  if s3 rm "s3://$BUCKET/$PREFIX/delta/" --recursive > /dev/null 2>&1; then
    echo "score_store: swept the merged deltas"
  else
    echo "score_store: nothing to sweep"
  fi
}

# ---- the self-test ---------------------------------------------------------
#
# A stub `aws` in a temp directory, for the reason `fetch_submissions.sh` has
# one: the thing tested has to be the thing that runs. What is asserted is the
# SHAPE — unconfigured is silent and green, a put names the key it was handed,
# an empty file banks nothing, a get flattens what it synced, and a sweep only
# ever names the delta prefix.
self_test() {
  local dir; dir=$(mktemp -d)
  trap 'rm -rf "$dir"' RETURN
  mkdir -p "$dir/bin"
  cat > "$dir/bin/aws" <<'STUB'
#!/usr/bin/env bash
echo "AWS $*" >> "$STUB_LOG"
if [ "$2" = "sync" ]; then
  mkdir -p "$4/delta"
  printf '{}' > "$4/single_target.json"
  printf '{}' > "$4/delta/r1-0.json"
fi
exit 0
STUB
  chmod +x "$dir/bin/aws"
  export STUB_LOG="$dir/log"
  : > "$STUB_LOG"
  local bad=0 out
  check() {
    if [ "$2" = "1" ]; then echo "  ok    $1"; else echo "  FAIL  $1${3:+  — $3}"; bad=1; fi
  }
  has() { if [ "${1#*"$2"}" != "$1" ]; then echo 1; else echo 0; fi; }

  printf '{"scores":{"a":"1"}}' > "$dir/one.json"
  : > "$dir/empty.json"

  out=$(R2_ENDPOINT="" R2_ACCESS_KEY_ID="" R2_SECRET_ACCESS_KEY="" \
    bash "$0" put "$dir/one.json" x.json 2>&1) || bad=1
  check "unconfigured put is silent and green" "$(has "$out" "not configured")" "$out"
  out=$(R2_ENDPOINT="" R2_ACCESS_KEY_ID="" R2_SECRET_ACCESS_KEY="" \
    bash "$0" sweep 2>&1) || bad=1
  check "unconfigured sweep is silent and green" "$(has "$out" "nothing to sweep")" "$out"

  out=$(PATH="$dir/bin:$PATH" R2_ENDPOINT=e R2_ACCESS_KEY_ID=k R2_SECRET_ACCESS_KEY=s \
    bash "$0" put "$dir/empty.json" x.json 2>&1) || bad=1
  check "an empty score file banks nothing" "$(has "$out" "is empty")" "$out"

  : > "$STUB_LOG"
  PATH="$dir/bin:$PATH" R2_ENDPOINT=e R2_ACCESS_KEY_ID=k R2_SECRET_ACCESS_KEY=s \
    bash "$0" put "$dir/one.json" delta/r9-7.json > /dev/null 2>&1 || bad=1
  check "a put names the key it was handed" \
    "$(has "$(cat "$STUB_LOG")" "s3://wfsim/scores/delta/r9-7.json")" "$(cat "$STUB_LOG")"

  PATH="$dir/bin:$PATH" R2_ENDPOINT=e R2_ACCESS_KEY_ID=k R2_SECRET_ACCESS_KEY=s \
    bash "$0" get "$dir/in" > /dev/null 2>&1 || bad=1
  local n; n=$(find "$dir/in" -maxdepth 1 -name '*.json' | wc -l)
  check "a get flattens the deltas into one directory" \
    "$([ "$n" = "2" ] && echo 1 || echo 0)" "got $n"

  : > "$STUB_LOG"
  PATH="$dir/bin:$PATH" R2_ENDPOINT=e R2_ACCESS_KEY_ID=k R2_SECRET_ACCESS_KEY=s \
    bash "$0" sweep > /dev/null 2>&1 || bad=1
  check "a sweep only ever names the delta prefix" \
    "$(has "$(cat "$STUB_LOG")" "scores/delta/ --recursive")" "$(cat "$STUB_LOG")"

  echo ""
  if [ "$bad" = "0" ]; then echo "the store banks what a run computed"
  else echo "score_store self-test failed"; fi
  return "$bad"
}

case "${1:-}" in
  put) put_scores "${2:?usage: score_store.sh put <file> <key>}" "${3:?missing key}" ;;
  get) get_scores "${2:?usage: score_store.sh get <dir>}" ;;
  sweep) sweep_deltas ;;
  --self-test) self_test ;;
  *) echo "usage: score_store.sh get <dir> | put <file> <key> | sweep | --self-test" >&2; exit 2 ;;
esac
