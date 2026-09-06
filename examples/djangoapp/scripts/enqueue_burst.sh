#!/usr/bin/env bash
# Enqueue a burst of busy_work tasks so Celery --autoscale can grow the pool.
# Usage: ./scripts/enqueue_burst.sh [count] [seconds_per_task]
set -euo pipefail

APP_ROOT="${DEMO_APP_ROOT:-/srv/djangoapp}"
COUNT="${1:-20}"
SECONDS_PER="${2:-3}"

set -a
# shellcheck disable=SC1091
. "$APP_ROOT/.env"
set +a

cd "$APP_ROOT"
echo "==> enqueue $COUNT × demoapp.busy_work(seconds=$SECONDS_PER)"
.venv/bin/python - <<PY
from demoapp.tasks import busy_work
n = int("$COUNT")
sec = float("$SECONDS_PER")
ids = [busy_work.delay(sec).id for _ in range(n)]
print(f"enqueued {len(ids)} tasks (first={ids[0]}, last={ids[-1]})")
PY

echo "==> active / reserved (celery inspect)"
.venv/bin/celery -A myproject inspect active --timeout 5 2>/dev/null | head -80 || true
.venv/bin/celery -A myproject inspect stats --timeout 5 2>/dev/null \
  | grep -E 'autoscaler|pool|processes|max-concurrency|min-concurrency' || true
