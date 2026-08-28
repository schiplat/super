#!/usr/bin/env bash
# Switch gate: teardown residue → quiet period → baseline recovery.
# Exit 0 = proceed; exit 2 = abort the round (do not start the next arm dirty).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$ROOT/lib.sh"

BASELINE_LOAD="${1:?baseline loadavg}"
BASELINE_MEM_KB="${2:?baseline MemAvailable kB}"
QUIET_SEC="${SUPER_BENCH_QUIET_SEC:-30}"
MAX_WAIT="${SUPER_BENCH_GATE_MAX_WAIT:-180}"
LOAD_DELTA="${SUPER_BENCH_LOAD_DELTA:-0.5}"
MEM_RATIO="${SUPER_BENCH_MEM_RATIO:-0.85}"
RECORD_JSON="${SUPER_BENCH_GATE_RECORD:-}"

log "switch-gate: quiet ${QUIET_SEC}s (drop_caches=$([[ ${SUPER_BENCH_DROP_CACHES:-1} == 1 ]] && echo yes || echo no))"
drop_caches_if_root
sleep "$QUIET_SEC"

ok() {
  python3 - "$BASELINE_LOAD" "$BASELINE_MEM_KB" "$LOAD_DELTA" "$MEM_RATIO" <<'PY'
import sys
base_load, base_mem, dload, mratio = map(float, sys.argv[1:5])
load = float(open("/proc/loadavg").read().split()[0])
mem = None
for line in open("/proc/meminfo"):
    if line.startswith("MemAvailable:"):
        mem = float(line.split()[1])
        break
assert mem is not None
load_ok = load <= base_load + dload
mem_ok = mem >= base_mem * mratio
print(f"load={load:.2f} (base {base_load:.2f}+{dload}) mem_kb={int(mem)} (need {int(base_mem*mratio)})")
sys.exit(0 if (load_ok and mem_ok) else 1)
PY
}

elapsed=0
while (( elapsed < MAX_WAIT )); do
  if msg=$(ok); then
    log "switch-gate PASS: $msg"
    if [[ -n "$RECORD_JSON" ]]; then
      python3 - "$RECORD_JSON" "$msg" <<'PY'
import json, sys, time
path, msg = sys.argv[1], sys.argv[2]
rec = {"ts": time.time(), "result": "pass", "detail": msg}
open(path, "a").write(json.dumps(rec) + "\n")
PY
    fi
    exit 0
  else
    log "switch-gate wait: $msg"
  fi
  sleep 5
  elapsed=$((elapsed + 5))
done

log "switch-gate FAIL after ${MAX_WAIT}s — abort round"
if [[ -n "$RECORD_JSON" ]]; then
  python3 - "$RECORD_JSON" <<'PY'
import json, sys, time
path = sys.argv[1]
rec = {"ts": time.time(), "result": "fail"}
open(path, "a").write(json.dumps(rec) + "\n")
PY
fi
exit 2
