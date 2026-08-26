#!/usr/bin/env bash
# THE LIBRARY IS FETCHED ONCE AND THEN ONLY ADDED TO.
#
# Reads every stored submission out of Cloudflare KV and writes them as
# `submissions.json`, the array the scorer takes on stdin. Between runs it keeps
# `library.json` — a map of key to record — so a run pays only for what arrived
# since the last one.
#
#   scripts/fetch_submissions.sh            # in a checkout, with CF_* in the env
#   scripts/fetch_submissions.sh --self-test
#
# WHY THIS IS NOT INLINE IN THE WORKFLOW. It was, and the only way to exercise
# it was to copy the block out of the yaml by hand and run that — so the thing
# tested was never the thing that ran. It is a file now, and `--self-test` runs
# the four cases against a stub `curl` in a temp directory: cold start, warm
# start with one new key, a key that expired out of KV, and a corrupt cache.
#
# ---------------------------------------------------------------------------
#
# THE PROBLEM IT SOLVES is the shape of the cost rather than its size. KV has no
# bulk read, so this was one HTTP request per stored build — 2,447 of them at
# about 220 ms apiece, **9 minutes of the 20 the schedule allows**, paid on every
# run whether anything had been submitted or not. That is O(the store) against an
# input of O(what is new), so it got worse with every user, and no shard count
# could touch it: it is one unsharded job (owner, 2026-08-26).
#
# 220 ms is not the network. Cloudflare's API allows 1200 requests per five
# minutes, which is 4 a second, which is what this was already doing — so
# PARALLELISING IT WOULD ONLY EARN 429s. The fix has to be fetching fewer, not
# fetching faster.
#
# THE LICENCE FOR CACHING A VALUE IS THAT THE KEY DETERMINES IT. The key is
# `identity(rec)` in `worker/index.js` — every identity-bearing axis of the build
# — and the scorer reads nothing else off a record: not `at` (the submission day,
# which exists only so KV can expire old entries) and not `benchmark` (which
# ruler it arrived under, kept as provenance since the library model made every
# ruler cross the whole store). So a value under a given key cannot come to mean
# something different, and re-fetching it could only reproduce it.
#
# THE LISTING STAYS AUTHORITATIVE. Keys are listed in full every run — 3 requests
# for 2,447 keys, not 2,447 — and the cache is PRUNED to what the listing holds,
# so a record that expired out of KV leaves the library on the next run rather
# than living forever in a cache nobody audits.
set -euo pipefail

# ---- the library, from a key list and whatever is already held --------------
#
# Split out from the fetching so the self-test can drive it with a stub `curl`.
# Everything here is jq over two files; the only I/O is the value fetch.
build_library() {
  local api="$1"

  jq -Rs 'split("\n") | map(select(length > 0) | sub("\r$"; ""))' keys.txt > keylist.json

  # A MISSING OR UNREADABLE CACHE IS AN EMPTY ONE, never a failure: the cold
  # path is exactly the path this had before there was a cache.
  jq -e 'type == "object"' library.json >/dev/null 2>&1 || echo '{}' > library.json

  # PRUNE TO THE LISTING FIRST, before anything reads it.
  jq -s '.[0] as $lib | reduce .[1][] as $k ({};
           . + (if $lib[$k] then {($k): $lib[$k]} else {} end))' \
    library.json keylist.json > lib.pruned && mv lib.pruned library.json

  # …and fetch only what is not already held. `tr -d` is not superstition: ONE
  # stray carriage return makes every key a cache MISS, silently — the run still
  # builds a correct library, it just pays the full price again and reads as
  # "the cache did not help". Found by running this against a Windows jq, which
  # writes CRLF where the runner's does not.
  jq -s -r '.[0] as $lib | .[1][] | select($lib[.] == null)' \
    library.json keylist.json | tr -d '\r' > missing.txt
  echo "library: $(jq 'length' library.json) cached, $(wc -l < missing.txt | tr -d ' ') to fetch"

  : > new.ndjson
  while read -r key; do
    [ -n "$key" ] || continue
    # A value that does not arrive, or does not parse, is simply not added —
    # the next run finds it missing and asks again.
    if v=$(curl -sf -H "Authorization: Bearer ${CF_TOKEN:-x}" --get \
             --data-urlencode "x=1" \
             "$api/values/$(jq -rn --arg v "$key" '$v|@uri')"); then
      printf '%s' "$v" | jq -c --arg k "$key" '{key: $k, val: .}' >> new.ndjson || true
    fi
  done < missing.txt

  # ONE merge rather than one per key: rewriting the library per fetch is
  # quadratic, and a cold start fetches thousands.
  jq -s 'map({(.key): .val}) | add // {}' new.ndjson > new.json
  jq -s '.[0] * .[1]' library.json new.json > lib.merged && mv lib.merged library.json
  jq '[.[]]' library.json > submissions.json
  echo "submissions: $(jq 'length' submissions.json)"
}

# ---- the library may not SHRINK by surprise --------------------------------
#
# THE STORE IS THE ONLY IRREPLACEABLE THING HERE. The boards are derived and can
# be rebuilt from it; the site is generated; the code is in git. The library is
# what players sent, and there is no copy of it anywhere else.
#
# The way it could be destroyed is not a delete — it is a QUIET TRUNCATION. If
# this script returned a short list (a token that half-worked, a listing that
# stopped early, a cache bug), `publish` would build a perfectly valid board out
# of it, commit it over the real one, and every row that was missing from the
# short list would be gone from the board with nothing anywhere saying so.
#
# So a run that comes back with materially fewer builds than the last board was
# built from REFUSES rather than publishing. The floor is 90%: records expire
# after a year, so a slow decline is legal, and losing a tenth of the library
# between two runs twenty minutes apart is not.
#
# It is a tripwire and not a backup, deliberately. A backup restores after the
# damage; this declines to do the damage, which is the only one of the two that
# works while nobody is watching.
guard_shrink() {
  local floor="${1:-0}" have
  have=$(jq 'length' submissions.json)
  if [ "$floor" -gt 0 ] && [ "$have" -lt "$floor" ]; then
    echo "::error::the library came back with $have builds, under the floor of $floor."
    echo "::error::refusing to publish a board from a short list — see scripts/fetch_submissions.sh."
    return 1
  fi
  echo "library floor: $have >= ${floor:-0}"
}

