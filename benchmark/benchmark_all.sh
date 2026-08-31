#!/usr/bin/env bash
# One-host-one-arm orchestrator (formal) or MODE=colocated smoke (never published).
# Usage (formal, on each host):
#   BENCH_ARM=super-oss   PHASE=A ./benchmark_all.sh
#   BENCH_ARM=super-pro   PHASE=A ./benchmark_all.sh   # + SUPER_BENCH_*
#   BENCH_ARM=supervisord PHASE=A ./benchmark_all.sh
#   BENCH_ARM=pm2         PHASE=A ./benchmark_all.sh
#   PHASE=B ... -> 3 rounds per arm
# Smoke only (never use results publicly):
#   MODE=colocated PHASE=A ./benchmark_all.sh
set -euo pipefail

BENCH_ROOT="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/lib.sh
source "$BENCH_ROOT/scripts/lib.sh"

PHASE="${PHASE:-A}"
MODE="${MODE:-one-host-one-arm}"
BENCH_ARM="${BENCH_ARM:-super-oss}"
RUN_PRO=0
if [[ "$BENCH_ARM" == "super-pro" ]]; then RUN_PRO=1; fi
ROUNDS=1
if [[ "$PHASE" == "B" ]]; then
  ROUNDS=3   # 3 independent rounds per arm
fi

# RES-1 scalability gradient: daemon-set RSS vs managed-process count.
N_IDLE="${BENCH_N_IDLE:-50}"
RES1_GRADIENT="${BENCH_RES1_GRADIENT:-50,200,500}"

BIN_DIR="${BIN_DIR:-$BENCH_ROOT/target/release}"
PAYLOAD="$BIN_DIR/payloads"
GENERATOR="$BIN_DIR/generator"
RUNNER="$BIN_DIR/runner"
SUPERD_BIN="${SUPERD_BIN:-$(command -v superd || true)}"
RESULTS="${RESULTS_DIR:-$BENCH_ROOT/results/${MODE}/${BENCH_ARM}/${PHASE}_$(date -u +%Y%m%dT%H%M%S)}"
CAP_MB="${CAP_MB:-64}"
# STB-2-PRO memory_limit: same order as CAP_MB (default 64 MiB), not a large heap.
CGROUP_MEM="${CGROUP_MEM_MB:-$CAP_MB}"
N_CRASH="${BENCH_N_CRASH:-30}"
N_LOG="${BENCH_N_LOG:-10}"
N_MEM="${BENCH_N_MEM:-10}"

mkdir -p "$RESULTS"
if [[ "$MODE" == "colocated" ]]; then
  echo '{"colocated_smoke":true,"publishable":false}' > "$RESULTS/mode.json"
else
  echo '{"colocated_smoke":false,"publishable":true}' > "$RESULTS/mode.json"
fi
export SUPER_BENCH_GATE_RECORD="$RESULTS/switch_gate.jsonl"

dur() {
  # name -> seconds
  case "$PHASE:$1" in
    A:RES-1) echo 30 ;; A:RES-2) echo 30 ;; A:STB-1) echo 60 ;; A:STB-2) echo 60 ;;
    A:STB-3) echo 180 ;; A:STB-4) echo 60 ;;
    B:RES-1) echo 60 ;; B:RES-2) echo 60 ;; B:STB-1) echo 120 ;; B:STB-2) echo 300 ;;
    B:STB-3) echo 600 ;; B:STB-4) echo 300 ;;
    *) echo 30 ;;
  esac
}

count_for() {
  case "$1" in
    RES-1|STB-3) echo "$N_IDLE" ;;
    STB-1) echo "$N_CRASH" ;;
    RES-2|STB-4) echo "$N_LOG" ;;
    STB-2|STB-2-PRO) echo "$N_MEM" ;;
    *) echo "$N_LOG" ;;
  esac
}
mode_for() {
  case "$1" in
    RES-1|STB-3) echo idle ;;
    STB-1) echo crash ;;
    RES-2|STB-4) echo log ;;
    STB-2|STB-2-PRO) echo mem-leak ;;
    *) echo idle ;;
  esac
}

