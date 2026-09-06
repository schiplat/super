#!/usr/bin/env bash
# Wait until the demo edge responds 200 on /health/ (default :8088).
# Scratch body under /tmp.
#
# Default budget is 15 tries ≈ 15s — this is the nginx entry-check ceiling.
# Pass a larger second arg only for non-demo/debug use.
set -euo pipefail

URL="${1:-http://127.0.0.1:8088/health/}"
TRIES="${2:-15}"
BODY="${TMPDIR:-/tmp}/djangoapp-edge-health.json"

for i in $(seq 1 "$TRIES"); do
  code="$(curl -sS -o "$BODY" -w '%{http_code}' "$URL" 2>/dev/null || echo 000)"
  if [[ "$code" == "200" ]]; then
    echo "edge OK ($URL) → $(cat "$BODY")"
    exit 0
  fi
  echo "waiting for edge ($i/$TRIES) HTTP $code"
  sleep 1
done
echo "edge not ready within ${TRIES}s: $URL" >&2
exit 1
