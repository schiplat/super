#!/usr/bin/env bash
# Bootstrap the OSS djangoapp example on a lab host.
#
# Defaults (override with env):
#   APP_ROOT=/srv/djangoapp
#   SUPER_ROOT=/opt/super-django-demo
#   EXAMPLE_SRC=<repo>/examples/djangoapp  (directory containing this script's parent)
#
# Requires: docker (compose), python3+venv, nginx, redis-cli, superd, super.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXAMPLE_SRC="${EXAMPLE_SRC:-$(cd "$SCRIPT_DIR/.." && pwd)}"
# Do not inherit ambient SUPER_ROOT / APP_ROOT — they may point at another instance.
APP_ROOT="${DEMO_APP_ROOT:-/srv/djangoapp}"
SUPER_ROOT="${DEMO_SUPER_ROOT:-/opt/super-django-demo}"
SUPER_BIN_DIR="${SUPER_BIN_DIR:-/usr/local/bin}"
export SUPER_ROOT

echo "==> example source: $EXAMPLE_SRC"
echo "==> APP_ROOT=$APP_ROOT"
echo "==> SUPER_ROOT=$SUPER_ROOT"

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required command: $1" >&2
    exit 1
  }
}

need docker
need python3
need nginx
need redis-cli
need "$SUPER_BIN_DIR/superd"
need "$SUPER_BIN_DIR/super"

if ! docker compose version >/dev/null 2>&1; then
  echo "docker compose plugin required" >&2
  exit 1
fi

# --- sync app tree (preserve local .venv / .env / run if present) ------------
mkdir -p "$APP_ROOT"
rsync -a --delete \
  --exclude '.venv/' \
  --exclude '.env' \
  --exclude 'run/' \
  --exclude '__pycache__/' \
  --exclude 'celerybeat-schedule*' \
  --exclude 'tapes/out/' \
  "$EXAMPLE_SRC/" "$APP_ROOT/"

# nginx pid dir is runtime-only (not in git). Create AFTER rsync — otherwise
# `rsync --delete` removes destination `run/` because the example tree has none.
mkdir -p "$APP_ROOT/run"

# Template paths in conf/nginx use /srv/djangoapp — rewrite to APP_ROOT.
echo "==> rewrite install paths → $APP_ROOT"
for f in \
  "$APP_ROOT/conf/conf.d/djangoapp.toml" \
  "$APP_ROOT/nginx/nginx.conf"; do
  [[ -f "$f" ]] || continue
  # portable in-place substitute (macOS / GNU sed)
  tmp="$f.tmp.$$"
  sed "s|/srv/djangoapp|$APP_ROOT|g" "$f" >"$tmp" && mv "$tmp" "$f"
done

if [[ ! -f "$APP_ROOT/.env" ]]; then
  cp "$APP_ROOT/.env.example" "$APP_ROOT/.env"
  chmod 600 "$APP_ROOT/.env"
  echo "==> wrote $APP_ROOT/.env from .env.example"
fi

# --- data plane ------------------------------------------------------------
echo "==> starting Postgres + Redis (docker compose)"
(
  cd "$APP_ROOT"
  docker compose up -d
  for i in $(seq 1 30); do
    if docker compose exec -T postgres pg_isready -U djangoapp -d djangoapp >/dev/null 2>&1 \
      && docker compose exec -T redis redis-cli ping 2>/dev/null | grep -q PONG; then
      break
    fi
    sleep 1
  done
)

# --- Python env ------------------------------------------------------------
if [[ ! -x "$APP_ROOT/.venv/bin/python" ]]; then
  echo "==> creating venv"
  python3 -m venv "$APP_ROOT/.venv"
fi
echo "==> pip install"
"$APP_ROOT/.venv/bin/pip" install -q --upgrade pip
"$APP_ROOT/.venv/bin/pip" install -q -r "$APP_ROOT/requirements.txt"

echo "==> migrate"
set -a
# shellcheck disable=SC1091
. "$APP_ROOT/.env"
set +a
(
  cd "$APP_ROOT"
  .venv/bin/python manage.py migrate --noinput
)

nginx -t -c "$APP_ROOT/nginx/nginx.conf"

# --- Super instance --------------------------------------------------------
mkdir -p "$SUPER_ROOT/conf/conf.d" "$SUPER_ROOT/data" "$SUPER_ROOT/logs" "$SUPER_ROOT/run"
chmod 755 "$SUPER_ROOT/run" "$SUPER_ROOT/logs" "$SUPER_ROOT/data"
cp "$APP_ROOT/conf/super.toml" "$SUPER_ROOT/conf/super.toml"
cp "$APP_ROOT/conf/conf.d/djangoapp.toml" "$SUPER_ROOT/conf/conf.d/djangoapp.toml"

if ! curl -sf "http://127.0.0.1:9012/health" >/dev/null 2>&1; then
  echo "==> super check (before first start)"
  "$SUPER_BIN_DIR/super" check --file "$SUPER_ROOT/conf/super.toml"
  echo "==> starting superd (SUPER_ROOT=$SUPER_ROOT)"
  nohup env SUPER_ROOT="$SUPER_ROOT" "$SUPER_BIN_DIR/superd" \
    >"$SUPER_ROOT/logs/superd.bootstrap.out" 2>&1 &
  for i in $(seq 1 30); do
    if curl -sf "http://127.0.0.1:9012/health" >/dev/null 2>&1; then
      break
    fi
    sleep 0.5
  done
fi

if ! curl -sf "http://127.0.0.1:9012/health" >/dev/null 2>&1; then
  echo "superd did not become healthy on :9012" >&2
  tail -n 50 "$SUPER_ROOT/logs/superd.bootstrap.out" >&2 || true
  exit 1
fi

export SUPER_SERVER="${SUPER_SERVER:-http://127.0.0.1:9012}"
echo "==> super apply"
"$SUPER_BIN_DIR/super" --server "$SUPER_SERVER" apply --file "$SUPER_ROOT/conf/conf.d/djangoapp.toml"

echo "==> waiting for Healthy programs"
for i in $(seq 1 60); do
  list="$("$SUPER_BIN_DIR/super" --server "$SUPER_SERVER" list 2>/dev/null || true)"
  worker_ok=0
  # numprocs>1 → worker-0, worker-1, …; require every worker* row Healthy
  while IFS= read -r line; do
    echo "$line" | grep -qi Healthy || { worker_ok=0; break; }
    worker_ok=$((worker_ok + 1))
  done < <(echo "$list" | grep -E 'worker' || true)
  if echo "$list" | grep -E 'web' | grep -qi Healthy \
    && echo "$list" | grep -E 'nginx' | grep -qi Healthy \
    && [[ "$worker_ok" -ge 1 ]] \
    && echo "$list" | grep -E 'beat' | grep -qi Healthy; then
    echo "$list"
    echo "==> edge probe"
    curl -sf "http://127.0.0.1:8088/health/"
    echo
    echo "OK — demo ready (API $SUPER_SERVER, HTTP http://127.0.0.1:8088/)"
    exit 0
  fi
  sleep 2
done

echo "timed out waiting for Healthy; last list:" >&2
"$SUPER_BIN_DIR/super" --server "$SUPER_SERVER" list || true
exit 1