# Arms to run in one round.
round_arms() {
  if [[ "$MODE" == "colocated" ]]; then
    latin_order_colocated "$1"
  else
    echo "$BENCH_ARM"
  fi
}

log "Building bench crates"
(cd "$BENCH_ROOT" && cargo build --release)

if [[ ! -x "$PAYLOAD" ]]; then
  echo "missing $PAYLOAD" >&2
  exit 1
fi

"$BENCH_ROOT/scripts/collect_manifest.sh" "$RESULTS/manifest.json"
echo "$BENCH_ARM" > "$RESULTS/arm.txt"
echo "$MODE" > "$RESULTS/mode.txt"

# ONB facts: N=1 idle generator tree (no daemon start, no score)
ONB_GEN="$RESULTS/onboard/generated"
mkdir -p "$ONB_GEN"
"$GENERATOR" --count 1 --mode idle --payload-path "$PAYLOAD" --output-dir "$ONB_GEN" --cap-mb "$CAP_MB"
"$BENCH_ROOT/scripts/onboard_facts.sh" "$RESULTS/onboard_facts.json" --generated "$ONB_GEN"

if [[ "$PHASE" == "B" || "${RUN_RAM_GATE:-1}" == "1" ]]; then
  "$BENCH_ROOT/scripts/ram_gate.sh" "$N_MEM" "$CAP_MB"
fi

if [[ "$RUN_PRO" == "1" ]]; then
  if [[ -z "${SUPER_BENCH_AUTH_SECRET:-}" ]]; then
    echo "super-pro arm requested but SUPER_BENCH_AUTH_SECRET unset. Export license env." >&2
    exit 1
  fi
fi

# Primary metrics (pre-registered)
cat > "$RESULTS/primary_metrics.json" <<'JSON'
{
  "RES-1": "daemon_set_rss_mb_median",
  "RES-2": "child_log_lines_per_sec_vs_bare",
  "STB-1": "daemon_alive_and_restart_sum",
  "STB-2": "daemon_alive_and_tree_rss_bounded_light_pressure",
  "STB-3": "daemon_rss_drift_mb_no_leak_primary",
  "STB-4": "daemon_alive_and_child_throughput",
  "MGT-1": "cold_poll_ms",
  "MGT-2": "status_p95_ms",
  "MGT-3": "reload_ms_and_pid_unchanged",
  "STB-2-PRO": "cgroup_containment_not_cross_tool"
}
JSON

BASE_LOAD=$(loadavg_1)
BASE_MEM=$(mem_available_kb)
echo "$BASE_LOAD" > "$RESULTS/baseline_loadavg"
echo "$BASE_MEM" > "$RESULTS/baseline_mem_kb"

abort_round() {
  echo "$1" >> "$RESULTS/invalid_rounds.txt"
  log "ROUND ABORT: $1"
}

run_arm_scenario() {
  local round="$1" scenario="$2" arm="$3" duration="$4" count="$5" mode="$6" cgroup_bytes="$7"
  local gen_dir="$RESULTS/run_${round}/${scenario}/generated"
  local work="$RESULTS/run_${round}/${scenario}/work/${arm}"
  local data="$RESULTS/run_${round}/${scenario}/data"
  mkdir -p "$data" "$work"

  rm -rf "$work"
  cp -a "$gen_dir/$arm" "$work"

  local extra=()
  if [[ "$arm" == "super-pro" ]]; then
    extra+=(--auth-token "${SUPER_BENCH_AUTH_SECRET}")
    if [[ -n "${SUPERD_BIN}" ]]; then extra+=(--superd "$SUPERD_BIN"); fi
  elif [[ "$arm" == "super-oss" && -n "${SUPERD_BIN}" ]]; then
    extra+=(--superd "$SUPERD_BIN")
  fi

  log "round=$round scenario=$scenario arm=$arm duration=${duration}s n=$count"
  "$RUNNER" \
    --target "$arm" \
    --instance-dir "$work" \
    --duration "$duration" \
    --expected-n "$count" \
    --output-csv "$data/${arm}.csv" \
    "${extra[@]}"
}

