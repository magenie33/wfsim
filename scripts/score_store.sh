#!/usr/bin/env bash
# SCORES OUTLIVE THE RUN THAT COMPUTED THEM.
#
# A shard writes what it scored here the moment it finishes; every later run
# reads the lot back before deciding what is left to do. That is the whole of
# it, and what it buys is that a run cancelled at 95% has banked 95% of its
# work instead of throwing all of it away.
#
#   scripts/score_store.sh put <file>    # CF_* and ENGINE_FP in the env
#   scripts/score_store.sh get <dir>
#   scripts/score_store.sh --self-test
#
# A SEPARATE NAMESPACE FROM THE SUBMISSIONS, and that is not tidiness. The
# submissions are the one irreplaceable thing in this pipeline; scores are
# derived and can always be recomputed. Sharing a namespace would put score
# blobs into the listing `fetch_submissions.sh` walks, where they are neither
# builds nor a thing it could refuse cleanly.
#
# UNCONFIGURED IS A WORKING STATE, checked before anything else: with no
# namespace both verbs succeed doing nothing, and the pipeline is exactly what
# it was before the store existed. That is the rollback, and it is one secret.
#
# THE ENGINE FINGERPRINT IS THE PREFIX, so a code change makes every stored
# score unreachable rather than wrong — the reader would refuse them anyway
# (`--scores-engine`), and this way it does not pay to fetch them first. What is
# left behind expires: entries are written with a TTL rather than collected,
# because a cleanup pass is a second thing that can fail and a stale blob under
# an old prefix costs nothing but bytes.
set -euo pipefail

# TEN DAYS. Long enough that a fingerprint can sit unpublished over a weekend
# and still find its work, short enough that abandoned generations do not
# accumulate. KV's own floor is 60 seconds.
TTL="${SCORE_TTL_SECONDS:-864000}"

api_base() {
  printf 'https://api.cloudflare.com/client/v4/accounts/%s/storage/kv/namespaces/%s' \
    "${CF_ACCOUNT:-}" "${CF_SCORES_NAMESPACE:-}"
}

configured() {
  [ -n "${CF_ACCOUNT:-}" ] && [ -n "${CF_SCORES_NAMESPACE:-}" ] && [ -n "${CF_TOKEN:-}" ] \
    && [ -n "${ENGINE_FP:-}" ]
}

# WHAT THIS BLOB IS CALLED. The run and the shard, under the engine that
# computed it: unique without coordination, which is what lets thirty-two shards
# write at once without knowing about each other.
blob_key() {
  printf 's/%s/%s-%s' "$ENGINE_FP" "${GITHUB_RUN_ID:-local}" "${SHARD_ID:-0}"
}

# NO `jq` HERE, unlike `fetch_submissions.sh`. That script encodes SUBMISSION
# keys — build identities carrying `|`, `,` and `+` — where these are ours and
# are `s/<hex>/<alnum>-<int>`, so a slash is the only character needing escape.
# What it buys is a self-test that runs anywhere bash does, which is where it
# actually gets run.
urlenc() { printf '%s' "${1//\//%2F}"; }
# One repeated field out of a flat JSON object, by name. Enough for a listing.
field() { grep -o "\"$1\":\"[^\"]*\"" | sed 's/.*:"//;s/"$//'; }

put_scores() {
  local file="$1"
  [ -s "$file" ] || { echo "score_store: $file is empty, nothing stored"; return 0; }
  configured || { echo "score_store: not configured, nothing stored"; return 0; }
  local key; key=$(blob_key)
  # A FAILED WRITE IS NOT A FAILED RUN. The scores are still in this run's
  # artifacts and the board is still assembled from them; what is lost is the
  # banking, so the next run recomputes what this one did. Loud, and green.
  if curl -sf -X PUT -H "Authorization: Bearer $CF_TOKEN" \
       -F "value=@$file" -F "metadata={}" \
       "$(api_base)/values/$(urlenc "$key")?expiration_ttl=$TTL" \
       > /dev/null; then
    echo "score_store: stored $(wc -c < "$file") bytes at $key"
  else
    echo "score_store: could not store $key — the next run recomputes this slice"
  fi
}

get_scores() {
  local dir="$1"
  mkdir -p "$dir"
  configured || { echo "score_store: not configured, starting from the boards alone"; return 0; }
  local api cursor="" resp n=0
  api=$(api_base)
  : > "$dir/.keys"
  while :; do
    resp=$(curl -sf -H "Authorization: Bearer $CF_TOKEN" \
      "$api/keys?limit=1000&prefix=$(urlenc "s/$ENGINE_FP/")${cursor:+&cursor=$cursor}") \
      || { echo "score_store: listing failed, starting from the boards alone"; return 0; }
    # `|| true` on both: a listing with no keys and one with no cursor are the
    # ORDINARY end states, and `grep` reports "found nothing" as a failure —
    # which under `set -e` ended the script before it read anything at all.
    printf '%s' "$resp" | field name >> "$dir/.keys" || true
    cursor=$(printf '%s' "$resp" | field cursor || true)
    [ -n "$cursor" ] || break
  done
  while read -r key; do
    [ -n "$key" ] || continue
    # A MISS IS ORDINARY: the entry expired between the listing and the read, or
    # the read failed. The row it held is simply scored again.
    if curl -sf -H "Authorization: Bearer $CF_TOKEN" \
         "$api/values/$(urlenc "$key")" \
         > "$dir/$(printf '%s' "$key" | tr '/' '_').json"; then
      n=$(( n + 1 ))
    else
      rm -f "$dir/$(printf '%s' "$key" | tr '/' '_').json"
    fi
  done < "$dir/.keys"
  rm -f "$dir/.keys"
  echo "score_store: read $n blob(s) for engine $ENGINE_FP"
}

