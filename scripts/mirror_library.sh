#!/usr/bin/env bash
# THE LIBRARY, MIRRORED INTO THE DATABASE, AND THE COUNT THAT PROVES IT.
#
#   scripts/mirror_library.sh out/library.ndjson
#   scripts/mirror_library.sh --self-test
#
# KV IS STILL THE AUTHORITY. Nothing reads this copy — the scorer and the board
# read KV — so the mirror can be wrong without anything published being wrong.
# What it may not do is be wrong SILENTLY, which is what the count is for.
#
# IT RUNS OFF THE BACKUP'S SNAPSHOT rather than fetching its own: that file
# already carries the KEY beside each record, and recomputing `identity()` here
# would be a second implementation of the one thing that must not drift.
set -euo pipefail

# THREE BOUND PARAMETERS A ROW, and D1 takes a hundred in one query.
BATCH=30

configured() {
  [ -n "${CF_ACCOUNT:-}" ] && [ -n "${CF_D1_DATABASE:-}" ] && [ -n "${CF_TOKEN:-}" ]
}

d1() {
  curl -sf -X POST \
    -H "Authorization: Bearer ${CF_TOKEN:-x}" \
    -H "content-type: application/json" \
    --data-binary "$1" \
    "https://api.cloudflare.com/client/v4/accounts/${CF_ACCOUNT:-x}/d1/database/${CF_D1_DATABASE:-x}/query"
}

