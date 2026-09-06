#!/usr/bin/env bash
# Super-side only: ensure instance layout, apply djangoapp stack, print status.
# Assumes APP_ROOT already has venv/.env/migrate (run bootstrap.sh for full setup).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXAMPLE_SRC="${EXAMPLE_SRC:-$(cd "$SCRIPT_DIR/.." && pwd)}"
APP_ROOT="${DEMO_APP_ROOT:-/srv/djangoapp}"
SUPER_ROOT="${DEMO_SUPER_ROOT:-/opt/super-django-demo}"
SUPER_BIN_DIR="${SUPER_BIN_DIR:-/usr/local/bin}"
SUPER_SERVER="${SUPER_SERVER:-http://127.0.0.1:9012}"
export SUPER_ROOT

echo "==> SUPER_ROOT=$SUPER_ROOT"
echo "==> APP_ROOT=$APP_ROOT"

mkdir -p "$SUPER_ROOT/conf/conf.d" "$SUPER_ROOT/data" "$SUPER_ROOT/logs" "$SUPER_ROOT/run"
chmod 755 "$SUPER_ROOT/run" "$SUPER_ROOT/logs" "$SUPER_ROOT/data"
cp "$APP_ROOT/conf/super.toml" "$SUPER_ROOT/conf/super.toml"
cp "$APP_ROOT/conf/conf.d/djangoapp.toml" "$SUPER_ROOT/conf/conf.d/djangoapp.toml"

if ! curl -sf "http://127.0.0.1:9012/health" >/dev/null 2>&1; then
  echo "==> super check (before first start)"
  "$SUPER_BIN_DIR/super" check --file "$SUPER_ROOT/conf/super.toml"
  echo "==> starting superd"
  nohup env SUPER_ROOT="$SUPER_ROOT" "$SUPER_BIN_DIR/superd" \
    >"$SUPER_ROOT/logs/superd.bootstrap.out" 2>&1 &
  for _ in $(seq 1 30); do
    curl -sf "http://127.0.0.1:9012/health" >/dev/null 2>&1 && break
    sleep 0.5
  done
fi

echo "==> apply djangoapp stack"
"$SUPER_BIN_DIR/super" --server "$SUPER_SERVER" apply --file "$SUPER_ROOT/conf/conf.d/djangoapp.toml"

echo "==> status"
"$SUPER_BIN_DIR/super" --server "$SUPER_SERVER" list
echo
"$SUPER_BIN_DIR/super" --server "$SUPER_SERVER" info web 2>/dev/null | head -30 || true
echo
curl -sS "http://127.0.0.1:8088/health/"
echo
echo "OK — stack applied; edge http://127.0.0.1:8088/"
