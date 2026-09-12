#!/usr/bin/env bash
# Prepare layout for the Quick Start VHS tape (does NOT start superd).
# The tape records: deploy → create program → day-to-day ops.
# Docs: https://super.docs.sconts.com/docs/01-getting-started/quick-start/
#
# Layout: SUPER_ROOT=/tmp/super-quickstart-demo (never the monorepo cwd).
# Scratch: TMPDIR=/tmp
set -euo pipefail

export TMPDIR="${TMPDIR:-/tmp}"
# Pin instance root — do not inherit ambient SUPER_ROOT from the lab shell.
export SUPER_ROOT="${SUPER_ROOT_OVERRIDE:-/tmp/super-quickstart-demo}"
export SUPER_SERVER="${SUPER_SERVER:-http://127.0.0.1:9002}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXAMPLE_SRC="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$EXAMPLE_SRC/../.." && pwd)"

SUPER_BIN="${SUPER_BIN:-}"
if [[ -z "$SUPER_BIN" ]]; then
  if [[ -x /tmp/super-demo/bin/superd ]]; then
    SUPER_BIN=/tmp/super-demo/bin
  elif [[ -x "$REPO_ROOT/target/release/superd" ]]; then
    SUPER_BIN="$REPO_ROOT/target/release"
  else
    echo "missing superd: set SUPER_BIN=… or build/deploy OSS release binaries" >&2
    exit 1
  fi
fi

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing: $1" >&2
    exit 1
  }
}

need curl
need python3
[[ -x "$SUPER_BIN/superd" && -x "$SUPER_BIN/super" ]] || {
  echo "SUPER_BIN=$SUPER_BIN must contain superd and super" >&2
  exit 1
}

mkdir -p "$SUPER_ROOT"/{conf,data,logs,run,bin}
chmod 755 "$SUPER_ROOT"/{data,logs,run}

# Stage binaries into the instance so the tape looks like a real deploy.
cp -f "$SUPER_BIN/superd" "$SUPER_BIN/super" "$SUPER_ROOT/bin/"
chmod +x "$SUPER_ROOT/bin/superd" "$SUPER_ROOT/bin/super"

# Minimal conf matching the Quick Start sample.
cat >"$SUPER_ROOT/conf/super.toml" <<'EOF'
# super.toml

[server]
host = "127.0.0.1"
port = 9002

[logging]
log_level = "info"

[storage]
data_file = "data/snapshot.json"
events_file = "data/events.db"
log_dir = "logs"
EOF

# Stop a previous recording daemon for this SUPER_ROOT only.
PIDFILE="$SUPER_ROOT/run/superd.pid"
if [[ -f "$PIDFILE" ]]; then
  old_pid="$(tr -d '[:space:]' <"$PIDFILE" || true)"
  if [[ -n "${old_pid:-}" ]] && kill -0 "$old_pid" 2>/dev/null; then
    echo "stopping previous superd pid=$old_pid"
    kill "$old_pid" 2>/dev/null || true
    sleep 1
    kill -9 "$old_pid" 2>/dev/null || true
  fi
  rm -f "$PIDFILE"
fi
# Also clear anything still bound to the API port from a crashed run.
if command -v ss >/dev/null 2>&1; then
  if ss -ltn 2>/dev/null | grep -qE ':9002\b'; then
    echo "warning: something still listens on :9002 — free it before recording" >&2
  fi
fi

# Fresh registry so the tape can add demo-web cleanly.
rm -f "$SUPER_ROOT/data/snapshot.json" "$SUPER_ROOT/data/events.db"

pick_port() {
  local prefer="$1" fallback="$2"
  if ss -ltn 2>/dev/null | grep -qE ":${prefer}\\b"; then
    echo "$fallback"
  else
    echo "$prefer"
  fi
}
DEMO_WEB_PORT="$(pick_port 8080 18080)"
DEMO_WEB_REST_PORT="$(pick_port 8081 18081)"
if [[ "$DEMO_WEB_REST_PORT" == "$DEMO_WEB_PORT" ]]; then
  DEMO_WEB_REST_PORT=18081
fi

cat >"$SUPER_ROOT/record.env" <<EOF
export SUPER_ROOT=$SUPER_ROOT
export SUPER_SERVER=$SUPER_SERVER
export TMPDIR=${TMPDIR:-/tmp}
export PATH=$SUPER_ROOT/bin:\$PATH
export DEMO_WEB_PORT=$DEMO_WEB_PORT
export DEMO_WEB_REST_PORT=$DEMO_WEB_REST_PORT
EOF

# Fish env for VHS (fish highlights the typed command line; bash cannot).
cat >"$SUPER_ROOT/record.fish" <<EOF
set -gx SUPER_ROOT $SUPER_ROOT
set -gx SUPER_SERVER $SUPER_SERVER
set -gx TMPDIR ${TMPDIR:-/tmp}
set -gx PATH $SUPER_ROOT/bin \$PATH
set -gx DEMO_WEB_PORT $DEMO_WEB_PORT
set -gx DEMO_WEB_REST_PORT $DEMO_WEB_REST_PORT
set -g fish_greeting
EOF

echo "READY (daemon not started — tape will deploy)"
echo "  SUPER_ROOT=$SUPER_ROOT"
echo "  SUPER_SERVER=$SUPER_SERVER"
echo "  DEMO_WEB_PORT=$DEMO_WEB_PORT"
echo "  bin: $SUPER_ROOT/bin/superd  $SUPER_ROOT/bin/super"
echo "  record.env → $SUPER_ROOT/record.env"
echo "  record.fish → $SUPER_ROOT/record.fish"
