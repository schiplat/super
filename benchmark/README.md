# Super Benchmark Suite

A scientifically rigorous benchmark suite comparing `super`, `supervisord`, and `pm2`.

## Prerequisites

- **Rust**: Stable **1.85+** (matches the main workspace `rust-version`)
- **Python**: 3.8+ (with `pandas`, `matplotlib`)
- **Targets**:
  - `superd` (in PATH)
  - `supervisord` (`pip install supervisor`)
  - `pm2` (`npm install -g pm2`)

## Usage

### 1. Build Tools
```bash
cargo build --release
```

### 2. Generate Payloads & Configs
Generate configs for 100 idle processes:
```bash
./target/release/generator \
  --count 100 \
  --mode idle \
  --payload-path ./target/release/payloads \
  --output-dir ./configs
```

### 3. Run Benchmark
Run each target for 60 seconds:

```bash
mkdir -p results

# Bench Super
./target/release/runner --target super --config-dir ./configs --duration 60 --output-csv ./results/super.csv

# Bench Supervisor
./target/release/runner --target supervisor --config-dir ./configs --duration 60 --output-csv ./results/supervisor.csv

# Bench PM2
./target/release/runner --target pm2 --config-dir ./configs --duration 60 --output-csv ./results/pm2.csv
```

### 4. Analyze Results
```bash
pip install -r analysis/requirements.txt
python analysis/plot.py ./results
```

Check `./results/benchmark_result.png` for the comparison graph.
