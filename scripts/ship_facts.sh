#!/usr/bin/env bash
# WHAT A SHARD HAS COMPUTED, PUT WHERE NOTHING CAN DESTROY IT.
#
#   scripts/ship_facts.sh facts.ndjson
#   scripts/ship_facts.sh --self-test
#
# `wfsim-board --facts` appends one row per score and flushes per row. This
# reads that file and writes the rows into `scores`, and it is meant to run
# BESIDE the scorer, again and again, so a shard killed at nine tenths keeps
# nine tenths — where before it kept nothing at all (docs/BOARD.md §"The
# pipeline, designed around one rule").
#
# THE CURSOR IS AN OPTIMISATION, NOT CORRECTNESS. The write is
# `INSERT OR REPLACE` on the row's own key, so shipping a line twice writes the
# same row twice and a lost cursor costs one repeat, never a wrong number.
set -euo pipefail

# AS MANY ROWS A STATEMENT AS D1'S HUNDRED BOUND PARAMETERS ALLOW, and the count
# is DERIVED so that a column added below shrinks it. Written as a number it
# goes one parameter over on the twelfth column, and what that buys is every
# write refused and the cursor never moving — loud, but for a reason nobody
# would look for here.
FACT_COLUMNS=11
BATCH=$((100 / FACT_COLUMNS))

configured() {
  [ -n "${CF_ACCOUNT:-}" ] && [ -n "${CF_D1_DATABASE:-}" ] && [ -n "${CF_TOKEN:-}" ]
}

# A REFUSED WRITE HAS TO SAY WHY — the body to a file, the status to a
# variable. `curl -sf` swallows the body, and a token without D1 permission, a
# missing table and a malformed statement then arrive as the same silent
# non-zero.
D1_OUT=""
D1_CODE=""
d1() {
  [ -n "$D1_OUT" ] || D1_OUT=$(mktemp)
  D1_CODE=$(curl -s -o "$D1_OUT" -w '%{http_code}' -X POST \
    -H "Authorization: Bearer ${CF_TOKEN:-x}" \
    -H "content-type: application/json" \
    --data-binary "$1" \
    "https://api.cloudflare.com/client/v4/accounts/${CF_ACCOUNT:-x}/d1/database/${CF_D1_DATABASE:-x}/query") \
    || D1_CODE="000"
  [ "$D1_CODE" = "200" ]
}

