#!/usr/bin/env bash
# THE DEPLOY, WITH A RETRY FOR CLOUDFLARE'S OWN BAD DAYS.
#
# Point the Workers Build's deploy command at this instead of `npx wrangler
# deploy`. A push whose build fails leaves the site on the PREVIOUS commit with
# nothing in the repo to show for it, and the failure that has actually
# happened was not ours:
#
#   GET /accounts/<id>/workers/scripts/wfsim/secrets -> 503 Service Unavailable
#   upstream connect error or disconnect/reset before headers
#   [ERROR] Received a malformed response from the API
#
# RETRIED ONLY ON THAT SHAPE. A wrong binding, a missing site/ or a bad
# wrangler.jsonc fails the same way three times in a row, so retrying it buys
# nothing and costs the reader three copies of one error — this exits on the
# first attempt instead, and the log says which of the two it decided.
set -uo pipefail

ATTEMPTS=${DEPLOY_ATTEMPTS:-4}
# 10s, 30s, 60s. Long enough for an API blip to pass, short enough that a real
# outage still fails the build inside a couple of minutes.
BACKOFF=(10 30 60)

# Every marker is a phrase Cloudflare's own API or edge produced, not a guess
# at what an error might look like.
TRANSIENT='Received a malformed response from the API|upstream connect error|connection termination|5[0-9][0-9] Service Unavailable|Internal Server Error|ECONNRESET|socket hang up|fetch failed'

for i in $(seq 1 "$ATTEMPTS"); do
  echo "deploy: attempt $i of $ATTEMPTS"
  out=$(npx wrangler deploy 2>&1)
  code=$?
  echo "$out"
  if [ $code -eq 0 ]; then
    exit 0
  fi
  if ! echo "$out" | grep -qE "$TRANSIENT"; then
    echo "deploy: this is not Cloudflare having a bad day — not retrying."
    exit $code
  fi
  if [ "$i" -lt "$ATTEMPTS" ]; then
    wait=${BACKOFF[$((i - 1))]:-60}
    echo "deploy: transient API failure, waiting ${wait}s"
    sleep "$wait"
  fi
done

echo "deploy: still failing after $ATTEMPTS attempts"
exit 1
