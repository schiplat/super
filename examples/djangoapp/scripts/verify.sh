#!/usr/bin/env bash
# Smoke-check a running OSS djangoapp demo (no mutations beyond enqueue + read).
# Scratch files under /tmp (or $TMPDIR).
set -euo pipefail

SUPER_ROOT="${DEMO_SUPER_ROOT:-/opt/super-django-demo}"
SUPER_SERVER="${SUPER_SERVER:-http://127.0.0.1:9012}"
SUPER_BIN="${SUPER_BIN:-/usr/local/bin/super}"
APP_ROOT="${DEMO_APP_ROOT:-/srv/djangoapp}"
EDGE="${EDGE_URL:-http://127.0.0.1:8088}"
TMP_DIR="${TMPDIR:-/tmp}"
export SUPER_ROOT

echo "==> doctor"
env SUPER_ROOT="$SUPER_ROOT" "$SUPER_BIN" --server "$SUPER_SERVER" doctor || true

echo "==> list"
"$SUPER_BIN" --server "$SUPER_SERVER" list

echo "==> curl edge /health/ (DB + Redis)"
code="$(curl -sS -o "$TMP_DIR/djangoapp-health.json" -w '%{http_code}' "$EDGE/health/")"
echo "HTTP $code $(cat "$TMP_DIR/djangoapp-health.json")"
[[ "$code" == "200" ]]
grep -q '"db": true' "$TMP_DIR/djangoapp-health.json"
grep -q '"redis": true' "$TMP_DIR/djangoapp-health.json"

echo "==> curl gunicorn /health/"
curl -sf http://127.0.0.1:8000/health/ >/dev/null

echo "==> enqueue WorkItem via edge API"
enqueue="$(curl -sS -o "$TMP_DIR/djangoapp-enqueue.json" -w '%{http_code}' \
  "$EDGE/api/work/enqueue/?seconds=1")"
echo "HTTP $enqueue $(cat "$TMP_DIR/djangoapp-enqueue.json")"
[[ "$enqueue" == "202" ]]
work_id="$(
  python3 -c "import json; print(json.load(open('$TMP_DIR/djangoapp-enqueue.json'))['id'])"
)"

echo "==> wait for WorkItem $work_id → done"
ok=0
for _ in $(seq 1 30); do
  st_code="$(curl -sS -o "$TMP_DIR/djangoapp-work.json" -w '%{http_code}' \
    "$EDGE/api/work/${work_id}/")"
  [[ "$st_code" == "200" ]] || { sleep 0.5; continue; }
  status="$(python3 -c "import json; print(json.load(open('$TMP_DIR/djangoapp-work.json'))['status'])")"
  echo "  status=$status"
  if [[ "$status" == "done" ]]; then
    ok=1
    break
  fi
  if [[ "$status" == "failed" ]]; then
    cat "$TMP_DIR/djangoapp-work.json" >&2
    exit 1
  fi
  sleep 0.5
done
[[ "$ok" -eq 1 ]]

echo "==> manage.py enqueue_work (CLI path)"
set -a
# shellcheck disable=SC1091
. "$APP_ROOT/.env"
set +a
(
  cd "$APP_ROOT"
  .venv/bin/python manage.py enqueue_work --seconds 0.5
)

echo "==> logs (tails)"
for name in web nginx worker-0 worker-1 beat; do
  echo "---- $name ----"
  "$SUPER_BIN" --server "$SUPER_SERVER" logs "$name" --tail 5 >"$TMP_DIR/djangoapp-log-${name}.txt"
  lines="$(wc -l < "$TMP_DIR/djangoapp-log-${name}.txt" | tr -d ' ')"
  echo "$name: $lines line(s)"
done

echo "==> logs (brief live follow on web)"
set +e
timeout --signal=INT 3 \
  "$SUPER_BIN" --server "$SUPER_SERVER" logs web --tail 3 --follow >"$TMP_DIR/djangoapp-log-follow.txt"
rc=$?
set -e
[[ "$rc" -eq 0 || "$rc" -eq 124 || "$rc" -eq 130 ]]
echo "follow captured $(wc -l < "$TMP_DIR/djangoapp-log-follow.txt" | tr -d ' ') line(s)"

echo "OK"