list_keys() {
  local api="$1" cursor="" resp
  : > keys.txt
  while :; do
    resp=$(curl -sf -H "Authorization: Bearer $CF_TOKEN" \
      "$api/keys?limit=1000${cursor:+&cursor=$cursor}")
    printf '%s' "$resp" | jq -r '.result[].name' >> keys.txt
    cursor=$(printf '%s' "$resp" | jq -r '.result_info.cursor // empty')
    [ -n "$cursor" ] || break
  done
}

# ---- the self-test ---------------------------------------------------------
#
# A stub `curl` in a temp directory answers a value request with a record named
# after the key, so every assertion below is about the CACHING rather than about
# Cloudflare. Run it after touching anything above: the failure this guards is
# silent, and a wrong library publishes a wrong board.
self_test() {
  local dir; dir=$(mktemp -d)
  mkdir -p "$dir/bin" "$dir/work"
  cat > "$dir/bin/curl" <<'STUB'
#!/usr/bin/env bash
for a in "$@"; do
  case "$a" in *"/values/"*) printf '{"weapon":"%s","at":"2026-01-01"}' "${a##*/values/}"; exit 0;; esac
done
exit 1
STUB
  chmod +x "$dir/bin/curl"
  PATH="$dir/bin:$PATH"
  cd "$dir/work"
  local fails=0
  say() { printf '  %-5s %s\n' "$1" "$2"; [ "$1" = "FAIL" ] && fails=$((fails + 1)); return 0; }
  got() { jq -r 'map(.weapon) | sort | join(",")' submissions.json; }
  cached_now() { jq 'length' library.json; }

  printf 'a\nb\nc\n' > keys.txt
  build_library x > /dev/null
  [ "$(got)" = "a,b,c" ] && say ok "cold start fetches every key" \
    || say FAIL "cold start: $(got)"

  # WARM: the same three plus one. Only the new one may be fetched, which is
  # the whole point — asserted on the REPORTED count, not inferred.
  printf 'a\nb\nc\nd\n' > keys.txt
  # CAPTURED WHOLE, then read — `| head -1` closes the pipe on the first line
  # and `set -o pipefail` turns that SIGPIPE into a failed run.
  local out line; out=$(build_library x); read -r line <<< "$out"
  [ "$line" = "library: 3 cached, 1 to fetch" ] && say ok "a warm run fetches only what is new" \
    || say FAIL "warm run said: $line"
  [ "$(got)" = "a,b,c,d" ] && say ok "...and still reports the whole library" \
    || say FAIL "warm library: $(got)"

  # EXPIRY: a key that left KV must leave the cache, or it lives for ever in a
  # file nobody audits.
  printf 'a\nc\nd\n' > keys.txt
  build_library x > /dev/null
  [ "$(got)" = "a,c,d" ] && say ok "a key that expired out of KV leaves the library" \
    || say FAIL "after expiry: $(got)"
  [ "$(cached_now)" = "3" ] && say ok "...and is gone from the cache itself" \
    || say FAIL "cache still holds $(cached_now)"

  # A CORRUPT CACHE IS AN EMPTY ONE. The cold path is the old path, so the worst
  # a bad cache can cost is time.
  echo 'not json' > library.json
  printf 'a\nc\n' > keys.txt
  out=$(build_library x); read -r line <<< "$out"
  [ "$line" = "library: 0 cached, 2 to fetch" ] && say ok "a corrupt cache falls back to a full fetch" \
    || say FAIL "corrupt cache said: $line"
  [ "$(got)" = "a,c" ] && say ok "...and still produces the right library" \
    || say FAIL "after corruption: $(got)"

  # THE TRIPWIRE, both ways. A run that lost most of the library must refuse;
  # one that merely grew must not — a guard that fires on a good run is a guard
  # somebody disables.
  printf 'a\nc\n' > keys.txt
  build_library x > /dev/null
  if guard_shrink 100 > /dev/null 2>&1; then
    say FAIL "a truncated library was allowed to publish"
  else
    say ok "a library that shrank refuses to publish"
  fi
  guard_shrink 2 > /dev/null 2>&1 && say ok "...and an intact one passes" \
    || say FAIL "the tripwire fired on a full library"

  cd /; rm -rf "$dir"
  if [ "$fails" -gt 0 ]; then echo "$fails failed"; return 1; fi
  echo "the library is fetched once and then only added to"
}

if [ "${1:-}" = "--self-test" ]; then
  self_test
  exit $?
fi

: "${CF_ACCOUNT:?CF_ACCOUNT is required}"
: "${CF_NAMESPACE:?CF_NAMESPACE is required}"
: "${CF_TOKEN:?CF_TOKEN is required}"
API="https://api.cloudflare.com/client/v4/accounts/$CF_ACCOUNT/storage/kv/namespaces/$CF_NAMESPACE"
list_keys "$API"
build_library "$API"
# The floor comes from the caller, which knows what the last board was built
# from; zero (the default) means "no prior board", which is a legal state.
guard_shrink "${FLOOR:-0}"
