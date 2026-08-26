#!/usr/bin/env bash
# PUTTING THE LIBRARY BACK — the other half of `.github/workflows/backup.yml`.
#
# A BACKUP NOBODY HAS RESTORED IS A HOPE, not a backup. This is the restore, it
# is DRY BY DEFAULT, and `--self-test` exercises the whole path against a stub
# so the day it is needed is not the first day it has ever run.
#
#   scripts/restore_library.sh --self-test          # no network, no secrets
#   scripts/restore_library.sh library.ndjson       # says what it WOULD write
#   scripts/restore_library.sh library.ndjson --write
#
# WHERE THE FILE COMES FROM, in the order you would reach for them:
#
#   git clone --branch library-backups <repo>       # every night, for ever
#   gh run download --name library-<n>              # the last 90 days
#
# THE TOKEN IS NOT THE ONE IN THE REPO. `CF_API_TOKEN` grants *Workers KV
# Storage: Read* and nothing else, which is why a compromised repo cannot touch
# the library. A restore needs *Write*, and the right way to do it is to create
# that token, use it, and revoke it — not to keep one lying around for a job
# that runs once a decade.
#
#   CF_ACCOUNT=... CF_NAMESPACE=... CF_TOKEN=<a WRITE token> \
#     scripts/restore_library.sh library.ndjson --write
#
# IT WRITES IN BULK, which is the one place KV is generous: there is no bulk
# READ (which is why a full fetch costs 7.5 minutes) but there IS a bulk write
# of up to 10,000 pairs per request, so putting 2,474 records back is a single
# call rather than 2,474.
#
# IT IS ADDITIVE, NEVER DESTRUCTIVE. It writes the records in the file and
# touches nothing else — a key that exists in KV and not in the snapshot is left
# alone. Restoring an old snapshot therefore cannot delete newer submissions,
# which is the failure a restore is most likely to cause and the one nobody
# thinks about while restoring.
set -euo pipefail

# The same expiry the endpoint writes with, so a restored record ages exactly
# as it would have. Without it a restore would make every record immortal.
TTL=$(( 60 * 60 * 24 * 365 ))

# ---- turn a snapshot into KV's bulk-write shape ----------------------------
#
# `{"k": ..., "v": {...}}` per line becomes `[{"key": ..., "value": "<json>"}]`.
# The VALUE IS A STRING, which is KV's own shape: it stores bytes, and the
# record was written with `JSON.stringify` by the worker. Getting this wrong
# would store a record that reads back as an object-shaped string.
to_bulk() {
  jq -sc --argjson ttl "$TTL" \
    'map({key: .k, value: (.v | tojson), expiration_ttl: $ttl})' "$1"
}

restore() {
  local file="$1" write="$2" api="$3"
  local n; n=$(wc -l < "$file" | tr -d ' ')
  [ "$n" -gt 0 ] || { echo "the snapshot is empty — refusing"; return 1; }

  # A SNAPSHOT THAT DOES NOT PARSE IS NOT A SNAPSHOT. Checked before anything is
  # sent, because a half-written restore is worse than a failed one.
  jq -e 'has("k") and has("v")' "$file" > /dev/null \
    || { echo "not a library snapshot: every line needs k and v"; return 1; }

  to_bulk "$file" > bulk.json
  local pairs; pairs=$(jq 'length' bulk.json)
  echo "snapshot: $n lines -> $pairs pairs"
  [ "$pairs" = "$n" ] || { echo "lost a record turning it into pairs — refusing"; return 1; }

  if [ "$write" != "yes" ]; then
    echo "DRY RUN — nothing was sent. Pass --write to restore."
    echo "first key: $(jq -r '.[0].key' bulk.json)"
    return 0
  fi

  # KV's bulk write takes up to 10,000 pairs; split anyway, so this does not
  # quietly stop working the day the library passes that.
  local total=0 i=0
  while :; do
    jq -c --argjson i "$i" '.[$i:($i+10000)]' bulk.json > chunk.json
    local m; m=$(jq 'length' chunk.json)
    [ "$m" -gt 0 ] || break
    curl -sf -X PUT "$api/bulk" \
      -H "Authorization: Bearer $CF_TOKEN" \
      -H "Content-Type: application/json" \
      --data-binary @chunk.json > /dev/null
    total=$(( total + m )); i=$(( i + 10000 ))
    echo "  wrote $total / $pairs"
  done
  echo "restored $total records"
}