# ---- the statements -------------------------------------------------------
#
# PARAMETERS ARE BOUND, NEVER INTERPOLATED. An identity is built from ids that
# arrived at a public endpoint, and this is the one place in the pipeline where
# that text meets a language.
#
# `INSERT OR REPLACE` on (identity, ruler, mode, data_fp): the same row measured
# twice is one row, and a data change produces a NEW row because `data_fp` is in
# the key — so correcting a data file cannot destroy the answer the old file
# produced, and reverting it restores that answer without recomputing.
#
# THE CLOCKS ARE THE SCORER'S, NOT THE SHIPPER'S AND NOT THE DATABASE'S. A row
# may be shipped minutes after it was measured, so a timestamp invented here
# would be the shipping time wearing the fight's name; the fact log carries both
# ends of the fight and this binds them. A strftime call inside the statement is
# refused by D1 outright — "near %: syntax error at offset 164".
batches() {
  local src="$1"
  jq -s -c --argjson n "$BATCH" '
    . as $all
    | range(0; ($all | length); $n)
    | . as $i
    | $all[$i : $i + $n] as $chunk
    | {
        sql: ("INSERT OR REPLACE INTO scores (identity, ruler, mode, data_fp,"
              + " measured_by, score, metric, rolls, cost_seconds,"
              + " started_at, finished_at)"
              + " VALUES "
              + ([$chunk[] | "(?,?,?,?,?,?,?,?,?,?,?)"]
                 | join(","))),
        params: [$chunk[]
                 | .identity, .ruler, .mode, .data_fp, (.measured_by // ""),
                   .score, .metric,
                   (if .rolls == null then null else (.rolls | tojson) end),
                   .cost_seconds, .started_at, .finished_at]
      }
  ' "$src"
}

# ---- shipping, from wherever it got to last time --------------------------
ship() {
  local src="$1" cursor="$1.shipped" from=0 total sent=0 failed=0
  [ -s "$src" ] || { echo "facts: nothing to ship"; return 0; }
  [ -f "$cursor" ] && from=$(cat "$cursor")
  total=$(wc -l < "$src" | tr -d ' ')
  if [ "$total" -le "$from" ]; then
    echo "facts: $total rows, all shipped"
    return 0
  fi
  local pending; pending=$(mktemp)
  tail -n +$((from + 1)) "$src" > "$pending"
  while IFS= read -r body; do
    [ -n "$body" ] || continue
    if d1 "$body"; then
      sent=$((sent + 1))
    else
      failed=$((failed + 1))
      if [ "$failed" -eq 1 ]; then
        echo "::error::facts: the database refused a write [HTTP $D1_CODE]"
        head -c 500 "$D1_OUT" || true
        echo
      fi
    fi
  done < <(batches "$pending")
  rm -f "$pending"
  # THE CURSOR MOVES ONLY WHEN EVERY BATCH LANDED. Moving it past a refusal
  # would turn a retryable failure into a row nobody ships again.
  if [ "$failed" -eq 0 ]; then
    echo "$total" > "$cursor"
    echo "facts: shipped rows $((from + 1))..$total in $sent batches"
  else
    echo "facts: $sent batches landed, $failed refused — the cursor stays at $from"
    return 1
  fi
}

# ---- self-test ------------------------------------------------------------
self_test() {
  ok=0; bad=0
  DIR=$(mktemp -d)
  trap 'rm -rf "$DIR"' EXIT
  mkdir -p "$DIR/bin" "$DIR/work"
  say() { if [ "$1" = ok ]; then ok=$((ok + 1)); else bad=$((bad + 1)); fi; echo "  $1    $2"; }
  cd "$DIR/work"

  row() {
    printf '{"identity":"%s","ruler":"single_target","mode":"%s","data_fp":"fp","measured_by":"abc","score":%s,"metric":"kpm","cost_seconds":1.5,"rolls":null,"started_at":"T0","finished_at":"T1"}\n' "$1" "$2" "$3"
  }
  row 'a|b' base 1.0 > facts.ndjson
  row 'a|b' heavy_slam 2.0 >> facts.ndjson
  # ESCAPED IN THE FILE, a bare quote after jq has read it — which is what a
  # build id would look like if one ever carried one.
  row 'quote\"key' base 3.0 >> facts.ndjson
  for i in 4 5 6 7 8 9 10 11 12; do row "k$i" base "$i" >> facts.ndjson; done

  BATCH=9
  local n; n=$(batches facts.ndjson | wc -l | tr -d ' ')
  [ "$n" = "2" ] && say ok "twelve rows at nine a batch is two statements" \
    || say FAIL "batched into $n"

  local first; first=$(batches facts.ndjson | head -1)
  [ "$(printf '%s' "$first" | jq -r '.params | length')" = "$((9 * FACT_COLUMNS))" ] \
    && say ok "...$FACT_COLUMNS bound parameters a row" \
    || say FAIL "$(printf '%s' "$first" | jq -r '.params|length') parameters"
  [ $((9 * FACT_COLUMNS)) -le 100 ] \
    && say ok "...and a batch stays inside D1's hundred" \
    || say FAIL "$((9 * FACT_COLUMNS)) parameters is over D1's hundred"

  printf '%s' "$first" | jq -e '.sql | contains("quote") | not' >/dev/null \
    && printf '%s' "$first" | jq -e '.params | index("quote\"key")' >/dev/null \
    && say ok "...and an identity with a quote in it is bound, not interpolated" \
    || say FAIL "a quoted identity reached the sql"

  # THE CLOCKS ARE THE SCORER'S. A shipper that invented them would be writing
  # the shipping time under the fight's name, and a row may be shipped minutes
  # after it was measured.
  printf '%s' "$first" | jq -e --argjson n "$FACT_COLUMNS" \
      '[.params[$n - 2], .params[$n - 1]] == ["T0","T1"]' >/dev/null \
    && say ok "...and the fight's own two clocks are what is bound" \
    || say FAIL "the clocks did not come from the log: $(printf '%s' "$first" | jq -c '.params[-2:]')"
  printf '%s' "$first" | jq -e '.sql | test("[0-9]{4}-[0-9]{2}-[0-9]{2}") | not' >/dev/null \
    && say ok "...and no clock of this shell's reached the statement" \
    || say FAIL "a timestamp was interpolated into the sql"

  # ONE ROW PER MODE. Two modes of one build are two rows, and a statement that
  # collapsed them would file seven melee measurements as one.
  printf '%s' "$first" | jq -e --argjson n "$FACT_COLUMNS" \
      '[.params[2], .params[2 + $n]] == ["base","heavy_slam"]' >/dev/null \
    && say ok "...and two modes of one build are two rows" \
    || say FAIL "the modes collapsed"

  # THE UNITS TRAVEL WITH THE NUMBER, or a score is a bare float whose meaning
  # lives in a file that moves under it.
  printf '%s' "$first" | jq -e '.params[6] == "kpm"' >/dev/null \
    && say ok "...and the score's own metric is bound beside it" \
    || say FAIL "the metric did not reach the statement: $(printf '%s' "$first" | jq -c '.params[6]')"

  export PATH="$DIR/bin:$PATH"
  export CF_ACCOUNT=a CF_D1_DATABASE=d CF_TOKEN=t

  cat > "$DIR/bin/curl" <<'OK'
#!/usr/bin/env bash
out=/dev/null; prev=
for a in "$@"; do [ "$prev" = "-o" ] && out="$a"; prev="$a"; done
printf '{"result":[{"results":[],"success":true}],"success":true}' > "$out"
printf '200'
OK
  chmod +x "$DIR/bin/curl"
  ship facts.ndjson > out.txt 2>&1 \
    && grep -q "rows 1..12" out.txt \
    && say ok "a first run ships every row" || say FAIL "$(cat out.txt)"
  [ "$(cat facts.ndjson.shipped)" = "12" ] \
    && say ok "...and records how far it got" || say FAIL "cursor: $(cat facts.ndjson.shipped)"

  # …AND THE SECOND RUN SHIPS ONLY WHAT IS NEW, which is the whole point of
  # running this again and again beside a scorer that keeps appending.
  ship facts.ndjson > out.txt 2>&1 && grep -q "all shipped" out.txt \
    && say ok "...and a second run with nothing new ships nothing" || say FAIL "$(cat out.txt)"
  row 'later' base 9.9 >> facts.ndjson
  ship facts.ndjson > out.txt 2>&1 && grep -q "rows 13..13" out.txt \
    && say ok "...and a row appended after it ships alone" || say FAIL "$(cat out.txt)"

  cat > "$DIR/bin/curl" <<'DEAD'
#!/usr/bin/env bash
out=/dev/null; prev=
for a in "$@"; do [ "$prev" = "-o" ] && out="$a"; prev="$a"; done
printf '{"success":false,"errors":[{"code":7403,"message":"D1 not authorized"}]}' > "$out"
printf '403'
DEAD
  chmod +x "$DIR/bin/curl"
  row 'refused' base 1.0 >> facts.ndjson
  if ship facts.ndjson > out.txt 2>&1; then
    say FAIL "a refused write passed"
  elif grep -q "D1 not authorized" out.txt; then
    say ok "a refused write reports what the database said"
  else
    say FAIL "$(cat out.txt)"
  fi
  # THE CURSOR MAY NOT MOVE PAST A REFUSAL, or the row is never shipped again.
  [ "$(cat facts.ndjson.shipped)" = "13" ] \
    && say ok "...and the cursor stays where it was" \
    || say FAIL "cursor moved to $(cat facts.ndjson.shipped)"

  unset CF_D1_DATABASE
  configured && say FAIL "unconfigured read as configured" \
    || say ok "no database configured is a working state"

  echo
  echo "$ok ok, $bad failed"
  [ "$bad" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then self_test; exit $?; fi

# UNCONFIGURED IS SILENT AND GREEN — but HALF-configured is not, and the two
# read identically from inside `configured()`.
#
# A DATABASE ID WITHOUT A TOKEN IS A MISTAKE, NOT AN ABSENCE. The id comes out
# of `wrangler.jsonc`, which is committed, so its presence says this repo HAS a
# database; the token comes from a secret, which a step can simply forget to
# pass. Measured: a shard wrote its whole fact log, shipped none of it, and
# reported success — because the env block carrying the secrets had been put on
# the wrong step.
if [ -n "${CF_D1_DATABASE:-}" ] && ! configured; then
  echo "::error::facts: a database is declared but the credentials are not set"
  echo "::error::CF_ACCOUNT=${CF_ACCOUNT:+set}${CF_ACCOUNT:-MISSING} CF_TOKEN=${CF_TOKEN:+set}${CF_TOKEN:-MISSING}"
  exit 1
fi

if ! configured; then
  echo "no database configured — nothing shipped (docs/BOARD.md §Setup)"
  exit 0
fi

ship "${1:?the fact log to ship, one row a line}"
