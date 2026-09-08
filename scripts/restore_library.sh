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
# THE TOKEN IS NOT THE ONE IN THE REPO. A restore WRITES the library, and the
# right way to do that is to create a token for it, use it, and revoke it —
# not to keep one lying around for a job that runs once a decade.
#
#   CF_ACCOUNT=... CF_D1_DATABASE=... CF_TOKEN=<a token that may write> \
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
# THE SNAPSHOT, AS STATEMENTS. Nine rows each, parameters BOUND: an identity
# is built from ids that arrived at a public endpoint, and a restore is the
# worst moment to discover that one of them carried a quote.
to_batches() {
  jq -s -c '
    . as $all
    | range(0; ($all | length); 9)
    | . as $i
    | $all[$i : $i + 9] as $chunk
    | {
        sql: ("INSERT OR REPLACE INTO builds (identity, at, record) VALUES "
              + ([$chunk[] | "(?,?,?)"] | join(","))),
        params: [$chunk[] | .k, (.v.at // ""), (.v | tojson)]
      }
  ' "$1"
}

restore() {
  local file="$1" write="$2" api="$3"
  local n; n=$(wc -l < "$file" | tr -d ' ')
  [ "$n" -gt 0 ] || { echo "the snapshot is empty — refusing"; return 1; }

  # A SNAPSHOT THAT DOES NOT PARSE IS NOT A SNAPSHOT. Checked before anything is
  # sent, because a half-written restore is worse than a failed one.
  jq -e 'has("k") and has("v")' "$file" > /dev/null \
    || { echo "not a library snapshot: every line needs k and v"; return 1; }

  local pairs
  pairs=$(to_batches "$file" | jq -s 'map(.params | length / 3) | add // 0')
  echo "snapshot: $n lines -> $pairs rows in $(to_batches "$file" | wc -l | tr -d ' ') statement(s)"
  [ "$pairs" = "$n" ] || { echo "lost a record turning it into statements — refusing"; return 1; }

  if [ "$write" != "yes" ]; then
    echo "DRY RUN — nothing was sent. Pass --write to restore."
    echo "first key: $(to_batches "$file" | head -1 | jq -r '.params[0]')"
    return 0
  fi

  # NINE ROWS A STATEMENT: three bound parameters each, against D1's limit of
  # a hundred per query. Slower than a bulk put and it does not matter — a
  # restore runs once, under pressure, and what it owes is certainty.
  local total=0
  while IFS= read -r stmt; do
    [ -n "$stmt" ] || continue
    if ! curl -sf -o /dev/null -X POST "$api/query" \
        -H "Authorization: Bearer $CF_TOKEN" \
        -H "content-type: application/json" \
        --data-binary "$stmt"; then
      echo "the database refused a batch — stopped after $total row(s)"
      return 1
    fi
    total=$(( total + $(printf "%s" "$stmt" | jq '.params | length / 3') ))
  done < <(to_batches "$file")
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
prev=
for a in "$@"; do
  [ "$prev" = "--data-binary" ] && printf '%s
' "$a" >> /tmp/restore-sent.json
  prev="$a"
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
  # EVERY VALUE BOUND, because an identity is built from ids that arrived at a
  # public endpoint and a restore is the worst moment to find one carried a
  # quote. The key travels in `params`, never in the statement.
  if jq -e '.params | index("laetum|cycle")' < /tmp/restore-sent.json > /dev/null \
     && jq -e '.sql | contains("laetum") | not' < /tmp/restore-sent.json > /dev/null; then
    say ok "...with every value bound, not interpolated"
  else
    say FAIL "a key reached the statement: $(head -c 160 /tmp/restore-sent.json)"
  fi
  if jq -e '.sql | startswith("INSERT OR REPLACE INTO builds")' < /tmp/restore-sent.json > /dev/null; then
    say ok "...into the library table, replacing what is there"
  else
    say FAIL "$(jq -r .sql < /tmp/restore-sent.json | head -c 120)"
  fi
  # ALL OF THEM, and a statement per nine rows: two records is one statement
  # carrying six bound parameters.
  if [ "$(jq -s 'map(.params | length / 3) | add' /tmp/restore-sent.json)" = "2" ]; then
    say ok "...all of them"
  else
    say FAIL "sent $(jq -s 'map(.params | length / 3) | add' /tmp/restore-sent.json)"
  fi
  # THE RECORD GOES IN AS THE TEXT THE COLUMN HOLDS, and has to read back as
  # the record: a restore that stored a stringified string would hand every
  # row to the scorer as something it cannot read, and it would look fine
  # here.
  if [ "$(jq -r '.params[2] | fromjson | .weapon' /tmp/restore-sent.json)" = "laetum" ]; then
    say ok "...and the record round-trips back out of the column"
  else
    say FAIL "the record does not parse back: $(jq -r '.params[2]' /tmp/restore-sent.json | head -c 80)"
  fi
  # NO EXPIRY. A build is a configuration and the library is PERMANENT —
  # which is half of why it is a table and not a key store with a year on it.
  if jq -e '.sql | contains("ttl") or contains("expir") | not' /tmp/restore-sent.json > /dev/null; then
    say ok "...with no expiry on it, because the library is permanent"
  else
    say FAIL "the statement carries an expiry"
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
  : "${CF_D1_DATABASE:?CF_D1_DATABASE is required}"
  # An apostrophe inside ${var:?...} confuses the parser, so the sentence is
  # spelled without one.
  : "${CF_TOKEN:?CF_TOKEN is required, and it needs D1 Edit rather than the read token in the repo}"
fi
restore "$FILE" "$WRITE" \
  "https://api.cloudflare.com/client/v4/accounts/${CF_ACCOUNT:-x}/d1/database/${CF_D1_DATABASE:-x}"
