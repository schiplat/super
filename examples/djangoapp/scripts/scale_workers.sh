#!/usr/bin/env bash
# Horizontally scale Celery workers via Super numprocs (worker-0 .. worker-N-1).
#
# With prune=false, `super apply` does NOT stop or remove leftover worker-N after
# scale-down — this script must stop+remove them. Remove only succeeds once the
# program is fully Stopped (not Stopping/Running).
#
# Usage: ./scripts/scale_workers.sh <N>   # N >= 1
set -euo pipefail

N="${1:?usage: scale_workers.sh <numprocs>}"
if ! [[ "$N" =~ ^[1-9][0-9]*$ ]]; then
  echo "numprocs must be a positive integer" >&2
  exit 1
fi

APP_ROOT="${DEMO_APP_ROOT:-/srv/djangoapp}"
SUPER_ROOT="${DEMO_SUPER_ROOT:-/opt/super-django-demo}"
SUPER_SERVER="${SUPER_SERVER:-http://127.0.0.1:9012}"
SUPER_BIN="${SUPER_BIN:-/usr/local/bin/super}"
STACK_APP="$APP_ROOT/conf/conf.d/djangoapp.toml"
STACK_SUPER="$SUPER_ROOT/conf/conf.d/djangoapp.toml"
export SUPER_ROOT

list_worker_names() {
  "$SUPER_BIN" --server "$SUPER_SERVER" list 2>/dev/null \
    | awk -F'┆' '/worker-[0-9]/ { gsub(/ /,"",$2); print $2 }' || true
}

worker_status() {
  local name="$1"
  "$SUPER_BIN" --server "$SUPER_SERVER" list 2>/dev/null | awk -F'┆' -v n="$name" '
    { gsub(/ /,"",$2); gsub(/ /,"",$4); if ($2==n) { print $4; exit } }'
}

# Stop (wait until Stopped) then remove. Fail loudly if the program remains.
remove_worker() {
  local name="$1"
  echo "==> remove leftover $name"

  local st
  st="$(worker_status "$name" || true)"
  if [[ -z "$st" ]]; then
    echo "    (already gone)"
    return 0
  fi

  if [[ "$st" != "Stopped" ]]; then
    # --wait is required: remove rejects Running/Stopping with
    # "Cannot remove running program", and a bare `stop` + immediate remove
    # leaves a Stopped orphan when Celery is slow to exit.
    "$SUPER_BIN" --server "$SUPER_SERVER" stop "$name" -y --wait --timeout 90
  fi

  local i
  for i in $(seq 1 10); do
    st="$(worker_status "$name" || true)"
    [[ -z "$st" ]] && return 0
    if [[ "$st" == "Stopped" ]]; then
      if "$SUPER_BIN" --server "$SUPER_SERVER" remove "$name" -y; then
        st="$(worker_status "$name" || true)"
        [[ -z "$st" ]] && return 0
      fi
    else
      "$SUPER_BIN" --server "$SUPER_SERVER" stop "$name" -y --wait --timeout 30 || true
    fi
    sleep 0.5
  done

  st="$(worker_status "$name" || true)"
  echo "error: failed to remove $name (status=${st:-gone})" >&2
  return 1
}

echo "==> scale workers → numprocs=$N"

mapfile -t OLD_WORKERS < <(list_worker_names)

for f in "$STACK_APP" "$STACK_SUPER"; do
  [[ -f "$f" ]] || { echo "missing $f" >&2; exit 1; }
  awk -v n="$N" '
    $0 ~ /^name = "worker"$/ { in_worker=1 }
    in_worker && $0 ~ /^numprocs = / {
      print "numprocs = " n
      in_worker=0
      next
    }
    { print }
  ' "$f" >"$f.tmp" && mv "$f.tmp" "$f"
done

"$SUPER_BIN" --server "$SUPER_SERVER" apply --file "$STACK_SUPER"

if [[ ${#OLD_WORKERS[@]} -gt 0 ]]; then
  for name in "${OLD_WORKERS[@]}"; do
    idx="${name#worker-}"
    if [[ "$idx" =~ ^[0-9]+$ ]] && (( idx >= N )); then
      remove_worker "$name"
    fi
  done
fi

echo "==> workers now:"
"$SUPER_BIN" --server "$SUPER_SERVER" list | grep -E 'Name|worker-' || true

# Assert: no worker index >= N remains (Stopped orphans count as failure).
leftover="$(list_worker_names | awk -v n="$N" -F- '$2+0 >= n { print }')"
if [[ -n "$leftover" ]]; then
  echo "error: scale-down left leftover workers:" >&2
  echo "$leftover" >&2
  exit 1
fi
