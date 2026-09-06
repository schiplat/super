#!/usr/bin/env bash
# Record the continuous OSS djangoapp story (Parts 1–5) with VHS.
# Requires: vhs, ttyd, ffmpeg, Chromium. Prefer VHS_NO_SANDBOX=true as root.
#
# Story order (must stay sequential for continuity):
#   01 create services → 02 check status → 03 scale workers
#   → 04 view logs → 05 maintenance
#
# Edge check budget: wait_edge.sh … 15 (≤15s) for the final nginx :8088 probe.
# Other Wait+Screen / CLI --timeout use 30s abort ceilings (not idle sleeps).
# Scratch / VHS frames use /tmp (or TMPDIR if set).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXAMPLE_SRC="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="${OUT_DIR:-$EXAMPLE_SRC/tapes/out}"
SUPER_ROOT="${DEMO_SUPER_ROOT:-/opt/super-django-demo}"
SUPER_SERVER="${SUPER_SERVER:-http://127.0.0.1:9012}"
export SUPER_ROOT SUPER_SERVER

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing: $1 (install vhs + ttyd + ffmpeg)" >&2
    exit 1
  }
}

need vhs
need ttyd
need ffmpeg

export VHS_NO_SANDBOX="${VHS_NO_SANDBOX:-true}"
export TMPDIR="${TMPDIR:-/tmp}"

echo "NOTE: Final nginx edge check ≤15s; other VHS/CLI wait ceilings are 30s (abort budget, not idle)."
echo "NOTE: TMPDIR=$TMPDIR"

mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR"/*.gif "$OUT_DIR"/*.mp4 2>/dev/null || true

cd "$EXAMPLE_SRC/tapes"

TAPES=(
  01-create-services.tape
  02-check-status.tape
  03-scale-workers.tape
  04-view-logs.tape
  05-maintenance.tape
)

for tape in "${TAPES[@]}"; do
  echo "==> vhs $tape"
  vhs "$tape"
  sleep 2
done

for mp4 in "$OUT_DIR"/*.mp4; do
  [[ -f "$mp4" ]] || continue
  gif="${mp4%.mp4}.gif"
  if [[ ! -s "$gif" && -s "$mp4" ]]; then
    echo "==> ffmpeg gif from $(basename "$mp4")"
    ffmpeg -y -i "$mp4" -vf "fps=10,scale=960:-1:flags=lanczos" -loop 0 "$gif" >/dev/null 2>&1 || true
  fi
done

echo "Story outputs under $OUT_DIR"
ls -la "$OUT_DIR"
