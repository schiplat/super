#!/bin/bash
set -e

# --- Configuration ---
WORK_DIR=$(pwd)
PAYLOAD_BIN="$WORK_DIR/target/release/payloads"
SUPER_BIN="superd" # Assume superd is on PATH, or use an absolute path
SUPER_CLI="super"
CONFIG_DIR="$WORK_DIR/stress_configs"
LOG_DIR="$WORK_DIR/stress_logs"

# Build payload
echo "Building payload..."
cargo build --release --manifest-path ./payloads/Cargo.toml

mkdir -p "$CONFIG_DIR"
mkdir -p "$LOG_DIR"

# ==============================================================================
# Test 1: Log pipe performance (log backpressure)
# Key metric: does Super block the business process?
# ==============================================================================
echo -e "\n[Test 1/2] Log Throughput Overhead"

# A. Baseline
# Run payload directly; discard stdout (simulates an infinitely fast consumer)
echo "   Running Baseline (Direct -> /dev/null)..."
BASELINE_SPEED=$("$PAYLOAD_BIN" --mode log-throughput 2>&1 >/dev/null | grep "BENCH_RESULT" | cut -d':' -f2)
echo "   Baseline speed: $BASELINE_SPEED lines/sec"

# B. Super test
# Generate config
cat > "$CONFIG_DIR/throughput.toml" <<EOF
[[program]]
name = "throughput-test"
command = "$PAYLOAD_BIN"
args = ["--mode", "log-throughput"]
autostart = true
retry_limit = 0
EOF

echo "   Running via Super (Payload -> Pipe -> Super -> File)..."
# Start Super
"$SUPER_BIN" > /dev/null 2>&1 &
SUPER_PID=$!
sleep 1 # Wait for startup

# Apply config
"$SUPER_CLI" apply --file "$CONFIG_DIR/throughput.toml" > /dev/null

# Wait for process exit (payload exits when done)
# Poll super list until status is Stopped
echo "   Waiting for payload to finish..."
while true; do
    STATUS=$("$SUPER_CLI" list | grep "throughput-test" | awk '{print $4}')
    if [[ "$STATUS" == "Stopped" ]]; then
        break
    fi
    sleep 0.5
done

# Read stderr log for throughput (assumes super writes stderr under logs/)
# Ensure super's default config persists stderr
# For simplicity, assume log dir is ./logs
LOG_FILE="./logs/throughput-test.err"
if [ -f "$LOG_FILE" ]; then
    ACTUAL_SPEED=$(grep "BENCH_RESULT" "$LOG_FILE" | tail -n 1 | cut -d':' -f2)
else
    # Missing log file indicates a config issue
    echo "   Failed to find log file. Did super capture stderr?"
    ACTUAL_SPEED=0
fi

echo "   Actual speed:   $ACTUAL_SPEED lines/sec"

# C. Compute overhead
if (( $(echo "$ACTUAL_SPEED > 0" | bc -l) )); then
    RATIO=$(echo "$ACTUAL_SPEED / $BASELINE_SPEED * 100" | bc -l)
    echo "   Performance retained: ${RATIO:0:5}%"

    # Threshold check
    if (( $(echo "$RATIO < 50" | bc -l) )); then
        echo "   WARNING: Logging performance is POOR (<50%). Blocking business logic!"
    else
        echo "   Performance is acceptable."
    fi
else
    echo "   Test failed."
fi

# Cleanup
kill $SUPER_PID 2>/dev/null || true
rm -rf ./logs ./data

# ==============================================================================
# Test 2: Crash recovery stability
# Key metric: Super resource usage under frequent SIGCHLD handling
# ==============================================================================
echo -e "\n[Test 2/2] Crash Storm Stability (60s)"

# Generate configs for 50 random crashers
echo "" > "$CONFIG_DIR/crash.toml"
for i in {1..50}; do
cat >> "$CONFIG_DIR/crash.toml" <<EOF
[[program]]
name = "crasher-$i"
command = "$PAYLOAD_BIN"
args = ["--mode", "crash-random"]
autostart = true
retry_limit = 999
EOF
done

# Start Super
"$SUPER_BIN" > /dev/null 2>&1 &
SUPER_PID=$!
sleep 1

# Apply config
"$SUPER_CLI" apply --file "$CONFIG_DIR/crash.toml" > /dev/null

echo "   Running crash storm for 60 seconds..."
echo "   (Monitoring Super PID: $SUPER_PID)"

# Simple monitoring loop
for i in {1..12}; do
    # Sample Super CPU and RSS
    STATS=$(ps -p $SUPER_PID -o %cpu,rss --no-headers)
    CPU=$(echo $STATS | awk '{print $1}')
    MEM_KB=$(echo $STATS | awk '{print $2}')
    MEM_MB=$(echo "$MEM_KB / 1024" | bc -l)

    echo "   [$((i*5))s] CPU: ${CPU}% | Mem: ${MEM_MB:0:4} MB"
    sleep 5