generate_once() {
  local round="$1" scenario="$2" count="$3" mode="$4" cgroup_mb="$5"
  local gen_dir="$RESULTS/run_${round}/${scenario}/generated"
  mkdir -p "$gen_dir"
  local cg_args=()
  if [[ "$cgroup_mb" != "0" ]]; then
    cg_args+=(--cgroup-memory-mb "$cgroup_mb")
  fi
  "$GENERATOR" \
    --count "$count" \
    --mode "$mode" \
    --payload-path "$PAYLOAD" \
    --output-dir "$gen_dir" \
    --cap-mb "$CAP_MB" \
    "${cg_args[@]}"
}

SCENARIOS=(RES-1 RES-2 STB-1 STB-2 STB-3 STB-4)

# RES-1 scalability gradient, executed before the main rounds.
run_res1_gradient() {
  local arm="$1" n total ms
  total=$(awk -F, '{print NF}' <<<"$RES1_GRADIENT")
  local i=0
  for n in ${RES1_GRADIENT//,/ }; do
    i=$((i + 1))
    local gen_dir="$RESULTS/gradient/RES-1/n${n}/generated"
    local work="$RESULTS/gradient/RES-1/n${n}/work/${arm}"
    local data="$RESULTS/gradient/RES-1/n${n}/data"
    mkdir -p "$gen_dir" "$work" "$data"
    "$GENERATOR" --count "$n" --mode idle --payload-path "$PAYLOAD" \
      --output-dir "$gen_dir" --cap-mb "$CAP_MB"
    cp -a "$gen_dir/$arm" "$work"
    local extra=()
    if [[ "$arm" == "super-pro" ]]; then
      extra+=(--auth-token "${SUPER_BENCH_AUTH_SECRET}")
      if [[ -n "${SUPERD_BIN}" ]]; then extra+=(--superd "$SUPERD_BIN"); fi
    elif [[ "$arm" == "super-oss" && -n "${SUPERD_BIN}" ]]; then
      extra+=(--superd "$SUPERD_BIN")
    fi
    log "gradient arm=$arm n=$n/$total duration=60s"
    if ! "$RUNNER" --target "$arm" --instance-dir "$work" --duration 60 --expected-n "$n" \
        --output-csv "$data/${arm}.csv" "${extra[@]}"; then
      log "gradient arm=$arm n=$n failed"
      return 1
    fi
  done
  return 0
}

if [[ "$PHASE" == "A" || "$PHASE" == "B" ]]; then
  for arm in $(round_arms 1); do
    run_res1_gradient "$arm" || log "gradient skipped for $arm"
  done
fi

for round in $(seq 1 "$ROUNDS"); do
  arms=$(round_arms "$round")
  log "===== ROUND $round / $ROUNDS  arms=$arms mode=$MODE ====="
  BASE_LOAD=$(loadavg_1)
  BASE_MEM=$(mem_available_kb)
  ROUND_OK=1

  for scenario in "${SCENARIOS[@]}"; do
    count=$(count_for "$scenario")
    mode=$(mode_for "$scenario")
    duration=$(dur "$scenario")
    generate_once "$round" "$scenario" "$count" "$mode" 0

    if [[ "$scenario" == "RES-2" || "$scenario" == "STB-4" ]]; then
      mkdir -p "$RESULTS/run_${round}/${scenario}"
      "$BENCH_ROOT/scripts/bare_log_baseline.sh" "$PAYLOAD" 8 \
        > "$RESULTS/run_${round}/${scenario}/bare_log.txt" || true
    fi

    for arm in $arms; do
      if [[ "$MODE" == "colocated" ]]; then
        if ! "$BENCH_ROOT/scripts/switch_gate.sh" "$BASE_LOAD" "$BASE_MEM"; then
          abort_round "round=${round} scenario=${scenario} next_arm=${arm} gate_fail"
          ROUND_OK=0
          break 2
        fi
      fi
      if ! run_arm_scenario "$round" "$scenario" "$arm" "$duration" "$count" "$mode" 0; then
        abort_round "round=${round} scenario=${scenario} arm=${arm} runner_fail"
        ROUND_OK=0
        break 2
      fi
    done

    python3 "$BENCH_ROOT/analysis/plot.py" "$RESULTS/run_${round}/${scenario}/data" \
      --title "${scenario} round ${round}" || true
  done

  if [[ "$ROUND_OK" != 1 ]]; then
    log "skipping MGT/SEC for aborted round $round"
    sleep 60
    continue
  fi

  # MGT + SEC
  generate_once "$round" "MGT" "$N_IDLE" idle 0
  mkdir -p "$RESULTS/run_${round}/MGT/data" "$RESULTS/run_${round}/security"
  for arm in $arms; do
    if [[ "$MODE" == "colocated" ]]; then
      if ! "$BENCH_ROOT/scripts/switch_gate.sh" "$BASE_LOAD" "$BASE_MEM"; then
        abort_round "round=${round} mgt gate_fail arm=${arm}"
        ROUND_OK=0
        break
      fi
    fi
    mkdir -p "$RESULTS/run_${round}/MGT/data" "$RESULTS/run_${round}/security"
    work="$RESULTS/run_${round}/MGT/work/${arm}"
    mkdir -p "$work"
    rm -rf "$work"
    cp -a "$RESULTS/run_${round}/MGT/generated/${arm}" "$work"
    extra=()
    if [[ "$arm" == "super-pro" ]]; then extra+=(--auth-token "${SUPER_BENCH_AUTH_SECRET}"); fi
    if [[ -n "${SUPERD_BIN}" && "$arm" == super-* ]]; then extra+=(--superd "$SUPERD_BIN"); fi
    "$RUNNER" --target "$arm" --instance-dir "$work" --duration 25 --expected-n "$N_IDLE" \
      --output-csv "$RESULTS/run_${round}/MGT/data/${arm}.csv" "${extra[@]}" &
    rpid=$!
    sleep 8
    "$BENCH_ROOT/scripts/mgt_run.sh" "$arm" "$work" "$RESULTS/run_${round}/MGT/${arm}.json" "$N_IDLE" || true
    "$BENCH_ROOT/scripts/sec_probes.sh" "$arm" "$work" "$RESULTS/run_${round}/security/${arm}.json" || true
    wait "$rpid" || true
  done

  if [[ "$RUN_PRO" == "1" && "$MODE" != "colocated" ]]; then
    if "$BENCH_ROOT/scripts/cgroup_gate.sh"; then
      if ! "$BENCH_ROOT/scripts/switch_gate.sh" "$BASE_LOAD" "$BASE_MEM"; then
        abort_round "round=${round} STB-2-PRO gate_fail"
      else
        generate_once "$round" "STB-2-PRO" "$N_MEM" mem-leak "$CGROUP_MEM"
        run_arm_scenario "$round" "STB-2-PRO" "super-pro" "$(dur STB-2)" "$N_MEM" mem-leak "$CGROUP_MEM" || \
          abort_round "round=${round} STB-2-PRO runner_fail"
      fi
    else
      echo "STB-2-PRO skipped (cgroup v2 memory controller unavailable)" \
        >> "$RESULTS/run_${round}/STB-2-PRO.skipped.txt"
    fi
  fi

  log "round $round cooldown 60s"
  sleep 60
done

python3 "$BENCH_ROOT/analysis/summarize.py" "$RESULTS" || true
log "Done. Results: $RESULTS"
echo "$RESULTS"