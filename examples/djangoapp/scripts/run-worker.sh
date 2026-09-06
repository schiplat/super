#!/usr/bin/env bash
# Launch one Celery worker with a unique nodename (for Super numprocs).
# SUPER_PROCESS_NUM / SUPER_PROCESS_TOTAL are injected by superd when numprocs > 1.
set -euo pipefail

# Prefer the tree that contains this script (works after bootstrap to any APP_ROOT).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_ROOT="${DEMO_APP_ROOT:-$(cd "$SCRIPT_DIR/.." && pwd)}"
NUM="${SUPER_PROCESS_NUM:-0}"
cd "$APP_ROOT"
exec .venv/bin/celery -A myproject worker \
  --loglevel=INFO \
  --autoscale=2,1 \
  --hostname="worker-${NUM}@%h"