# ---- the self-test ---------------------------------------------------------
#
# A stub `curl` in a temp directory, for the same reason `fetch_submissions.sh`
# has one: the thing tested has to be the thing that runs. What is asserted is
# the SHAPE — unconfigured is silent and green, a put names the run and shard
# under the engine, a get writes one file per listed key, and a failing read
# leaves no file behind rather than an empty one.
self_test() {
  local dir; dir=$(mktemp -d)
  trap 'rm -rf "$dir"' RETURN
  mkdir -p "$dir/bin"
  cat > "$dir/bin/curl" <<'STUB'
#!/usr/bin/env bash
url=""
for a in "$@"; do case "$a" in https://*) url="$a";; esac; done
case "$url" in
  *"/keys?"*) printf '{"result":[{"name":"s/ENG/r1-0"},{"name":"s/ENG/r1-1"},{"name":"s/ENG/gone"}],"result_info":{}}'; exit 0;;
  *"/values/s%2FENG%2Fgone"*) exit 22;;
  *"/values/"*) printf '{"benchmark":"single_target","scores":{}}'; exit 0;;
esac
exit 1
STUB
  chmod +x "$dir/bin/curl"
  local bad=0
  check() {
    if [ "$2" = "1" ]; then echo "  ok    $1"; else echo "  FAIL  $1${3:+  — $3}"; bad=1; fi
  }

  printf '{"scores":{"a":"1"}}' > "$dir/one.json"

  # 1. Unconfigured is a working state.
  local out
  out=$(CF_ACCOUNT="" CF_SCORES_NAMESPACE="" CF_TOKEN="" ENGINE_FP="" \
    bash "$0" put "$dir/one.json" 2>&1) || bad=1
  check "unconfigured put is silent and green" \
    "$([ "${out#*nothing stored}" != "$out" ] && echo 1 || echo 0)" "$out"
  out=$(CF_ACCOUNT="" CF_SCORES_NAMESPACE="" CF_TOKEN="" ENGINE_FP="" \
    bash "$0" get "$dir/in" 2>&1) || bad=1
  check "unconfigured get is silent and green" \
    "$([ "${out#*boards alone}" != "$out" ] && echo 1 || echo 0)" "$out"

  # 2. A put names the run and the shard under the engine.
  out=$(PATH="$dir/bin:$PATH" CF_ACCOUNT=a CF_SCORES_NAMESPACE=n CF_TOKEN=t \
    ENGINE_FP=ENG GITHUB_RUN_ID=r9 SHARD_ID=7 bash "$0" put "$dir/one.json" 2>&1) || bad=1
  check "a put is keyed by engine, run and shard" \
    "$([ "${out#*s/ENG/r9-7}" != "$out" ] && echo 1 || echo 0)" "$out"

  # 3. An empty file stores nothing rather than an empty blob.
  : > "$dir/empty.json"
  out=$(PATH="$dir/bin:$PATH" CF_ACCOUNT=a CF_SCORES_NAMESPACE=n CF_TOKEN=t \
    ENGINE_FP=ENG bash "$0" put "$dir/empty.json" 2>&1) || bad=1
  check "an empty score file stores nothing" \
    "$([ "${out#*is empty}" != "$out" ] && echo 1 || echo 0)" "$out"

  # 4. A get writes one file per key it could read, and none for the one it could not.
  PATH="$dir/bin:$PATH" CF_ACCOUNT=a CF_SCORES_NAMESPACE=n CF_TOKEN=t \
    ENGINE_FP=ENG bash "$0" get "$dir/in" > /dev/null 2>&1 || bad=1
  local n; n=$(find "$dir/in" -name '*.json' | wc -l)
  check "a get writes one file per readable key" "$([ "$n" = "2" ] && echo 1 || echo 0)" "got $n"
  check "a key that could not be read leaves no file" \
    "$([ ! -f "$dir/in/s_ENG_gone.json" ] && echo 1 || echo 0)" "an empty file was left behind"

  echo ""
  [ "$bad" = "0" ] && echo "the store banks what a run computed" || echo "score_store self-test failed"
  return "$bad"
}

case "${1:-}" in
  put) put_scores "${2:?usage: score_store.sh put <file>}" ;;
  get) get_scores "${2:?usage: score_store.sh get <dir>}" ;;
  --self-test) self_test ;;
  *) echo "usage: score_store.sh put <file> | get <dir> | --self-test" >&2; exit 2 ;;
esac
