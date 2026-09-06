#!/usr/bin/env bash
# Wait until every worker-* program is Healthy (or gone).
# Usage: wait_workers_healthy.sh [timeout_sec]
set -euo pipefail

TIMEOUT="${1:-60}"
SUPER_SERVER="${SUPER_SERVER:-http://127.0.0.1:9012}"
SUPER_BIN="${SUPER_BIN:-/usr/local/bin/super}"
export SUPER_ROOT="${DEMO_SUPER_ROOT:-${SUPER_ROOT:-/opt/super-django-demo}}"

deadline=$((SECONDS + TIMEOUT))
while (( SECONDS < deadline )); do
  mapfile -t rows < <(
    "$SUPER_BIN" --server "$SUPER_SERVER" list 2>/dev/null \
      | awk -F'┆' '/worker-[0-9]/ {
          gsub(/ /,"",$2); gsub(/ /,"",$4);
          print $2 "=" $4
        }' || true
  )
  if [[ ${#rows[@]} -eq 0 ]]; then
    echo "no worker-* programs yet; retrying…"
    sleep 1
    continue
  fi
  bad=0
  for r in "${rows[@]}"; do
    st="${r#*=}"
    if [[ "$st" != "Healthy" ]]; then
      bad=1
      break
    fi
  done
  echo "workers: ${rows[*]}"
  if [[ "$bad" -eq 0 ]]; then
    echo "all workers Healthy"
    exit 0
  fi
  sleep 1
done
echo "timeout waiting for workers Healthy" >&2
exit 1
