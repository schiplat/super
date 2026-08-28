#!/usr/bin/env bash
# Refuse STB-2 if N * cap_mb would consume more than half of MemAvailable.
set -euo pipefail
COUNT="${1:?count}"
CAP_MB="${2:?cap_mb}"
avail_kb=$(awk '/MemAvailable:/ {print $2}' /proc/meminfo)
need_kb=$(( COUNT * CAP_MB * 1024 ))
half=$(( avail_kb / 2 ))
echo "RAM gate: need ${need_kb} kB (N=${COUNT} * ${CAP_MB} MiB), MemAvailable=${avail_kb} kB, limit(half)=${half} kB"
if (( need_kb > half )); then
  echo "FAIL: reduce --count or --cap-mb, or use a larger VM. Do not run unbounded mem-eat." >&2
  exit 1
fi