done


# ==============================================================================
# Test 3: Cgroup isolation enforcement
# Key metric: does Super actually limit runaway processes?
# ==============================================================================
echo -e "\n[Test 3/3] Cgroup Enforcement Verification (Linux Only)"

if [[ "$(uname)" != "Linux" ]]; then
    echo "   Skipping Cgroup test (OS is not Linux)."
    exit 0
fi

# Check cgroup write access (superd needs root or delegated cgroup)
if [[ $EUID -ne 0 ]]; then
    echo "   Warning: Not running as root. Cgroup tests might fail if permissions are missing."
fi

# Start Super if not already running
if ! pgrep -x "$SUPER_BIN" > /dev/null; then
    "$SUPER_BIN" > /dev/null 2>&1 &
    SUPER_PID=$!
    sleep 2
fi

# --- A. CPU limit test ---
echo "   [A] Testing CPU Quota (Target: 20%)..."

# Config: 20% CPU quota (0.2 core)
cat > "$CONFIG_DIR/cpu_limit.toml" <<EOF
[[program]]
name = "cpu-burner"
command = "$PAYLOAD_BIN"
args = ["--mode", "cpu-burn"]
autostart = true
retry_limit = 0
[program.resource_limits]
cpu_quota = 0.2
EOF

"$SUPER_CLI" apply --file "$CONFIG_DIR/cpu_limit.toml" > /dev/null
sleep 2

# Get child process PID
BURNER_PID=$("$SUPER_CLI" list | grep "cpu-burner" | awk '{print $5}')

if [[ -z "$BURNER_PID" || "$BURNER_PID" == "-" ]]; then
    echo "   Failed to start cpu-burner."
else
    echo "   Process started with PID: $BURNER_PID"
    echo "   Sampling CPU usage for 5 seconds..."

    # Sample 5 times and average
    TOTAL_CPU=0
    for i in {1..5}; do
        # Use top -b -n 1 for instantaneous CPU
        USAGE=$(top -b -n 1 -p "$BURNER_PID" | tail -1 | awk '{print $9}')
        echo "     Sample $i: ${USAGE}%"
        TOTAL_CPU=$(echo "$TOTAL_CPU + $USAGE" | bc)
        sleep 1
    done

    AVG_CPU=$(echo "$TOTAL_CPU / 5" | bc)
    echo "   Average CPU: ${AVG_CPU}% (Expected: ~20%)"

    # Allow 5% margin (15%-25% passes)
    if (( $(echo "$AVG_CPU >= 15 && $AVG_CPU <= 25" | bc -l) )); then
        echo "   CPU limit enforced."
    else
        echo "   CPU limit FAILED (or cgroup not supported)."
    fi
fi

# Cleanup
"$SUPER_CLI" remove cpu-burner > /dev/null

# --- B. Memory OOM test ---
echo "   [B] Testing Memory Limit (Target: 50MB OOM Kill)..."

# Config: 50MB memory limit
# mem-eat allocates 5MB every 100ms; should be killed in ~1s
cat > "$CONFIG_DIR/mem_limit.toml" <<EOF
[[program]]
name = "mem-eater"
command = "$PAYLOAD_BIN"
args = ["--mode", "mem-eat"]
autostart = true
retry_limit = 0
[program.resource_limits]
memory_limit = 50
EOF

"$SUPER_CLI" apply --file "$CONFIG_DIR/mem_limit.toml" > /dev/null
sleep 1 # Wait for startup

# Monitor status changes
echo "   Watching for OOM Kill..."
OOM_DETECTED=false

for i in {1..10}; do
    # Get status
    STATUS_LINE=$("$SUPER_CLI" list | grep "mem-eater")
    STATUS=$(echo "$STATUS_LINE" | awk '{print $4}')

    # Check for Fatal, Stopped, or Backoff after kill
    if [[ "$STATUS" == "Fatal" || "$STATUS" == "Stopped" || "$STATUS" == "Backoff" ]]; then
        echo "   Process died as expected. Status: $STATUS"
        OOM_DETECTED=true
        break
    fi

    # Print current memory if PID exists
    PID=$(echo "$STATUS_LINE" | awk '{print $5}')
    if [[ "$PID" != "-" ]]; then
        RSS=$(ps -p "$PID" -o rss --no-headers 2>/dev/null || echo 0)
        MB=$(echo "$RSS / 1024" | bc)
        echo "     Current Mem: ${MB} MB"
    fi

    sleep 0.5
done

if [ "$OOM_DETECTED" = true ]; then
    echo "   Memory limit enforced (OOM killer triggered)."
else
    echo "   Memory limit FAILED (process ran too long)."
fi

# Final cleanup
kill $SUPER_PID 2>/dev/null || true
rm -rf ./logs ./data "$CONFIG_DIR"

echo -e "\nAll tests complete."