# ---- the self-test ---------------------------------------------------------
#
# The whole path, against a stub `curl` that records what it was sent. It exists
# because a restore is run once, under pressure, by someone who has just lost
# data — which is the worst possible moment to discover a typo.
self_test() {
  local dir; dir=$(mktemp -d)
  mkdir -p "$dir/bin" "$dir/work"
  cat > "$dir/bin/curl" <<'STUB'
#!/usr/bin/env bash
for a in "$@"; do
  case "$a" in --data-binary) : ;; @*) cp "${a#@}" /tmp/restore-sent.json;; esac
done
exit 0
STUB
  chmod +x "$dir/bin/curl"
  PATH="$dir/bin:$PATH"
  cd "$dir/work"
  local fails=0
  say() { printf '  %-5s %s\n' "$1" "$2"; [ "$1" = "FAIL" ] && fails=$((fails + 1)); return 0; }

  printf '%s\n' \
    '{"k":"laetum|cycle","v":{"at":"2026-08-26","weapon":"laetum","mods":["serration"]}}' \
    '{"k":"torid|base","v":{"at":"2026-08-25","weapon":"torid","mods":[]}}' > snap.ndjson

  # DRY IS THE DEFAULT, and it has to send nothing at all.
  rm -f /tmp/restore-sent.json
  restore snap.ndjson no x > /dev/null
  [ ! -f /tmp/restore-sent.json ] && say ok "a dry run sends nothing" \
    || say FAIL "a dry run sent a request"

  CF_TOKEN=stub restore snap.ndjson yes x > /dev/null
  [ -f /tmp/restore-sent.json ] && say ok "a write run sends the records" \
    || say FAIL "a write run sent nothing"
  if [ -f /tmp/restore-sent.json ]; then
    [ "$(jq 'length' /tmp/restore-sent.json)" = "2" ] && say ok "...all of them" \
      || say FAIL "sent $(jq 'length' /tmp/restore-sent.json)"
    # THE VALUE IS A STRING. KV stores bytes; a record sent as an object comes
    # back as something the scorer cannot read, and it would look fine here.
    [ "$(jq -r '.[0].value | type' /tmp/restore-sent.json)" = "string" ] \
      && say ok "...with the record as a json STRING, the way KV stores it" \
      || say FAIL "value is $(jq -r '.[0].value | type' /tmp/restore-sent.json)"
    [ "$(jq -r '.[0].value | fromjson | .weapon' /tmp/restore-sent.json)" = "laetum" ] \
      && say ok "...and it round-trips back to the record" \
      || say FAIL "the value does not parse back"
    [ "$(jq -r '.[0].expiration_ttl' /tmp/restore-sent.json)" = "31536000" ] \
      && say ok "...carrying the same year of expiry the endpoint writes" \
      || say FAIL "ttl is $(jq -r '.[0].expiration_ttl' /tmp/restore-sent.json)"
  fi

  # A FILE THAT IS NOT A SNAPSHOT MUST BE REFUSED BEFORE ANYTHING IS SENT.
  echo '{"nonsense":1}' > bad.ndjson
  rm -f /tmp/restore-sent.json
  if CF_TOKEN=stub restore bad.ndjson yes x > /dev/null 2>&1; then
    say FAIL "a file that is not a snapshot was accepted"
  else
    [ ! -f /tmp/restore-sent.json ] && say ok "a file that is not a snapshot is refused, before sending" \
      || say FAIL "it was refused after sending"
  fi

  : > empty.ndjson
  CF_TOKEN=stub restore empty.ndjson yes x > /dev/null 2>&1 \
    && say FAIL "an empty snapshot was accepted" \
    || say ok "an empty snapshot is refused"

  cd /; rm -rf "$dir"
  if [ "$fails" -gt 0 ]; then echo "$fails failed"; return 1; fi
  echo "the library can be put back"
}

if [ "${1:-}" = "--self-test" ]; then
  self_test
  exit $?
fi

FILE="${1:-}"
[ -n "$FILE" ] && [ -f "$FILE" ] || {
  echo "usage: scripts/restore_library.sh <library.ndjson> [--write]"
  exit 2
}
WRITE=no
[ "${2:-}" = "--write" ] && WRITE=yes
if [ "$WRITE" = "yes" ]; then
  : "${CF_ACCOUNT:?CF_ACCOUNT is required}"
  : "${CF_NAMESPACE:?CF_NAMESPACE is required}"
  # An apostrophe inside ${var:?...} confuses the parser, so the sentence is
  # spelled without one.
  : "${CF_TOKEN:?CF_TOKEN is required, and it needs KV Write rather than the read token in the repo}"
fi
restore "$FILE" "$WRITE" \
  "https://api.cloudflare.com/client/v4/accounts/${CF_ACCOUNT:-x}/storage/kv/namespaces/${CF_NAMESPACE:-x}"
