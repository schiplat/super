#!/usr/bin/env bash
# Bare log-throughput baseline: payload stdout → /dev/null (infinite-speed consumer).
set -euo pipefail
PAYLOAD="${1:?payloads binary}"
SECONDS_RUN="${2:-10}"
timeout --preserve-status "$SECONDS_RUN" "$PAYLOAD" --mode log-throughput --report-interval-ms 1000 \
  >/dev/null 2>"${TMPDIR:-/tmp}/bench-bare-log.err" || true
grep '^BENCH_RESULT:' "${TMPDIR:-/tmp}/bench-bare-log.err" | tail -n 3
