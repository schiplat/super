#!/bin/bash
set -e

# --- Colors ---
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

# --- Paths ---
WORK_DIR=$(pwd)
BIN_DIR="$WORK_DIR/target/release"
PAYLOAD_BIN="$BIN_DIR/payloads"
GENERATOR_BIN="$BIN_DIR/generator"
RUNNER_BIN="$BIN_DIR/runner"
PLOT_SCRIPT="$WORK_DIR/analysis/plot.py"
FINAL_RESULTS_DIR="$WORK_DIR/benchmark_report"

# --- Build ---
echo -e "${BLUE}>>> Building Tools...${NC}"
cargo build --release
mkdir -p "$FINAL_RESULTS_DIR"

# ==========================================
# Phase 1: Memory footprint comparison (Super vs others)
# ==========================================
run_memory_comparison() {
    SCENARIO="01_memory_comparison"
    COUNT=50
    DURATION=30

    echo -e "\n${YELLOW}=== Phase 1: Memory Footprint Comparison ===${NC}"
    DIR="$FINAL_RESULTS_DIR/$SCENARIO"
    mkdir -p "$DIR/configs" "$DIR/data"

    # Generate configs for all targets
    "$GENERATOR_BIN" --count "$COUNT" --mode "idle" --payload-path "$PAYLOAD_BIN" --output-dir "$DIR/configs"

    # Run Super
    echo -e "${GREEN}Running Super...${NC}"
    "$RUNNER_BIN" --target super --config-dir "$DIR/configs" --duration "$DURATION" --output-csv "$DIR/data/super.csv"
    sleep 3

    # Run Supervisor
    echo -e "${GREEN}Running Supervisor...${NC}"
    "$RUNNER_BIN" --target supervisor --config-dir "$DIR/configs" --duration "$DURATION" --output-csv "$DIR/data/supervisor.csv"
    sleep 3

    # Run PM2
    echo -e "${GREEN}Running PM2...${NC}"
    "$RUNNER_BIN" --target pm2 --config-dir "$DIR/configs" --duration "$DURATION" --output-csv "$DIR/data/pm2.csv"

    # Plot memory comparison chart
    echo -e "${BLUE}Generating Comparison Chart...${NC}"
    python3 "$PLOT_SCRIPT" "$DIR/data" --mode compare
}

# ==========================================
# Phase 2: Super high-load stability tests
# ==========================================
run_super_stability() {
    SCENARIO_NAME=$1
    MODE=$2
    COUNT=$3
    DURATION=$4

    echo -e "\n${YELLOW}=== Phase 2: Super Stability Test ($SCENARIO_NAME) ===${NC}"
    DIR="$FINAL_RESULTS_DIR/$SCENARIO_NAME"
    mkdir -p "$DIR/configs" "$DIR/data"

    # Generate Super-only configs
    "$GENERATOR_BIN" --count "$COUNT" --mode "$MODE" --payload-path "$PAYLOAD_BIN" --output-dir "$DIR/configs"

    echo -e "${GREEN}Stressing Super...${NC}"
    "$RUNNER_BIN" --target super --config-dir "$DIR/configs" --duration "$DURATION" --output-csv "$DIR/data/super.csv"

    # Plot Super CPU/memory trends
    echo -e "${BLUE}Generating Stability Chart...${NC}"
    python3 "$PLOT_SCRIPT" "$DIR/data" --mode self
}

# --- Run ---

# 1. Memory comparison: 50 idle processes, 30s
run_memory_comparison

# 2. Crash storm: 50 random crashers, 60s
run_super_stability "02_super_crash_storm" "crash" 50 60

# 3. Log pressure: 20 high-volume loggers, 60s
run_super_stability "03_super_log_pressure" "log" 20 60

echo -e "\n${GREEN}Benchmark complete. Check '$FINAL_RESULTS_DIR'${NC}"
