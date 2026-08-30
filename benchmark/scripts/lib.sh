# Shared helpers for Super bench orchestrator.
# shellcheck shell=bash

ALL_ARMS=(super-oss super-pro supervisord pm2)

# Cyclic Latin square of order 4 — ONLY for MODE=colocated smoke (never publication).
# A=super-oss B=super-pro C=supervisord D=pm2
latin_order_colocated() {
  local round="${1:?round}"
  local -a sq=(
    "super-oss super-pro supervisord pm2"
    "super-pro supervisord pm2 super-oss"
    "supervisord pm2 super-oss super-pro"
    "pm2 super-oss super-pro supervisord"
  )
  local idx=$(( (round - 1) % 4 ))
  echo "${sq[$idx]}"
}

mem_available_kb() {
  awk '/MemAvailable:/ {print $2}' /proc/meminfo
}

loadavg_1() {
  awk '{print $1}' /proc/loadavg
}

drop_caches_if_root() {
  if [[ "${SUPER_BENCH_DROP_CACHES:-1}" == "1" && "$(id -u)" == "0" ]]; then
    sync
    echo 3 >/proc/sys/vm/drop_caches || true
  fi
}

log() { echo "[$(date -u +%H:%M:%S)] $*"; }
