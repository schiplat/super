#!/usr/bin/env bash
# Scenario-outer orchestrator: Latin square of 4 arms, switch gate, abort-round on gate fail.
# Usage:
#   PHASE=A ./benchmark_all.sh          # 1 shortened round (method smoke)
#   PHASE=B ./benchmark_all.sh          # 4 full rounds
#   SKIP_PRO=1 ...                      # OSS + supervisor + pm2 only
set -euo pipefail

BENCH_ROOT="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/lib.sh
source "$BENCH_ROOT/scripts/lib.sh"

PHASE="${PHASE:-A}"
SKIP_PRO="${SKIP_PRO:-0}"
ROUNDS=1
if [[ "$PHASE" == "B" ]]; then
  ROUNDS=4
fi

BIN_DIR="${BIN_DIR:-$BENCH_ROOT/target/release}"
PAYLOAD="$BIN_DIR/payloads"
GENERATOR="$BIN_DIR/generator"
RUNNER="$BIN_DIR/runner"
SUPERD_BIN="${SUPERD_BIN:-$(command -v superd || true)}"
RESULTS="${RESULTS_DIR:-$BENCH_ROOT/results/${PHASE}_$(date -u +%Y%m%dT%H%M%S)}"
CAP_MB="${CAP_MB:-512}"
CGROUP_MEM="${CGROUP_MEM_BYTES:-134217728}"  # 128 MiB for STB-2-PRO

mkdir -p "$RESULTS"
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
    RES-1|STB-3) echo 100 ;;
    STB-1) echo 50 ;;
    RES-2|STB-2|STB-4|STB-2-PRO) echo 20 ;;
    *) echo 20 ;;
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

arms_for_round() {
  local order
  order=$(latin_order "$1")
  if [[ "$SKIP_PRO" == "1" ]]; then
    echo "$order" | sed 's/super-pro//'
  else
    echo "$order"
  fi
}

log "Building bench crates"
(cd "$BENCH_ROOT" && cargo build --release)

if [[ ! -x "$PAYLOAD" ]]; then
  echo "missing $PAYLOAD" >&2
  exit 1
fi

"$BENCH_ROOT/scripts/collect_manifest.sh" "$RESULTS/manifest.json"

# ONB facts: N=1 idle generator tree (no daemon start, no score)
ONB_GEN="$RESULTS/onboard/generated"
mkdir -p "$ONB_GEN"
"$GENERATOR" --count 1 --mode idle --payload-path "$PAYLOAD" --output-dir "$ONB_GEN" --cap-mb "$CAP_MB"
"$BENCH_ROOT/scripts/onboard_facts.sh" "$RESULTS/onboard_facts.json" --generated "$ONB_GEN"

if [[ "$PHASE" == "B" || "${RUN_RAM_GATE:-1}" == "1" ]]; then
  "$BENCH_ROOT/scripts/ram_gate.sh" 20 "$CAP_MB"
fi

if [[ "$SKIP_PRO" != "1" ]]; then
  if [[ -z "${SUPER_BENCH_AUTH_SECRET:-}" ]]; then
    echo "PRO arm requested but SUPER_BENCH_AUTH_SECRET unset. Set SKIP_PRO=1 or export license env." >&2
    exit 1
  fi
fi

# Primary metrics (pre-registered)
cat > "$RESULTS/primary_metrics.json" <<'JSON'
{
  "RES-1": "daemon_set_rss_mb_median",
  "RES-2": "child_log_lines_per_sec_vs_bare",
  "STB-1": "daemon_alive_and_restart_sum",
  "STB-2": "daemon_alive_and_tree_rss_bounded",
  "STB-3": "daemon_rss_drift_mb",
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
  local round="$1" scenario="$2" count="$3" mode="$4" cgroup_bytes="$5"
  local gen_dir="$RESULTS/run_${round}/${scenario}/generated"
  mkdir -p "$gen_dir"
  local cg_args=()
  if [[ "$cgroup_bytes" != "0" ]]; then
    cg_args+=(--cgroup-memory-bytes "$cgroup_bytes")
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

for round in $(seq 1 "$ROUNDS"); do
  log "===== ROUND $round / $ROUNDS  order=$(arms_for_round "$round") ====="
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

    for arm in $(arms_for_round "$round"); do
      if ! "$BENCH_ROOT/scripts/switch_gate.sh" "$BASE_LOAD" "$BASE_MEM"; then
        abort_round "round=${round} scenario=${scenario} next_arm=${arm} gate_fail"
        ROUND_OK=0
        break 2
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

  # MGT + SEC: start idle N=100 per arm with gates
  generate_once "$round" "MGT" 100 idle 0
  mkdir -p "$RESULTS/run_${round}/MGT/data" "$RESULTS/run_${round}/security"
  for arm in $(arms_for_round "$round"); do
    if ! "$BENCH_ROOT/scripts/switch_gate.sh" "$BASE_LOAD" "$BASE_MEM"; then
      abort_round "round=${round} mgt gate_fail arm=${arm}"
      ROUND_OK=0
      break
    fi
    mkdir -p "$RESULTS/run_${round}/MGT/data" "$RESULTS/run_${round}/security"
    work="$RESULTS/run_${round}/MGT/work/${arm}"
    mkdir -p "$work"
    rm -rf "$work"
    cp -a "$RESULTS/run_${round}/MGT/generated/${arm}" "$work"
    extra=()
    if [[ "$arm" == "super-pro" ]]; then extra+=(--auth-token "${SUPER_BENCH_AUTH_SECRET}"); fi
    if [[ -n "${SUPERD_BIN}" && "$arm" == super-* ]]; then extra+=(--superd "$SUPERD_BIN"); fi
    # Short hold so MGT can talk to a live daemon, then probes, then runner teardown.
    "$RUNNER" --target "$arm" --instance-dir "$work" --duration 25 --expected-n 100 \
      --output-csv "$RESULTS/run_${round}/MGT/data/${arm}.csv" "${extra[@]}" &
    rpid=$!
    sleep 8
    "$BENCH_ROOT/scripts/mgt_run.sh" "$arm" "$work" "$RESULTS/run_${round}/MGT/${arm}.json" 100 || true
    "$BENCH_ROOT/scripts/sec_probes.sh" "$arm" "$work" "$RESULTS/run_${round}/security/${arm}.json" || true
    wait "$rpid" || true
  done

  if [[ "$SKIP_PRO" != "1" ]]; then
    if "$BENCH_ROOT/scripts/cgroup_gate.sh"; then
      if ! "$BENCH_ROOT/scripts/switch_gate.sh" "$BASE_LOAD" "$BASE_MEM"; then
        abort_round "round=${round} STB-2-PRO gate_fail"
      else
        generate_once "$round" "STB-2-PRO" 20 mem-leak "$CGROUP_MEM"
        run_arm_scenario "$round" "STB-2-PRO" "super-pro" "$(dur STB-2)" 20 mem-leak "$CGROUP_MEM" || \
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