# ---- the upsert -------------------------------------------------------------
#
# PARAMETERS ARE BOUND, NEVER INTERPOLATED. A record is arbitrary json that
# arrived at a public endpoint, so a build whose id carried a quote would end
# the statement — the one place in this pipeline where untrusted text meets a
# language.
#
# `INSERT OR REPLACE` on the identity, so running this twice is running it once:
# the key IS the build, and re-sending a record can only reproduce it.
batches() {
  jq -s -c --argjson n "$BATCH" '
    . as $all
    | range(0; ($all | length); $n)
    | . as $i
    | $all[$i : $i + $n] as $chunk
    | {
        sql: ("INSERT OR REPLACE INTO builds (identity, at, record) VALUES "
              + ([$chunk[] | "(?,?,?)"] | join(","))),
        params: [$chunk[] | .k, (.v.at // ""), (.v | tojson)]
      }
  ' "$1"
}

upsert() {
  local src="$1" sent=0 failed=0
  # A SNAPSHOT WITH NOTHING IN IT IS A FAILURE, never a quiet success. Zero
  # batches is what a missing file, an empty artifact and a mangled path all
  # look like, and every one of them means the mirror did not happen.
  if [ ! -s "$src" ]; then
    echo "::error::mirror: $src holds no records — nothing was mirrored"
    return 1
  fi
  while IFS= read -r body; do
    [ -n "$body" ] || continue
    if d1 "$body" > /dev/null 2>&1; then sent=$((sent + 1)); else failed=$((failed + 1)); fi
  done < <(batches "$src")
  echo "mirror: $sent batches written, $failed refused"
  [ "$failed" -eq 0 ]
}

# ---- the count, and which direction a gap is allowed to go ------------------
#
# D1 MAY HOLD MORE THAN KV AND MAY NOT HOLD LESS. Records expire out of KV after
# a year and nothing expires out of the database, so the surplus is the library
# being permanent — which is what it is for. A DEFICIT is the mirror failing,
# and that is the only direction worth a red run.
two_way_count() {
  local src="$1" here there
  here=$(wc -l < "$src" | tr -d ' ')
  there=$(d1 '{"sql":"SELECT COUNT(*) AS n FROM builds"}' \
    | jq -r '.result[0].results[0].n // empty' 2>/dev/null || true)
  if [ -z "$there" ]; then
    echo "::error::mirror: the database would not say how many rows it holds"
    return 1
  fi
  if [ "$there" -lt "$here" ]; then
    echo "::error::mirror: the database holds $there rows against $here in the snapshot."
    echo "::error::a deficit is the mirror failing — see scripts/mirror_library.sh."
    return 1
  fi
  echo "library: $here live in KV, $there held in the database ($((there - here)) expired but kept)"
}

# ---- self-test --------------------------------------------------------------
#
# A stub `curl` on PATH, so what is exercised is the batching and the counting
# rather than a mock of them.
self_test() {
  # NOT `local`: the trap fires after this function has returned, and a local
  # is out of scope by then — under `set -u` that is an unbound variable at
  # exit, which prints after the summary and reads as a failing check.
  ok=0; bad=0
  DIR=$(mktemp -d)
  trap 'rm -rf "$DIR"' EXIT
  mkdir -p "$DIR/bin" "$DIR/work"
  say() { if [ "$1" = ok ]; then ok=$((ok + 1)); else bad=$((bad + 1)); fi; echo "  $1    $2"; }
  cd "$DIR/work"

  printf '{"k":"a","v":{"at":"2026-01-01","weapon":"braton"}}\n' > lib.ndjson
  printf '{"k":"b\\"quote","v":{"at":"2026-01-02","weapon":"soma"}}\n' >> lib.ndjson
  for i in 3 4 5 6 7; do
    printf '{"k":"k%s","v":{"at":"2026-01-0%s","weapon":"w%s"}}\n' "$i" "$i" "$i" >> lib.ndjson
  done

  BATCH=3
  local n
  n=$(batches lib.ndjson | wc -l | tr -d ' ')
  [ "$n" = "3" ] && say ok "seven records at three a batch is three requests" \
    || say FAIL "batched into $n requests"

  local first
  first=$(batches lib.ndjson | head -1)
  [ "$(printf '%s' "$first" | jq -r '.params | length')" = "9" ] \
    && say ok "...each carrying three parameters a row" \
    || say FAIL "first batch had $(printf '%s' "$first" | jq -r '.params|length') parameters"

  # THE ASSERTION THE ESCAPING IS FOR: a key with a quote in it reaches the
  # database as a PARAMETER and never as text inside the statement.
  printf '%s' "$first" | jq -e '.sql | contains("quote") | not' >/dev/null \
    && printf '%s' "$first" | jq -e '.params | index("b\"quote")' >/dev/null \
    && say ok "...and a key with a quote in it is bound, not interpolated" \
    || say FAIL "a quoted key reached the sql"

  printf '%s' "$first" | jq -e '.sql | test("VALUES \\(\\?,\\?,\\?\\),\\(\\?,\\?,\\?\\),\\(\\?,\\?,\\?\\)$")' >/dev/null \
    && say ok "...and the statement is placeholders only" \
    || say FAIL "sql was: $(printf '%s' "$first" | jq -r .sql)"

  export PATH="$DIR/bin:$PATH"
  export CF_ACCOUNT=a CF_D1_DATABASE=d CF_TOKEN=t

  cat > "$DIR/bin/curl" <<'AHEAD'
#!/usr/bin/env bash
printf '{"result":[{"results":[{"n":9}],"success":true}],"success":true}'
AHEAD
  chmod +x "$DIR/bin/curl"
  two_way_count lib.ndjson > out.txt 2>&1 \
    && grep -q "2 expired but kept" out.txt \
    && say ok "a database ahead of KV is the library being permanent" \
    || say FAIL "$(cat out.txt)"

  cat > "$DIR/bin/curl" <<'SHORT'
#!/usr/bin/env bash
printf '{"result":[{"results":[{"n":4}],"success":true}],"success":true}'
SHORT
  chmod +x "$DIR/bin/curl"
  # IF/ELIF, NOT A SUBSHELL. `say` in `( ... )` increments a counter the parent
  # never sees, so the line printed ok and the summary counted nothing.
  if two_way_count lib.ndjson > out.txt 2>&1; then
    say FAIL "a short database passed"
  elif grep -q "deficit" out.txt; then
    say ok "...and a database behind it is a failure"
  else
    say FAIL "$(cat out.txt)"
  fi

  cat > "$DIR/bin/curl" <<'DEAD'
#!/usr/bin/env bash
exit 22
DEAD
  chmod +x "$DIR/bin/curl"
  two_way_count lib.ndjson > out.txt 2>&1 \
    && say FAIL "an unreachable database passed" \
    || say ok "...and a database that will not answer is a failure"
  if upsert lib.ndjson > out.txt 2>&1; then
    say FAIL "a refused write passed"
  elif grep -q "3 refused" out.txt; then
    say ok "...and a refused batch is counted and reported"
  else
    say FAIL "$(cat out.txt)"
  fi

  : > empty.ndjson
  if upsert empty.ndjson > out.txt 2>&1; then
    say FAIL "an empty snapshot passed"
  elif grep -q "holds no records" out.txt; then
    say ok "an empty snapshot is a failure, not a quiet success"
  else
    say FAIL "$(cat out.txt)"
  fi
  if upsert no-such-file.ndjson > out.txt 2>&1; then
    say FAIL "a missing snapshot passed"
  else
    say ok "...and so is one that is not there"
  fi

  unset CF_D1_DATABASE
  configured && say FAIL "unconfigured read as configured" \
    || say ok "no database configured is a working state"

  echo
  echo "$ok ok, $bad failed"
  [ "$bad" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then self_test; exit $?; fi

# UNCONFIGURED IS SILENT AND GREEN, the rule every other job here follows:
# without a database there is nowhere to mirror to, and reddening a backup for
# a copy nobody reads yet teaches people to ignore the colour.
if ! configured; then
  echo "no library database configured — nothing mirrored (docs/BOARD.md §Setup)"
  exit 0
fi

# NO BRACES IN THE MESSAGE. `${1:?...}` ends at the FIRST `}`, so a message
# naming the record shape closed the expansion early and the tail of the
# sentence was appended to the PATH — which then failed to open under a name
# that reads as a corrupted argument rather than as a quoting bug.
SRC="${1:?the snapshot to mirror, one record a line}"
upsert "$SRC"
two_way_count "$SRC"
