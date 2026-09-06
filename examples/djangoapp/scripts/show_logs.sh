#!/usr/bin/env bash
# Show recent Super-managed logs for every djangoapp program.
# Usage:
#   ./scripts/show_logs.sh              # tail all
#   ./scripts/show_logs.sh --follow web # live stream one program (Ctrl+C to stop)
#   ./scripts/show_logs.sh --follow worker-0 --seconds 8
set -euo pipefail

SUPER_ROOT="${DEMO_SUPER_ROOT:-/opt/super-django-demo}"
SUPER_SERVER="${SUPER_SERVER:-http://127.0.0.1:9012}"
SUPER_BIN="${SUPER_BIN:-/usr/local/bin/super}"
export SUPER_ROOT

TAIL_N="${TAIL_N:-40}"
FOLLOW_SECONDS="${FOLLOW_SECONDS:-8}"

programs=(web nginx worker-0 worker-1 beat)

follow_one() {
  local name="$1"
  local secs="${2:-$FOLLOW_SECONDS}"
  echo "==> live: super logs $name --tail $TAIL_N --follow  (${secs}s)"
  # timeout exits 124 when the follow window ends — treat as success.
  set +e
  timeout --signal=INT "$secs" \
    "$SUPER_BIN" --server "$SUPER_SERVER" logs "$name" --tail "$TAIL_N" --follow
  local rc=$?
  set -e
  if [[ "$rc" -eq 124 || "$rc" -eq 130 || "$rc" -eq 0 ]]; then
    return 0
  fi
  return "$rc"
}

if [[ "${1:-}" == "--follow" ]]; then
  target="${2:-web}"
  secs="${FOLLOW_SECONDS}"
  if [[ "${3:-}" == "--seconds" && -n "${4:-}" ]]; then
    secs="$4"
  elif [[ "${3:-}" =~ ^[0-9]+$ ]]; then
    secs="$3"
  fi
  follow_one "$target" "$secs"
  exit 0
fi

echo "==> historical tails (SUPER_SERVER=$SUPER_SERVER)"
for name in "${programs[@]}"; do
  echo
  echo "-------- $name (last ${TAIL_N}) --------"
  if ! "$SUPER_BIN" --server "$SUPER_SERVER" logs "$name" --tail "$TAIL_N"; then
    echo "(skip $name — not found or no logs yet)" >&2
  fi
done

echo
echo "Tip: live stream one program — ./scripts/show_logs.sh --follow worker-0"
echo "     or: super --server $SUPER_SERVER logs web --tail 50 --follow"
